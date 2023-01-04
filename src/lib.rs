// use config::Backend;

// use bytes::Buf;

// use log::*;
// use std::collections::HashMap;

// use hyper::server::conn::Http;
// use hyper::service::service_fn;
// use hyper::{Body, Method, Request, Response, StatusCode};

use serde::Serialize;

pub mod api;
pub mod config;
pub mod db;
pub mod lnd;
pub mod lsat;
pub mod upstream;

/// An API error serializable to JSON.
#[derive(Serialize)]
struct ErrorMessage {
    code: u16,
    message: String,
}
