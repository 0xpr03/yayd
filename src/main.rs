

#![feature(path_ext)]
extern crate mysql;
extern crate toml;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;

mod lib;

use mysql::conn::MyOpts;
use std::default::Default;
use mysql::conn::pool;
use mysql::conn::pool::MyPooledConn;
use mysql::value::from_value;

use std::error::Error;

use std::ascii::AsciiExt;

use lib::config;
use lib::downloader::DownloadDB;
use lib::downloader::Downloader;
use lib::downloader::DownloadError;
use lib::converter::Converter;

use std::fs::{rename,remove_file};

static VERSION : &'static str = "0.1"; // String not valid
static SLEEP_MS: u32 = 5000;

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
    let opts = mysql_options();
    let pool = match pool::MyPool::new(opts) {
        Ok(conn) => { println!("Connected successfully."); conn},
        Err(err) => panic!("Unable to establish a connection!\n{}",err),
    };
    let converter = Converter::new(&CONFIG.general.ffmpeg_bin, &CONFIG.codecs.audio, pool.clone());
    let mut print_pause = true;
    loop {
        if let Some(result) = request_entry(& pool) {
            print_pause = true;
            if result.playlist {
                println!("Playlist not supported atm!");
                //TODO: set playlist entry to errg
            }
            let qid = result.qid.clone();                 //&QueryCodes::InProgress as i32
            set_query_code(&mut pool.get_conn().unwrap(), &1, &result.qid).ok().expect("Failed to set query code!");
            set_query_state(&pool.clone(),&qid, "started", false);
            let succes = match handle_download(result, None, &converter) {
                Ok(v) => v,
                Err(e) => {println!("Error: {:?}", e); false }
            };
            let code = if succes {
                2//QueryCodes::Finished as i32
            } else {
                3//QueryCodes::Failed as i32
            };
            set_query_code(&mut pool.get_conn().unwrap(), &code,&qid).ok().expect("Failed to set query code!");
            let state = if code == 2 {
                "finished"
            } else {
                "failed"
            };
            set_query_state(&pool.clone(),&qid, state, true);
        } else {
            if print_pause { println!("Pausing.."); print_pause = false; }
            std::thread::sleep_ms(SLEEP_MS);
        }
    }

    println!("EOL!");
}

///Download handler
///Used by the playlist/file handler to download one file
///Based on the quality it needs to download audio & video splitted & convert them together
///In case of a playlist download it depends on the target download folder & if should bezipped
///In case of a DMCA we need to download the file via the socket connector,
///which will output a mp3, or if requested, a hard quality depending on the max-available of
///the video url. Thus in case of a DMCA we can't pick a quality anymore.
///Also the filename depends on the socket output then.
///
///If it's a non-zipped single file, the file is moved after a success download,convert etc to the
///main folder from which it should be downloadable.
///The original non-ascii & url_encode name of the file is stored in the DB
fn handle_download(downl_db: DownloadDB, folder: Option<String>, converter: &Converter) -> Result<bool,DownloadError>{
    //update progress
    let is_zipped = match folder {
        Some(_) => true,
        None => false,
    };

    let dbcopy = downl_db.clone(); //copy, all implement copy & no &'s in use
    update_steps(&dbcopy.pool.clone(),&dbcopy.qid, 1, 0);
    let download = Downloader::new(downl_db, &CONFIG.general);
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

    let name_http_valid = format_file_name(&name, &download);

    println!("Filename: {}", name);

    if !dmca {
        //TODO: insert title name for file,
        //copy file to download folder
    
        let file_path = format_file_path(&dbcopy.qid, folder.clone());
        let save_path = &format_save_path(folder.clone(),&name, &download);

        let is_splitted_format = is_split_container(&dbcopy.quality);
        let total_steps = if is_splitted_format {
            4
        } else {
            2
        };
        update_steps(&dbcopy.pool.clone(),&dbcopy.qid, 2, total_steps);

        //download video, which is video/audio(m4a)
        try!(download.download_file(&file_path, false));

        

        if is_splitted_format { // download audio file & convert together
            update_steps(&dbcopy.pool.clone(),&dbcopy.qid, 3, total_steps);

            let audio_path = format_audio_path(&dbcopy.qid, folder.clone());
            
            println!("Downloading audio.. {}", audio_path);
            try!(download.download_file(&audio_path, true));

            update_steps(&dbcopy.pool.clone(),&dbcopy.qid, 4, total_steps);

            match converter.merge_files(&dbcopy.qid,&file_path, &audio_path,&save_path) {
                Err(e) => {println!("merge error: {:?}",e); return Err(e);},
                Ok(()) => {},
            }

            try!(remove_file(&audio_path));

        }else{ // we're already done, only need to copy / convert to mp3 if requested
            if download.is_audio(){ // if audio-> convert m4a to mp3, which converts directly to downl. dir
                //TODO: convert -> saves already ?
            }else{
                try!(move_file(&file_path, &save_path));
            }
        }
        try!(remove_file(&file_path));
    }

    if !is_zipped { // add file to list, except it's for zip-compression later (=folder set)
        add_file_entry(&dbcopy.pool.clone(), &dbcopy.qid,&name_http_valid, &name);
    }
    
    //TODO: download file, convert if audio ?!
    Ok(true)
}

