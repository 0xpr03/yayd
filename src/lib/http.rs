use reqwest::header::HeaderMap;
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_ENCODING, LOCATION, USER_AGENT,
};
use reqwest::blocking::{Client, Response};
use serde::de::DeserializeOwned;

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use crate::lib::{Error, Result};

use crate::C_USER_AGENT;

/// Download into file
pub fn http_download<P: AsRef<Path>>(url: &str, target: P) -> Result<()> {
    let mut response = get_raw(url, HeaderType::Html)?;
    let mut file = File::create(target)?;
    response.copy_to(&mut file)?;
    Ok(())
}

/// Header type for get requests
pub enum HeaderType {
    /// Html browser request
    Html,
    /// Ajax js request
    Ajax,
}

/// Does a raw get request under the provided url & header
fn get_raw(url: &str, htype: HeaderType) -> Result<Response> {
    trace!("Starting request {}", url);

    let client = Client::builder()
        .gzip(true)
        .timeout(Duration::from_secs(10))
        .build()?;
    let builder = client.get(url);
    let res = builder.headers(header(htype)).send()?;

    debug!("Response header: {:?}", res.headers());
    debug!("Response status: {:?}", res.status());
    debug!("Final URL: {:?}", res.headers().get(LOCATION));
    trace!("DEV header: {:?}", res.headers().get(CONTENT_ENCODING));
    Ok(res)
}

/// Do an http(s) get request, returning text
pub fn http_text_get(url: &str) -> Result<String> {
    trace!("Starting request {}", url);
    let response = get_raw(url, HeaderType::Ajax)?;
    Ok(response.text()?)
}

/// Do an http(s) get request, returning JSON
pub fn http_json_get<T>(url: &str) -> Result<T>
where T: DeserializeOwned {
    trace!("Starting request {}", url);
    let response = get_raw(url, HeaderType::Ajax)?;
    let body = response.text()?;
    match serde_json::from_str(&body) {
        Err(e) => {
            Err(Error::InternalError(format!("Parsing error {} for {}", e,body)))
        },
        Ok(v) => Ok(v),
    }
}

/// Construct a header
/// This function does not check for errors and is
/// verified by the tests
fn header(htype: HeaderType) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT_ENCODING, "gzip, deflate, br".parse().unwrap());

    match htype {
        HeaderType::Html => {
            headers.insert(
                ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
                    .parse()
                    .unwrap(),
            );
        }
        HeaderType::Ajax => {
            headers.insert(
                ACCEPT,
                "application/json, text/javascript, */*; q=0.01"
                    .parse()
                    .unwrap(),
            );
        }
    }
    headers.insert(CONNECTION, "close".parse().unwrap());
    headers.insert(USER_AGENT, C_USER_AGENT.parse().unwrap());

    trace!("Generated headers: {:?}", headers);
    headers
}

#[cfg(test)]
mod test {
    use serde_json::Value;

    use super::*;

    #[test]
    fn get_ajax() {
        assert!(http_json_get::<Value>("https://httpbin.org/user-agent").is_ok());
    }
}
