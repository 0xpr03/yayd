
use std::path::Path;
use std::io::Read;
use std::io::copy;
use std::fs::File;

use hyper::header::{Headers,AcceptEncoding,Connection,ConnectionOption,AcceptCharset,Accept,Encoding,UserAgent,ContentEncoding,qitem,QualityItem,Charset,Quality};
use hyper::mime::{Mime, TopLevel, SubLevel, Attr, Value};
use hyper::client::Client;

use hyper::net::HttpsConnector;
use hyper_native_tls::NativeTlsClient;

use json::JsonValue;
use json;

use flate2::read::GzDecoder;

use lib::Error;

use USER_AGENT;

/// Do an http(s) get request, returning JSON
pub fn http_json_get(url: &str) -> Result<JsonValue,Error> {
    trace!("Starting request {}",url);
    let ssl = NativeTlsClient::new().unwrap();
    let connector = HttpsConnector::new(ssl);
    
    let client = Client::with_connector(connector);
    let builder = client.get(url);
    let mut res = builder.headers(create_headers()).send()?;
    debug!("Response header: {:?}",res.headers);
    debug!("Response status: {:?}",res.status);
    debug!("Response http version: {:?}",res.version);
    debug!("DEV header: {:?}",res.headers.get::<ContentEncoding>());
    let mut body = String::new();
    let gzipped = if res.headers.has::<ContentEncoding>() {
        res.headers.get::<ContentEncoding>().unwrap().contains(&Encoding::Gzip)
    }else{
        false
    };
    debug!("Gzip compressed: {}",gzipped);
    
    if gzipped {
        let mut decoder = try!(GzDecoder::new(res));
        try!(decoder.read_to_string(&mut body));
    }else{
        try!(res.read_to_string(&mut body));
    }
    
    json::parse(&body).map_err(|e| Error::InternalError(format!("Parsing error {}",e)))
}

/// Download from an http origin
pub fn http_download<P: AsRef<Path>>(url: &str, target: P) -> Result<(),Error> {
    trace!("Starting download");
    let ssl = NativeTlsClient::new().unwrap();
    let connector = HttpsConnector::new(ssl);
    
    let client = Client::with_connector(connector);
    trace!("Creating builder");
    let builder = client.get(url);
    trace!("Creating target file");
    let mut target_file = try!(File::create(target));
    trace!("starting file stream");
    let mut res = try!(builder.headers(create_headers()).send());
    try!(copy(&mut res, &mut target_file));
    trace!("finished http download");
    Ok(())
}

/// Create headers
fn create_headers() -> Headers {
    let mut headers = Headers::new();
    
    headers.set(
        AcceptEncoding(vec![
            qitem(Encoding::Chunked),
            qitem(Encoding::Gzip),
        ])
    );
    headers.set(
        AcceptCharset(vec![
            QualityItem::new(Charset::Us_Ascii, Quality(100)),
            QualityItem::new(Charset::Ext("utf-8".to_owned()), Quality(900)),
        ])
    );
    headers.set(
        Accept(vec![
            qitem(Mime(TopLevel::Application,SubLevel::Json,vec![(Attr::Charset,Value::Utf8)])),
        ])
    );
    headers.set(
            Connection(
            vec![(ConnectionOption::Close)]
            )
    );
    headers.set(UserAgent(USER_AGENT.to_owned()));
    
    debug!("Headers: {}",headers);
    headers
}

#[cfg(test)]
mod test {
    use super::*;
    
    #[test]
    fn get_ajax() {
        assert!(http_json_get("https://httpbin.org/user-agent").is_ok());
    }
}
