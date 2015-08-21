extern crate mysql;
extern crate toml;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;

mod lib;

use lib::config;
use lib::downloader::DownloadDB;
use lib::downloader::Downloader;
use lib::DownloadError;
use lib::converter::Converter;

use std::fs::{remove_file};

static VERSION : &'static str = "0.1"; // String not valid
static SLEEP_MS: u32 = 5000;
static CODE_FAILED: i8 = 3;
static CODE_SUCCESS: i8 = 2;

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

fn main() {
    
    let pool = lib::db_connect(lib::mysql_options(), SLEEP_MS);
    
    let converter = Converter::new(&CONFIG.general.ffmpeg_bin,&CONFIG.general.mp3_quality , pool.clone());
    let mut print_pause = true;
    loop {
        if let Some(result) = lib::request_entry(& pool) {
            print_pause = true;
            let qid = result.qid.clone();                 //&QueryCodes::InProgress as i32
            lib::set_query_code(&mut pool.get_conn().unwrap(), &1, &result.qid).ok().expect("Failed to set query code!");
            lib::set_query_state(&pool.clone(),&qid, "started", false);
            
            let succes;
            {
                let mut left_files: Vec<String> = Vec::with_capacity(2);
                if result.playlist {
                    succes = match handle_playlist(result, &converter,&mut left_files) {
                        Ok(v) => v,
                        Err(e) => {println!("Playlist Error: {:?}", e); false }
                    };
                }else{
                    succes = match handle_download(result, None, &converter,&mut left_files) {
                        Ok(v) => v,
                        Err(e) => {println!("Download Error: {:?}", e); false }
                    };
                }
                
            
                if !left_files.is_empty() {
                    println!("cleaning up files");
                    for i in &left_files {
                        match remove_file(&i) {
                            Ok(_) => (println!("cleaning up {}",i)),
                            Err(e) => println!("unable to remove file '{}' {}",i,e),
                        }
                    }
                }
            }
            
            let code: i8 = if succes {
                CODE_SUCCESS//QueryCode
            } else {
                CODE_FAILED//QueryCode
            };
            lib::set_query_code(&mut pool.get_conn().unwrap(), &code,&qid).ok().expect("Failed to set query code!");
            
            let state = if code == 2 {
                "finished"
            } else {
                "failed"
            };
            lib::set_query_state(&pool.clone(),&qid, state, true);
            
        } else {
            if print_pause { println!("Pausing.."); print_pause = false; }
            std::thread::sleep_ms(SLEEP_MS);
        }
    }
}

///Download handler
///Used by the playlist/file handler to download one file
///Based on the quality it needs to download audio & video separately and convert them together
///In case of a playlist download it depends on the target download folder & if it should be bezipped
///In case of a DMCA we need to download the file via the socket connector,
///which will output a mp3, or if requested, the video but with the highest quality.
///Thus in case of a DMCA we can't pick a quality anymore.
///Also the filename depends on the socket output then.
///
///If it's a non-zipped single file, it's moved after a successful download, converted etc to the
///main folder from which it should be downloadable.
///The original non-ascii & url_encode'd name of the file is stored in the DB
fn handle_download<'a>(downl_db: DownloadDB, folder: Option<String>, converter: &Converter, file_db: &mut Vec<String>) -> Result<bool,DownloadError>{
    //update progress
    let is_zipped = match folder {
        Some(_) => true,
        None => false,
    };
    lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, 0,false);
    
    let download = Downloader::new(&downl_db, &CONFIG.general);
    //get filename, check for DMCA
    let mut dmca = false; // "succ." dmca -> file already downloaded
    
    let name = match download.get_file_name() { // get filename
        Ok(v) => v,
        Err(DownloadError::DMCAError) => { //now request via lib.. // k if( k == Err(DownloadError::DMCAError) ) 
            println!("DMCA error!");
            match download.lib_request_video() {
                Err(err) => { println!("Offliberty-call error {:?}", err); return Err(err); },
                Ok(v) => { dmca = true; v },
            }
        },
        Err(e) => { // unknown error / restricted source etc.. abort
            println!("Unknown error: {:?}", e);
            return Err(e);
        },
    };

    let name_http_valid = lib::format_file_name(&name, &download, &downl_db.qid);

    let file_path = lib::format_file_path(&downl_db.qid, folder.clone(), false);
    file_db.push(file_path.clone());
    let save_path = &lib::format_save_path(folder.clone(),&name, &download, &downl_db.qid);

    println!("Filename: {}", name);
    
    let is_splitted_video = if dmca {
        false
    } else {
        lib::is_split_container(&downl_db.quality)
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
        
        lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, total_steps,false);

        //download first file, download audio raw source if specified or video
        try!(download.download_file(&file_path, convert_audio));

        if is_splitted_video {
            // download audio file & convert together
            lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 3, total_steps,false);

            let audio_path = lib::format_file_path(&downl_db.qid, folder.clone(), true);
            file_db.push(audio_path.clone());
            
            println!("Downloading audio.. {}", audio_path);
            try!(download.download_file(&audio_path, true));

            lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 4, total_steps,false);

            match converter.merge_files(&downl_db.qid,&file_path, &audio_path,&save_path) {
                Err(e) => {println!("merge error: {:?}",e); return Err(e);},
                Ok(()) => {},
            }

            try!(remove_file(&audio_path));
            file_db.pop();

        }else{ // we're already done, only need to copy if it's not raw audio source
            if !download.is_audio() {
                try!(lib::move_file(&file_path, &save_path));
            }
        }
    }
    
    if download.is_audio(){ // if audio-> convert m4a to mp3, which converts directly to downl. dir
        lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, total_steps, total_steps, false);
        try!(converter.extract_audio(&file_path, &save_path,convert_audio));
        try!(remove_file(&file_path));
    }else{
        if is_splitted_video {
            try!(remove_file(&file_path));
        }else{
            try!(lib::move_file(&file_path, &save_path));
        }
    }
    file_db.pop();
    
    if !is_zipped { // add file to list, except it's for zip-compression later (=folder set)
        lib::add_file_entry(&downl_db.pool.clone(), &downl_db.qid,&name_http_valid, &name);
    }
    lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, total_steps, total_steps, true);
    Ok(true)
}

///Handles a playlist request
///If zipping isn't requested the downloads will be split up,
///so for each video in the playlist an own query entry will be created
fn handle_playlist(downl_db: DownloadDB, converter: &Converter, file_db: &mut Vec<String>) -> Result<bool, DownloadError>{
    lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, if downl_db.compress { 4 }else{ 3 },false);
    let download = Downloader::new(&downl_db, &CONFIG.general);
    let playlist_name = try!(download.get_playlist_name());
    
    
    Ok(true)
}
