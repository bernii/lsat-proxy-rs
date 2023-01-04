use anyhow::Context;
use hex::ToHex;
use lazy_static::lazy_static;
use macaroon::MacaroonKey;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::lsat::{self, MiliSats, ToSha256};

pub static DEFAULT_NAME: &str = "lsat-proxy.db";

lazy_static! {
    pub static ref DB: sled::Db = sled::open(DEFAULT_NAME).unwrap();
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Entry {
    id: String,
    secret: [u8; 32],
    pub quota: MiliSats,
}

impl Entry {
    pub fn secret(&self) -> MacaroonKey {
        MacaroonKey::from(self.secret)
    }

    pub async fn get(id: &lsat::Id) -> Result<Self, anyhow::Error> {
        let db_id = format!(
            "lsat/proxy/secrets/{}",
            id.to_sha256()?.encode_hex::<String>()
        );
        info!(id = db_id, "getting entry from db");

        let entry = DB
            .get(&db_id)
            .context("failed interact with db")?
            .context("should be an entry in db")?;
        debug!(id = db_id, "Got entry from db: {:?}", entry);
        Ok(rmp_serde::from_slice(&entry)?)
    }

    pub async fn update(&self) -> Result<(), anyhow::Error> {
        info!(id = self.id, "updated in db");
        let value = rmp_serde::to_vec_named(&self)?;
        DB.insert(self.id.clone(), value)?;
        Ok(())
    }

    pub async fn insert(
        id: &lsat::Id,
        secret: &MacaroonKey,
        quota: MiliSats,
    ) -> Result<(), anyhow::Error> {
        let db_id = format!(
            "lsat/proxy/secrets/{}",
            id.to_sha256()?.encode_hex::<String>()
        );
        info!(id = db_id, "inserting into db");

        let value = rmp_serde::to_vec_named(&Self {
            id: db_id.clone(),
            secret: *secret.as_ref(),
            quota,
        })?;
        DB.insert(db_id, value)?;
        Ok(())
    }

    pub async fn remove(&self) {
        info!(id = self.id, "removing from db");
        DB.remove(&self.id).unwrap();
    }
}
