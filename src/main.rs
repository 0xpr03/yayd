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

use std::fs::{remove_file,remove_dir};

static VERSION : &'static str = "0.2"; // String not valid
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
                        Ok(v) => Ok(v),
                        Err(e) => {println!("Playlist Error: {:?}", e); Err(e) }
                    };
                }else{
                    succes = match handle_download(result, &None, &converter,&mut left_files) {
                        Ok(v) => Ok(v),
                        Err(e) => {println!("Download Error: {:?}", e); Err(e) }
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
            
            let code: i8 = if succes.is_ok() {
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
fn handle_download<'a>(downl_db: DownloadDB, folder: &Option<String>, converter: &Converter, file_db: &mut Vec<String>) -> Result<Option<String>,DownloadError>{
    //update progress
    let is_zipped = match *folder {
        Some(_) => true,
        None => false,
    };
    if !downl_db.compress {
        lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, 0,false);
    }
    
    let download = Downloader::new(&downl_db, &CONFIG.general);
    //get filename, check for DMCA
    let mut dmca = false; // "succ." dmca -> file already downloaded
    
    let name = match download.get_file_name() { // get filename
        Ok(v) => v,
        Err(DownloadError::DMCAError) => { //now request via lib.. // k if( k == Err(DownloadError::DMCAError) ) 
            println!("DMCA error!");
            match download.lib_request_video(1,0) {
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
    let save_path = lib::format_save_path(folder.clone(),&name, &download, &downl_db.qid);

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
        if !downl_db.compress {
            lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, total_steps,false);
        }

        //download first file, download audio raw source if specified or video
        try!(download.download_file(&file_path, convert_audio));

        if is_splitted_video {
            // download audio file & convert together
			if !downl_db.compress {
                lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 3, total_steps,false);
			}

            let audio_path = lib::format_file_path(&downl_db.qid, folder.clone(), true);
            file_db.push(audio_path.clone());
            
            println!("Downloading audio.. {}", audio_path);
            try!(download.download_file(&audio_path, true));
            
            if !downl_db.compress {
                lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 4, total_steps,false);
            }

            match converter.merge_files(&downl_db.qid,&file_path, &audio_path,&save_path, !downl_db.compress) {
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
        if !downl_db.compress {
            lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, total_steps, total_steps, false);
        }
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
    if !downl_db.compress {
        lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, total_steps, total_steps, true);
    }
    
    if folder.is_some() {
        Ok(Some(save_path))
    } else {
        Ok(None)
    }
}

///Handles a playlist request
///If zipping isn't requested the downloads will be split up,
///so for each video in the playlist an own query entry will be created
fn handle_playlist(mut downl_db: DownloadDB, converter: &Converter, file_db: &mut Vec<String>) -> Result<Option<String>, DownloadError>{
    let mut max_steps: i32 = if downl_db.compress { 4 } else { 3 };
    lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, max_steps,false);
    
    let pl_id: i64 = downl_db.qid;
    downl_db.update_folder(format!("{}/{}",&CONFIG.general.save_dir,pl_id));
    println!("Folder:  {}",downl_db.folder);
    
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
    lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, max_steps,false);
    println!("retriving playlist videos..");
    let file_ids: Vec<String> = try!(download.get_playlist_ids());
    
    let handler_folder = if downl_db.compress {
        Some(pl_id.to_string())
    }else {
        None
    };
    
    println!("got em");
    max_steps += file_ids.len() as i32;
    let mut current_step = 2;
    let mut current_url: String;
    // we'll store all files and delete em later, so we don't need rm -rf
    let mut file_delete_list: Vec<String> = Vec::with_capacity(if downl_db.compress { file_ids.len() } else { 0 });
    for id in file_ids.iter() {
        current_step += 1;
        if downl_db.compress {
            lib::update_steps(&downl_db.pool.clone(),&pl_id, current_step, max_steps,false);
        }
        downl_db.update_video(format!("https://wwww.youtube.com/watch?v={}",id), current_step as i64);
        println!("id: {}",id);
        let db_copy = downl_db.clone();
        match handle_download(db_copy, &handler_folder, converter, file_db) {
            Err(e) => println!("error downloading {}: {:?}",id,e),
            Ok(e) => {  if downl_db.compress {
                            file_delete_list.push(e.unwrap());
                        }
                },
        }
    }
    
    println!("downloaded all videos");
    if downl_db.compress { // zip to file, add to db & remove all sources
        current_step += 1;
        lib::update_steps(&downl_db.pool.clone(),&pl_id, current_step, max_steps,false);
        let zip_name = format!("{}.zip",playlist_name);
        let zip_file = format!("{}/{}",&CONFIG.general.download_dir,zip_name);
        println!("zip file: {} \n zip source {}",zip_file, &downl_db.folder);
        try!(lib::zip_folder(&downl_db.folder, &zip_file));
        lib::add_file_entry(&downl_db.pool.clone(), &pl_id,&zip_name, &playlist_name);
        current_step += 1;
        lib::update_steps(&downl_db.pool.clone(),&pl_id, current_step, max_steps,false);
        try!(lib::delete_files(file_delete_list));
        try!(remove_dir(downl_db.folder));
        file_db.pop();
    }
    
    Ok(None)
}
