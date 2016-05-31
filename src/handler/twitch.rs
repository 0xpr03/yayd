extern crate regex;

use super::{Registry, Module, HandleData};
use lib::{self, Error, Request, status};
use std::fs::remove_dir_all;
use std::fs::create_dir;
use std::fs::rename;

use CODE_IN_PROGRESS;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

lazy_static! {
// https://regex101.com/r/sI0lK2/1
// we need to remove the / escaping!
    pub static ref REGEX_VIDEO: regex::Regex = regex!(r"https?://(secure|www)\.twitch\.tv/[A-Za-z0-9]+/v/[0-9]+");
}

/// Init twitch handler, registering it
pub fn init(registry: &mut Registry) {
    registry.register(Module {
        checker: Box::new(checker_file),
        handler: Box::new(handle_file),
    });
}

/// Check if the data matches for the file handler
fn checker_file(data: &Request) -> bool {
    if data.playlist {
        return false;
    }
    REGEX_VIDEO.is_match(&data.url)
}

/// Handle file request
fn handle_file(hdb: &mut HandleData, request: &mut Request) -> Result<(), Error> {
    let state = status::Status::new(request.pool, request.playlist, &request.internal_id);
    state.set_status_code(&CODE_IN_PROGRESS);
    
    request.temp_path.push(request.qid.to_string()); // create sub dir for part files
    try!(create_dir(&request.temp_path));
    let mut temp_file_v = request.temp_path.clone(); // create file with qid in dir
    temp_file_v.push(request.internal_id.to_string());
    hdb.push(&temp_file_v);
	
    trace!("Retriving name");
    state.set_status("1/2", false);
    let quality = try!(get_quality(&request.quality));
    let name = try!(hdb.downloader.get_file_name(&request.url, None));
    debug!("name: {}.{}", &name.name, &name.extension);
	
    let save_file = try!(lib::format_save_path(&request.path, &name));
	
    state.set_status("2/2", false);
    trace!("downloading video");
    try!(hdb.downloader.download_file(&request, &temp_file_v, &quality));
	try!(rename(&temp_file_v,&save_file));
	
    hdb.addFile(&save_file, &name.full_name());
    try!(remove_dir_all(&request.temp_path));
    hdb.pop();
	
    Ok(())
}

/// Get audio quality to use
fn get_quality(qual_id: &i16) -> Result<String, Error> {
    let qual = match *qual_id {
        -14 => "Source",
        -13 => "High",
        -12 => "Medium",
        -11 => "Low",
        -10 => "Mobile",
        _ => return Err(Error::InputError("Unknown quality!".to_string())),
    };
    Ok(String::from(qual))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn regex() {
        assert!(REGEX_VIDEO.is_match(r"https://www.twitch.tv/fleddi/v/68777299"));
        assert!(REGEX_VIDEO.is_match(r"https://secure.twitch.tv/fleddi/v/68777299"));
        assert!(!REGEX_VIDEO.is_match(r"https://www.twitch.tv/fleddi/profile"));
    }

}
