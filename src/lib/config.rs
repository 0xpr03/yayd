use toml::decode_str;

use std::io::Write;
use std::io::Read;

use std::fs::{File,metadata,OpenOptions};
use std::path::Path;

use std::process::exit;

use CONFIG_PATH;
use lib::{self,l_expect};

// pub mod config;
// Config section

/// Config Error struct
#[derive(Debug)]
pub enum ConfigError {
    ReadError,
    WriteError,
    CreateError,
    ParseError,
}

/// Main config struct
#[derive(Debug, RustcDecodable)]
pub struct Config {
    pub db: ConfigDB,
    pub general: ConfigGen,
    pub codecs: ConfigCodecs,
}

/// Config struct DBMS related
#[derive(Debug, RustcDecodable)]
pub struct ConfigDB {
    pub user: String,
    pub password: String,
    pub port: u16,
    pub db: String,
    pub ip: String,
}

/// General settings config struct
#[derive(Clone, Debug, RustcDecodable)]
pub struct ConfigGen{
    pub link_subqueries: bool,
    pub link_files: bool,
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

/// Codec config struct
#[derive(Debug, RustcDecodable,Clone)]
pub struct ConfigCodecs {
    pub audio_raw: i16,
    pub audio_source_hq: i16,
    pub audio_mp3: i16,
    pub yt: ConfigYT,
}

/// Youtube config struct
#[derive(Debug, RustcDecodable,Clone)]
pub struct ConfigYT {
    pub audio_normal_mp4: i16,
    pub audio_normal_webm: i16,
    pub audio_hq: i16,
    
}

/// Init config, reading from file or creating such
#[cfg(not(test))]
pub fn init_config() -> Config {
    let mut path = l_expect(lib::get_executable_folder(), "config folder"); // PathBuf
    path.push(CONFIG_PATH); // set_file_name doesn't return smth -> needs to be run on mut path
    trace!("config path {:?}",path );
    let data: String;
    if metadata(&path).is_ok() { // PathExt for path..as_path().exists() is unstable
        info!("Config file found.");
        data = l_expect(read_config(&path),"unable to read config!");
    }else{
        info!("Config file not found.");
        data = create_config();
        l_expect(write_config_file(&path, &data),"unable to write config");
        
        exit(0);
    }
    
    l_expect(parse_config(data), "unable to parse config")
}

/// Config for test builds, using environment variables
#[cfg(test)]
pub fn init_config() -> Config {
    use std::env;
    macro_rules! env(
        ($s:expr) => (match env::var($s) { Ok(val) => val, Err(_) => panic!("unable to read env var {}",$s),});
    );

    let data = create_config();
    let mut conf = l_expect(parse_config(data),"invalid default config!");
    conf.general.ffmpeg_bin_dir = env!("ffmpeg_dir");
    conf.general.download_dir = env!("download_dir");
    conf.general.temp_dir = env!("temp_dir");
    conf.general.download_mbps = l_expect(env!("mbps").parse::<u16>(),"parse mbps");
    conf.db.user = env!("user");
    conf.db.password = env!("pass");
    conf.db.ip = env!("ip");
    conf.db.port = env!("port").parse::<u16>().unwrap();
    conf.db.db = env!("db");
    conf
}

/// Parse input toml to config struct
fn parse_config(input: String) -> Result<Config, ConfigError> {
    match decode_str(&input) {
        None => Err(ConfigError::ParseError),
        Some(dconfig) => Ok(dconfig),
    }
}

/// Read config from file.
pub fn read_config(file: &Path) -> Result<String,ConfigError> {
    let mut f = try!(OpenOptions::new().read(true).open(file).map_err(|_| ConfigError::ReadError));
    let mut data = String::new();
    try!(f.read_to_string(&mut data).map_err(|_| ConfigError::ReadError));
    Ok(data)
}

/// Create a new config.
pub fn create_config() -> String {
    trace!("Creating config..");
    let toml = r#"[db]
user = "user"
password = "password"
db = "yayd"
port = 3306
ip = "127.0.0.1"

[general]

# insert subquery relations into table subqueries
link_subqueries = true
# store file-query relations in query_files table
link_files = true

# temporary dir for downloads before the conversion etc
temp_dir = "~/downloads/temp"

# final destination of downloaded files / playlists
download_dir = "~/downloads"
download_mbps = 48 # MBit/s limit
mp3_quality = 3 #see https://trac.ffmpeg.org/wiki/Encode/MP3

# folder in which the ffmpeg binary lies
ffmpeg_bin_dir = "~/ffmpeg/ffmpeg-2.6.2-64bit-static/"

# additional lib callable in case of country-locks
# will be called with {[optional arguments]} -q {quality} -r {speed limit} -f {dest. file} -v {video/audio -> true/false} {url}
# the lib's return after 'name: ' will be taken as the name of the video/file to use
lib_use = false
lib_bin = "/binary" # path to binary
lib_args = ["arg1", "arg2"] # additional arguments
lib_dir = "/" # working dir to use

[codecs]
# audio type : quality value
audio_mp3 = -1
audio_raw = -2
audio_source_hq = -3

# see https://en.wikipedia.org/wiki/YouTube#Quality_and_formats
# the individual values for video-downloads are set by the db-entry
# these values here are for music/mp3 extract/conversion
[codecs.yt]
# audio type : itag
audio_normal_mp4 = 140
audio_normal_webm = 171
audio_hq = 22
    "#;
    trace!("Raw new config: {:?}", toml);
    
    toml.to_owned()
}

/// Writes the recived string into the file
fn write_config_file(path: &Path, data: &str) -> Result<(),ConfigError> {
    let mut file = try!(File::create(path).map_err(|_| ConfigError::CreateError ));
    try!(file.write_all(data.as_bytes()).map_err(|_| ConfigError::WriteError));
    Ok(())
}