extern crate regex;

use super::{HandleData, Module, Registry};
use crate::lib::db;
use crate::lib::{self, Error, Request, Result};
use std::fs::create_dir;
use std::fs::remove_dir_all;
use std::fs::rename;

use crate::CODE_IN_PROGRESS;

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
fn handle_file(hdb: &mut HandleData, request: &mut Request) -> Result<()> {
    db::set_query_code(&mut request.get_conn(), &request.qid, &CODE_IN_PROGRESS);

    request.temp_path.push(request.qid.to_string()); // create sub dir for part files
    create_dir(&request.temp_path)?;
    let mut temp_file_v = request.temp_path.clone(); // create file with qid in dir
    temp_file_v.push(request.qid.to_string());
    hdb.push(&temp_file_v);

    trace!("Retriving name");
    db::update_steps(&mut request.get_conn(), &request.qid, 1, 2);
    let quality = get_quality(&request.quality)?;
    let name = hdb.downloader.get_file_name(&request.url, None)?;
    debug!("name: {}.{}", &name.name, &name.extension);

    let save_file = lib::format_save_path(&request.path, &name)?;

    db::update_steps(&mut request.get_conn(), &request.qid, 2, 2);
    trace!("downloading video");
    hdb.downloader
        .download_file(&request, &temp_file_v, &quality)?;
    rename(&temp_file_v, &save_file)?;

    hdb.addFile(&save_file, &name.full_name());
    remove_dir_all(&request.temp_path)?;
    hdb.pop();

    Ok(())
}

/// Get audio quality to use
fn get_quality(qual_id: &i16) -> Result<String> {
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
