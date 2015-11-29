use toml::decode_str;

use std::io::Write;
use std::io::Read;

use std::fs::metadata;
use std::fs::File;

use std::process::exit;

use CONFIG_PATH;
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
    pub temp_dir: String, // folder to temp. save the raw files
    pub download_dir: String, // folder to which the files should be moved
    pub mp3_quality: i16,
    pub download_mbps: u16, // download speed limit, curr. not supported by the DMCA lib
    pub ffmpeg_bin_dir: String, // path to ffmpeg binary, which can be another dir for non-free mp3
    pub lib_use: bool,
    pub lib_dir: String,
    pub lib_bin: String,
    pub lib_args: Vec<String>,
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
    path.push(CONFIG_PATH); // set_file_name doesn't return smth -> needs to be run on mut path
    trace!("config path {:?}",path );
    let config : Config;
    if metadata(&path).is_ok() { // PathExt for path..as_path().exists() is unstable
        info!("Config file found.");
        config = l_expect(read_config(&path.to_str().unwrap()),"config read");
    }else{
        info!("Config file not found.");
        l_expect(create_config(&path.to_str().unwrap()), "config creation");
        
        exit(0);
    }
    config
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
    trace!("Creating config..");
    let toml = r#"[db]
user = "root"
password = ""
db = "ytdownl"
port = 3306
ip = "127.0.0.1"

[general]

#temporary dir for downloads before the conversion etc
temp_dir = "/downloads/temp"

#final destination of downloaded files / playlists
download_dir = "/downloads"
download_mbps = 6 #mb/s limit
mp3_quality = 3 #see https://trac.ffmpeg.org/wiki/Encode/MP3

#folder in which the ffmpeg binary lies
ffmpeg_bin_dir = "/ffmpeg/ffmpeg-2.6.2-64bit-static/"

#additional lib callable in case of country-locks
#will be called with {[optional arguments]} -q {quality} -f {dest. file} -v {video/audio -> true/false} {url}
#the lib's return after 'name: ' will be taken as the name of the video/file to use
lib_use = true
lib_bin = "/binary" #path to binary
lib_args = ["arg1", "arg2"] #additional arguments
lib_dir = "/" #working dir to use

#see https://en.wikipedia.org/wiki/YouTube#Quality_and_formats
#the individual values for video-downloads are set by the db-entry
#these values here are for music/mp3 extract/conversion
[codecs]
audio_mp3 = 1
audio_raw = 140
audio_source_hq = 22

#quality codes listes here and which file ending should be used for them
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