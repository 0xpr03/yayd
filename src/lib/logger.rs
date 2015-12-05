extern crate log;
extern crate log4rs;
use lib::get_executable_folder;

use {LOG_CONFIG,LOG_PATTERN};
use std;
use std::path::PathBuf;
use std::fs::metadata;
use std::default::Default;

pub fn initialize() {
    let mut log_path = get_executable_folder().unwrap_or(std::path::PathBuf::from("/"));
    log_path.set_file_name(LOG_CONFIG);
    if metadata(log_path.as_path()).is_ok() {
        init_file();
    }else{
        init_config();
    }
}

fn init_file() {
    match log4rs::init_file(LOG_CONFIG, Default::default()) {
        Ok(_) => (),
        Err(e) => panic!("Log initialisation failed! {:?}",e),
    }
}

fn init_config() {
    let root = log4rs::config::Root::builder(log::LogLevelFilter::max())
        .appender("console".to_string())
        .appender("file".to_string());
    let console = Box::new(log4rs::appender::ConsoleAppender::builder()
        .pattern(log4rs::pattern::PatternLayout::new(LOG_PATTERN).unwrap())
        .build());
    let file = Box::new(log4rs::appender::FileAppender::builder("log/hc_log.log")
        .pattern(log4rs::pattern::PatternLayout::new(LOG_PATTERN).unwrap())
        .build().unwrap()); // this needs to be catched, can faiL!
    let config = log4rs::config::Config::builder(root.build())
        .appender(log4rs::config::Appender::builder("console".to_string(), console).build())
        .appender(log4rs::config::Appender::builder("file".to_string(), file).build());
    println!("{:?}",log4rs::init_config(config.build().unwrap()));
    warn!("No log config file found, please create file {}",LOG_CONFIG);
    warn!("According to https://github.com/sfackler/log4rs");
    info!("Using internal logging configuration on most verbose level.");
}