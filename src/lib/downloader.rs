extern crate regex;
use mysql::conn::pool::PooledConn;
use mysql::conn::Stmt;

use std::process::{Command, Stdio, Child};
use std::error::Error as EType;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::convert::Into;

use lib::db::prepare_progress_updater;
use lib::config::ConfigGen;
use lib::Request;
use lib::Error;

use lib;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

lazy_static! {
    // [download]  13.4% of 275.27MiB at 525.36KiB/s ETA 07:52
    // we need to remove the / escaping!
    pub static ref regex_name: regex::Regex = regex!(r"(.*)\.([a-zA-Z0-9]+)\z");
    pub static ref regex_progress: regex::Regex = regex!(r"(\d+\.\d)%");
}

pub struct Downloader<'a> {
    defaults: &'a ConfigGen,
}

/// Filename and extension storage
pub struct Filename {
    pub name: String,
    pub extension: String,
}

impl Filename {
    pub fn full_name(&self) -> String{
        format!("{}.{}", &self.name, &self.extension)
    }
}

impl<'a> Downloader<'a>{
    pub fn new(defaults: &'a ConfigGen) -> Downloader<'a>{
        Downloader {defaults: defaults}
    }
    
    /// Downloads the requested file.
    /// file_path specifies the download location.
    /// DMCA errors will get thrown.
    /// download_audio option: ignore the specified quality & download CONFIG.codecs.yt.audio_normal quality for split containers
    fn download_file_in(&self, request: &Request, file_path: &Path, quality: &str) -> Result<bool,Error> {
        trace!("{:?}", request.url);
        
        trace!("quality: {}",quality);
        let mut child = try!(self.run_download_process(file_path, &request.url,quality));
        let stdout = BufReader::new(child.stdout.take().unwrap());
        
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());
        
        let mut conn = request.get_conn();
        let mut statement = try!(prepare_progress_updater(&mut conn));
        
        for line in stdout.lines(){
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", EType::description(&why)); panic!();},
                Ok(text) => {
                        trace!("Out: {}",text);
                        match regex_progress.captures(&text) {
                            Some(cap) => { //println!("Match at {}", s.0);
                                        debug!("{}",  cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                        try!(self.update_progress(&request.qid,&mut statement, cap.at(1).unwrap()));
                                    },
                            None => (),
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
            Err(Error::QualityNotAvailable)
        } else if stderr.contains("ExtractorError") {   
            Err(Error::ExtractorError)
        } else {
            Err(Error::InternalError(stderr))
        }

    }
    
    /// Wrapper for download_file_fn to retry on Extract Error's, which are appearing randomly.
    pub fn download_file(&self, request: &Request, file_path: &Path, quality: &str) -> Result<bool,Error> {
        for attempts in 0..2 {
            match self.download_file_in(&request, file_path, quality) {
                Ok(v) => return Ok(v),
                Err(e) => {
                    match e {
                        Error::ExtractorError =>  { warn!("download try no {}",attempts)},
                        _ => return Err(e),
                    }
                },
            }
        }
        Err(Error::ExtractorError)
    }
    
    /// Trys to get the original name of a file, while checking for availability
    /// As an ExtractError can appear randomly, bug 11, we're retrying again 2 times if it should occour
    /// Through specifying a quality it's possible to get extension specific for the format.
    pub fn get_file_name(&self, url: &str, quality: Option<String>) -> Result<Filename,Error> {
        for attempts in 0..2 {
            let mut child = try!(self.run_filename_process(url,quality.as_ref()));
            let mut stdout_buffer = BufReader::new(child.stdout.take().unwrap());
            let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());
            
            let mut stdout: String = String::new();
            try!(stdout_buffer.read_to_string(&mut stdout));
            let mut stderr: String = String::new();
            try!(stderr_buffer.read_to_string(&mut stderr));
    
            try!(child.wait());
            let capture = regex_name.captures(&stdout.trim());
            if stderr.is_empty() && capture.is_some() {
                let caps = capture.unwrap();
                debug!("get_file_name: {:?}", stdout);
                return Ok(Filename {name: caps[1].to_string(),extension: caps[2].to_string()})
            }else{
                if stderr.contains("not available in your country") || stderr.contains("contains content from") || stderr.contains("This video is available in") {
                    return Err(Error::DMCAError);
                } else if stderr.contains("Please sign in to view this video") {
                    return Err(Error::NotAvailable);
                } else if stderr.contains("ExtractorError") { // #11
                    info!("ExtractorError on attempt {}", attempts +1);
                } else {
                    return Err(Error::DownloadError(stderr));
                }
            }
        }
        Err(Error::ExtractorError)
    }
    
    /// Gets the playlist ids needed for furture download requests.
    /// The output is a vector of IDs
    pub fn get_playlist_ids(&self, request: &Request) -> Result<Vec<String>,Error> {
        let mut child = try!(self.run_playlist_extract(request));
        trace!("retrieving playlist ids");
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let re = regex!(r#""url": "([a-zA-Z0-9_-]+)""#);
        
        let mut id_list: Vec<String> = Vec::new();
        for line in stdout.lines(){
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", EType::description(&why)); panic!();},
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
            return Err(Error::InternalError(stderr));
        }
        
        Ok(id_list)
    }
    
    /// Retrives the playlist name, will kill the process due to yt-dl starting detailed retrieval afterwards.
    pub fn get_playlist_name(&self, url: &str) -> Result<String,Error> {
        let mut child = try!(self.run_playlist_get_name(url));
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let re = regex!(r"\[download\] Downloading playlist: (.*)");
        
        let name: String;
        for line in stdout.lines(){
            
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", EType::description(&why)); panic!();},
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
        
        Err(Error::DownloadError("no playlist name".to_string()))
    }
    
    /// This function does a 3rd party binding in case it's needed
    /// due to the country restrictions
    /// The returned value has to contain the original video name, the lib has to download & save
    /// the file to the given location
    pub fn lib_request_video(&self, current_steps: i32,max_steps: i32, file_path: &Path, request: &Request, quality: &str, get_video: bool) -> Result<Filename,Error> {
        let mut child = try!(self.lib_request_video_cmd(&request.url,file_path,quality,get_video));
        trace!("Requesting video via lib..");
        let stdout = BufReader::new(try!(child.stdout.take().ok_or(Error::InternalError("stdout socket error!".into()))));
        let mut stderr_buffer = BufReader::new(try!(child.stderr.take().ok_or(Error::InternalError("stderr socket error".into()))));

        let re = regex!(r"step (\d)");
        
        let mut last_line = String::new();
        for line in stdout.lines(){
            match line{
                Err(why) => {error!("couldn't read cmd stdout: {}", EType::description(&why));panic!();}, // we'll abort, kinda the floor vanishing under the feet
                Ok(text) => {
                        trace!("Out: {}",text);
                        match re.captures(&text) {
                            Some(cap) => {
                                        debug!("Match: {}", cap.at(1).unwrap()); // ONLY with ASCII chars makeable!
                                        if !request.playlist {
                                            lib::db::update_steps(&mut request.get_conn() ,&request.qid, current_steps + &cap.at(1).unwrap().parse::<i32>().unwrap(), max_steps);
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
            return Err(Error::InternalError(stderr));
        }
        //this ONLY works because `filename: ` is ASCII..
        let mut out = last_line[last_line.find("filename: ").unwrap()+9..].trim().to_string();
        out = lib::url_sanitize(&out);
        if let Some(caps) = regex_name.captures(&out) {
            Ok(Filename {name: caps[1].to_string(),extension: caps[2].to_string()})
        } else {
            return Err(Error::InternalError(format!("no name match! {}",out)));
        }
    }

    /// Formats the download command.
    fn run_download_process(&self, file_path: &Path, url: &str,quality: &str) -> Result<Child,Error> {
        match Command::new("youtube-dl")
        .arg("--newline")
        .arg("--no-warnings")
        .args(&["-r", &format!("{}M",self.defaults.download_mbps / 8)]) // yt-dl uses MB/s, we're using MBit/s
        .args(&["-f",&quality.to_string()])
        .arg("-o")
        .arg(file_path)
        .arg("--hls-prefer-native") // this is needed for twitch extraction
        .args(&["--ffmpeg-location", &self.defaults.ffmpeg_bin_dir]) // backup if internal converter fails
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn() {
            Err(why) => Err(Error::InternalError(EType::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }
    
    /// Runs the filename retrival process.
    fn run_filename_process(&self, url: &str, quality: Option<&String>) -> Result<Child,Error> {
        let mut cmd = Command::new("youtube-dl");
        cmd.arg("--get-filename")
        .args(&["-o","%(title)s.%(ext)s"]);
        if quality.is_some() {
            cmd.args(&["-f",&quality.unwrap()]);
        }
        match cmd.arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn() {
            Err(why) => Err(Error::InternalError(EType::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }
    
    /// Generate the lib command.
    /// binary [args] -q {quality} -r {rate} -f {file} -v {true/false} {url}
    fn lib_request_video_cmd(&self, url: &str, file_path: &Path, quality: &str, get_video: bool) -> Result<Child,Error> {
        let java_path = Path::new(&self.defaults.lib_dir);
        
        debug!("{} {:?} -q {} -r {}M -f {} -v {} {}", self.defaults.lib_bin, self.defaults.lib_args, quality,self.defaults.download_mbps, file_path.to_string_lossy(), get_video, url);
        match Command::new(&self.defaults.lib_bin)
        .current_dir(&java_path)
        .args(&self.defaults.lib_args)
        .args(&["-q",quality])
        .args(&["-r",&format!("{}M", self.defaults.download_mbps)])
        .arg("-f")
        .arg(file_path)
        .args(&["-v",&(get_video).to_string()])
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn() {
                Err(why) => {warn!("{:?}",why); Err(Error::InternalError(EType::description(&why).into()))},
                Ok(process) => Ok(process),
            }
    }
    
    /// Runs the playlist extraction process.
    fn run_playlist_extract(&self, request: &Request) -> Result<Child,Error> {
        let mut cmd = Command::new("youtube-dl");
        cmd.arg("-s").arg("--dump-json").arg("--flat-playlist");
        if request.from > 0 {
            cmd.arg("--playlist-start");
            cmd.arg(request.from.to_string());
        }
        if request.to > 0 {
            cmd.arg("--playlist-end");
            cmd.arg(request.to.to_string());
        }
        match cmd.arg(&request.url)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn() {
            Err(why) => Err(Error::InternalError(EType::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }
    
    /// Runs the playlist name retrival process.
    fn run_playlist_get_name(&self, url: &str) -> Result<Child,Error> {
        match Command::new("youtube-dl")
        .arg("-s")
        .arg("--no-warnings")
        .args(&["--playlist-start","1"])
        .args(&["--playlist-end","1"])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn() {
            Err(why) => Err(Error::InternalError(EType::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }

    /// Executes the progress update statement.
    fn update_progress(&self,qid: &u64,stmt: &mut Stmt, progress: &str) -> Result<(),Error>{
        try!(stmt.execute((progress,qid)).map(|_| Ok(())))
        //-> only return errors, ignore the return value of stmt.execute
    }
}

#[cfg(test)]
mod test {
    use super::*;
    
    #[test]
    fn filenames() {
        assert!(regex_name.is_match("A#B\"C.ABCÜ02.mp4"));
        assert!(regex_name.is_match("A#B\"C.ABCÜ02.webm"));
        assert!(!regex_name.is_match("A#B\"C.ABCÜ02."));
    }
}