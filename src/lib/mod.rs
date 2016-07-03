extern crate regex;
extern crate zip;

pub mod downloader;
pub mod converter;
pub mod config;
pub mod logger;
pub mod db;

use lib::downloader::Filename;


use mysql::conn::pool::PooledConn;
use mysql;

use std::fs::{rename, metadata,File,read_dir};
use std::error::Error as OriginError;
use std::path::{PathBuf,Path};
use std::env::current_exe;
use std::ascii::AsciiExt;
use std::cell::RefCell;
use std::cell::RefMut;
use std::io::{copy};
use std::{self,io};


macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

/// Struct holding all data concerning the request
pub struct Request {
    pub url: String,
    pub quality: i16,
    pub playlist: bool,
    pub compress: bool,
    /// query id
    pub qid: u64,
    /// Reserved int to specify other options in case quality & playlist codes aren't enough
    /// Can be used for additional conversion targets etc
    pub r_type: i16,
    pub from: i16,
    pub to: i16,
    /// Path for save folder
    pub path: PathBuf,
    /// Path for temp save folder, can be changed to, for example, sub dirs
    /// If it should differe from the default folder this folder will be deleted on failure with all it's content! 
    pub temp_path: PathBuf,
    pub conn: RefCell<PooledConn>,
    /// User ID, needed for non-zipped playlist downloads, creating new query & job entries
    pub uid: u32
}

/// Core for assertions
#[cfg(test)]
pub struct ReqCore {
    url: String,
    quality: i16,
    playlist: bool,
    compress: bool,
    qid: u64,
    r_type: i16,
    from: i16,
    to: i16,
    path: PathBuf,
    temp_path: PathBuf,
    uid: u32
}

#[cfg(test)]
impl ReqCore {
    pub fn new(origin: &Request) -> ReqCore{
        ReqCore {
            url: origin.url.clone(),
            quality: origin.quality.clone(),
            playlist: origin.playlist.clone(),
            compress: origin.compress.clone(),
            r_type: origin.r_type.clone(),
            qid: origin.qid.clone(),
            from: origin.from.clone(),
            to: origin.to.clone(),
            path: origin.path.clone(),
            temp_path: origin.temp_path.clone(),
            uid: origin.uid.clone()
        }
    }
    
    pub fn verify(&self, input: &Request) {
        assert_eq!(self.url,input.url);
        assert_eq!(self.quality,input.quality);
        assert_eq!(self.compress,input.compress);
        assert_eq!(self.qid,input.qid);
        assert_eq!(self.r_type,input.r_type);
        assert_eq!(self.from,input.from);
        assert_eq!(self.to,input.to);
        assert_eq!(self.playlist,input.playlist);
        assert_eq!(self.path,input.path);
        assert_eq!(self.temp_path,input.temp_path);
        assert_eq!(self.uid,input.uid);
    }
}

impl<'a> Request {
    pub fn get_conn(&self) -> RefMut<PooledConn> {
        self.conn.borrow_mut()
    }
    fn set_dir(&mut self,new_path: &'a Path) {
        self.path = new_path.to_path_buf();
    }
}

/// Error trait
/// HandlerWarn is NOT for errors, it's value will be inserted into the warn DB
#[derive(Debug)]
pub enum Error{
    /// used by downloader lib
    DownloadError(String),
    /// used by converter lib
    FFMPEGError(String),
    /// Content down as of region lock, could be bypassed, see youtube handler
    DMCAError,
    /// Unavailable (login, region lock etc)
    NotAvailable,
    /// Quality not available => valid input, but unavailable
    QualityNotAvailable,
    /// Error thrown by youtube-dl for some DASH containers, see youtube handler
    ExtractorError,
    /// For wrong quality => invalid input, always unavailable
    InputError(String),
    /// Unexpected lib internal error
    InternalError(String),
    /// Unexpected error in handler
    HandlerError(String),
    /// Can't handle this URL, no valid handler found
    UnknownURL,
    /// used by db lib
    DBError(mysql::Error),
}


impl From<mysql::Error> for Error {
    fn from(err: mysql::Error) -> Error {
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
