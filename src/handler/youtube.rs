extern crate regex;

use super::{Registry, Module, HandleData};
use lib::{self, Error, Request, db};
use lib::downloader::Filename;
use std::fs::remove_file;
use std::fs::remove_dir_all;
use std::fs::create_dir;
use std::path::Path;

use CODE_IN_PROGRESS;
use CONFIG;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

macro_rules! condition(
    ($e:expr,$y:expr) => (match $y { true => {$e;}, false => ()} );
);

const YT_VIDEO_URL: &'static str = "https://www.youtube.com/watch?v=";

lazy_static! {
// https://regex101.com/r/lZ6lC1/3
// we need to remove the / escaping!
    pub static ref REGEX_VIDEO: regex::Regex = regex!(r"https?://(www\.|m\.)?(youtube\.[a-z]{2,3}/watch\?(feature=player_embedded&)?(list=[a-zA-Z0-9_-]+&)?v=[a-zA-Z0-9_-]+|youtu\.be/[a-zA-Z0-9_-]+)");
// https://regex101.com/r/aV1jS1/2
    pub static ref REGEX_PLAYLIST: regex::Regex = regex!(r"https?://(www\.|m\.)?youtube\.[a-z]{2,4}/(watch\?(feature=player_embedded&)?(v=[a-zA-Z0-9_-]+.*&)?list=[A-Za-z0-9_-]+|playlist\?list=[a-zA-Z0-9_-]+)");
}

/// Init youtube handler, registering it
pub fn init(registry: &mut Registry) {
    registry.register(Module {
        checker: Box::new(checker_file),
        handler: Box::new(handle_file),
    });
    registry.register(Module {
        checker: Box::new(checker_playlist),
        handler: Box::new(handle_playlist),
    });
}

/// Check if the data matches for the file handler
fn checker_file(data: &Request) -> bool {
    if data.playlist {
        return false;
    }
    REGEX_VIDEO.is_match(&data.url)
}

/// Check if the data matches for the playlist handler
fn checker_playlist(data: &Request) -> bool {
    if !data.playlist {
        return false;
    }
    REGEX_PLAYLIST.is_match(&data.url)
}

/// Playlist request handler for youtube
/// If compression is enabled all files will be downloaded into one dir and zipped afterwards
/// Otherwise for every entry in the playlist a new query is created. These will be handled one after another,
/// creating a file per entry.
fn handle_playlist(handle_db: &mut HandleData, request: &mut Request) -> Result<(), Error> {
    trace!("youtube playlist handler started");
    db::set_query_code(&mut request.get_conn(), &request.qid, &CODE_IN_PROGRESS);
    
    let name = Filename {
            name: handle_db.downloader.get_playlist_name(&request.url)?,
            extension: "zip".to_string(),
    };
    let mut step: i32 = 1;
    
    db::set_query_state(&mut request.get_conn(), &request.qid, "1/?");
    trace!("crawling ids");
    let file_ids = handle_db.downloader.get_playlist_ids(request)?;
    
    if request.split {
        trace!("creating new requests for playlist entries");
        let mut current_url: String;
        for id in file_ids.iter() {
            current_url = String::from(YT_VIDEO_URL);
            current_url.push_str(&id);
            db::add_sub_query(&current_url,&request)?;
        }
    } else {
        let save_path = lib::format_save_path(&request.path, &name)?;
        request.temp_path.push(request.qid.to_string());
        request.path = request.temp_path.clone();
        create_dir(&request.path)?;
        let mut warnings = false;
        let mut current_url: String;
        let mut failed_log: String = String::from("Following urls couldn't be downloaded: \n");
        
        let max_steps = file_ids.len() as i32 + 2;
        db::update_steps(&mut request.get_conn(), &request.qid, 2,max_steps);
        for id in file_ids.iter() {
            step += 1;
            db::update_steps(&mut request.get_conn(), &request.qid, step,max_steps);
            current_url = String::from(YT_VIDEO_URL);
            current_url.push_str(&id);
            request.url = current_url.clone();
            match handle_file_int(handle_db, &request) {
                Err(e) => {
                    warn!("error downloading {}: {:?}", id, e);
                    failed_log.push_str(&format!("{} {:?}\n", current_url, e));
                    warnings = true;
                }
                Ok(_) => {}
            }
        }

        if warnings {
            debug!("found warnings");
            db::add_query_error(&mut request.get_conn(), &request.qid, &failed_log);
        }
        
        step += 1;
        db::update_steps(&mut request.get_conn(), &request.qid, step,max_steps);
        trace!("starting zipping");
        lib::zip_folder(&request.temp_path, &save_path)?;
        trace!("adding file");
        handle_db.addFile(&save_path, &name.full_name());
        trace!("removing dir {}",request.path.to_string_lossy());
        remove_dir_all(&request.path)?;
        trace!("updating state");
    }


    Ok(())
}

