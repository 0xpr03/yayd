extern crate log;
extern crate log4rs;
use lib::get_executable_folder;

use {LOG_CONFIG,LOG_PATTERN};
use std;
use std::fs::metadata;
use std::default::Default;
use std::path::Path;

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
	//init_config();
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
/// Conisiting of log to conole & if possible to file.
#[cfg(not(test))]
fn init_config() {
    /*let console = Box::new(log4rs::appender::ConsoleAppender::builder()
        .pattern(log4rs::pattern::PatternLayout::new(LOG_PATTERN).unwrap())
        .build());
    
    let file_appender = log4rs::appender::FileAppender::builder("log/hc_log.log")
        .pattern(log4rs::pattern::PatternLayout::new(LOG_PATTERN).unwrap()).build();
    let file_success = file_appender.is_ok();
    
    let mut root = log4rs::config::Root::builder(log::LogLevelFilter::max())
        .appender("console".to_string());
    if file_success {
        root = root.appender("file".to_string());
    }else{
        error!("Could not initialize file based logging!");
    }
    
    let mut config = log4rs::config::Config::builder(root.build())
        .appender(log4rs::config::Appender::builder("console".to_string(), console).build());
    if file_success {
        let file = Box::new(file_appender.unwrap());
        config = config.appender(log4rs::config::Appender::builder("file".to_string(), file).build());
    }
        
    println!("{:?}",log4rs::init_config(config.build().unwrap()));
    warn!("No log config file found, please create file {}",LOG_CONFIG);
    warn!("According to https://github.com/sfackler/log4rs");
    info!("Using internal logging configuration on most verbose level.");*/
}

/// Test logger configuration, without file support, ignoring external config
#[cfg(test)]
pub fn init_config() {
    let console = Box::new(log4rs::appender::ConsoleAppender::builder()
        .pattern(log4rs::pattern::PatternLayout::new(LOG_PATTERN).unwrap())
        .build());
    
    let mut root = log4rs::config::Root::builder(log::LogLevelFilter::max())
        .appender("console".to_string());
    
    let mut config = log4rs::config::Config::builder(root.build())
        .appender(log4rs::config::Appender::builder("console".to_string(), console).build());
        
    info!("Test logger configuration");
}