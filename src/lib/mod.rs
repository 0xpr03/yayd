extern crate regex;
extern crate zip;

pub mod config;
pub mod converter;
pub mod db;
pub mod downloader;
pub mod http;
pub mod logger;

use crate::lib::downloader::Filename;

use mysql;
use mysql::{Pool, PooledConn};
use thiserror::Error;

use std::cell::RefCell;
use std::cell::RefMut;
use std::env::current_exe;
use std::error::Error as OriginError;
use std::fs::remove_file;
use std::fs::{metadata, read_dir, rename, File};
use std::io::copy;
use std::path::{Path, PathBuf};
use std::{self, io};

use crate::CONFIG;

/// Struct holding all data concerning the request
pub struct Request {
    pub url: String,
    pub quality: i16,
    pub playlist: bool,
    /// split up a playlist request into multiple requests instead of one big file
    pub split: bool,
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
    pub uid: u32,
}

/// Core for assertions
#[cfg(test)]
#[derive(Clone, Debug)]
pub struct ReqCore {
    url: String,
    quality: i16,
    playlist: bool,
    split: bool,
    qid: u64,
    r_type: i16,
    from: i16,
    to: i16,
    path: PathBuf,
    temp_path: PathBuf,
    uid: u32,
}

#[cfg(test)]
impl ReqCore {
    pub fn verify(&self, input: &Request) {
        assert_eq!(self.url, input.url);
        assert_eq!(self.quality, input.quality);
        assert_eq!(self.split, input.split);
        assert_eq!(self.qid, input.qid);
        assert_eq!(self.r_type, input.r_type);
        assert_eq!(self.from, input.from);
        assert_eq!(self.to, input.to);
        assert_eq!(self.playlist, input.playlist);
        assert_eq!(self.path, input.path);
        assert_eq!(self.temp_path, input.temp_path);
        assert_eq!(self.uid, input.uid);
    }
}

impl<'a> Request {
    pub fn get_conn(&self) -> RefMut<PooledConn> {
        self.conn.borrow_mut()
    }
}

