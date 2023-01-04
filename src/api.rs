use std::{collections::HashMap, convert::Infallible};

use bitcoin_hashes::Hash;

use lightning_invoice::Invoice;
use serde_json::json;

use tonic_lnd::lnrpc::invoice::InvoiceState;
use tracing::{debug, error, info, instrument};
use warp::{
    hyper::{HeaderMap, StatusCode},
    reject, Rejection, Reply,
};

use crate::{
    config::{Backend, Config},
    db, lnd,
    lsat::{self, HeadersParser, MiliSats, ToSha256},
    upstream::Upstream,
};

#[derive(Debug)]
struct MyRejection<'a>(&'a str);
impl reject::Reject for MyRejection<'static> {}

#[derive(Debug)]
struct Nope;
impl warp::reject::Reject for Nope {}

#[instrument(level = "info", skip(_config, lnd))]
pub async fn handle_invoice_status(
    _config: Config,
    indata: HashMap<String, String>,
    lnd: lnd::Client,
) -> Result<impl warp::Reply, warp::Rejection> {
    let inv: Invoice = indata
        .get("invoice")
        .ok_or_else(|| {
            error!("No invoice field found");
            MyRejection("Invoice field not found")
        })?
        .parse()
        .map_err(|e| {
            error!(error=%e, "Problem parsing invoice");
            MyRejection("Unable to parse invoice")
        })?;

    let ph = lnd::PaymentHash {
        r_hash: inv.payment_hash().to_vec(),
        ..Default::default()
    };

    let inv = lnd.lookup_invoice(ph).await.map_err(|e| {
        error!(status=%e, "Provided invoice not found");
        MyRejection("Unable to find invoice")
    })?;

    info!(state=?inv.state(), "retrived invoice state");

    let resp = json!({
        "preimage": hex::encode(inv.r_preimage),
        "state": inv.state,
    });
    Ok(warp::reply::json(&resp).into_response())
}

#[instrument(level = "info", skip(lnd))]
pub async fn handle_protected(
    backend: Backend,
    indata: HashMap<String, String>,
    headers: HeaderMap,
    lnd: lnd::Client,
) -> Result<impl warp::Reply, warp::Rejection> {
    debug!(headers=?headers, indata=?indata, "Handling protected resource");

    if !headers.contains_key("Authorization") {
        let indata_sha = indata.to_sha256().unwrap();
        return lsat::Lsat::generate_challange(lnd, &backend, &indata_sha)
            .await
            .map_err(|e| {
                error!(error=%e, "Unable to generate auth header");
                MyRejection("Unable to generate challange").into()
            });
    }

    let (lsat, preimage) = headers.parse_lsat().map_err(|e| {
        error!(error=%e, "Unable to parse LSAT header");
        MyRejection("LSAT incorrect")
    })?;

    let mut entry = db::Entry::get(&lsat.id).await.map_err(|e| {
        error!(error=%e, "No lsat found in the database for id");
        MyRejection("No db entry for LSAT, possibly expired")
    })?;

    let indata_sha = indata.to_sha256().unwrap();
    lsat.verify(&entry.secret(), &backend.path, indata_sha)
        .await
        .map_err(|e| {
            error!(error=%e, "LSAT macaroon verification failed");
            MyRejection("LSAT incorrect")
        })?;

    // update quota / user budget
    entry.quota -= backend.get_price();
    match entry.quota {
        MiliSats(0) => {
            info!("Avaiable budget exhausted, removing entry from DB");
            entry.remove().await;
        }
        _ => entry.update().await.unwrap(),
    }

    let preimage_sha = preimage.to_sha256().map_err(|e| {
        error!(error=%e, "Can't get preimage sha");
        MyRejection("Problem calculating sha256")
    })?;

    if lsat.id.payment_hash.0 != preimage_sha.into_inner() {
        error!("Preimage does not match payment hash");
        return Err(MyRejection("Preimage does not match payment hash").into());
    }

    // TODO: verify invoice status
    // TODO: should be a pre-cached cache db call instead
    // with API call fallback if we're not aware of such invoice
    debug!(
        "Getting invoice state for preimage: {:?}",
        hex::encode(preimage.0)
    );

    let ph = lnd::PaymentHash {
        r_hash: preimage
            .to_sha256()
            .expect("this is hashable for sure")
            .to_vec(),
        ..Default::default()
    };
    let inv = lnd.lookup_invoice(ph).await.map_err(|e| {
        error!(error=%e, "Unable to get invoice state");
        MyRejection("Unable to get invoice state")
    })?;

    if inv.state() != InvoiceState::Settled {
        error!("Invoice is not settled!");
        return Err(MyRejection("Invoice is not settled").into());
    }
    // we're finally happy after all the checks,
    // make the actual call with provided data
    let mut upstream = Upstream::new(backend.clone());

    let data = upstream
        .build(&indata)
        .map_err(|e| {
            error!(error=%e, "Unable to construct upstream request");
            reject::custom(Nope)
        })?
        .make()
        .await
        .map_err(|e| {
            error!(error=%e, "Unable to contact upstream");
            reject::custom(Nope)
        })?
        .parse()
        .map_err(|e| {
            error!(error=%e, "Unable to parse upstream response");
            reject::custom(Nope)
        })?;

    let paragraphs: Vec<&str> = data.trim().split("\n\n").collect();

    let mut resp = warp::reply::json(&json!({ "data": paragraphs })).into_response();
    resp.headers_mut()
        .insert("x-msats-quota", entry.quota.into());
    Ok(resp)
}

/// Receives a `Rejection` and tries to return a custom
/// value, otherwise simply passes the rejection along.
pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let message: String;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "NOT_FOUND".into();
    } else if let Some(MyRejection(e)) = err.find() {
        // error!(error=?e.to_string(), "Error when processing request");
        code = StatusCode::BAD_REQUEST;
        message = e.to_string();
    // } else if let Some(DivideByZero) = err.find() {
    //     code = StatusCode::BAD_REQUEST;
    //     message = "DIVIDE_BY_ZERO";
    // } else if let Some(e) = err.find::<warp::filters::body::BodyDeserializeError>() {
    //     // This error happens if the body could not be deserialized correctly
    //     // We can use the cause to analyze the error and customize the error message
    //     message = match e.source() {
    //         Some(cause) => {
    //             if cause.to_string().contains("denom") {
    //                 "FIELD_ERROR: denom"
    //             } else {
    //                 "BAD_REQUEST"
    //             }
    //         }
    //         None => "BAD_REQUEST",
    //     };
    //     code = StatusCode::BAD_REQUEST;
    // } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
    //     // We can handle a specific error, here METHOD_NOT_ALLOWED,
    //     // and render it however we want
    //     code = StatusCode::METHOD_NOT_ALLOWED;
    //     message = "METHOD_NOT_ALLOWED";
    } else {
        // We should have expected this... Just log and say its a 500
        error!("unhandled rejection: {:?}", err);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "UNHANDLED_REJECTION".into();
    }

    let json = warp::reply::json(&json!({
        "code": code.as_u16(),
        "message": message,
    }));

    Ok(warp::reply::with_status(json, code))
}
