extern crate log;
extern crate log4rs;
use lib::get_executable_folder;

use {LOG_CONFIG,LOG_PATTERN};
use std;
use std::fs::metadata;
use std::default::Default;
use std::path::Path;

use log::LogLevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::file::FileAppender;
use log4rs::encode::pattern::PatternEncoder;
use log4rs::config::{Appender, Config, Logger, Root};

const APPENDER_FILE: &'static str = "file";
const APPENDER_STDOUT: &'static str = "stdout";

/// Initializes the logger.
#[cfg(not(test))]
pub fn initialize() {
    let mut log_path = get_executable_folder().unwrap_or(std::path::PathBuf::from("/"));
    log_path.push(LOG_CONFIG);
    println!("Logging file: {:?}",log_path);
    match metadata(log_path.as_path()) {
        Ok(v) => { if v.is_file() { init_file(&log_path); return; } },
        Err(e) => println!("Error for log config: {:?}",e),
    }
    init_config(); // call fallback
}

/// Initialize log config from file
#[cfg(not(test))]
fn init_file(conf: &Path) {
    match log4rs::init_file(conf, Default::default()) {
        Ok(_) => (),
        Err(e) => panic!("Log initialization failed! {:?}",e),
    }
}

/// Initialize a fallback configurated logger.
/// Consisting of log to conole & if possible to file.
#[cfg(not(test))]
fn init_config() {
    let stdout_appender = ConsoleAppender::builder()
    .encoder(Box::new(PatternEncoder::new(LOG_PATTERN))).build();
    
    let file_appender = FileAppender::builder()
    .encoder(Box::new(PatternEncoder::new(LOG_PATTERN))).build("log/default.log");
    let file_success = file_appender.is_ok();
    
    let mut root_builder = Root::builder().appender(APPENDER_STDOUT);
    
    if file_success {
        root_builder = root_builder.appender(APPENDER_FILE);
    }
    
    let root = root_builder.build(LogLevelFilter::max());

    let mut config_builder = Config::builder()
    .appender(Appender::builder().build(APPENDER_STDOUT, Box::new(stdout_appender)));
    if file_success {
        config_builder = config_builder.appender(Appender::builder().build(APPENDER_FILE, Box::new(file_appender.unwrap())));
    }
    
    config_builder = config_builder.logger(Logger::builder().build("hyper", LogLevelFilter::Warn));
    
    let config = config_builder.build(root).unwrap();
    
    println!("Log fallback init: {}",log4rs::init_config(config).is_ok());
    
    if !file_success { // print after log init, useless otherwise
        error!("Could not initialize file based logging!");
    }
    
    warn!("No log config file found, please create file {}",LOG_CONFIG);
    warn!("According to https://github.com/sfackler/log4rs");
    info!("Using internal logging configuration on most verbose level.");
}

/// Test logger configuration, without file support, ignoring external configs
#[cfg(test)]
pub fn init_config() {
    let console = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(LOG_PATTERN))).build();
    
    let mut root = Root::builder().appender(APPENDER_STDOUT)
        .build(LogLevelFilter::max());
    
    let mut config = Config::builder()
        .appender(Appender::builder().build(APPENDER_STDOUT, Box::new(console))).build(root).unwrap();
        
    info!("Test logger configuration");
}
