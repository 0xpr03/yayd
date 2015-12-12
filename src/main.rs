extern crate mysql;
extern crate toml;
extern crate rustc_serialize;
#[macro_use]
extern crate log;
extern crate log4rs;
#[macro_use]
extern crate lazy_static;

mod lib;

use lib::config;
use lib::db;
use lib::logger;
use lib::downloader::DownloadDB;
use lib::downloader::Downloader;
use lib::DownloadError;
use lib::converter::Converter;

use std::fs::{remove_file,remove_dir_all};

const VERSION : &'static str = "0.4";
const CONFIG_PATH : &'static str = "config.cfg";
const LOG_CONFIG: &'static str = "log.conf";
const LOG_PATTERN: &'static str = "%d{%d-%m-%Y %H:%M:%S}\t[%l]\t%f:%L \t%m";
const SLEEP_MS: u32 = 5000;
const CODE_STARTED: i8 = 0;
const CODE_IN_PROGRESS: i8 = 1;
const CODE_SUCCESS: i8 = 2;
const CODE_SUCCESS_WARNINGS: i8 = 3; // finished with warnings
const CODE_FAILED_INTERNAL: i8 = 10; // internal error
const CODE_FAILED_QUALITY: i8 = 11; // qualitz not available
const CODE_FAILED_UNAVAILABLE: i8 = 12; // source unavailable (private / removed)
const TYPE_YT_VIDEO: i16 = 0;
const TYPE_YT_PL: i16 = 1;
const TYPE_TWITCH: i16 = 2;

lazy_static! {
    pub static ref CONFIG: config::Config = {
        println!("Starting yayd-backend v{}",&VERSION);
        config::init_config() //return
    };

}

//#[repr(i8)] broken, enum not usable as of #10292
// enum QueryCodes {
//     Waiting = 0,
//     InProgress = 1,
//     Finished = 2,
//     Failed = 3,
// }

enum Thing { String(String), Bool(bool), None }

fn main() {
    logger::initialize();
    
    let pool = db::db_connect(db::mysql_options(), SLEEP_MS);
    debug!("cleaning db");
    db::clear_query_states(&pool);
    
    let converter = Converter::new(&CONFIG.general.ffmpeg_bin_dir,&CONFIG.general.mp3_quality , pool.clone());
    let mut print_pause = true;
    debug!("finished startup");
    loop {
        if let Some(mut downl_db) = db::request_entry(& pool) {
            print_pause = true;
            let qid = downl_db.qid.clone();
            db::set_query_code(&pool, &CODE_IN_PROGRESS, &downl_db.qid).ok().expect("Failed to set query code!");
            db::set_query_state(&pool.clone(),&qid, "started", false);
            let action_result: Result<Thing,DownloadError>;
            {
                let mut left_files: Vec<String> = Vec::with_capacity(2);
                if downl_db.playlist {
                    action_result = handle_playlist(& mut downl_db, &converter,&mut left_files);
                }else{
                    action_result = handle_download(& downl_db, &None, &converter,&mut left_files);
                }

                if !left_files.is_empty() {
                    trace!("cleaning up files");
                    for i in &left_files {
                        match remove_file(&i) {
                            Ok(_) => (trace!("cleaning up {}",i)),
                            Err(e) => warn!("unable to remove file '{}' {}",i,e),
                        }
                    }
                }
            }
            
            let code: i8 = match action_result {
                Ok(t) => {
                    match t {
                        Thing::Bool(b) => {
                            if b { CODE_SUCCESS_WARNINGS } else { CODE_SUCCESS }
                        }
                        _ => CODE_SUCCESS,
                    }
                },
                Err(e) => {
                    warn!("Error: {:?}", e);
                    if downl_db.source_type == TYPE_TWITCH {
                        match lib::cleanup_temp_folder() {
                            Err(e) => error!("error doing cleanup {:?}", e),
                            Ok(_) => (),
                        }
                    }
                    match e {
                        DownloadError::NotAvailable => CODE_FAILED_UNAVAILABLE,
                        DownloadError::ExtractorError => CODE_FAILED_UNAVAILABLE,
                        DownloadError::QualityNotAvailable => CODE_FAILED_QUALITY,
                        _ => {
                            let details = match e {
                                DownloadError::DBError(s) => s,
                                DownloadError::DownloadError(s) => s,
                                DownloadError::FFMPEGError(s) => s,
                                DownloadError::InternalError(s) => s,
                                _ => unreachable!(),
                            };
                            db::add_query_status(&pool,&qid, &details);
                            CODE_FAILED_INTERNAL
                        },
                    }
                }
            };
            db::set_query_code(&pool, &code,&qid).ok().expect("Failed to set query code!");
			db::set_null_state(&pool, &qid);
            
        } else {
            if print_pause { debug!("Pausing.."); print_pause = false; }
            std::thread::sleep_ms(SLEEP_MS);
        }
    }
}

