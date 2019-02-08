use toml::from_str;

use std::io::Read;
use std::io::Write;

use std::fs::{metadata, File, OpenOptions};
use std::path::Path;

use std::process::exit;

use lib::{self, l_expect};
use CONFIG_PATH;

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
#[derive(Debug, Deserialize)]
pub struct Config {
    pub db: ConfigDB,
    pub general: ConfigGen,
    pub cleanup: ConfigCleanup,
    pub codecs: ConfigCodecs,
}

/// Config struct DBMS related
#[derive(Debug, Deserialize)]
pub struct ConfigDB {
    pub user: String,
    pub password: String,
    pub port: u16,
    pub db: String,
    pub ip: String,
}

/// General settings config struct
#[derive(Clone, Debug, Deserialize)]
pub struct ConfigGen {
    pub link_subqueries: bool,
    pub link_files: bool,
    pub temp_dir: String,     // folder to temp. save the raw files
    pub download_dir: String, // folder to which the files should be moved
    pub mp3_quality: i16,
    pub download_mbps: u16, // download speed limit, curr. not supported by the DMCA lib
    pub ffmpeg_bin_dir: String, // path to ffmpeg binary, which can be another dir for non-free mp3
    pub lib_use: bool,
    pub lib_dir: String,
    pub lib_bin: String,
    pub lib_args: Vec<String>,
    pub clean_temp_dir: bool, // debug function deleting all files inside the temp folder on startup
    pub youtube_dl_dir: String,
    pub youtube_dl_auto_update: bool,
}

/// Cleanup settings config struct
#[derive(Clone, Debug, Deserialize)]
pub struct ConfigCleanup {
    pub auto_delete_files: bool,   // auto delete files
    pub auto_delete_age: u16,      // max age s
    pub auto_delete_interval: u16, // cleanup interval
    pub auto_delete_request: bool, // delete also the request: db entries of these files
    pub delete_files: bool,        // delete files requested
    pub delete_request: bool,      // delete also the request itself
    pub delete_interval: u16,      // execution interval
}

/// Codec config struct
#[derive(Debug, Deserialize, Clone)]
pub struct ConfigCodecs {
    pub audio_raw: i16,
    pub audio_source_hq: i16,
    pub audio_mp3: i16,
    pub yt: ConfigYT,
}

/// Youtube config struct
#[derive(Debug, Deserialize, Clone)]
pub struct ConfigYT {
    pub audio_normal_mp4: i16,
    pub audio_normal_webm: i16,
    pub audio_hq: i16,
}

/// Init config, reading from file or creating such
pub fn init_config() -> Config {
    let mut path = l_expect(lib::get_executable_folder(), "config folder"); // PathBuf
    path.push(CONFIG_PATH); // set_file_name doesn't return smth -> needs to be run on mut path
    trace!("config path {:?}", path);
    let data: String;
    if metadata(&path).is_ok() {
        // PathExt for path..as_path().exists() is unstable
        info!("Config file found.");
        data = l_expect(read_config(&path), "unable to read config!");
    } else {
        info!("Config file not found.");
        data = create_config();
        l_expect(write_config_file(&path, &data), "unable to write config");

        exit(0);
    }

    l_expect(parse_config(data), "unable to parse config")
}

/// Config for test builds, using environment variables
#[allow(unused)]
pub fn init_config_test() -> Config {
    use std::env;
    macro_rules! env(
        ($s:expr) => (match env::var($s) { Ok(val) => val, Err(_) => panic!("unable to read env var {}",$s),});
    );

    let data = create_config();
    let mut conf = l_expect(parse_config(data), "invalid default config!");
    conf.general.ffmpeg_bin_dir = env!("ffmpeg_dir");
    conf.general.download_dir = env!("download_dir");
    conf.general.temp_dir = env!("temp_dir");
    conf.general.download_mbps = l_expect(env!("mbps").parse::<u16>(), "parse mbps");
    conf.general.link_files = true;
    conf.general.link_subqueries = true;
    conf.db.user = env!("user");
    conf.db.password = env!("pass");
    conf.db.ip = env!("ip");
    conf.db.port = env!("port").parse::<u16>().unwrap();
    conf.db.db = env!("db");
    conf.cleanup.auto_delete_request = true;
    conf
}

/// Parse input toml to config struct
fn parse_config(input: String) -> Result<Config, ConfigError> {
    match from_str(&input) {
        Err(e) => {
            error!("{}", e);
            Err(ConfigError::ParseError)
        }
        Ok(dconfig) => Ok(dconfig),
    }
}

/// Read config from file.
pub fn read_config(file: &Path) -> Result<String, ConfigError> {
    let mut f = OpenOptions::new()
        .read(true)
        .open(file)
        .map_err(|_| ConfigError::ReadError)?;
    let mut data = String::new();
    f.read_to_string(&mut data)
        .map_err(|_| ConfigError::ReadError)?;
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
# this is required for auto_delete_files!
link_files = true

# temporary dir for downloads before the conversion etc
temp_dir = "~/downloads/temp"

# final destination of downloaded files / playlists
download_dir = "~/downloads"

# download speed limit
download_mbps = 48 # MBit/s limit
# mp3 quality to use for conversion, see https://trac.ffmpeg.org/wiki/Encode/MP3
mp3_quality = 2

# folder in which the ffmpeg binaries are lying
ffmpeg_bin_dir = "~/ffmpeg/ffmpeg-2.6.2-64bit-static/"

# additional lib callable in case of country-locks
# will be called with {[optional arguments]} -q {quality} -r {speed limit} -f {dest. file} -v {video/audio -> true/false} {url}
# the lib's return after 'name: ' will be taken as the name of the video/file to use
lib_use = false
lib_bin = "/binary" # path to binary
lib_args = ["arg1", "arg2"] # additional arguments
lib_dir = "/" # working dir to use

# clean the temp dir on startup (deletes ALL files insides!)
# for crash cleanups at debugging
clean_temp_dir = false

# auto update youtube-dl
# if set to false you've to provide youtube-dl yourself and keep it up to date
# to guarantee keeping up with website changes
# (the command youtube-dl has to be availble from the command line)
youtube_dl_auto_update = true

# folder of youtube-dl (yt-dl.org)
# if youtube_dl_dir is true, this will also be the update directory
# so make sure yayd has write permission on it
youtube_dl_dir = "/path/to/ytdl/"

[cleanup]
# auto delete files older then X minutes
auto_delete_files = true
auto_delete_age = 4320
# delete execution interval: minutes
auto_delete_interval = 1440
# set to true to also delete the DB entries along with those files
# requires link_files
auto_delete_request = false

# delete marked files
# deletes files marked with the "delete" flag
# by this all delete IO is handled by yayd and not the webserver
# this can also be used to give the web server only read access
delete_files = true
# delete interval (re-check for entries) in minutes
delete_interval = 900
# delte the request db entry along with the file
# requires link_files
delete_request = false

[codecs]
# general audio only quality ids, if supported by the handler
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

#[codecs.twitch]
# supported twitch options
#supported = ["Source","High","Medium","Low","Mobile"]
    "#;
    trace!("Raw new config: {:?}", toml);

    toml.to_owned()
}

/// Writes the recived string into the file
fn write_config_file(path: &Path, data: &str) -> Result<(), ConfigError> {
    let mut file = File::create(path).map_err(|_| ConfigError::CreateError)?;
    file.write_all(data.as_bytes())
        .map_err(|_| ConfigError::WriteError)?;
    Ok(())
}
