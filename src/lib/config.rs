use toml::decode_str;

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
    pub codecs: ConfigCodecs,
}

#[derive(Debug, RustcDecodable)]
pub struct ConfigDB {
    pub user: String,
    pub password: String,
    pub port: u16,
    pub db: String,
    pub ip: String,
}

#[derive(Clone, Debug, RustcDecodable)]
pub struct ConfigGen{
    pub save_dir: String, // folder to temp. save the raw files
    pub download_dir: String, // folder to which the files should be moved
    pub jar_folder: String, // DMCA lib
    pub jar_cmd: String, // command for the DMCA lib
    pub download_mbps: u16, // download speed limit, curr. not supported by the DMCA lib
    pub ffmpeg_bin: String, // path to ffmpeg binary, which can be another dir for non-free mp3
}

#[derive(Debug, RustcDecodable)]
pub struct ConfigCodecs {
    pub audio: i16,
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
save_dir = "/home/dev/downloads/temp"
download_dir = "/home/dev/downloads"
jar_folder = "/home/dev/yayd"
jar_cmd = "/home/dev/Downloads/jdk1.7.0_75/jre/bin/java -jar"
download_mbps = 6
ffmpeg_bin = "/ffmpeg/ffmpeg-2.6.2-64bit-static/"

[codecs]
audio = 140
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