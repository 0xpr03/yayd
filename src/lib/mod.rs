pub mod config;
pub mod downloader;
pub mod converter;

use mysql::conn::MyOpts;
use mysql::conn::pool;
use mysql::conn::pool::{MyPooledConn,MyPool};
use lib::downloader::{Downloader};

use mysql::error::MyError;
use std::error::Error;
use std::io;
use CONFIG;

use std::thread::sleep_ms;

use std::ascii::AsciiExt;

use std::fs::{rename};

#[derive(Debug)]
pub enum DownloadError{
    DownloadError(String),
    FFMPEGError(String),
    DMCAError,
    InternalError(String),
    DBError(String),
}

impl From<MyError> for DownloadError {
    fn from(err: MyError) -> DownloadError {
        DownloadError::DBError(err.description().into())
    }
}

impl From<io::Error> for DownloadError {
    fn from(err: io::Error) -> DownloadError {
        DownloadError::InternalError(err.description().into())
    }
}

pub fn db_connect(opts: MyOpts, sleep_time: u32) -> MyPool { 
    loop {
        match pool::MyPool::new(opts.clone()) {
            Ok(conn) => {return conn;},
            Err(err) => println!("Unable to establish a connection:\n{}",err),
        };
        sleep_ms(sleep_time);
    }
}

///Return whether the quality is a split container or not: video only
///as specified in the docs
pub fn is_split_container(quality: &i16) -> bool {
    if CONFIG.extensions.mp3.contains(quality) {
        false
    } else if CONFIG.extensions.aac.contains(quality) {
        false
    } else if CONFIG.extensions.m4a.contains(quality) {
        false
    } else {
        true
    }
}

///Set the state of the current query, code dependent, see QueryCodes
pub fn set_query_state(pool: & pool::MyPool,qid: &i64 , state: &str, finished: bool){ // same here
    let mut conn = pool.get_conn().unwrap();
    let progress: i32 = if finished {
        100
    }else{
        0
    };
    let mut stmt = conn.prepare("UPDATE querydetails SET status = ? , progress = ? WHERE qid = ?").unwrap();
    let result = stmt.execute(&[&state,&progress,qid]); // why is this var needed ?!
    match result {
        Ok(_) => (),
        Err(why) => println!("Error setting query state: {}",why),
    }
}

///Update status code for query entry
pub fn set_query_code(conn: & mut MyPooledConn, code: &i8, qid: &i64) -> Result<(), DownloadError> { // same here
    let mut stmt = conn.prepare("UPDATE querydetails SET code = ? WHERE qid = ?").unwrap();
    let result = stmt.execute(&[code,qid]);
    match result {
        Ok(_) => Ok(()),
        Err(why) => Err(DownloadError::DBError(why.description().into())),
    }
}

///Update progress steps for db entry
pub fn update_steps(pool: & pool::MyPool, qid: &i64, step: i32, max_steps: i32, finished: bool){
    set_query_state(&pool,qid, &format!("{}|{}", step, max_steps), finished);
}

///add file to db including it's name & fid based on qid
pub fn add_file_entry(pool: & pool::MyPool, fid: &i64, name: &str, real_name: &str){
    println!("name: {}",name);
    let mut conn = pool.get_conn().unwrap();
    let mut stmt = conn.prepare("INSERT INTO files (fid,name,rname,valid) VALUES (?,?,?,?)").unwrap();
    let result = stmt.execute(&[fid,&real_name,&name,&1]); // why is this var needed ?!
    match result {
        Ok(_) => (),
        Err(why) => println!("Error adding file: {}",why),
    }
}

///Format file name for 
pub fn format_file_name<'a>(name: &str, download: &'a Downloader, qid: &i64) -> String {
    println!("Fileextension: {:?}", get_file_ext(download));
    format!("{}-{}.{}",url_encode(name), qid, get_file_ext(download))
}

///Returns the file extension
pub fn get_file_ext<'a>(download: &Downloader) -> &'a str {
    if download.is_audio() {
        if CONFIG.extensions.aac.contains(&download.ddb.quality) {
            "aac"
        }else if CONFIG.extensions.mp3.contains(&download.ddb.quality) {
            "mp3"
        }else{
            "unknown"
        }
    }else{
        if CONFIG.extensions.mp4.contains(&download.ddb.quality) {
            "mp4"
        } else if CONFIG.extensions.flv.contains(&download.ddb.quality) {
            "flv"
        } else {
            "unknown"
        }
    }
}

///Move file to location
pub fn move_file(original: &str, destination: &str) -> Result<(),DownloadError> {
    match rename(original, destination) { // no try possible..
        Err(v) => Err(v.into()),
        Ok(_) => Ok(()),
    }
}

///Return an sanitized String (url encode still required)
pub fn url_encode(input: &str) -> String {
    // iterator over input, apply function to each element(function
    input.chars().map(|char| {
        match char {
            '\'' | '"' | '\\' => '_',
            '&' => '-',
            c if c.is_ascii() => c,
            _ => '_'
        }
    }).collect()
    // match for each char, then do collect: loop through the iterator, collect all elements
    // into container FromIterator
}

