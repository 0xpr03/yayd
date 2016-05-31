extern crate regex;
extern crate zip;

pub mod config;
pub mod downloader;
pub mod converter;
pub mod db;
pub mod logger;
pub mod status;

use mysql::error::MyError;
use mysql::conn::pool::MyPool;
use std::env::current_exe;
use std::error::Error as OriginError;
use std::{self,io};
use std::fs::{rename, metadata,File,read_dir};
use std::path::PathBuf;
use std::path::Path;
use std::io::{copy};
use lib::downloader::Filename;

use std::ascii::AsciiExt;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

/// Struct holding all data concerning the request
pub struct Request<'a> {
    pub url: String,
    pub quality: i16,
    pub playlist: bool,
    pub compress: bool,
    /// query id, origin ID for un-zipped playlists
    pub qid: i64,
    /// ID set and used for playlist queries
    /// should be used by single-file handlers, as it can be set by the playlist handler
    pub internal_id: i64,
    pub from: i32,
    pub to: i32,
    /// Path for save folder
    pub path: PathBuf,
    /// Path for temp save folder, can be changed to, for example, sub dirs
    /// If it should differe from the default folder this folder will be deleted on failure with all it's content! 
    pub temp_path: PathBuf,
    pub pool: &'a MyPool,
}

impl<'a> Request<'a> {
    fn set_dir(&mut self,new_path: &'a Path) {
        self.path = new_path.to_path_buf();
    }
    fn set_int_id(&mut self, id: i64) {
        self.internal_id = id;
    }
}

/// Error trait
/// HandlerWarn is NOT for errors, it's value will be inserted into the warn DB
#[derive(Debug)]
pub enum Error{
    DownloadError(String),
    FFMPEGError(String),
    DMCAError,
    NotAvailable,
    QualityNotAvailable,
    ExtractorError,
    InputError(String),
    InternalError(String),
    DBError(MyError),
    HandlerWarn(String),
}

impl From<MyError> for Error {
    fn from(err: MyError) -> Error {
        Error::DBError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::InternalError(format!("descr:{} kind:{:?} cause:{:?} id:{:?}",err.description(), err.kind(), err.cause(), err.raw_os_error()))
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(err: zip::result::ZipError) -> Error {
        Error::InternalError(format!("{}: {}",err, err.description()))
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

/// Move file to location
pub fn move_file<P: AsRef<Path>, Q: AsRef<Path>>(original: P, destination: Q) -> Result<(),Error> {
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
            c if c.is_ascii() => c,
            _ => '_'
        }
    }).collect()
    // match for each char, then do collect: loop through the iterator, collect all elements
    // into container from iterator
}

/// Returns a unique path, if the file already exists, a '-X' number will be added to it.
pub fn format_save_path<'a>(path: &Path, fname: &Filename) -> Result<PathBuf,Error> {
    let clean_name = &url_sanitize(&fname.name);
    let mut path = path.to_path_buf();
    
    path.push(format!("{}.{}",clean_name,fname.extension));
    if metadata(path.as_path()).is_ok() { // 90% of the time we don't need this
        for i in 1..100 {
            if metadata(path.as_path()).is_ok() {
                debug!("Path exists: {}",path.to_string_lossy());
                path.pop(); // we can't use set_file_name, as some extensions will overwrite the name
                path.push(format!("{}-{}.{}",clean_name,i,fname.extension));
            }else{
                break;
            }
        }
    }
    debug!("Path: {}",path.to_string_lossy());
    Ok(path)
}

/// Zips all files inside folder into one file
pub fn zip_folder(folder: &Path, destination: &Path) -> Result<(), Error> {
    trace!("Starting zipping..");
    if try!(metadata(folder)).is_dir() {
        
        let output_file = try!(File::create(destination));
        let mut writer = zip::ZipWriter::new(output_file);
        
        for entry in try!(read_dir(folder)) {
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
        Err(Error::InternalError("zip source is not a folder!".to_string()))
    }
}

/// Returns the current executable folder
pub fn get_executable_folder() -> Result<std::path::PathBuf, io::Error> {
    let mut folder = try!(current_exe());
    folder.pop();
    Ok(folder)
}