fn update_steps(pool: & pool::MyPool, qid: &i64, step: i32, max_steps: i32){
    let finished = if max_steps == step {
        true
    }else{
        false
    };
    set_query_state(&pool,qid, &format!("{}|{}", step, max_steps), finished);
}

fn get_file_ext<'a>(download: &Downloader) -> &'a str {
    match download.is_audio() {
        true => "mp3",
        false => "mp4",
    }
}

fn move_file(original: &str, destination: &str) -> Result<(),DownloadError> {
    match rename(original, destination) { // no try possible..
        Err(v) => Err(v.into()),
        Ok(_) => Ok(()),
    }
}

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

///Format save path, which stays inside the optional folder, if set for zipping later
fn format_save_path<'a>(folder: Option<String>, name: &str, download: &'a Downloader) -> String {
    match folder {
        Some(v) => format!("{}/{}/{}.{}", &CONFIG.general.save_dir, v, url_encode(name),get_file_ext(download)),
        None => format!("{}/{}.{}", &CONFIG.general.download_dir, url_encode(name),get_file_ext(download)),
    }
}

fn format_file_name<'a>(name: &str, download: &'a Downloader) -> String {
    format!("{}.{}",name, get_file_ext(download))
}

///Set the state of the current query, also in dependence of the code, see QueryCodes
fn set_query_state(pool: & pool::MyPool,qid: &i64 , state: &str, finished: bool){ // same here
    let mut conn = pool.get_conn().unwrap();
    let progress: i32 = if finished {
        100
    }else{
        0
    };
    let mut stmt = conn.prepare("UPDATE querydetails SET status = ? , progress = ? WHERE qid = ?").unwrap();
    let result = stmt.execute(&[&state,&progress,qid]); // why is this var needed ?!
    match result {
        Ok(_) => (),
        Err(why) => println!("Error setting query state: {}",why),
    }
}

fn add_file_entry(pool: & pool::MyPool, fid: &i64, name: &str, real_name: &str){
    let mut conn = pool.get_conn().unwrap();
    let mut stmt = conn.prepare("INSERT INTO files (fid,name,rname,valid) VALUES (?,?,?,?)").unwrap();
    let result = stmt.execute(&[fid,&real_name,&name,&1]); // why is this var needed ?!
    match result {
        Ok(_) => (),
        Err(why) => println!("Error adding file: {}",why),
    }
}

///Return whether the quality is a split container: video only
///as specified in the docs
fn is_split_container(quality: &i16) -> bool {
    match *quality {
        141 | 83 | 82 | 84 | 85 => false,
        _ => true,
    }
}

///Return an url-conform String
fn url_encode(input: &str) -> String {
    // iterator over input, apply function to each element(function
    input.chars().map(|char| {
        match char {
            ' ' | '?' | '!' | '\\' | '/' | '.' | '(' | ')' | '[' | ']' => '_',
            '&' => '-',
            c if c.is_ascii() => c,
            _ => '_'
        }
    }).collect()
    // match for each char, then do collect: loop through the iterator, collect all elements
    // into container FromIterator
}

///Request an entry from the DB that should be handled
fn request_entry(pool: & pool::MyPool) -> Option<DownloadDB> {
    let mut conn = pool.get_conn().unwrap();
    let mut stmt = conn.prepare("SELECT queries.qid,url,type,quality FROM querydetails \
                    INNER JOIN queries \
                    ON querydetails.qid = queries.qid \
                    WHERE querydetails.code = 0 \
                    ORDER BY queries.created \
                    LIMIT 1").unwrap();
    let mut result = stmt.execute(&[]).unwrap();
    let result = match result.next() {
        Some(val) => val.unwrap(),
        None => {return None; },
    };
    println!("Result: {:?}", result[0]);
    println!("result str: {}", result[1].into_str());
    let download_db = DownloadDB { url: from_value::<String>(&result[1]),
                                                quality: from_value::<i16>(&result[3]),
                                                qid: from_value::<i64>(&result[0]),
                                                audioquality: CONFIG.codecs.audio,
                                                folder: CONFIG.general.save_dir.clone(),
                                                pool: pool.clone(),
                                                playlist: false, //TEMP
                                                compress: false };
    Some(download_db)
}

fn set_query_code(conn: & mut MyPooledConn, code: &i8, qid: &i64) -> Result<(), DownloadError> { // same here
    let mut stmt = conn.prepare("UPDATE querydetails SET code = ? WHERE qid = ?").unwrap();
    let result = stmt.execute(&[code,qid]); // why is this var needed ?!
    match result {
        Ok(_) => Ok(()),
        Err(why) => Err(DownloadError::DBError(why.description().into())),
    }
}

///Set options for the connection
fn mysql_options() -> MyOpts {
    //let dbconfig = CONFIG.get("db").unwrap().clone();
    //let dbconfig = dbconfig.as_table().unwrap(); // shadow binding to workaround borrow / lifetime problems

    MyOpts {
        tcp_addr: Some(CONFIG.db.ip.clone()),
        tcp_port: CONFIG.db.port,
        user: Some(CONFIG.db.user.clone()),
        pass: Some(CONFIG.db.password.clone()),
        db_name: Some(CONFIG.db.db.clone()),
        ..Default::default() // set other to default
    }
}