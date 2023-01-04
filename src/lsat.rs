use std::{
    ops::SubAssign,
    time::{SystemTime, UNIX_EPOCH}, collections::HashMap,
};

use anyhow::{bail, Context};
use bitcoin_hashes::{sha256, Hash};
use lightning::ln::{PaymentHash, PaymentPreimage};
use lightning_invoice::Invoice;
use regex::Regex;
use macaroon::{ByteString, Caveat, Format, Macaroon, MacaroonKey, Verifier};
use rand::Rng;
use serde::{Deserialize, Serialize};
use itertools::Itertools;

const TOKEN_ID_SIZE: usize = 32;
const ID_VERSION: usize = 0;
static AUTH_REG_FORMAT: &str = "LSAT (.*?):([a-f0-9]{64})";

/// LSAT structure
pub struct Lsat {
    pub id: Id,
    pub mac: Macaroon,
}

/// Simple wrapper for milisats units so it's easy to convert
/// to and from sats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiliSats(pub u32);

impl SubAssign<MiliSats> for MiliSats {
    fn sub_assign(&mut self, rhs: MiliSats) {
        self.0 -= rhs.0;
    }
}

impl From<MiliSats> for HeaderValue {
    fn from(val: MiliSats) -> Self {
        HeaderValue::from(val.0)
    }
}

/// Varoius permuatations of the LSAT header construction
pub enum HeaderName {
    /// Used by REST clients.
    Authorization,
    /// Used by certain REST and gRPC clients.
    MacaroonMeta,
    /// Used by LNLabs gRPC clients.
    Macaroon,
}

impl HeaderName {
    /// Convert enum to string representation for matching etc.
    fn as_str(&self) -> &'static str {
        match self {
            HeaderName::Authorization => "Authorization",
            HeaderName::MacaroonMeta => "Grpc-Metadata-Macaroon",
            HeaderName::Macaroon => "Macaroon",
        }
    }
}

impl From<HeaderName> for &str {
    fn from(val: HeaderName) -> Self {
        val.as_str()
    }
}

#[derive(Serialize, Deserialize)]
struct Token([u8; TOKEN_ID_SIZE]);

#[derive(Serialize, Deserialize)]
#[serde(remote = "PaymentHash")]
pub struct PaymentHashDef(pub [u8; 32]);

/// LSAT header identifier
#[derive(Serialize, Deserialize)]
pub struct Id {
    version: usize,
    #[serde(with = "PaymentHashDef")]
    pub payment_hash: PaymentHash,
    token_id: Token,
}

pub trait ToSha256 {
    fn to_sha256(&self) -> Result<sha256::Hash, anyhow::Error>;
}

impl Id {
    pub fn new(payment_hash: PaymentHash) -> Self {
        let mut rng = rand::thread_rng();
        let token_id = Token(rng.gen());
        Self {
            version: ID_VERSION,
            payment_hash,
            token_id,
        }
    }
}

impl ToSha256 for Id {
    fn to_sha256(&self) -> Result<sha256::Hash, anyhow::Error> {
        let data = bincode::serialize(&self)?;
        Ok(sha256::Hash::hash(data.as_slice()))
    }
}

impl From<Id> for ByteString {
    fn from(val: Id) -> Self {
        let bytes = bincode::serialize(&val).unwrap();
        ByteString::from(hex::encode(bytes))
    }
}

impl TryFrom<&Macaroon> for Id {
    type Error = anyhow::Error;

    fn try_from(mac: &Macaroon) -> Result<Self, Self::Error> {
        let bytes = &hex::decode(mac.identifier())?;
        let id: Id = bincode::deserialize(bytes)?;

        if id.version != ID_VERSION {
            bail!("wrong hash version");
        }
        Ok(id)
    }
}

impl ToSha256 for PaymentPreimage {
    fn to_sha256(&self) -> Result<sha256::Hash, anyhow::Error> {
        Ok(sha256::Hash::hash(&self.0))
    }
}

