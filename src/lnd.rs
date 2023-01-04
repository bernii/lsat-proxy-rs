use std::{fmt::Debug, sync::Arc};

use lazy_static::lazy_static;
use std::time::Duration;
use stretto::AsyncCache;
use tokio::{sync::Mutex, time::sleep};
use tonic_lnd::{
    lnrpc::{self, AddInvoiceResponse, GetInfoResponse, InvoiceSubscription},
    tonic::Status,
};
use tracing::{error, info, warn};

use crate::lsat::MiliSats;

pub use tonic_lnd::lnrpc::PaymentHash;

lazy_static! {
    static ref CACHE: AsyncCache<Vec<u8>, lnrpc::Invoice> =
        AsyncCache::new(1024, 1e6 as i64, tokio::spawn).unwrap();
}

/// Clonable LndClient that wraps arc/mutex with a clean
/// api to use with wrap async framework
pub struct Client {
    lnd: Arc<Mutex<tonic_lnd::Client>>,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            lnd: self.lnd.clone(),
        }
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("lnd_status", &"initialized")
            .finish()
    }
}

impl Client {
    pub async fn init(host: String, tls_path: String, mac_path: String) -> Client {
        let client = tonic_lnd::connect(host, tls_path, mac_path)
            .await
            .expect("failed to connect");

        Self {
            lnd: Arc::new(Mutex::new(client)),
        }
    }

    /// Subscribe to invoice events
    pub async fn subscribe_invoices(&self) {
        let client = self.clone();

        info!("Sprawing task to handle invoice stream updates");
        tokio::task::spawn(async move {
            loop {
                let inv_stream = client
                    .lnd
                    .lock()
                    .await
                    .lightning()
                    .subscribe_invoices(InvoiceSubscription::default())
                    .await;

                if inv_stream.is_err() {
                    error!("Unable to connect via GRPC, restarting loop");
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
                let mut inv_stream = inv_stream.unwrap().into_inner();

                loop {
                    match inv_stream.message().await {
                        Ok(Some(inv)) => {
                            info!(inv=?inv, "Invoice update arrived");
                            CACHE
                                .insert_with_ttl(
                                    inv.r_hash.clone(),
                                    inv,
                                    1,
                                    Duration::from_secs(60 * 10),
                                )
                                .await;
                        }
                        _ => {
                            error!("Something went wrong, restarting loop");
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        });
    }

    /// Create a new invoice with LND
    pub async fn add_invoice(
        &self,
        invoice: tonic_lnd::lnrpc::Invoice,
    ) -> Result<AddInvoiceResponse, Status> {
        let add_inv = self
            .lnd
            .lock()
            .await
            .lightning()
            .add_invoice(invoice)
            .await?
            .into_inner();
        Ok(add_inv)
    }

    /// Find invoice in the LND node
    pub async fn lookup_invoice(&self, ph: PaymentHash) -> Result<lnrpc::Invoice, Status> {
        match CACHE.get(&ph.r_hash.to_vec()) {
            Some(cache_state) => Ok(cache_state.value().clone()),
            None => {
                warn!("checking invoice at LND server");
                let inv = self
                    .lnd
                    .lock()
                    .await
                    .lightning()
                    .lookup_invoice(ph)
                    .await?
                    .into_inner();

                // update cache
                CACHE
                    .insert_with_ttl(
                        inv.r_hash.clone(),
                        inv.clone(),
                        1,
                        Duration::from_secs(10 * 60),
                    )
                    .await;
                Ok(inv)
            }
        }
    }

    /// Get basic info about the LND node
    pub async fn get_info(&self) -> Result<GetInfoResponse, Status> {
        Ok(self
            .lnd
            .lock()
            .await
            .lightning()
            .get_info(tonic_lnd::lnrpc::GetInfoRequest {})
            .await?
            .into_inner())
    }
}

/// Generate a basic structure for the invoice, with given value/price
pub fn generate_invoice(price: MiliSats) -> tonic_lnd::lnrpc::Invoice {
    tonic_lnd::lnrpc::Invoice {
        memo: "LSAT payment".to_string(),
        value_msat: price.0 as i64,
        expiry: 60 * 10, // 10 minutes
        // expiry: 60 * 60 * 24 * 7, // 1 week
        ..Default::default()
    }
}
