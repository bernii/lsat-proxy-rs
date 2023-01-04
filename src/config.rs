use serde::Deserialize;
use std::{collections::HashMap, net::IpAddr};

use crate::lsat::MiliSats;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: Server,
    pub lnd: Lnd,
    pub backends: Vec<Backend>,
}
// https://github.com/mehcode/config-rs

#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    pub host: IpAddr,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Lnd {
    pub host: String,
    pub tls_path: String,
    pub mac_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Backend {
    pub name: String,
    pub path: String,
    pub upstream: String,
    pub headers: Vec<String>,
    pub body: String,
    // dest_protocol: String,
    pub pass_fields: HashMap<String, String>,
    pub capabilties: String,                  // add/subtract/?
    pub constraints: HashMap<String, String>, // ENUM lifetime
    pub price_msat: u32,
    pub budget_multiple: Option<u32>,
    pub price_passthrough: bool, // ask the backend
    pub response_fields: String,
}

impl Backend {
    pub fn amount_total(&self) -> MiliSats {
        MiliSats(self.price_msat * self.budget_multiple.unwrap_or(1))
    }
    pub fn get_price(&self) -> MiliSats {
        MiliSats(self.price_msat)
    }
}

enum Constraints {
    Timeout(u32),
}