/// Error trait
/// HandlerWarn is NOT for errors, it's value will be inserted into the warn DB
#[derive(Debug, Error)]
pub enum Error {
    /// used by downloader lib
    #[error("Failed to download source: `{0}`")]
    DownloadError(String),
    /// used by converter lib
    #[error("Failed to convert: `{0}`")]
    FFMPEGError(String),
    /// Content down as of region lock, could be bypassed, see youtube handler
    #[error("DMCA error, failed to retrieve source")]
    DMCAError,
    /// Unavailable (login, region lock etc)
    #[error("Source not available, region lock or login required")]
    NotAvailable,
    /// Quality not available => valid input, but unavailable
    #[error("Quality not vailable for source")]
    QualityNotAvailable,
    /// Error thrown by youtube-dl for some DASH containers, see youtube handler
    #[error("Website extraction error, can't parse website")]
    ExtractorError,
    /// For wrong quality => invalid input, always unavailable
    #[error("Invalid quality selected: `{0}`")]
    InputError(String),
    /// Unexpected lib internal error
    #[error("Internal error: `{0}`")]
    InternalError(String),
    /// Can't handle this URL, no valid handler found
    #[error("Unknown URL, can't handle this website")]
    UnknownURL,
    #[error("Database error: `{0}`")]
    MysqlError(#[from] mysql::Error),
    #[error("Database error: `{0}`")]
    MysqlDeserializeError(#[from] mysql::FromRowError)
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::InternalError(format!(
            "descr:{} kind:{:?} cause:{:?} id:{:?}",
            err.to_string(),
            err.kind(),
            err.source(),
            err.raw_os_error()
        ))
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(err: zip::result::ZipError) -> Error {
        Error::InternalError(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Error {
        Error::InternalError(err.to_string())
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(err: std::sync::PoisonError<T>) -> Error {
        Error::InternalError(format!(
            "descr:{} cause:{:?}",
            err.to_string(),
            err.source()
        ))
    }
}

/// Check the SHA256 of a given file against the provided expected output
/// The expected value has to be in lowercase
#[allow(non_snake_case)]
pub fn check_SHA256<P: AsRef<Path>>(path: P, expected: &str) -> Result<bool> {
    use sha2::{Digest, Sha256};
    trace!("Checking SHA256..");

    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    let result = format!("{:X}", hasher.finalize());
    let result = result.to_lowercase();
    let is_matching = result == expected;
    if !is_matching {
        debug!("SHA Expected: {} Result: {}", expected, result);
    }
    Ok(is_matching)
}

/// Custom expect function logging errors plus custom messages on panic
/// &'static str to prevent the usage of format!(), which would result in overhead
#[inline]
pub fn l_expect<T, E: std::fmt::Debug>(result: std::result::Result<T, E>, msg: &'static str) -> T {
    match result {
        Ok(v) => v,
        Err(e) => {
            error!("{}: {:?}", msg, e);
            panic!();
        }
    }
}

/// Move file to location
pub fn move_file<P: AsRef<Path>, Q: AsRef<Path>>(original: P, destination: Q) -> Result<()> {
    match rename(original, destination) {
        // no try possible..
        Err(v) => Err(v.into()),
        Ok(_) => Ok(()),
    }
}

/// Returns a sanitized String, usable via url encode
pub fn url_sanitize(input: &str) -> String {
    // iterator over input, apply function to each element(function
    input
        .chars()
        .map(|char| match char {
            c if c.is_ascii() => c,
            _ => '_',
        })
        .collect()
    // match for each char, then do collect: loop through the iterator, collect all elements
    // into container from iterator
}

/// Returns a unique path, if the file already exists, a '-X' number will be added to it.
pub fn format_save_path<'a>(path: &Path, fname: &Filename) -> Result<PathBuf> {
    let clean_name = &url_sanitize(&fname.name);
    let mut path = path.to_path_buf();

    path.push(format!("{}.{}", clean_name, fname.extension));
    if metadata(path.as_path()).is_ok() {
        // 90% of the time we don't need this
        for i in 1..100 {
            if metadata(path.as_path()).is_ok() {
                debug!("Path exists: {}", path.to_string_lossy());
                path.pop(); // we can't use set_file_name, as some extensions will overwrite the name
                path.push(format!("{}-{}.{}", clean_name, i, fname.extension));
            } else {
                break;
            }
        }
    }
    debug!("Path: {}", path.to_string_lossy());
    Ok(path)
}

/// Zips all files inside folder into one file
pub fn zip_folder(folder: &Path, destination: &Path) -> Result<()> {
    trace!("Starting zipping..");
    if metadata(folder)?.is_dir() {
        let output_file = File::create(destination)?;
        let mut writer = zip::ZipWriter::new(output_file);

        for entry in read_dir(folder)? {
            let entry = entry?;
            if entry.metadata()?.is_file() {
                let mut f_options = zip::write::FileOptions::default();
                f_options = f_options.compression_method(zip::CompressionMethod::Deflated);
                writer.start_file(entry.file_name().to_string_lossy().into_owned(), f_options)?;
                let mut reader = File::open(entry.path())?;
                let _ = reader.sync_data();
                copy(&mut reader, &mut writer)?;
            }
        }
        writer.finish()?;
        trace!("finished zipping");
        Ok(())
    } else {
        Err(Error::InternalError(
            "zip source is not a folder!".to_string(),
        ))
    }
}

/// Returns the current executable folder
pub fn get_executable_folder() -> std::result::Result<std::path::PathBuf, io::Error> {
    let mut folder = current_exe()?;
    folder.pop();
    Ok(folder)
}

/// Delete files aged or marked for removal
/// Additionally erases the DB entries if configured to do so
/// dir_path markes the directory the files are located in
pub fn delete_files(
    pool: &Pool,
    delete_type: db::DeleteRequestType,
    dir_path: &Path,
) -> Result<()> {
    let mut conn = pool.get_conn()?;
    let (qids, mut files) = db::get_files_to_delete(&mut conn, delete_type)?;

    debug!("Len before: {}", files.len());
    files.retain(|&(_, ref url)| {
        // remove all not matching
        trace!("deleting {:?}", url);
        let mut path = dir_path.to_path_buf();
        path.push(url);
        match remove_file(&path) {
            Ok(_) => true,
            Err(e) => {
                if path.exists() {
                    error!("Couldn't delete file {:?} {:?}", path, e);
                    false
                } else {
                    warn!("File was already deleted: {:?}", path);
                    true
                }
            }
        }
    });
    debug!("Len after: {}", files.len());
    if CONFIG.cleanup.auto_delete_request {
        db::delete_requests(&mut conn, qids, files)?;
    } else {
        for (fid, _) in files {
            db::set_file_delete_flag(&mut conn, &fid, false)?;
            db::set_file_valid_flag(&mut conn, &fid, false)?;
        }
    }
    Ok(())
}
