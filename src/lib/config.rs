use toml::decode_str;

use std::io::Write;
use std::io::Read;

use std::fs::metadata;
use std::fs::File;

use std::process::exit;

use std::collections::BTreeMap;

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
    pub yt: ConfigYT,
    pub twitch: BTreeMap<String,String>, 
}

#[derive(Debug, RustcDecodable,Clone)]
pub struct ConfigYT {
    pub audio_normal_mp4: i16,
    pub audio_normal_webm: i16,
    pub audio_hq: i16,
}

#[derive(Debug, RustcDecodable,Clone)]
pub struct Extensions {
    pub aac: Vec<i16>,
    pub mp3: Vec<i16>,
    pub m4a: Vec<i16>,
    pub mp4: Vec<i16>,
    pub flv: Vec<i16>,
    pub webm: Vec<i16>,
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

/// Read config from file.
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

/// Create, read & write a new config.
pub fn create_config(path: &str) -> Result<Config,ConfigError> {
    //TODO: replace with import_string
    trace!("Creating config..");
    let toml = r#"[db]
user = "user"
password = "password"
db = "ytdownl"
port = 3306
ip = "127.0.0.1"

[general]

# temporary dir for downloads before the conversion etc
temp_dir = "/downloads/temp"

# final destination of downloaded files / playlists
download_dir = "/downloads"
download_mbps = 48 # MBit/s limit
mp3_quality = 3 #see https://trac.ffmpeg.org/wiki/Encode/MP3

# folder in which the ffmpeg binary lies
ffmpeg_bin_dir = "/ffmpeg/ffmpeg-2.6.2-64bit-static/"

# additional lib callable in case of country-locks
# will be called with {[optional arguments]} -q {quality} -r {speed limit} -f {dest. file} -v {video/audio -> true/false} {url}
# the lib's return after 'name: ' will be taken as the name of the video/file to use
lib_use = false
lib_bin = "/binary" # path to binary
lib_args = ["arg1", "arg2"] # additional arguments
lib_dir = "/" # working dir to use

[codecs]
# values used from external
audio_mp3 = -1
audio_raw = -2
audio_source_hq = -3

# see https://en.wikipedia.org/wiki/YouTube#Quality_and_formats
# the individual values for video-downloads are set by the db-entry
# these values here are for music/mp3 extract/conversion
[codecs.yt]
audio_normal_mp4 = 140
audio_normal_webm = 171
audio_hq = 22

# quality ids for twitch
# quality id - twitch quality
[codecs.twitch]
-10 = "Mobile"
-11 = "Low"
-12 = "Medium"
-13 = "High"
-14 = "Source"


# which file ending should be used for the quality codes from youtube
# this is also needed to download the right audio part for every video container
[extensions]
aac = [-2,-3]
mp3 = [-1]
m4a = []
mp4 = [299,298,137,136,135,134,133,22,18]
flv = []
webm = [303,302,248]
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