impl ToSha256 for HashMap<String, String> {
    // TODO: this is quite costly, improve if higher traffix
    fn to_sha256(&self) -> Result<sha256::Hash, anyhow::Error> {
        let str = &self.iter()
            .sorted_by_key(|(k, _)| *k)
            .flat_map(|(x, y)| [x.to_string(), y.to_string()])
            .collect::<Vec<_>>()
            .join("");
        Ok(sha256::Hash::hash(&str.as_bytes()))
    }
}

pub trait FromPreimage {
    fn from_preimage(preimage: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

impl FromPreimage for PaymentPreimage {
    /// converts &str representing hex-encoded preimage value
    /// to an actual PaymentPreimage instance
    fn from_preimage(preimage: &str) -> Result<Self, anyhow::Error> {
        let bytes = hex::decode(preimage)?;
        let arr = bytes[..32].try_into()?;
        Ok(Self(arr))
    }
}

impl Lsat {
    /// initalize LSAT with the macaroon value
    /// useful when parsing incoming headers
    fn init(mac: Macaroon) -> Result<Self, anyhow::Error> {
        Ok(Self {
            id: (&mac).try_into()?,
            mac,
            // qouta: MiliSats(0),
        })
    }

    /// exract value of a predicate of given nanme from
    /// the macaroon that is part of the LSAT
    fn get_predicate(&self, name: &str) -> Result<String, anyhow::Error> {
        Ok(self
            .mac
            .caveats()
            .iter()
            .find_map(|c| {
                if let Caveat::FirstParty(p) = c {
                    let pred_s = p.predicate().to_string();
                    let s = pred_s.split('=').next().expect("two elements");
                    if s.len() == 2 && s == name {
                        return Some(pred_s);
                    }
                }
                None
            })
            .expect("macaroon predicate not found"))
    }

    /// obtain an invoice from LND and extract the payment request & hash
    async fn new_challenge(lnd: lnd::Client, price: MiliSats) -> Result<Invoice, anyhow::Error> {
        // generate new invoice via lnd first. We need to know the payment hash
        // so we can add it as a caveat to the macaroon.
        let resp = lnd
            .add_invoice(lnd::generate_invoice(price))
            .await
            .context("failed to generate invoice")?;

        let inv = str::parse::<Invoice>(&resp.payment_request)?;
        Ok(inv)
    }

    pub async fn generate_challange(
        lnd: lnd::Client,
        backend: &Backend,
        body_sha: &sha256::Hash,
    ) -> Result<Response, anyhow::Error> {
        // We'll start by retrieving a new challenge in the form of a Lightning
        // payment request to present the requester of the LSAT with.
        let inv = Lsat::new_challenge(lnd, backend.amount_total()).await?;

        // We can then proceed to mint the LSAT with a unique identifier that is
        // mapped to a unique secret.
        let id = Id::new(PaymentHash(inv.payment_hash().into_inner()));

        let secret = MacaroonKey::generate(&id.to_sha256()?);

        db::Entry::insert(&id, &secret, backend.amount_total()).await?;

        let mut mac = Macaroon::create(
            Some("https://lsat-playground.bucko.vercel.app".to_string()),
            &secret,
            id.into(),
        )?;

        // apply restrictions to the LSAT/macaroon.
        let curr_ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        // TODO: make this configurable
        mac.add_first_party_caveat(format!("time<{}", curr_ts + 120).into());
        mac.add_first_party_caveat(format!("path={}", backend.path).into());
        mac.add_first_party_caveat(format!("payload={}", body_sha.encode_hex::<String>()).into());


        let mut res = Response::default();
        let hval = format!(
            r#"LSAT macaroon="{}" invoice="{}""#,
            mac.serialize(Format::V1)?,
            inv,
        );
        res.headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_str(&hval)?);

        *res.status_mut() = StatusCode::PAYMENT_REQUIRED;
        Ok(res)
    }

    pub async fn verify(&self, secret: &MacaroonKey, path: &str, body_sha: sha256::Hash) -> Result<(), anyhow::Error> {
        // ensure the LSAT was minted by us.
        let signature = MacaroonKey::generate(&self.id.to_sha256()?);

        info!(
            "LSAT mac signature is {} raw {} sig {}",
            hex::encode(self.mac.signature()),
            ByteString::from(self.mac.signature()).to_string(),
            hex::encode(signature),
        );
        info!("Id HASH is {}", self.id.to_sha256()?.encode_hex::<String>());

        if secret != &signature {
            bail!("macaroon signature mismatch".to_string());
        }

        // LSAT verified, inspect caveats to ensure the
        // target service is authorized.
        let mut verifier = Verifier::default();
        verifier.satisfy_general(timestamp_verifier);
        verifier.satisfy_exact(format!("path={}", path).into());
        // TODO: this causes issues with quota
        // verifier.satisfy_exact(format!("payload={}", body_sha.encode_hex::<String>()).into());

        let cc: Vec<String> = self
            .mac
            .caveats()
            .iter()
            .map(|c| {
                if let Caveat::FirstParty(p) = c {
                    return p.predicate().to_string();
                }
                "".to_string()
            })
            .collect();

        info!("TODO: fix this pls {:?}", cc);

        verifier
            .verify(&self.mac, secret, Default::default())?;

        Ok(())
    }
}

pub trait HeadersParser {
    fn parse_lsat(self) -> Result<(Lsat, PaymentPreimage), anyhow::Error>;
}

impl HeadersParser for HeaderMap {