/// Handle file request
fn handle_file(handle_db: &mut HandleData, request: &mut Request) -> Result<(), Error> {
    handle_file_int(handle_db, &request)?;

    Ok(())
}

/// Real handler, avoiding mut Request borrows for internal use
fn handle_file_int(handle_db: &mut HandleData, request: &Request) -> Result<(), Error> {
    trace!("youtube file handler started");
    if request.quality < 0 {
        handle_audio(handle_db,request)?
    } else {
        handle_video(handle_db,request)?
    }

    Ok(())
}

/// Handler for videos
fn handle_video(hdb: &mut HandleData, request: &Request) -> Result<(), Error> {
    condition!(db::set_query_code(&mut request.get_conn(), &request.qid, &CODE_IN_PROGRESS),!request.playlist);
    let mut temp_file_v = request.temp_path.clone();
    temp_file_v.push(request.qid.to_string());
    hdb.push(&temp_file_v);

    let mut dmca = false;

    trace!("Retriving name");
    let name = get_name(&hdb, &request, true, true, &temp_file_v, &mut dmca)?;
    let origin_name = name.full_name();
    debug!("name: {}.{}", &name.name, &name.extension);

    let save_file = lib::format_save_path(&request.path, &name)?;

    if dmca {
        lib::move_file(&temp_file_v, &save_file)?;
        hdb.addFile(save_file.as_ref(), &origin_name);
        hdb.pop();
        return Ok(());
    }

    let audio_id = if name.extension == "mp4" {
        CONFIG.codecs.yt.audio_normal_mp4
    } else {
        CONFIG.codecs.yt.audio_normal_webm
    };
    
    condition!(db::update_steps(&mut request.get_conn(), &request.qid, 1,3),!request.playlist);
    trace!("downloading video");
    hdb.downloader.download_file(&request, &temp_file_v, &request.quality.to_string())?;
    
    condition!(db::update_steps(&mut request.get_conn(), &request.qid, 2,3),!request.playlist);
    let mut temp_file_a = request.temp_path.clone();
    temp_file_a.push(format!("{}a", request.qid));
    hdb.push(&temp_file_a);
    trace!("downloading audio");
    hdb.downloader.download_file(&request, &temp_file_a, &audio_id.to_string())?;

    condition!(db::update_steps(&mut request.get_conn(), &request.qid, 3,3),!request.playlist);
    trace!("merging");
    hdb.converter.merge_files(&request.qid, &temp_file_v, &temp_file_a, &save_file,&mut request.get_conn())?;
    if !request.playlist {
        hdb.addFile(&save_file, &origin_name);
    }
    remove_file(&temp_file_a)?;
    hdb.pop();
    remove_file(&temp_file_v)?;
    hdb.pop();

    Ok(())
}

/// Handler for audios
fn handle_audio(hdb: &mut HandleData, request: &Request) -> Result<(), Error> {
    condition!(db::set_query_code(&mut request.get_conn(), &request.qid, &CODE_IN_PROGRESS),!request.playlist);
    let mut dmca = false;
    let quality = get_audio_quality(&request.quality)?;

    let mut temp_file_v = request.temp_path.clone();
    temp_file_v.push(request.qid.to_string());
    hdb.push(&temp_file_v);

    condition!(db::update_steps(&mut request.get_conn(), &request.qid, 1,3),!request.playlist);
    let mut name = get_name(&hdb, &request, false, false, &temp_file_v, &mut dmca)?;

    if dmca {
        let save_file = lib::format_save_path(&request.path, &name)?;
        lib::move_file(&temp_file_v, &save_file)?;
        if !request.playlist {
            hdb.pop();
            hdb.addFile(&save_file, &name.full_name());
        }
        return Ok(());
    }
    
    condition!(db::update_steps(&mut request.get_conn(), &request.qid, 2,3),!request.playlist);
    hdb.downloader.download_file(&request, &temp_file_v, &quality)?;

    if request.quality == CONFIG.codecs.audio_raw ||
       request.quality == CONFIG.codecs.audio_source_hq {
        name.extension = String::from("m4a");
    } else if request.quality == CONFIG.codecs.audio_mp3 {
        name.extension = String::from("mp3");
    }
    
    condition!(db::update_steps(&mut request.get_conn(), &request.qid, 3,3),!request.playlist);
    let file = lib::format_save_path(&request.path, &name)?;
    hdb.converter.extract_audio(&request.qid,
                                     &temp_file_v,
                                     &file,
                                     request.quality == CONFIG.codecs.audio_mp3,
                                     &mut request.get_conn())?;
    remove_file(&temp_file_v)?;
    hdb.pop();
    if !request.playlist {
        hdb.addFile(&file, &name.full_name());
    }

    Ok(())
}


