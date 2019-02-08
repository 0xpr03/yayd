mod soundcloud;
mod twitch;
mod youtube;

use lib::converter::Converter;
use lib::db;
use lib::downloader::Downloader;
use lib::Error;
use lib::Request;
use std::fs::remove_dir_all;
use std::fs::remove_file;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::vec::Vec;

use CONFIG;

/// Structure holding a list of produced and left files
/// `left_files` is a storage used for temporary files
/// on handler failure all files listed in the temporary variable will be deleted
/// thus no handler has to worry about possible
struct HandleData<'a> {
    /// temporary files, deleted on failure by handler manager
    files: Vec<FileEntry>,
    /// Processed files, which should be downloaded by the user at the end
    left_files: Vec<PathBuf>,
    pub downloader: &'a Downloader,
    pub converter: &'a Converter<'a>,
}

/// Structure for storing files to be inserted into the file db later
/// or used as file output for playlist calls
struct FileEntry {
    pub path: PathBuf,
    pub origin_name: String,
}

#[allow(non_snake_case)]
impl<'a> HandleData<'a> {
    pub fn new(converter: &'a Converter, downloader: &'a Downloader) -> HandleData<'a> {
        HandleData {
            files: Vec::new(),
            left_files: Vec::new(),
            converter: converter,
            downloader: downloader,
        }
    }

    /// Add a file to be inserted into the file db
    pub fn addFile(&mut self, file: &Path, origin_name: &str) {
        self.files.push(FileEntry {
            origin_name: origin_name.to_string(),
            path: file.to_path_buf(),
        });
    }

    /// Push a file to the left_files list//
    pub fn push(&mut self, file: &Path) {
        self.left_files.push(file.to_path_buf());
    }

    /// Pop a file from the left_files list
    pub fn pop(&mut self) {
        self.left_files.pop();
    }

    /// Retrive left files
    pub fn getLeftFiles(&mut self) -> &Vec<PathBuf> {
        &self.left_files
    }

    pub fn getFiles(&mut self) -> &Vec<FileEntry> {
        &self.files
    }
}

/// A Module consisting of it's checker, handler & information if files need to be deleted in a extended way on errors.
/// Every handler (XY.rs) can register multiple modules, see youtube.rs for example
pub struct Module {
    /// Checking module, returning true if it's able to handle the URL
    checker: Box<Fn(&Request) -> bool>,
    /// Handler, called when the checking module returns true
    handler: Box<Fn(&mut HandleData, &mut Request) -> Result<(), Error>>,
}

/// Registry holding all available modules
pub struct Registry<'a> {
    modules: Vec<Module>,
    downloader: Arc<Downloader>,
    converter: Converter<'a>,
}

impl<'a> Registry<'a> {
    pub fn new(downloader: Arc<Downloader>, converter: Converter<'a>) -> Registry<'a> {
        Registry {
            downloader: downloader,
            converter: converter,
            modules: Vec::new(),
        }
    }

    /// Register a module
    fn register(&mut self, module: Module) {
        self.modules.push(module);
    }

    /// Handle a request with it's appropriate handler, if existing
    /// Returns an error on failure
    pub fn handle(&mut self, data: &mut Request) -> Result<(), Error> {
        let mut handle_db = HandleData::new(&self.converter, &self.downloader);

        if let Some(module) = self.modules.iter().find(|module| (module.checker)(&data)) {
            let result = (module.handler)(&mut handle_db, data);

            if !handle_db.getLeftFiles().is_empty() {
                // cleanup if left files isn't empty
                trace!("cleaning up files");
                for i in handle_db.getLeftFiles() {
                    match remove_file(&i) {
                        Ok(_) => (trace!("cleaning up {:?}", i)),
                        Err(e) => warn!("unable to remove file '{:?}' {}", i, e),
                    }
                }
            }

            if data.temp_path != PathBuf::from(&CONFIG.general.temp_dir) {
                // delete temp path if different from default
                match remove_dir_all(&data.temp_path) {
                    Ok(_) => trace!("cleaning up {:?}", data.temp_path),
                    Err(e) => warn!("unable to remove dir {:?} {}", data.temp_path, e),
                }
            }

            if !handle_db.getFiles().is_empty() {
                // insert processed files into the db
                for file in handle_db.getFiles() {
                    db::add_file_entry(
                        &mut data.get_conn(),
                        &data.qid,
                        &file.path.file_name().unwrap().to_string_lossy(),
                        &file.origin_name,
                    )?;
                }
            }
            result
        } else {
            Err(Error::UnknownURL)
        }
    }
}

/// Init handlers
pub fn init_handlers<'a>(downloader: Arc<Downloader>, converter: Converter<'a>) -> Registry<'a> {
    let mut registry = Registry::new(downloader, converter);
    youtube::init(&mut registry);
    twitch::init(&mut registry);

    registry
}
