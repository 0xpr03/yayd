pub mod config;
pub mod downloader;
pub mod converter;
pub mod db;

use lib::downloader::{Downloader};

use mysql::error::MyError;
use std::env::current_exe;
use std::process::{Command,Output};
use std::error::Error;
use std::{self,io,str};
use std::fs::remove_file;
use CONFIG;

use std::ascii::AsciiExt;

use std::fs::{rename};

#[derive(Debug)]
pub enum DownloadError{
    DownloadError(String),
    FFMPEGError(String),
    DMCAError,
    NotAvailable,
    QualityNotAvailable,
    ExtractorError,
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

///Custom expect function logging the error and a msg when panicing
///&'static str to prevent the usage of format!(), which would result in overhead
#[inline]
pub fn l_expect<T,E: std::fmt::Debug>(result: Result<T,E>, msg: &'static str) -> T {
	match result {
		Ok(v) => v,
		Err(e) => {error!("{}: {:?}",msg,e);
				panic!();
		}
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

///Removed file name invalid chars
pub fn file_encode(input: &str) -> String {
    input.chars().map(|char| {
        match char {
            '\\' | '/' | '\0' => '_',
            c => c,
        }
    }).collect()
}

///Format save location for file, zip dependent
///audio files get an 'a' as suffix
pub fn format_file_path(qid: &i64, folder: Option<String>, audio: bool) -> String {
    let suffix = if audio {
        "a"
    }else {
        ""
    };
    match folder {
        Some(v) => format!("{}/{}/{}{}", &CONFIG.general.save_dir, v, qid,suffix),
        None => format!("{}/{}{}", &CONFIG.general.save_dir, qid,suffix),
    }
}

///Format save path, dependent on zip option.
pub fn format_save_path<'a>(folder: Option<String>, name: &str, download: &'a Downloader, qid: &i64) -> String {
    let clean_name = &file_encode(&name);
    match folder {
        Some(v) => format!("{}/{}/{}", &CONFIG.general.save_dir, v, format_file_name(clean_name,download,qid)),
        None => format!("{}/{}", &CONFIG.general.download_dir, format_file_name(clean_name,download,qid)),
    }
}

///Zip folder to file
pub fn zip_folder(folder: &str, zip_name: &str) -> Result<(), DownloadError> {
    let io = try!(create_zip_cmd(folder,zip_name));
    println!("zip stdout: {}\nzip stderr: {}", str::from_utf8(&io.stdout).unwrap(),str::from_utf8(&io.stderr).unwrap());
    if str::from_utf8(&io.stderr).unwrap().contains("error") {
        return Err(DownloadError::DownloadError(format!("error: {:?}",&io.stdout)))
    }
    Ok(())
}

fn create_zip_cmd(folder: &str, zip_file: &str) -> Result<Output, DownloadError> {
    match Command::new("tar").arg("-zcf").arg(zip_file).arg("-C").arg(folder).arg(".").output() {
                     Err(e) => Err(DownloadError::InternalError(format!("failed to zip: {}", e))),
                     Ok(v) => Ok(v),
    }
}

///Delete all files in the list
pub fn delete_files(files: Vec<String>) -> Result<(), DownloadError>{
    for file in files.iter() {
        println!("deleting {}",file);
        try!(remove_file(file));
    }
    Ok(())
}

pub fn get_executable_folder() -> Result<std::path::PathBuf, io::Error> {
	let mut folder = try!(current_exe());
	folder.pop();
	Ok(folder)
}
