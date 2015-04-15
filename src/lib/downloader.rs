extern crate mysql;
extern crate regex;

use mysql::conn::pool::MyPool;
use mysql::conn::pool::MyPooledConn;
use mysql::conn::Stmt;
use mysql::conn::QueryResult;
use mysql::error::MyError;

use std::process::{Command, Stdio};
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::Child;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

pub struct DownloadDB {
	pub url: String,
	pub quality: i16,
	pub qid: i64,
	pub folder_format: String,
	pub pool: MyPool,
	pub download_limit: i16,
}

pub struct Downloader {
	ddb: DownloadDB,
	// pool: MyPool,
}

#[derive(Debug)]
pub enum DownloadError{
    ConsoleError(String),
    ReadError,
    DMCAError,
    InternalError,
    DBError(String),
}

impl From<MyError> for DownloadError {
	fn from(err: MyError) -> DownloadError {
		DownloadError::DBError(err.description().into())
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
	pub fn download_video(&self) -> Result<bool,DownloadError> {
	    println!("{:?}", self.ddb.url);
	    let process = try!(self.run_ytdl_process());
	    let stdout = BufReader::new(process.stdout.unwrap());

	    let mut conn = self.ddb.pool.get_conn().unwrap();
	    let mut statement = self.prepare_progress_updater(&mut conn);
	    let mut conn = self.ddb.pool.get_conn().unwrap();
	    let re = regex!(r"\d+\.\d%");

	    try!(self.set_query_code(&mut conn, &1));

	    for line in &mut stdout.lines(){
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
	                    //if re.is_match(&text) {println!("Match: {:?}", text);}
	                },
	        }
	    }

	    self.update_progress(&mut statement, &"Finished".to_string());
	    try!(self.set_query_code(&mut conn, &3));

	    Ok(true)
	}

	fn run_ytdl_process(&self) -> Result<Child,DownloadError> {
		match Command::new("youtube-dl")
	                                .arg("--newline")
	                                .arg(format!("-r {}M",self.ddb.download_limit))
	                                .arg(format!("-o {}",self.ddb.folder_format))
	                                .arg(&self.ddb.url)
	                                .stdin(Stdio::null())
	                                .stdout(Stdio::piped())
	                                .spawn() {
	        Err(why) => Err(DownloadError::ConsoleError(Error::description(&why).into())),
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
	    conn.prepare("UPDATE querydetails SET status = ? WHERE qid = ?").unwrap()
	}

	fn set_query_code(&self,conn: & mut MyPooledConn, code: &i8) -> Result<(), DownloadError> { // same here
	    let mut stmt = conn.prepare("UPDATE querydetails SET code = ? WHERE qid = ?").unwrap();
	    let result = stmt.execute(&[code,&self.ddb.qid]);
	    match result {
	    	Ok(_) => Ok(()),
	    	Err(why) => Err(DownloadError::DBError(why.description().into())),
	    }
	    
	}

	///updater called from the stdout progress
	fn update_progress(&self,stmt: &mut Stmt, progress: &String){
	    stmt.execute(&[progress,&self.ddb.qid]);
	}
}