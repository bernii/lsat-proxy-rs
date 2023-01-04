use std::{collections::HashMap, str::FromStr};

use anyhow::{anyhow, bail};
use hyper_tls::HttpsConnector;
use serde_json::Value;
use tracing::debug;
use warp::{
    http::HeaderValue,
    hyper::{header::HeaderName, Body, Method, Request},
};

use crate::config::Backend;

/// Handing connnectivity and processing to the upstream
/// server.
#[derive(Debug)]
pub struct Upstream {
    backend: Backend,
    req: Option<Request<Body>>,
    resp_data: Option<String>,
}

impl Upstream {
    /// Create a new object based on the Backend definition
    pub fn new(backend: Backend) -> Self {
        Self {
            backend,
            req: None,
            resp_data: None,
        }
    }

    /// Buld the request based on the input data that we want
    /// to forward and the Backend config.
    /// Using the builder pattern
    pub fn build<'a>(
        &'a mut self,
        indata: &HashMap<String, String>,
    ) -> Result<&'a mut Self, anyhow::Error> {
        let mut req = warp::hyper::Request::builder()
            .method(Method::POST)
            .uri(&self.backend.upstream);

        let header = req.headers_mut().unwrap();

        for h in &self.backend.headers {
            let hvec: Vec<&str> = h.split(':').collect();

            let key = hvec.first().unwrap().trim();
            let val = hvec.get(1).unwrap().trim();
            header.insert(
                HeaderName::from_str(key).unwrap(),
                HeaderValue::from_str(val).unwrap(),
            );
        }

        let mut body: Value = serde_json::from_str(&self.backend.body).unwrap();

        // update body with data provided by the user in the
        // inboud request
        parse_indata(indata, &self.backend.pass_fields)?
            .iter()
            .for_each(|(k, v)| body[k] = v.to_owned());

        debug!("Prepared request {:?}, with body {:?}", req, body);
        self.req = Some(req.body(Body::from(body.to_string()))?);
        Ok(self)
    }

    /// Perform the HTTP call to the upstream server
    pub async fn make(&mut self) -> Result<&mut Self, anyhow::Error> {
        let https = HttpsConnector::new();
        let client = warp::hyper::Client::builder().build(https);

        let req = self
            .req
            .take()
            .ok_or_else(|| anyhow!("Request not ready"))?;
        let resp = client.request(req).await?;

        // extract user_wallet_id, user_id, user_admin_key
        let bytes = warp::hyper::body::to_bytes(resp).await?;
        self.resp_data = Some(String::from_utf8(bytes.to_vec())?);
        Ok(self)
    }

    /// Parse the response from the upstream server
    pub fn parse<'a>(&'a mut self) -> Result<String, anyhow::Error> {
        let data = self
            .resp_data
            .take()
            .ok_or_else(|| anyhow!("Response data not ready"))?;
        let mut root: &Value = &serde_json::from_str(&data)?;
        debug!("JSON-parsed response {:?}", root);

        // parse values to forward to client based
        // on the response_fields definition
        for field in self
            .backend
            .response_fields
            .split('.')
            .collect::<Vec<&str>>()
        {
            root = match field.parse::<usize>() {
                Ok(num) => root.get(num).unwrap(),
                Err(_) => root.get(field).unwrap(),
            }
        }
        Ok(root
            .as_str()
            .ok_or_else(|| anyhow!("Unable to convert to string"))?
            .to_string())
    }
}

/// Validates input fields and passes only the ones that
/// we defined as desirable
fn parse_indata(
    indata: &HashMap<String, String>,
    pass_fields: &HashMap<String, String>,
) -> Result<HashMap<String, Value>, anyhow::Error> {
    let mut out = HashMap::new();
    for (key, ktype) in pass_fields.iter() {
        let val = indata.get(key).ok_or_else(|| anyhow!("No key in indata"))?;
        let casted = match ktype.as_str() {
            "string" => Value::try_from(val.to_string()),
            "int" => Value::try_from(val.parse::<i32>()?),
            "float" => Value::try_from(val.parse::<f32>()?),
            _ => bail!("Unknown field type: {}", ktype),
        }?;
        out.insert(key.to_string(), casted);
    }
    Ok(out)
}
