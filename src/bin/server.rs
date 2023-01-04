use lsat_proxy::config::Backend;
use tracing::info;

use lsat_proxy::{
    api::{handle_invoice_status, handle_protected, handle_rejection},
    config::Config,
    lnd,
};

use tracing_subscriber::EnvFilter;
use warp::{path::FullPath, Filter, hyper::HeaderMap, http::HeaderValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // install global collector configured based on RUST_LOG env var.
    let subscriber = tracing_subscriber::fmt()
        // Use a more compact, abbreviated log format
        .compact()
        // base filter on RUST_LOG
        .with_env_filter(EnvFilter::from_default_env())
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Display the thread ID an event was recorded on
        .with_thread_ids(true)
        // Don't display the event's target (module path)
        .with_target(false)
        // Build the subscriber
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config: Config = config::Config::builder()
        // Add in `./Settings.toml`
        .add_source(config::File::with_name("config"))
        // Add in settings from the environment (with a prefix of APP)
        // Eg.. `APP_DEBUG=1 ./target/app` would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build()
        .expect("problem building the config")
        .try_deserialize()
        .expect("problem deserializing config");

    info!("Connfiguration loaded on startup: {:?}", config);

    // Connecting to LND requires only address, cert file, and macaroon file
    let lnd_conf = config.lnd.clone();
    let lnd_client = lnd::Client::init(lnd_conf.host, lnd_conf.tls_path, lnd_conf.mac_path).await;

    info!("Spinning up streaming listener for LND RPC");
    let lnd_conf = config.lnd.clone();
    let lnd_stream = lnd::Client::init(lnd_conf.host, lnd_conf.tls_path, lnd_conf.mac_path).await;
    lnd_stream.subscribe_invoices().await;

    let info = lnd_client.get_info().await.expect("failed to get info");
    info!("LND Instance Info: {:#?}", info);

    info!("Listening on {}:{}", config.server.host, config.server.port);

    let mut headers = HeaderMap::new();
    headers.insert("Access-Control-Expose-Headers", HeaderValue::from_static("*"));
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST", "DELETE"])
        .allow_headers(vec!["accept-authenticate", "content-type", "authorization"]);
    
    let base = warp::any().and(with_clone(config.clone()));

    let invoice_status = base
        .clone()
        .and(warp::path!("invoice" / "status"))
        .and(warp::body::json())
        .and(with_clone(lnd_client.clone()))
        .and_then(handle_invoice_status);

    let protected = base
        .clone()
        .and(warp::path::full())
        .and_then(protected_path)
        .and(warp::body::json())
        .and(warp::header::headers_cloned())
        .and(with_clone(lnd_client.clone()))
        .and_then(handle_protected);

    let routes = warp::any()
        .and(invoice_status)
        .or(protected)
        .recover(handle_rejection)
        .with(cors)
        .with(warp::reply::with::headers(headers));
    info!("Starting server...");
    warp::serve(routes)
        .run((config.server.host, config.server.port))
        .await;
    Ok(())
}

pub async fn protected_path(config: Config, path: FullPath) -> Result<Backend, warp::Rejection> {
    let backend = config.backends.iter().find(|b| b.path == path.as_str());

    match backend {
        Some(backend) => Ok(backend.clone()),
        None => Err(warp::reject()),
    }
}

/// Warp helper for cloning configration and db references
/// so they can be passed into request handlers.
pub fn with_clone<C: Clone + Send>(
    c: C,
) -> impl Filter<Extract = (C,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || c.clone())
}
