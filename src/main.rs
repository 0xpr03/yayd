extern crate mysql;
extern crate toml;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;

mod lib;

use mysql::conn::MyOpts;
use std::default::Default;
use mysql::conn::pool;
use mysql::value::from_value;

use lib::config;
use lib::downloader::DownloadDB;
use lib::downloader::Downloader;
use lib::DownloadError;
use lib::converter::Converter;

use std::fs::{remove_file};

macro_rules! try_option { ($e:expr) => (match $e { Some(x) => x, None => return None }) }

///Move result value out, return with none on err & print
macro_rules! try_reoption { ($e:expr) => (match $e { Ok(x) => x, Err(e) => {println!("{}",e);return None }}) }

static VERSION : &'static str = "0.1"; // String not valid
static SLEEP_MS: u32 = 5000;
static CODE_FAILED: i8 = 3;
static CODE_SUCCESS: i8 = 2;

lazy_static! {
    static ref CONFIG: config::Config = {
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
    
    let pool = lib::db_connect(mysql_options(), SLEEP_MS);
    
    let converter = Converter::new(&CONFIG.general.ffmpeg_bin,&CONFIG.general.mp3_quality , pool.clone());
    let mut print_pause = true;
    loop {
        if let Some(result) = request_entry(& pool) {
            print_pause = true;
            if result.playlist {
                println!("Playlist not supported atm!");
                //TODO: set playlist entry to errg
            }
            let qid = result.qid.clone();                 //&QueryCodes::InProgress as i32
            lib::set_query_code(&mut pool.get_conn().unwrap(), &1, &result.qid).ok().expect("Failed to set query code!");
            lib::set_query_state(&pool.clone(),&qid, "started", false);
            
            let succes;
            {
                let mut left_files: Vec<String> = Vec::with_capacity(2);
                succes = match handle_download(result, None, &converter,&mut left_files) {
                    Ok(v) => v,
                    Err(e) => {println!("Error: {:?}", e); false }
                };
            
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
    lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 1, 0);
    
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
        Err(e) => { // unknown error / video private etc.. abort
            println!("Unknown error: {:?}", e);
            return Err(e);
        },
    };

    let name_http_valid = lib::format_file_name(&name, &download, &downl_db.qid);

    let file_path = format_file_path(&downl_db.qid, folder.clone());
    file_db.push(file_path.clone());
    let save_path = &format_save_path(folder.clone(),&name, &download, &downl_db.qid);

    println!("Filename: {}", name);
    
    let mut is_splitted_video = false;
    let convert_audio = downl_db.codecs.audio_mp3 == downl_db.quality;

    if !dmca {
    
        is_splitted_video = lib::is_split_container(&downl_db.quality);
        let total_steps = if is_splitted_video {
            4
        } else if download.is_audio() {
            3
        } else {
            2
        };
        
        lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 2, total_steps);

        //download first file, download audio raw source if specified or video
        try!(download.download_file(&file_path, convert_audio));

        if is_splitted_video {
            // download audio file & convert together
            lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 3, total_steps);

            let audio_path = format_audio_path(&downl_db.qid, folder.clone());
            file_db.push(audio_path.clone());
            
            println!("Downloading audio.. {}", audio_path);
            try!(download.download_file(&audio_path, true));

            lib::update_steps(&downl_db.pool.clone(),&downl_db.qid, 4, total_steps);

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
        try!(converter.extract_audio(&downl_db.qid,&file_path, &save_path,convert_audio));
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
    
    //TODO: download file, convert if audio ?!
    Ok(true)
}

///Format save location for file, zip dependent
fn format_file_path(qid: &i64, folder: Option<String>) -> String {
    match folder {
        Some(v) => format!("{}/{}/{}", &CONFIG.general.save_dir, v, qid),
        None => format!("{}/{}", &CONFIG.general.save_dir, qid),
    }
}

///Formats the audio path, based on the qid & optional folders
fn format_audio_path(qid: &i64, folder: Option<String>) -> String {
    match folder {
        Some(v) => format!("{}/{}/{}a", &CONFIG.general.save_dir, v, qid),
        None => format!("{}/{}a", &CONFIG.general.save_dir, qid),
    }
}

///Format save path, dependent on zip option.
fn format_save_path<'a>(folder: Option<String>, name: &str, download: &'a Downloader, qid: &i64) -> String {
    match folder {
        Some(v) => format!("{}/{}/{}", &CONFIG.general.save_dir, v, lib::format_file_name(name,download,qid)),
        None => format!("{}/{}", &CONFIG.general.download_dir, lib::format_file_name(name,download,qid)),
    }
}

///Request an entry from the DB to handle
fn request_entry(pool: & pool::MyPool) -> Option<DownloadDB> {
    let mut conn = try_reoption!(pool.get_conn());
    let mut stmt = try_reoption!(conn.prepare("SELECT queries.qid,url,type,quality FROM querydetails \
                    INNER JOIN queries \
                    ON querydetails.qid = queries.qid \
                    WHERE querydetails.code = 0 \
                    ORDER BY queries.created \
                    LIMIT 1"));
    let mut result = try_reoption!(stmt.execute(&[]));
    let result = try_reoption!(try_option!(result.next())); // result.next().'Some'->value.'unwrap'
    
    println!("Result: {:?}", result[0]);
    println!("result str: {}", result[1].into_str());
    let download_db = DownloadDB { url: from_value::<String>(&result[1]),
                                    quality: from_value::<i16>(&result[3]),
                                    qid: from_value::<i64>(&result[0]),
                                    codecs: &CONFIG.codecs,
                                    extensions: &CONFIG.extensions,
                                    folder: CONFIG.general.save_dir.clone(),
                                    pool: pool.clone(),
                                    playlist: false, //TEMP
                                    compress: false };
    Some(download_db)
}

///Set dbms connection settings
fn mysql_options() -> MyOpts {
    MyOpts {
        tcp_addr: Some(CONFIG.db.ip.clone()),
        tcp_port: CONFIG.db.port,
        user: Some(CONFIG.db.user.clone()),
        pass: Some(CONFIG.db.password.clone()),
        db_name: Some(CONFIG.db.db.clone()),
        ..Default::default() // set others to default
    }
}