use toml::{Table,decode_str};

use std::io::Write;
use std::io::Read;

use std::env::current_dir;
use std::fs::PathExt;
use std::fs::File;



// pub mod config;
// Config section

#[derive(Debug)]
pub enum ConfigError {
    ReadError,
    WriteError,
    CreateError,
    ParseError,
}

#[derive(Debug, RustcDecodable)]
pub struct Config {
	pub db: ConfigDB,
	pub general: ConfigGen,
}

#[derive(Debug, RustcDecodable)]
pub struct ConfigDB {
	pub user: String,
	pub password: String,
	pub port: u16,
	pub db: String,
	pub ip: String,
}

#[derive(Debug, RustcDecodable)]
pub struct ConfigGen{
	pub save_dir: String,
	pub jar_folder: String,
}

/// create PathBuf by getting the current working dir
pub fn init_config() -> Config {
    let mut path = current_dir().unwrap();
    path.set_file_name("config.cfg"); // set_file_name doesn't return smth -> needs to be run on mut path
    println!("{:?}",path );
    let mut config : Option<Config>;
    if path.as_path().is_file() {
        println!("Config file found.");
        config = read_config(&path.to_str().unwrap()).ok(); //result to option
    }else{
        println!("Config file not found.");
        config = create_config(&path.to_str().unwrap()).ok();
    }
    config.unwrap()
}

pub fn read_config(file: &str) -> Result<Config,ConfigError> {
    let mut f = try!(File::open(file).map_err(|_| ConfigError::ReadError));
    let mut toml = String::new();
    try!(f.read_to_string(&mut toml).map_err(|_| ConfigError::ReadError));
    let config: Config = match decode_str(&toml) {
        None => return Err(ConfigError::ParseError),
        Some(dconfig) => dconfig,
    };
    Ok(config)
}

pub fn create_config(path: &str) -> Result<Config,ConfigError> {
    //TODO: replace with import_string
    let toml = r#"[db]
user = "root"
password = ""
db = "ytdownl"
port = 3306
ip = "127.0.0.1"

[general]
save_dir = "~/downloads/"
jar_folder = "~/yayd"
    "#;
    let mut file = try!(File::create(path).map_err(|_| ConfigError::CreateError ));
    let config: Config = match decode_str(&toml) {
        None => return Err(ConfigError::ParseError),
        Some(dconfig) => dconfig,
    };
    println!("Raw new config: {:?}", config);
    try!(file.write_all(toml.as_bytes()).map_err(|_| ConfigError::WriteError));
    Ok(config)
}