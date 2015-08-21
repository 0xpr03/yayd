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
    pub folder: String, // download folder, changes for playlists
    pub pool: MyPool,
}

impl DownloadDB {
    pub fn update_video(&mut self,url: String, qid: i64){
        self.qid = qid;
        self.url = url;
    }
    pub fn update_folder(&mut self, folder: String){
        self.folder = folder;
    }
}

pub struct Downloader<'a> {
    pub ddb: &'a DownloadDB,
    defaults: &'a ConfigGen,
    // pool: MyPool,
}

impl<'a> Downloader<'a>{
    pub fn new(ddb: &'a DownloadDB, defaults: &'a ConfigGen) -> Downloader<'a>{
        Downloader {ddb: ddb, defaults: defaults}
    }
    
    ///Regex matching: [download]  13.4% of 275.27MiB at 525.36KiB/s ETA 07:52
    ///
    ///Downloads a video, updates the DB
    ///TODO: get the sql statements out of the class
    ///TODO: wrap errors
    ///Doesn't care about DMCAs, will emit errors on them
    ///download_audio: ignore quality & download audio_raw codec for split containers
    pub fn download_file(&self, file_path: &str, download_audio: bool) -> Result<bool,DownloadError> {
        println!("{:?}", self.ddb.url);
        
        let curr_quality = if download_audio {
            &CONFIG.codecs.audio_raw
        }else{
            &self.ddb.quality
        };
        
        println!("quality: {}",curr_quality);
        let mut child = try!(self.run_download_process(file_path,curr_quality));
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let mut conn = self.ddb.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"\d+\.\d%");

        for line in stdout.lines(){
            match line{
                Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
                Ok(text) => {
                        println!("Out: {}",text);
                        match re.find(&text) {
                            Some(s) => { //println!("Match at {}", s.0);
                                        println!("{}", &text[s.0..s.1]); // ONLY with ASCII chars makeable!
                                        try!(self.update_progress(&mut statement, &text[s.0..s.1].to_string()));
                                    },
                            None => {},
                        }
                    },
            }
        }

        try!(child.wait()); // waits for finish & then exists zombi process fixes #10

        Ok(true)
    }

    ///Trys to get the original name of a file, while checking for availability
    ///
    pub fn get_file_name(&self) -> Result<String,DownloadError> {
        let mut child = try!(self.run_filename_process());
        let mut stdout_buffer = BufReader::new(child.stdout.take().unwrap());
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let mut stdout: String = String::new();
        try!(stdout_buffer.read_to_string(&mut stdout));
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));

        try!(child.wait());
        //println!("stderr: {:?}", stderr);
        //println!("stdout: {:?}", stdout);
        if stderr.is_empty() == true {
            println!("get_file_name: {:?}", stdout);
            Ok(stdout.trim().to_string())
        }else{
            if stderr.contains("not available in your country") || stderr.contains("contains content from") {
                return Err(DownloadError::DMCAError);
            }else{
                return Err(DownloadError::DownloadError(stderr));
            }
        }
    }
    
    ///Get playlist file ids
    pub fn get_playlist_ids(&self) -> Result<Vec<String>,DownloadError> {
        let mut child = try!(self.run_playlist_extract());
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let re = regex!(r#""url": "([a-zA-Z0-9_-]+)""#);
        
        let mut id_list: Vec<String> = Vec::new();
        for line in stdout.lines(){
            match line{
                Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
                Ok(text) => {
                        println!("Out: {}",text);
                        match re.captures(&text) {
                            Some(cap) => { //println!("Match at {}", s.0);
                                        println!("{}", cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                        id_list.push(cap.at(1).unwrap().to_string());
                                    },
                            None => {},
                        }
                    },
            }
        }
        
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));

        try!(child.wait());
        
        if !stderr.is_empty() {
            println!("stderr: {:?}", stderr);
            return Err(DownloadError::InternalError(stderr));
        }
        
        Ok(id_list)
    }
    
    ///Retrive playlist name, will kill the process due to yt-dl starting detail retrieval
    pub fn get_playlist_name(&self) -> Result<String,DownloadError> {
        let mut child = try!(self.run_playlist_get_name());
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let re = regex!(r"[download] Downloading playlist: (.*)");
        
        let mut name: Option<String> = None;
        for line in stdout.lines(){
            match line{
                Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
                Ok(text) => {
                        println!("Out: {}",text);
                        if name.is_none() {
                            match re.captures(&text) {
                                Some(cap) => {
                                            println!("{}", cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                            name = Some(cap.at(1).unwrap().to_string());
                                            try!(child.kill());
                                        },
                                None => {},
                            }
                        }
                    },
            }
        }
        
        try!(child.wait()); // waits for finish & then exists zombi process fixes #10
        
        name.ok_or(DownloadError::DownloadError("no playlist name".to_string()))
    }

    fn run_download_process(&self, file_path: &str, quality: &i16) -> Result<Child,DownloadError> {
        match Command::new("youtube-dl")
                                    .arg("--newline")
                                    .arg("-r")
                                    .arg(format!("{}M",self.defaults.download_mbps))
                                    .arg("-f")
                                    .arg(quality.to_string())
                                    .arg("-o")
                                    .arg(file_path)
                                    .arg(&self.ddb.url)
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::piped())
                                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }

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
    
    fn run_playlist_extract(&self) -> Result<Child,DownloadError> {
        match Command::new("youtube-dl")
                                    .arg("-s")
                                    .arg("--print-json")
                                    .arg("--flat-playlist")
                                    .arg(&self.ddb.url)
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::piped())
                                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }
    
    fn run_playlist_get_name(&self) -> Result<Child,DownloadError> {
        match Command::new("youtube-dl")
                                    .arg("-s")
                                    .arg("--no-warnings")
                                    .arg(&self.ddb.url)
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::piped())
                                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }

    // MyPooledConn does only live when MyOpts is alive -> lifetime needs to be declared
    fn prepare_progress_updater(&'a self,conn: &'a mut MyPooledConn) -> Stmt<'a> { // no livetime needed: struct livetime used
        conn.prepare("UPDATE querydetails SET progress = ? WHERE qid = ?").unwrap()
    }

    ///updater called from the stdout progress
    fn update_progress(&self,stmt: &mut Stmt, progress: &String) -> Result<(),DownloadError>{
        try!(stmt.execute(&[progress,&self.ddb.qid]).map(|_| Ok(())))
        //-> only return errors, ignore the return value of stmt.execute
    }

    ///This function does a 3rd party binding in case it's needed
    ///due to the country restrictions
    ///Because hyper doesn't support timeout settings atm, we're calling an external
    ///lib
    ///The returned value contains the original video name, the lib downloads & saves
    ///the file at the given folder under the given name
    pub fn lib_request_video(&self) -> Result<String,DownloadError> {
        let mut child = try!(self.lib_request_video_cmd());
        println!("Requesting video via lib..");
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let mut conn = self.ddb.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"step (\d)");
        
        let mut last_line = String::new();
        for line in stdout.lines(){
            match line{
                Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
                Ok(text) => {
                        println!("Out: {}",text);
                        match re.captures(&text) {
                            Some(cap) => { //println!("Match at {}", s.0);
                                        println!("{}", cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                        try!(self.update_progress(&mut statement, &cap.at(1).unwrap().to_string()));
                                    },
                            None => {last_line = text.clone()},
                        }
                    },
            }
        }
        
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));
        

        try!(child.wait());
        
        if !stderr.is_empty() {
            println!("stderr: {:?}", stderr);
            return Err(DownloadError::InternalError(stderr));
        }
        //this ONLY works because `filename ` is ascii..
        let out = last_line[last_line.find("filename ").unwrap()+9..].trim().to_string();
        //stdout.trim();
        
        Ok(out)
    }

    ///Generate the lib-cmd `request [..]?v=asdf -folder /downloads -a -name testfile`
    fn lib_request_video_cmd(&self) -> Result<Child,DownloadError> {
        let java_path = Path::new(&self.defaults.jar_cmd);
        println!("{:?}", format!("{}/java -jar {}/offliberty.jar",self.defaults.jar_cmd,self.defaults.jar_folder));
        match Command::new("./java")
                                        .current_dir(java_path)
                                        .arg("-jar")
                                        .arg(format!("{}/offliberty.jar",&self.defaults.jar_folder))
                                        .arg("request")
                                        .arg(&self.ddb.url)
                                        .arg("-folder")
                                        .arg(&self.ddb.folder)
                                        .arg(self.gen_request_str())
                                        .arg("-name")
                                        .arg(self.ddb.qid.to_string()) //eq. format! https://botbot.me/mozilla/rust/msg/37524131/
                                        .stdin(Stdio::null())
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn() {
                Err(why) => {println!("{:?}",why); Err(DownloadError::InternalError(Error::description(&why).into()))},
                Ok(process) => Ok(process),
            }
    }

    ///Generate -a or -v, based on if an audio or video quality is requested
    fn gen_request_str(&self) -> &'a str{
        if self.is_audio() && !self.is_aac() {
            "-a"
        } else {
            "-v"
        }
    }

    ///Check if the quality is 141, standing for audio or not
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
    
    ///Check if is AAC file
    fn is_aac(&self) -> bool {
        CONFIG.extensions.aac.contains(&self.ddb.quality)
    }
}