extern crate zip;

pub mod config;
pub mod downloader;
pub mod converter;
pub mod db;
pub mod logger;

use lib::downloader::{Downloader};

use mysql::error::MyError;
use std::env::current_exe;
use std::error::Error;
use std::{self,io};
use std::fs::{remove_file,rename, metadata,File,read_dir};
use std::path::{Path, PathBuf};
use std::io::{copy};

use CONFIG;

use {TYPE_YT_PL,TYPE_YT_VIDEO};

use std::ascii::AsciiExt;

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
        DownloadError::DBError(format!("{:?}",err))
    }
}

impl From<io::Error> for DownloadError {
    fn from(err: io::Error) -> DownloadError {
        DownloadError::InternalError(err.description().into())
    }
}

impl From<zip::result::ZipError> for DownloadError {
    fn from(err: zip::result::ZipError) -> DownloadError {
        DownloadError::InternalError(format!("{}: {}",err, err.description()))
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
pub fn is_split_container(quality: &i16, source_type: &i16) -> bool {
    match *source_type {
        TYPE_YT_VIDEO | TYPE_YT_PL => {
            if CONFIG.extensions.mp3.contains(quality) {
            false
            } else if CONFIG.extensions.aac.contains(quality) {
                false
            } else if CONFIG.extensions.m4a.contains(quality) {
                false
            } else {
                true
            }
        },
        _ => {
            false
        }
    }
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
pub fn move_file<P: AsRef<Path>, Q: AsRef<Path>>(original: P, destination: Q) -> Result<(),DownloadError> {
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

///Removes file name invalid chars
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
        Some(v) => format!("{}/{}/{}{}", &CONFIG.general.temp_dir, v, qid,suffix),
        None => format!("{}/{}{}", &CONFIG.general.temp_dir, qid,suffix),
    }
}

///Return valid path for the file cache
///Checking for doubles, making the path unique
///If there should be files up till file_name-100.extension it will fail using the same name again!
pub fn format_save_path<'a>(folder: Option<String>, name: &str, download: &'a Downloader) -> PathBuf {
    let clean_name = &file_encode(&name);
    let extension = get_file_ext(download);
    let mut path = PathBuf::from(&CONFIG.general.download_dir);
	match folder {
	    Some(v) => path.push(v),
		None => {},
	}
	path.push(format!("{}.{}",clean_name,extension));
	if metadata(path.as_path()).is_ok() { // 90% of the time we don't need this
    	for i in 1..100 {
    	    if metadata(path.as_path()).is_ok() {
    	        debug!("Path exists: {}",path.to_string_lossy());
    	        path.pop(); // we can't use set_file_name, as some extensions will overwrite the name
    	        path.push(format!("{}-{}.{}",clean_name,i,extension));
    	    }else{
    	        break;
    	    }
    	}
	}
	debug!("Path: {}",path.to_string_lossy());
    path
}

/// Zips all files inside folder into one file
pub fn zip_folder(folder: &str, zip_path: PathBuf) -> Result<(), DownloadError> {
    trace!("Starting zipping..");
    let mut dir = PathBuf::from(&CONFIG.general.temp_dir);
    dir.push(folder);
    
    if try!(metadata(dir.as_path())).is_dir() {
        
        let output_file = try!(File::create(zip_path));
    	let mut writer = zip::ZipWriter::new(output_file);
        
        for entry in try!(read_dir(dir)) {
            let entry = try!(entry);
            if try!(entry.metadata()).is_file() {
                try!(writer.start_file(entry.file_name().to_string_lossy().into_owned(), zip::CompressionMethod::Deflated));
                let mut reader = try!(File::open(entry.path()));
                try!(copy(& mut reader,& mut writer));
            }
        }
        try!(writer.finish());
        trace!("finsiehd zipping");
        Ok(())
    }else{
        Err(DownloadError::InternalError("zip source is not a folder!".to_string()))
    }
}

///Delete all files in the list
pub fn delete_files(files: Vec<String>) -> Result<(), DownloadError>{
    for file in files.iter() {
        trace!("deleting {}",file);
        try!(remove_file(file));
    }
    Ok(())
}

pub fn get_executable_folder() -> Result<std::path::PathBuf, io::Error> {
    let mut folder = try!(current_exe());
    folder.pop();
    Ok(folder)
}
