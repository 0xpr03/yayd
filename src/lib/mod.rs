extern crate regex;
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

const TWITCH_FILE_PART_REGEX: &'static str = r"\d+\.part(-Frag\d+(\.part|)|)"; // regex to match twitch part files

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

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

/// Custom expect function logging errors plus custom messages on panic
/// &'static str to prevent the usage of format!(), which would result in overhead
#[inline]
pub fn l_expect<T,E: std::fmt::Debug>(result: Result<T,E>, msg: &'static str) -> T {
    match result {
        Ok(v) => v,
        Err(e) => {error!("{}: {:?}",msg,e);
                panic!();
        }
    }
}

/// Return whether the quality is a split container or not
/// as specified in the docs
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

/// Returns the file extension to be used depending on the quality
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
        } else if CONFIG.extensions.webm.contains(&download.ddb.quality) {
            "webm"
        } else {
            "unknown"
        }
    }
}

/// Move file to location
pub fn move_file<P: AsRef<Path>, Q: AsRef<Path>>(original: P, destination: Q) -> Result<(),DownloadError> {
    match rename(original, destination) { // no try possible..
        Err(v) => Err(v.into()),
        Ok(_) => Ok(()),
    }
}

/// Returns a sanitized String, usable via url encode
pub fn url_sanitize(input: &str) -> String {
    // iterator over input, apply function to each element(function
    input.chars().map(|char| {
        match char {
            '\'' | '"' | '\\' => '_',
            c if c.is_ascii() => c,
            _ => '_'
        }
    }).collect()
    // match for each char, then do collect: loop through the iterator, collect all elements
    // into container from iterator
}

/// Format temp save location, zip dependent
/// audio files get an 'a' as suffix
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

/// Returns a unique path, if the file already exists, a '-X' number will be added to it.
pub fn format_save_path<'a>(folder: Option<String>, name: &str, extension: &'a str) -> PathBuf {
    let clean_name = &url_sanitize(&name);
    let mut path = if folder.is_some() {
            PathBuf::from(&CONFIG.general.temp_dir)
    } else {
        PathBuf::from(&CONFIG.general.download_dir)
    };
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
pub fn zip_folder(folder: &str, zip_path: &PathBuf) -> Result<(), DownloadError> {
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
                let _ = reader.sync_data();
                try!(copy(& mut reader,& mut writer));
            }
        }
        try!(writer.finish());
        trace!("finished zipping");
        Ok(())
    }else{
        Err(DownloadError::InternalError("zip source is not a folder!".to_string()))
    }
}

/// Cleans the temp folder from twitch part files
/// This is necessary on failed twitch downloads as the part files remain
pub fn cleanup_temp_folder() -> Result<(),DownloadError> {
    let re = regex!(TWITCH_FILE_PART_REGEX);
    
    for entry in l_expect(read_dir(&CONFIG.general.temp_dir), "reading temp dir") {
        let entry = try!(entry);
        if re.is_match(&entry.file_name().to_string_lossy().into_owned()) {
            match remove_file(&entry.path()) {
                Err(e) => warn!("couldn't delete file {}",e),
                Ok(_) => (),
            }
        }
    }
    
    Ok(())
}

/// Delete all files in the list
pub fn delete_files(files: Vec<String>) -> Result<(), DownloadError>{
    for file in files.iter() {
        trace!("deleting {}",file);
        try!(remove_file(file));
    }
    Ok(())
}

/// Returns the current executable folder
pub fn get_executable_folder() -> Result<std::path::PathBuf, io::Error> {
    let mut folder = try!(current_exe());
    folder.pop();
    Ok(folder)
}