/// Download handler
/// Used by the playlist/file handler to download one file
/// Based on the quality it needs to download audio & video separately and convert them together
/// In case of a playlist download it depends on the target download folder & if it should be bezipped
/// In case of a DMCA & turned on lib_use, we're expecting the lib to handle this & return the filename to us.
///
/// If it's a non-zipped single file, it's moved after a successful download, converted etc to the
/// main folder from which it should be downloadable.
/// The original non-ascii & url_encode'd name of the file is stored in the DB
fn handle_download<'a>(downl_db: &DownloadDB, folder: &Option<String>, converter: &Converter, file_db: &mut Vec<String>) -> Result<Thing,DownloadError>{
    //update progress
    let is_zipped = match *folder {
        Some(_) => true,
        None => false,
    };
    if !downl_db.compress {
        db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, 0,false);
    }
    
    let download = Downloader::new(&downl_db, &CONFIG.general);
    //get filename, check for DMCA
    let mut dmca = false; // "succ." dmca -> file already downloaded
    
    let temp_path = lib::format_file_path(&downl_db.qid, folder.clone(), false);
    let name = match download.get_file_name() { // get filename
        Ok(v) => v,
        Err(DownloadError::DMCAError) => { //now request via lib.. // k if( k == Err(DownloadError::DMCAError) ) 
            info!("DMCA error!");
            if CONFIG.general.lib_use {
                match download.lib_request_video(1,0, &temp_path) {
                    Err(err) => { warn!("lib-call error {:?}", err); return Err(err); },
                    Ok(v) => { dmca = true; v },
                }
            } else {
                return Err(DownloadError::NotAvailable);
            }
        },
        Err(e) => { // unknown error / restricted source etc.. abort
            error!("Unknown error: {:?}", e);
            return Err(e);
        },
    };

    file_db.push(temp_path.clone());
    let save_path = lib::format_save_path(folder.clone(),&name, lib::get_file_ext(&download));

    trace!("Filename: {}", name);
    
    let is_splitted_video = if dmca {
        false
    } else {
        lib::is_split_container(&downl_db.quality, &downl_db.source_type)
    };
    let convert_audio = CONFIG.extensions.mp3.contains(&downl_db.quality);
    
    let total_steps = if dmca {
        if download.is_audio() {
            3
        } else {
            2
        }
    } else if is_splitted_video {
        4
    } else if download.is_audio() {
        3
    } else {
        2
    };
    
    if !dmca { // on dmca the called lib handles splitt videos, only audio conversion has to handled by us
        if !downl_db.compress {
            db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, total_steps,false);
        }
        
        //download first file, download audio raw source if specified or video
        match download.download_file(&temp_path, convert_audio) {
            Err(e) => {file_db.pop(); return Err(e); },
            Ok(_) => (),
        }
        
        if is_splitted_video {
            debug!("splitted video");
            // download audio file & convert together
            if !downl_db.compress {
                db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 3, total_steps,false);
            }
			
            let audio_path = lib::format_file_path(&downl_db.qid, folder.clone(), true);
            
            trace!("Downloading audio.. {}", audio_path);
            try!(download.download_file(&audio_path, true));
            file_db.push(audio_path.clone());
            
            if !downl_db.compress {
                db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 4, total_steps,false);
            }

            match converter.merge_files(&downl_db.qid,&temp_path, &audio_path,&save_path.to_string_lossy(), !downl_db.compress) {
                Err(e) => {println!("merge error: {:?}",e); return Err(e);},
                Ok(()) => {},
            }

            try!(remove_file(&audio_path));
            file_db.pop();
        }
    }
    
    if !is_splitted_video { // if it's no splitted video & no audio we're moving the file, regardless of an earlier DMCA (lib call)
        debug!("no split container");
        if !download.is_audio() {
            try!(lib::move_file(&temp_path, &save_path));
        }
    }
    
    if download.is_audio(){ // if audio-> convert m4a to mp3 or extract m4a, directly to output file
    	debug!("is audio file");
        if !downl_db.compress {
            db::update_steps(&downl_db.pool.clone(),&downl_db.qid, total_steps, total_steps, false);
        }
        try!(converter.extract_audio(&temp_path, &save_path.to_string_lossy(), convert_audio));
        try!(remove_file(&temp_path));
    }else{
        debug!("no audio");
        if is_splitted_video {
            try!(remove_file(&temp_path));
        }
    }
    file_db.pop();
    
    if !is_zipped { // add file to list, except it's for zip-compression later (=folder set)
        try!(db::add_file_entry(&downl_db.pool.clone(), &downl_db.qid,&save_path.file_name().unwrap().to_string_lossy(), &name));
    }
    if !downl_db.compress {
        db::update_steps(&downl_db.pool.clone(),&downl_db.qid, total_steps, total_steps, true);
    }
    
    if folder.is_some() {
        Ok(Thing::String(save_path.to_string_lossy().into_owned()))
    } else {
        Ok(Thing::None)
    }
}

