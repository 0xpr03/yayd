use toml::decode_str;

use std::io::Write;
use std::io::Read;

use std::fs::metadata;
use std::fs::File;

use lib::{self,l_expect};

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
    pub extensions: Extensions,
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
    pub mp3_quality: i16,
    pub download_mbps: u16, // download speed limit, curr. not supported by the DMCA lib
    pub ffmpeg_bin: String, // path to ffmpeg binary, which can be another dir for non-free mp3
}

#[derive(Debug, RustcDecodable,Clone)]
pub struct ConfigCodecs {
    pub audio_raw: i16,
    pub audio_source_hq: i16,
    pub audio_mp3: i16,
}

#[derive(Debug, RustcDecodable,Clone)]
pub struct Extensions {
    pub aac: Vec<i16>,
    pub mp3: Vec<i16>,
    pub m4a: Vec<i16>,
    pub mp4: Vec<i16>,
    pub flv: Vec<i16>,
}

/// create PathBuf by getting the current working dir
pub fn init_config() -> Config {
    let mut path = l_expect(lib::get_executable_folder(), "config folder"); // PathBuf
    path.set_file_name("config.cfg"); // set_file_name doesn't return smth -> needs to be run on mut path
    trace!("{:?}",path );
    let config : Option<Config>;
    if metadata(&path).is_ok() { // PathExt for path..as_path().exists() is unstable
        info!("Config file found.");
        config = read_config(&path.to_str().unwrap()).ok(); //result to option
    }else{
        info!("Config file not found.");
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

#these values need to be changed, example values of my dev setup
[general]
save_dir = "/home/dev/downloads/temp"
download_dir = "/home/dev/downloads"
jar_folder = "/home/dev/yayd"
jar_cmd = "/home/dev/Downloads/jdk1.7.0_75/jre/bin"
download_mbps = 6 #mb/s limit
mp3_quality = 3 #see https://trac.ffmpeg.org/wiki/Encode/MP3
ffmpeg_bin = "/ffmpeg/ffmpeg-2.6.2-64bit-static/"

#see https://en.wikipedia.org/wiki/YouTube#Quality_and_formats
[codecs]
audio_mp3 = 1
audio_raw = 140
audio_source_hq = 22

[extensions]
aac = [140,22]
mp3 = [1]
m4a = []
mp4 = [299,298,137,136,135,134,133,22,18]
flv = [5]

    "#;
    let mut file = try!(File::create(path).map_err(|_| ConfigError::CreateError ));
    let config: Config = match decode_str(&toml) {
        None => return Err(ConfigError::ParseError),
        Some(dconfig) => dconfig,
    };
    trace!("Raw new config: {:?}", config);
    try!(file.write_all(toml.as_bytes()).map_err(|_| ConfigError::WriteError));
    Ok(config)
}