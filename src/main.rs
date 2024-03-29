extern crate mysql;

extern crate toml;
#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;
extern crate log4rs;

#[macro_use]
extern crate lazy_static;
extern crate chrono;
extern crate flate2;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
extern crate sha2;
extern crate timer;

mod handler;
mod lib;

use color_eyre::eyre::eyre;
use mysql::Pool;

use crate::handler::init_handlers;
use crate::handler::Registry;
use crate::lib::config;
use crate::lib::converter::Converter;
use crate::lib::db;
use crate::lib::downloader::Downloader;
use crate::lib::logger;
use crate::lib::Error;
use std::sync::Arc;
use timer::Timer;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const CONFIG_PATH: &'static str = "config.cfg";
const C_USER_AGENT: &'static str = "hyper/yayd (github.com/0xpr03/yayd)";
const LOG_CONFIG: &'static str = "logger.yaml";
const LOG_PATTERN: &'static str = "{d(%d-%m-%Y %H:%M:%S)}\t{l}\t{f}:{L} \t{m:>10}{n}";
const CODE_WAITING: i8 = -1;
const CODE_STARTED: i8 = 0;
const CODE_IN_PROGRESS: i8 = 1;
const CODE_SUCCESS: i8 = 2;
const CODE_SUCCESS_WARNINGS: i8 = 3; // finished with warnings
const CODE_FAILED_INTERNAL: i8 = 10; // internal error
const CODE_FAILED_QUALITY: i8 = 11; // qualitz not available
const CODE_FAILED_UNAVAILABLE: i8 = 12; // source unavailable (private / removed)
const CODE_FAILED_UNKNOWN: i8 = 13; // URL invalid, no handler

lazy_static! {
    pub static ref CONFIG: config::Config = {
        println!("Starting yayd-backend v{}", &VERSION);
        config::init_config()
    };
    pub static ref SLEEP_TIME: std::time::Duration = std::time::Duration::new(5, 0);
}

//#[allow(non_camel_case_types)]
//#[derive(Clone, Eq, PartialEq, Debug, Copy)]
//#[repr(i8)]// broken, enum not usable as of #10292
//enum StatusCodes {
//    QueryStarted = 0,
//    InProgress = 1,
//    Success = 2,
//    Finished_Warnings = 3,
//    FailedInternal = 10,
//    FailedQuality = 11,
//    FailedUnavailable = 12,
//}
fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    logger::initialize();
    let pool = Arc::new(db::db_connect(
        db::mysql_options(&CONFIG),
        Some(*SLEEP_TIME),
    ));
    debug!("cleaning db...");
    let mut conn = pool
        .get_conn()
        .map_err(|_| panic!("Couldn't retrieve connection!"))
        .unwrap();
    db::clear_query_states(&mut conn);

    let converter = Converter::new(&CONFIG.general.ffmpeg_bin_dir, &CONFIG.general.mp3_quality);

    if !converter.startup_test() {
        error!("Converter self test failed! Shutting down");
        return Err(eyre!("Converter self test failed! Shutting down"));
    }

    let downloader = Arc::new(Downloader::new(&CONFIG.general));

    if !downloader.startup_test() {
        error!("Downloader self test failed! Shutting down");
        return Err(eyre!("Downloader self test failed! Shutting down"));
    }

    let handler = init_handlers(downloader.clone(), converter);

    let timer = timer::Timer::new();
    debug!(
        "Auto cleanup old files: {}",
        CONFIG.cleanup.auto_delete_files
    );
    if CONFIG.cleanup.auto_delete_files {
        run_auto_cleanup_thread(pool.clone(), &timer);
    }

    debug!(
        "Cleanup marked entries with `delete` flag: {}",
        CONFIG.cleanup.delete_files
    );
    if CONFIG.cleanup.delete_files {
        run_cleanup_thread(pool.clone(), &timer);
    }

    debug!(
        "Auto-Update yt-dl: {}",
        CONFIG.general.youtube_dl_auto_update
    );
    if CONFIG.general.youtube_dl_auto_update {
        run_update_thread(downloader.clone(), &timer);
    }

    debug!("finished startup");
    main_loop(&*pool, handler);
    Ok(())
}

fn main_loop(pool: &Pool, mut handler: Registry) {
    let mut print_pause = true;

    loop {
        if let Some(mut request) = db::request_entry(pool) {
            trace!("got request");
            print_pause = true;
            let qid = request.qid.clone();
            db::set_query_code(&mut request.get_conn(), &request.qid, &CODE_STARTED);
            db::set_query_state(&mut request.get_conn(), &request.qid, "started");
            trace!("starting handler");
            let code: i8 = match handler.handle(&mut request) {
                Ok(_) => CODE_SUCCESS,
                Err(e) => {
                    trace!("Error: {:?}", e);
                    match e {
                        Error::NotAvailable => CODE_FAILED_UNAVAILABLE,
                        Error::ExtractorError => CODE_FAILED_UNAVAILABLE,
                        Error::QualityNotAvailable => CODE_FAILED_QUALITY,
                        Error::UnknownURL => CODE_FAILED_UNKNOWN,
                        _ => {
                            error!("Internal Error: {:?}", e);
                            let details = e.to_string();
                            db::add_query_error(&mut request.get_conn(), &qid, &details);
                            CODE_FAILED_INTERNAL
                        }
                    }
                }
            };
            trace!("handler finished");
            db::set_query_code(&mut request.get_conn(), &qid, &code);
            db::set_null_state(&mut request.get_conn(), &qid);
        } else {
            if print_pause {
                trace!("Worker idle..");
                print_pause = false;
            }
            std::thread::sleep(*SLEEP_TIME);
        }
    }
}

/// Auto cleanup task
fn run_auto_cleanup_thread<'a>(pool: Arc<Pool>, timer: &'a Timer) {
    let path = std::path::PathBuf::from(&CONFIG.general.download_dir);
    let a = timer.schedule_repeating(
        chrono::Duration::minutes(CONFIG.cleanup.delete_interval as i64),
        move || {
            trace!("performing auto cleanup");
            let local_pool = &*pool;
            let val = lib::delete_files(
                local_pool,
                db::DeleteRequestType::AgedMin(&CONFIG.cleanup.auto_delete_age),
                &path,
            );
            match val {
                Ok(_) => (),
                Err(e) => error!("Couldn't auto cleanup! {:?}", e),
            }
        },
    );
    a.ignore(); // ignore schedule guard a
}

/// Cleanup requested task
fn run_cleanup_thread<'a>(pool: Arc<Pool>, timer: &'a Timer) {
    let path = std::path::PathBuf::from(&CONFIG.general.download_dir);
    let a = timer.schedule_repeating(
        chrono::Duration::minutes(CONFIG.cleanup.delete_interval as i64),
        move || {
            trace!("performing deletion requests");
            let local_pool = &*pool;
            let val = lib::delete_files(local_pool, db::DeleteRequestType::Marked, &path);
            match val {
                Ok(_) => (),
                Err(e) => error!("Couldn't perform deletions! {:?}", e),
            }
        },
    );
    a.ignore(); // ignore schedule guard a
}

/// youtube-dl update task
fn run_update_thread<'a>(downloader: Arc<Downloader>, timer: &'a Timer) {
    let a = timer.schedule_repeating(chrono::Duration::hours(24), move || {
        match downloader.update_downloader() {
            Ok(_) => (),
            Err(e) => error!("Couldn't perform youtube-dl update! {:?}", e),
        }
    });
    a.ignore(); // ignore schedule guard a
}