/// Handles a playlist request
/// If zipping isn't requested the downloads will be split up,
/// so for each video in the playlist an own query entry will be created
/// if warnings occured (unavailable video etc) the return will be true
fn handle_playlist<'a>(downl_db: & mut DownloadDB<'a>, converter: &Converter, file_db: &mut Vec<String>) -> Result<Thing, DownloadError>{
    let mut max_steps: i32 = if downl_db.compress { 4 } else { 3 };
    db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, max_steps,false);
    
    let pl_id: i64 = downl_db.qid;
    downl_db.update_folder(format!("{}/{}",&CONFIG.general.temp_dir,pl_id));
    trace!("Folder:  {}",downl_db.folder);
    
    let mut playlist_name;
    let file_ids: Vec<String>;
    {
        let download = Downloader::new(&downl_db, &CONFIG.general);
        playlist_name = String::new();
        if downl_db.compress {
            playlist_name = try!(download.get_playlist_name());
            println!("pl name {}",playlist_name);
            try!(std::fs::create_dir(&downl_db.folder));
            file_db.push(downl_db.folder.clone());
        }
        
        db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, max_steps,false);
        trace!("retriving playlist videos..");
        file_ids = try!(download.get_playlist_ids());
    }
    
    
    
    let handler_folder = if downl_db.compress {
        Some(pl_id.to_string())
    }else {
        None
    };
    
    trace!("got playlist videos");
    max_steps += file_ids.len() as i32;
    let mut current_step = 2;
    let mut warnings = false;
    let mut current_url: String;
    let mut failed_log: String = String::from("Following urls couldn't be downloaded: \n");
    // we'll store all files and delete em later, so we don't need rm -rf
    let mut file_delete_list: Vec<String> = Vec::with_capacity(if downl_db.compress { file_ids.len() } else { 0 });
    for id in file_ids.iter() {
        current_step += 1;
        if downl_db.compress {
            db::update_steps(&downl_db.pool.clone(),&pl_id, current_step, max_steps,false);
        }
        current_url = format!("https://www.youtube.com/watch?v={}",id);
        downl_db.update_video(current_url.clone(), current_step as i64);
        debug!("id: {}",id);
        match handle_download(downl_db, &handler_folder, converter, file_db) {
            Err(e) => { warn!("error downloading {}: {:?}",id,e);
                        failed_log.push_str(&format!("{} {:?}\n", current_url, e));
                        warnings = true;
            },
            Ok(e) => {  if downl_db.compress {
                            match e {
                                Thing::String(v) => file_delete_list.push(v),
                                _ => {error!("handle_download not returning a filename"); panic!();},
                            }
                        }
            },
        }
    }
    
    if warnings {
        db::add_query_status(&downl_db.pool.clone(),&pl_id, &failed_log);
    }

    trace!("downloaded all videos");
    if downl_db.compress { // zip to file, add to db & remove all sources
        current_step += 1;
        db::update_steps(&downl_db.pool.clone(),&pl_id, current_step, max_steps,false);
        let zip_path = lib::format_save_path(None, &lib::url_sanitize(&playlist_name),"zip");
        trace!("zip file: {} \n zip source {}",zip_path.to_string_lossy(), &downl_db.folder);
        try!(lib::zip_folder(&downl_db.folder, &zip_path));
        try!(db::add_file_entry(&downl_db.pool.clone(), &pl_id,&zip_path.file_name().unwrap().to_string_lossy(), &playlist_name));
        
        current_step += 1;
        db::update_steps(&downl_db.pool.clone(),&pl_id, current_step, max_steps,false);
        try!(lib::delete_files(file_delete_list));
        try!(remove_dir_all(&downl_db.folder));
        file_db.pop();
    }
    
    Ok(Thing::Bool(warnings))
}
