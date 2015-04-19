
extern crate regex;
use mysql::conn::pool::MyPool;
use mysql::conn::pool::MyPooledConn;
use mysql::conn::Stmt;
use mysql::error::MyError;

use std::process::{Command, Stdio, Child};
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;
use std::io;
use std::ascii::AsciiExt;

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
    pub subid: i32, // needed for playlists ?, can't use qid for all files..
    pub folder: String,
    pub pool: MyPool,
    pub download_limit: u16,
}

// impl copy(&self) for DownloadDB -> DownloadDB {
//     let t = DownloadDB{
//         url: self.url
//     }
// }

pub struct Downloader {
    ddb: DownloadDB,
    // pool: MyPool,
}

#[derive(Debug)]
pub enum DownloadError{
    DownloadError(String),
    ReadError,
    DMCAError,
    InternalError(String),
    DBError(String),
}

impl From<MyError> for DownloadError {
    fn from(err: MyError) -> DownloadError {
        DownloadError::DBError(err.description().into())
    }
}

impl From<io::Error> for DownloadError {
    fn from(err: io::Error) -> DownloadError {
        DownloadError::InternalError(err.description().into())
    }
}

impl Downloader {
    pub fn new(ddb: DownloadDB) -> Downloader{
        Downloader {ddb: ddb}
    }
    
    ///Regex: [download]  13.4% of 275.27MiB at 525.36KiB/s ETA 07:52
    ///
    ///Downloads a video, updates the DB
    ///TODO: get the sql statements out of the class
    ///TODO: wrap errors
    ///Doesn't care about DMCAs, will emit errors on them
    pub fn download_video(&self, filename: &str) -> Result<bool,DownloadError> {
        println!("{:?}", self.ddb.url);
        let process = try!(self.run_download_process(filename));
        let stdout = BufReader::new(process.stdout.unwrap());

        let mut conn = self.ddb.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"\d+\.\d");


        for line in stdout.lines(){
            match line{
                Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
                Ok(text) => {
                        println!("Out: {}",text);
                        match re.find(&text) {
                            Some(s) => { println!("Match at {}", s.0);
                                        println!("{}", &text[s.0..s.1]); // ONLY with ASCII chars makeable!
                                        self.update_progress(&mut statement, &text[s.0..s.1].to_string());
                                    },
                            None => println!("Detected no % match."),
                        }
                    },
            }
        }

        Ok(true)
    }

    ///Trys to get the original name of a file, while checking for availability
    ///
    pub fn get_file_name(&self) -> Result<String,DownloadError> {
        let process = try!(self.run_filename_process());
        let mut stdout_buffer = BufReader::new(process.stdout.unwrap());
        let mut stderr_buffer = BufReader::new(process.stderr.unwrap());

        let mut stdout: String = String::new();
        try!(stdout_buffer.read_to_string(&mut stdout));
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));
        println!("stderr: {:?}", stderr);
        println!("stdout: {:?}", stdout);
        if stderr.is_empty() != true {
            if stderr.contains("not available in your country") {
                return Err(DownloadError::DMCAError);
            }else{

            }
        }
        
        stdout.trim();
        println!("get_file_name: {:?}", stdout);
        Ok(stdout)
    }

    ///Return an url-conform String
    pub fn url_encode(input: &str) -> String {
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

    fn run_download_process(&self, filename: &str) -> Result<Child,DownloadError> {
        match Command::new("youtube-dl")
                                    .arg("--newline")
                                    .arg(format!("-r {}M",self.ddb.download_limit))
                                    .arg(format!("-o {}/{}",self.ddb.folder,filename))
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
                                    .arg(&self.ddb.url)
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::piped())
                                    .spawn() {
            Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
            Ok(process) => Ok(process),
        }
    }

    // fn create_regex(expression: & str) -> regex::Regex {
    //     match regex::Regex::new(expression) {
    //         Ok(re) => re,
    //         Err(err) => panic!("regex {}", err),
    //     }
    // }

    // MyPooledConn does only live when MyOpts is alive -> lifetime needs to be declared
    fn prepare_progress_updater<'a>(&'a self,conn: &'a mut MyPooledConn) -> Stmt<'a> { // no livetime needed: struct livetime used
        conn.prepare("UPDATE querydetails SET progress = ? WHERE qid = ?").unwrap()
    }

    ///updater called from the stdout progress
    fn update_progress(&self,stmt: &mut Stmt, progress: &String){
        stmt.execute(&[progress,&self.ddb.qid]);
    }

    ///This function does a 3rd party binding
    ///due to the country restrictions, in case it's needed
    ///Because hyper doesn't support timeout settings atm, we're calling an external
    ///lib
    ///The returned value contains the original video name, the lib downloads & saved
    ///the file at the given folder to the given name
    pub fn lib_request_video(&self,url: &str, jarfolder: &str, jar_cmd: &str) -> Result<String,SocketError> {
        let process = try!(request_video_cmd(url, jarfolder,jar_cmd));

        let mut stdout_buffer = BufReader::new(process.stdout.unwrap());
        let mut stderr_buffer = BufReader::new(process.stderr.unwrap());

        let mut stdout: String = String::new();
        try!(stdout_buffer.read_to_string(&mut stdout));
        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));
        println!("stdout: {:?}", stdout);
        if stderr.contains("error") {
            println!("stderr: {:?}", stderr);
            return Err(DownloadError::InternalError(stderr));
        }
        stdout.trim();
        println!("get_file_name: {:?}", stdout);
        Ok(stdout)
    }

    ///Generate the lib-cmd `request [..]?v=asdf -folder /downloads -a -name testfile`
    fn lib_request_video_cmd(&self) -> Result<Child,SocketError> {
        println!("{:?}", format!("{} {}/offliberty.jar",jar_cmd,jarfolder));
        match Command::new(format!("{} {}/offliberty.jar",jar_cmd,jarfolder))
                                        .arg("request")
                                        .arg(url)
                                        .arg("-folder")
                                        .arg(self.folder)
                                        .arg(gen_request_str())
                                        .arg("-name")
                                        .arg(self.qid)
                                        .stdin(Stdio::null())
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn() {
                Err(why) => Err(SocketError::InternalError(Error::description(&why).into())),
                Ok(process) => Ok(process),
            }
    }

    ///Generate -a or -v, based on if an audio or video quality is requested
    fn gen_request_str(&self) -> str{
        if self.is_audio {
            "-a"
        } else {
            "-v"
        }
    }

    ///Check if the quality is 141, standing for audio or not
    pub fn is_audio(&self){
        match self.quality {
            &CONFIG.codecs.audio => false,
            _ => true,
        }
    }
}