    fn parse_lsat(self) -> Result<(Lsat, PaymentPreimage), anyhow::Error> {

        if let Some(h_auth) = self.get(HeaderName::Authorization.as_str()) {

            let h_auth = h_auth.to_str()?;
            info!("Trying to authorize with header value [{}]", h_auth);

            let re = Regex::new(AUTH_REG_FORMAT)?;
            let matches = re.captures(h_auth).context("unable to find regex elems")?;

            if matches.len() != 3 {
                bail!("Invalid auth header format");
            }

            let (mac_base64, preimage_hex) = (matches.get(1).unwrap(), matches.get(2).unwrap());
            let mac = Macaroon::deserialize(mac_base64.as_str())?;

            let preimage = hex::decode(preimage_hex.as_str())?;
            let preimage = PaymentPreimage(preimage[..32].try_into()?);

            // all done, no need to extract anything from the
            // macaroon since the preimage was presented separately.
            return Ok((Lsat::init(mac)?, preimage));
        }

        let auth_header = if let Some(header) = self.get(HeaderName::MacaroonMeta.as_str()) {
            // Header field 2: Contains only the macaroon.
            header
        } else if let Some(header) = self.get(HeaderName::Macaroon.as_str()) {
            // Header field 3: Contains only the macaroon.
            header
        } else {
            bail!("No LSAT header found");
        };

        // for case 2 and 3,unmarshal the macaroon to
        // extract the preimage.
        // let mac = Macaroon::deserialize(auth_header)?;
        let lsat = Lsat::init(Macaroon::deserialize(auth_header)?)?;

        let preimage = PaymentPreimage::from_preimage(&lsat.get_predicate("preimage")?)?;
        Ok((lsat, preimage))
    }
}

use hex::ToHex;
use tracing::info;
use warp::{
    http::HeaderValue,
    hyper::{header, HeaderMap, StatusCode},
    reply::Response,
};

use crate::{config::Backend, db, lnd};

fn timestamp_verifier(caveat: &ByteString) -> bool {
    if !caveat.0.starts_with(b"time<") {
        return false;
    }
    let strcaveat = match std::str::from_utf8(&caveat.0) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let curr_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let ts: u64 = strcaveat.split('<').last().unwrap().trim().parse().unwrap();
    info!("Checking timestamps {} < {}", curr_ts, ts);
    curr_ts < ts
}
