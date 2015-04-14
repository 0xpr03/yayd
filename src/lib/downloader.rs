#![feature(plugin)]
extern crate mysql;
extern crate regex;

use mysql::conn::pool::MyPool;
use mysql::conn::pool::MyPooledConn;
use mysql::value::ToValue;
use mysql::conn::Stmt;

use std::process::{Command, Stdio};
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::Child;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

pub struct Downloader<'a> {
	url: &'a str,
	quality: i16,
	qid: i64,
	folderFormat: &'a str,
	pool: MyPool,
}

#[derive(Debug)]
pub enum DownloadError{
    ConsoleError(String),
    RadError,
    DMCAError,
    InternalError,
}


impl<'a> Downloader<'a> {
	pub fn new(url: &'a str, quality: i16, qid: i64, folderFormat: &'a str, pool: MyPool) -> Downloader<'a>{
		Downloader {url: url, quality: quality, qid: qid, folderFormat: folderFormat, pool: pool}
	}
	#[derive(Debug)]
	
	///Regex: [download]  13.4% of 275.27MiB at 525.36KiB/s ETA 07:52
	///
	///Downloads a video, updates the DB
	///TODO: get the sql statements out of the class
	///TODO: wrap errors
	///Doesn't care about DMCAs, will emit errors on them
	pub fn download_video(&self,url: & str, quality: i32, qid: i64, folderFormat: & str, pool: MyPool) -> Result<bool,DownloadError> {
	    println!("{:?}", url);
	    let process = try!(self.run_process(url, folderFormat));
	    let mut s = String::new(); //buffer prep
	    let mut stdout = BufReader::new(process.stdout.unwrap());

	    let mut conn = pool.get_conn().unwrap();
	    let mut statement = self.prepare_progress_updater(&mut conn);
	    let mut conn = pool.get_conn().unwrap();
	    let re = regex!(r"\d+\.\d%");

	    let i = 0;

	    self.set_query_code(&mut conn, &1, &qid);

	    for line in &mut stdout.lines(){
	        match line{
	            Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
	            Ok(text) => {
	                    println!("Out: {}",text);
	                    match re.find(&text) {
	                        Some(s) => { println!("Match at {}", s.0);
	                                    println!("{}", &text[s.0..s.1]); // ONLY with ASCII chars makeable!
	                                    self.update_progress(&mut statement, &text[s.0..s.1].to_string(), &qid);
	                                },
	                        None => println!("Detected no % match."),
	                    }
	                    //if re.is_match(&text) {println!("Match: {:?}", text);}
	                },
	        }
	    }

	    self.update_progress(&mut statement, &"Finished".to_string(), &qid);
	    self.set_query_code(&mut conn, &3, &qid);

	    Ok(true)
	}

	fn run_process(&self,url: &str, folderFormat: &str) -> Result<Child,DownloadError> {
		match Command::new("youtube-dl")
	                                .arg("--newline")
	                                .arg(format!("-o {}",folderFormat))
	                                .arg(url)
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
	fn prepare_progress_updater(&self,conn: &'a mut MyPooledConn) -> Stmt { // no livetime needed: struct livetime used
	    conn.prepare("UPDATE querydetails SET status = ? WHERE qid = ?").unwrap()
	}

	fn set_query_code(&self,conn: & mut MyPooledConn, code: &i8, qid: &i64){ // same here
	    let mut stmt = conn.prepare("UPDATE querydetails SET code = ? WHERE qid = ?").unwrap();
	    stmt.execute(&[code,qid]);
	}

	///updater called from the stdout progress
	fn update_progress(&self,stmt: &mut Stmt, progress: &String, qid: &i64){
	    stmt.execute(&[progress,qid]);
	}
}