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

static VERSION : &'static str = "0.3"; // String not valid
static CONFIG_PATH : &'static str = "config.cfg";
static LOG_CONFIG: &'static str = "log.conf";
static LOG_PATTERN: &'static str = "%d{%d-%m-%Y %H:%M:%S}\t[%l]\t%f:%L \t%m";
static SLEEP_MS: u32 = 5000;
static CODE_SUCCESS: i8 = 2;
static CODE_SUCCESS_WARNINGS: i8 = 3; // finished with warnings
static CODE_FAILED_INTERNAL: i8 = 10; // internal error
static CODE_FAILED_QUALITY: i8 = 11; // qualitz not available
static CODE_FAILED_UNAVAILABLE: i8 = 12; // source unavailable (private / removed)
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
    
    let converter = Converter::new(&CONFIG.general.ffmpeg_bin_dir,&CONFIG.general.mp3_quality , pool.clone());
    let mut print_pause = true;
    loop {
        if let Some(result) = db::request_entry(& pool) {
            print_pause = true;
            let qid = result.qid.clone();                 //&QueryCodes::InProgress as i32
            db::set_query_code(&mut pool.get_conn().unwrap(), &1, &result.qid).ok().expect("Failed to set query code!");
            db::set_query_state(&pool.clone(),&qid, "started", false);
            let action_result: Result<Thing,DownloadError>;
            {
                let mut left_files: Vec<String> = Vec::with_capacity(2);
                if result.playlist {
                    action_result = handle_playlist(result, &converter,&mut left_files);
                }else{
                    action_result = handle_download(result, &None, &converter,&mut left_files);
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
                            db::add_query_status(&pool.clone(),&qid, &details);
                            CODE_FAILED_INTERNAL
                        },
                    }
                }
            };
            db::set_query_code(&mut pool.get_conn().unwrap(), &code,&qid).ok().expect("Failed to set query code!");
			db::set_null_state(&pool.clone(), &qid);
            
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
/// In case of a DMCA we need to download the file via the socket connector,
/// which will output a mp3, or if requested, the video but with the highest quality.
/// Thus in case of a DMCA we can't pick a quality anymore.
/// Also the filename depends on the socket output then.
///
/// If it's a non-zipped single file, it's moved after a successful download, converted etc to the
/// main folder from which it should be downloadable.
/// The original non-ascii & url_encode'd name of the file is stored in the DB
fn handle_download<'a>(downl_db: DownloadDB, folder: &Option<String>, converter: &Converter, file_db: &mut Vec<String>) -> Result<Thing,DownloadError>{
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
    let save_path = lib::format_save_path(folder.clone(),&name, &download);

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
    
    if !dmca {
        if !downl_db.compress {
            db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, total_steps,false);
        }
        
        //download first file, download audio raw source if specified or video
        try!(download.download_file(&temp_path, convert_audio));
        
        if is_splitted_video {
            // download audio file & convert together
            if !downl_db.compress {
                db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 3, total_steps,false);
            }

            let audio_path = lib::format_file_path(&downl_db.qid, folder.clone(), true);
            file_db.push(audio_path.clone());
            
            trace!("Downloading audio.. {}", audio_path);
            try!(download.download_file(&audio_path, true));
            
            if !downl_db.compress {
                db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 4, total_steps,false);
            }

            match converter.merge_files(&downl_db.qid,&temp_path, &audio_path,&save_path.to_string_lossy(), !downl_db.compress) {
                Err(e) => {println!("merge error: {:?}",e); return Err(e);},
                Ok(()) => {},
            }

            try!(remove_file(&audio_path));
            file_db.pop();

        }else{ // we're already done, only need to copy if it's not raw audio source
            if !download.is_audio() {
                try!(lib::move_file(&temp_path, &save_path));
            }
        }
    }
    
    if download.is_audio(){ // if audio-> convert m4a to mp3, which converts directly to downl. dir
        if !downl_db.compress {
            db::update_steps(&downl_db.pool.clone(),&downl_db.qid, total_steps, total_steps, false);
        }
        try!(converter.extract_audio(&temp_path, &save_path.to_string_lossy(), convert_audio));
        try!(remove_file(&temp_path));
    }else{
        if is_splitted_video {
            try!(remove_file(&temp_path));
        }else{
            try!(lib::move_file(&temp_path, &save_path));
        }
    }
    file_db.pop();
    
    if !is_zipped { // add file to list, except it's for zip-compression later (=folder set)
        db::add_file_entry(&downl_db.pool.clone(), &downl_db.qid,&save_path.file_name().unwrap().to_string_lossy(), &name);
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
fn handle_playlist(mut downl_db: DownloadDB, converter: &Converter, file_db: &mut Vec<String>) -> Result<Thing, DownloadError>{
    let mut max_steps: i32 = if downl_db.compress { 4 } else { 3 };
    db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, max_steps,false);
    
    let pl_id: i64 = downl_db.qid;
    downl_db.update_folder(format!("{}/{}",&CONFIG.general.temp_dir,pl_id));
    trace!("Folder:  {}",downl_db.folder);
    
    let db_copy = downl_db.clone();
    let download = Downloader::new(&db_copy, &CONFIG.general);
    
    let mut playlist_name = String::new();
    if downl_db.compress {
        playlist_name = try!(download.get_playlist_name());
        playlist_name = lib::url_encode(&playlist_name);
        println!("pl name {}",playlist_name);
        try!(std::fs::create_dir(&downl_db.folder));
        file_db.push(downl_db.folder.clone());
    }
    db::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, max_steps,false);
    trace!("retriving playlist videos..");
    let file_ids: Vec<String> = try!(download.get_playlist_ids());
    
    let handler_folder = if downl_db.compress {
        Some(pl_id.to_string())
    }else {
        None
    };
    
    trace!("got em");
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
        let db_copy = downl_db.clone();
        match handle_download(db_copy, &handler_folder, converter, file_db) {
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
        let zip_name = format!("{}.zip",playlist_name);
        let zip_file = format!("{}/{}",&CONFIG.general.download_dir,zip_name);
        trace!("zip file: {} \n zip source {}",zip_file, &downl_db.folder);
        try!(lib::zip_folder(&downl_db.folder, &zip_file));
        db::add_file_entry(&downl_db.pool.clone(), &pl_id,&zip_name, &playlist_name);
        
        current_step += 1;
        db::update_steps(&downl_db.pool.clone(),&pl_id, current_step, max_steps,false);
        try!(lib::delete_files(file_delete_list));
        try!(remove_dir_all(downl_db.folder));
        file_db.pop();
    }
    
    Ok(Thing::Bool(warnings))
}
