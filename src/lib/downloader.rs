extern crate regex;
use mysql::conn::pool::{MyPool,MyPooledConn};
use mysql::conn::Stmt;

use std::process::{Command, Stdio, Child};
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use lib::config::ConfigGen;
use std::convert::Into;

use lib::DownloadError;

use CONFIG;
use lib;
use {TYPE_YT_PL,TYPE_YT_VIDEO,TYPE_TWITCH};

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

#[derive(Clone)]
pub struct DownloadDB {
    pub url: String,
    pub quality: i16,
    pub playlist: bool,
    pub compress: bool,
    pub qid: i64,
    pub source_type: i16,
    pub from: i32,
    pub to: i32,
    pub folder: String, // download folder, changes for playlists
    pub pool: MyPool,
}

impl DownloadDB {
    pub fn update_video(&mut self,url: String, qid: i64) -> &mut DownloadDB{
        self.qid = qid;
        self.url = url;
        self
    }
    pub fn update_folder(&mut self, folder: String){
        self.folder = folder;
    }
}

pub struct Downloader<'a> {
    pub ddb: &'a DownloadDB,
    defaults: &'a ConfigGen,
}

impl<'a> Downloader<'a>{
    pub fn new(ddb: &'a DownloadDB, defaults: &'a ConfigGen) -> Downloader<'a>{
        Downloader {ddb: ddb, defaults: defaults}
    }
    
    //Regex matching: [download]  13.4% of 275.27MiB at 525.36KiB/s ETA 07:52
    /// Downloads the requested file.
    /// file_path specifies the download location.
    /// DMCA errors will get thrown.
    /// download_audio option: ignore the specified quality & download CONFIG.codecs.yt.audio_normal quality for split containers
    fn download_file_in(&self, file_path: &str, download_audio: bool) -> Result<bool,DownloadError> {
        trace!("{:?}", self.ddb.url);
        
        let curr_quality = self.get_quality_name(&download_audio);
        
        trace!("quality: {}",curr_quality);
        let mut child = try!(self.run_download_process(file_path,curr_quality));
        let stdout = BufReader::new(child.stdout.take().unwrap());
        
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let mut conn = self.ddb.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"(\d+\.\d)%");

        for line in stdout.lines(){
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", Error::description(&why)); panic!();},
                Ok(text) => {
                        trace!("Out: {}",text);
                        if !self.ddb.playlist {
                            match re.captures(&text) {
                                Some(cap) => { //println!("Match at {}", s.0);
                                            debug!("{}",  cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                            try!(self.update_progress(&mut statement, cap.at(1).unwrap()));
                                        },
                                None => (),
                            }
                        }
                },
            }
        }

        try!(child.wait()); // waits for finish & then exists zombi process, fixes #10
        
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));
        
        if stderr.is_empty() {
            Ok(true)
        } else if stderr.contains("requested format not available") {
            Err(DownloadError::QualityNotAvailable)
        } else if stderr.contains("ExtractorError") {   
            Err(DownloadError::ExtractorError)
        } else {
            Err(DownloadError::InternalError(stderr))
        }

    }
    
    /// Wrapper for download_file_fn to retry on Extract Error's, which are appearing randomly.
    pub fn download_file(&self, file_path: &str, download_audio: bool) -> Result<bool,DownloadError> {
        for attempts in 0..2 {
            match self.download_file_in(file_path, download_audio) {
                Ok(v) => return Ok(v),
                Err(e) => {
                    match e {
                        DownloadError::ExtractorError =>  { warn!("download try no {}",attempts)},
                        _ => return Err(e),
                    }
                },
            }
        }
        Err(DownloadError::ExtractorError)
    }
    
    /// Trys to get the original name of a file, while checking for availability
    /// As an ExtractError can appear randomly, bug 11, we're retrying again 2 times if it should occour
    pub fn get_file_name(&self) -> Result<String,DownloadError> {
        for attempts in 0..2 {
            let mut child = try!(self.run_filename_process());
            let mut stdout_buffer = BufReader::new(child.stdout.take().unwrap());
            let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());
    
            let mut stdout: String = String::new();
            try!(stdout_buffer.read_to_string(&mut stdout));
            let mut stderr: String = String::new();
            try!(stderr_buffer.read_to_string(&mut stderr));
    
            try!(child.wait());
            if stderr.is_empty() {
                trace!("get_file_name: {:?}", stdout);
                return Ok(stdout.trim().to_string());
            }else{
                if stderr.contains("not available in your country") || stderr.contains("contains content from") || stderr.contains("This video is available in") {
                    return Err(DownloadError::DMCAError);
                } else if stderr.contains("Please sign in to view this video") {
                    return Err(DownloadError::NotAvailable);
                } else if stderr.contains("ExtractorError") { // #11
                    info!("ExtractorError on attempt {}", attempts +1);
                } else {
                    return Err(DownloadError::DownloadError(stderr));
                }
            }
        }
        Err(DownloadError::ExtractorError)
    }
    
    /// Gets the playlist ids needed for furture download requests.
    /// The output is a vector of IDs
    pub fn get_playlist_ids(&self) -> Result<Vec<String>,DownloadError> {
        let mut child = try!(self.run_playlist_extract());
        trace!("retrieving playlist ids");
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let re = regex!(r#""url": "([a-zA-Z0-9_-]+)""#);
        
        let mut id_list: Vec<String> = Vec::new();
        for line in stdout.lines(){
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", Error::description(&why)); panic!();},
                Ok(text) => {
                        trace!("Out: {}",text);
                        match re.captures(&text) {
                            Some(cap) => { //println!("Match at {}", s.0);
                                        debug!("{}", cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                        id_list.push(cap.at(1).unwrap().to_string());
                                    },
                            None => (),
                        }
                    },
            }
        }
        
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));

        try!(child.wait());
        
        if !stderr.is_empty() {
            warn!("stderr: {:?}", stderr);
            return Err(DownloadError::InternalError(stderr));
        }
        
        Ok(id_list)
    }
    
    /// Retrives the playlist name, will kill the process due to yt-dl starting detailed retrieval afterwards.
    pub fn get_playlist_name(&self) -> Result<String,DownloadError> {
        let mut child = try!(self.run_playlist_get_name());
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let re = regex!(r"\[download\] Downloading playlist: (.*)");
        
        let name: String;
        for line in stdout.lines(){
            
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", Error::description(&why)); panic!();},
                Ok(text) => {
                        println!("Out: {}",text);
                        match re.captures(&text) {
                            Some(cap) => {
                                        trace!("{}", cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                        name = cap.at(1).unwrap().to_string();
                                        try!(child.wait());
                                        trace!("done");
                                        return Ok(name);
                                    },
                            None => (),
                        }
                    },
            }
        }
        
        try!(child.wait()); // waits for finish & then exists zombi process fixes #10
        
        Err(DownloadError::DownloadError("no playlist name".to_string()))
    }
    
    /// This function does a 3rd party binding in case it's needed
    /// due to the country restrictions
    /// The returned value has to contain the original video name, the lib has to download & save
    /// the file to the given location
    pub fn lib_request_video(&self, current_steps: i32,max_steps: i32, file_path: &String) -> Result<String,DownloadError> {
        let mut child = try!(self.lib_request_video_cmd(file_path));
        trace!("Requesting video via lib..");
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let re = regex!(r"step (\d)");
        
        let mut last_line = String::new();
        for line in stdout.lines(){
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", Error::description(&why));panic!();},
                Ok(text) => {
                        trace!("Out: {}",text);
                        match re.captures(&text) {
                            Some(cap) => {
                                        debug!("Match: {}", cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                        if !self.ddb.playlist {
                                            lib::db::update_steps(&self.ddb.pool ,&self.ddb.qid, current_steps + &cap.at(1).unwrap().parse::<i32>().unwrap(), max_steps, false);
                                        }
                                    },
                            None => {last_line = text.clone()},
                        }
                    },
            }
        }
        
        trace!("reading stderr");
        
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));
        

        try!(child.wait());
        
        if !stderr.is_empty() {
            warn!("stderr: {:?}", stderr);
            return Err(DownloadError::InternalError(stderr));
        }
        //this ONLY works because `filename ` is ascii..
        let mut out = last_line[last_line.find("filename: ").unwrap()+9..].trim().to_string();
        out = lib::url_sanitize(&out);//stdout.trim();
        
        Ok(out)
    }

	/// Formats the download command.
    fn run_download_process(&self, file_path: &str, quality: String) -> Result<Child,DownloadError> {
        match Command::new("youtube-dl")
                                    .arg("--newline")
                                    .arg("--no-warnings")
                                    .arg("-r")
                                    .arg(format!("{}M",self.defaults.download_mbps))
                                    .arg("-f")
                                    .arg(quality.to_string())
                                    .arg("-o")
                                    .arg(file_path)
                                    .arg("--ffmpeg-location") // this is needed for twitch extraction
                                    .arg(&CONFIG.general.ffmpeg_bin_dir)
                                    .arg(&self.ddb.url)
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::piped())
                                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }
	
	/// Runs the filename retrival process.
    fn run_filename_process(&self) -> Result<Child,DownloadError> {
        match Command::new("youtube-dl")
                                    .arg("--get-filename")
                                    .arg("-o")
                                    .arg("%(title)s")
                                    .arg(&self.ddb.url)
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::piped())
                                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }
    
    /// Generate the lib command.
    fn lib_request_video_cmd(&self, file_path: &String) -> Result<Child,DownloadError> {
        let java_path = Path::new(&self.defaults.lib_dir);
        
        debug!("{} {:?} -q {} -f {} -v {} {}", self.defaults.lib_bin, self.defaults.lib_args, self.ddb.quality, file_path, !self.is_audio(), self.ddb.url);
        match Command::new(&self.defaults.lib_bin)
                                        .current_dir(&java_path)
                                        .args(&self.defaults.lib_args)
                                        .arg("-q")
                                        .arg(&self.ddb.quality.to_string())
                                        .arg("-f")
                                        .arg(file_path)
                                        .arg("-v")
                                        .arg((!self.is_audio()).to_string())
                                        .arg(&self.ddb.url)
                                        .stdin(Stdio::null())
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn() {
                Err(why) => {warn!("{:?}",why); Err(DownloadError::InternalError(Error::description(&why).into()))},
                Ok(process) => Ok(process),
            }
    }
    
    /// Runs the playlist extraction process.
    fn run_playlist_extract(&self) -> Result<Child,DownloadError> {
        let mut cmd = Command::new("youtube-dl");
        cmd.arg("-s").arg("--dump-json").arg("--flat-playlist");
        if self.ddb.from > 0 {
            cmd.arg("--playlist-start");
            cmd.arg(self.ddb.from.to_string());
        }
        if self.ddb.to > 0 {
            cmd.arg("--playlist-end");
            cmd.arg(self.ddb.to.to_string());
        }
        match cmd.arg(&self.ddb.url)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }
    
    /// Runs the playlist name retrival process.
    fn run_playlist_get_name(&self) -> Result<Child,DownloadError> {
        match Command::new("youtube-dl")
                                    .arg("-s")
                                    .arg("--no-warnings")
                                    .arg("--playlist-start")
                                    .arg("1")
                                    .arg("--playlist-end")
                                    .arg("1")
                                    .arg(&self.ddb.url)
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::piped())
                                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }

    /// Prepares the progress update statement.
    // MyPooledConn does only live when MyOpts is alive -> lifetime needs to be declared
    fn prepare_progress_updater(&'a self,conn: &'a mut MyPooledConn) -> Stmt<'a> { // no livetime needed: struct livetime used
        conn.prepare("UPDATE querydetails SET progress = ? WHERE qid = ?").unwrap()
    }

    /// Executes the progress update statement.
    fn update_progress(&self,stmt: &mut Stmt, progress: &str) -> Result<(),DownloadError>{
        try!(stmt.execute((progress,&self.ddb.qid)).map(|_| Ok(())))
        //-> only return errors, ignore the return value of stmt.execute
    }

    /// Returns the quality string used for the current download.
    /// This changes depending on type & source.
    fn get_quality_name(&self, download_audio: &bool) -> String {
        match self.ddb.source_type {
            TYPE_YT_VIDEO | TYPE_YT_PL => {
                if *download_audio {
                    CONFIG.codecs.yt.audio_normal.to_string()
                }else{
                    self.ddb.quality.to_string()
                }
            },
            TYPE_TWITCH => {
                match CONFIG.codecs.twitch.get(&self.ddb.quality.to_string()) {
                    Some(v) => v.clone(),
                    None => { warn!("Unknown twitch code!"); 0.to_string() },
                }
            },
            _ => { warn!("Unknown type!"); 0.to_string() },
        }
    }

    /// Check if audio is requested or not.
    pub fn is_audio(&self) -> bool {
        if self.ddb.quality == CONFIG.codecs.audio_raw {
            true
        } else if self.ddb.quality == CONFIG.codecs.audio_source_hq {
            true
        } else if self.ddb.quality == CONFIG.codecs.audio_mp3 {
            true
        } else {
            false
        }
    }
}