/// Retrive name of youtube video
/// If we should encounter an DMCA we'll let the lib call handle this
/// (if enabled), this gives us a downloaded file and the name
/// In this case we only need to move the file to it's destination
/// If use_quality is true, the name will be retrived for the request's quality
fn get_name<'a>(hdb: &HandleData,
                request: &Request,
                use_qality: bool,
                video: bool,
                path: &Path,
                dmca: &'a mut bool)
                -> Result<Filename, Error> {
    let quality = if use_qality {
        Some(request.quality.to_string())
    } else {
        None
    };
    Ok(match hdb.downloader.get_file_name(&request.url, quality) { // get filename
        Ok(v) => {
            v
        }
        Err(Error::DMCAError) => {
            // now request via lib.. // k if( k == Err(DownloadError::DMCAError) )
            info!("DMCA error!");
            if CONFIG.general.lib_use {
                if !request.split {
                    db::set_query_code(&mut request.get_conn(), &request.qid, &CODE_IN_PROGRESS);
                }
                match hdb.downloader.lib_request_video(1,
                                                       0,
                                                       path,
                                                       request,
                                                       &request.quality.to_string(),
                                                       video) {
                    Err(err) => {
                        warn!("lib-call error {:?}", err);
                        return Err(err);
                    }
                    Ok(v) => {
                        *dmca = true;
                        v
                    }
                }
            } else {
                return Err(Error::NotAvailable);
            }
        }
        Err(e) => {
            // unknown error / restricted source etc.. abort
            error!("Unknown error: {:?}", e);
            return Err(e);
        }
    })
}

/// Get audio quality to use
fn get_audio_quality(id: &i16) -> Result<String, Error> {
    let id = if *id == CONFIG.codecs.audio_raw {
        CONFIG.codecs.yt.audio_normal_mp4
    } else if *id == CONFIG.codecs.audio_source_hq {
        CONFIG.codecs.yt.audio_normal_mp4
    } else if *id == CONFIG.codecs.audio_mp3 {
        CONFIG.codecs.yt.audio_normal_mp4
    } else {
        return Err(Error::InputError("Unknown audio quality!".to_string()));
    };
    Ok(id.to_string())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn regex() {
        assert!(REGEX_VIDEO.is_match(r"https://m.youtube.com/watch?list=PLTXoSHLJey0RR60hjLhuUAaj_ftAdShqv&v=IO-_EoRSpUA"));
        assert!(REGEX_VIDEO.is_match(r"http://m.youtube.com/watch?v=IO-_EoRSpUA"));
        assert!(REGEX_VIDEO.is_match(r"https://m.youtube.com/watch?v=IO-_EoRSpUA"));
        assert!(REGEX_VIDEO.is_match(r"http://youtu.be/IO-_EoRSpUA"));
        assert!(REGEX_VIDEO.is_match(r"https://www.youtube.com/watch?v=IO-_EoRSpUA"));
        assert!(REGEX_VIDEO.is_match(r"https://www.youtube.com/watch?v=IO-_EoRSpUA&list=PL6DA1502C5DDC0317&index=21"));
        assert!(!REGEX_VIDEO.is_match(r"https://www.youtube.com/playlist?list=PLJYiF4qyO-fpaYWfTylcFu3VGUCVK4xfz"));

        assert!(REGEX_PLAYLIST.is_match(r"https://m.youtube.com/playlist?list=PLTXoSHLJey0RR60hjLhuUAaj_ftAdShqv"));
        assert!(REGEX_PLAYLIST.is_match(r"https://m.youtube.com/watch?list=PLTXoSHLJey0RR60hjLhuUAaj_ftAdShqv&v=IO-_EoRSpUA"));
        assert!(REGEX_PLAYLIST.is_match(r"https://www.youtube.com/playlist?list=PLBCC15D0E3ED5E67A"));
        assert!(REGEX_PLAYLIST.is_match(r"https://www.youtube.com/watch?v=IO-_EoRSpUA&index=2&list=PLBCC15D0E3ED5E67A"));
        assert!(REGEX_PLAYLIST.is_match(r"https://www.youtube.com/watch?v=IO-_EoRSpUA&list=PL6DA1502C5DDC0317&index=21"));
        assert!(!REGEX_PLAYLIST.is_match(r"https://www.youtube.com/watch?v=IO-_EoRSpUA"));
    }

}
