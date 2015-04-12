extern crate mysql;

use mysql::conn::pool::MyPool;
use mysql::conn::pool::MyPooledConn;
use mysql::conn::Stmt;

use std::process::{Command, Stdio};
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;

#[derive(Debug)]
pub enum DownloadError {
	ConsoleError,
	RadError,
	DMCAError,
	InternalError,
}

///Downloads a video, updates the DB
///TODO: get the sql statements out of the class
///TODO: wrap errors
///Doesn't care about DMCAs, will emit errors on them
pub fn download_video(url: &str, quality: i32, qid: i64, folderFormat: &str, pool: MyPool) -> Result<bool,DownloadError> {
	let process = match Command::new("youtube-dl")
								.arg("--newline")
								.arg(format!("-o {}",folderFormat))
								.arg(url)
								.stdin(Stdio::null())
                                .stdout(Stdio::piped())
                                .spawn() {
        Err(why) => panic!("couldn't spawn cmd: {}", Error::description(&why)),
        Ok(process) => process,
    };
	let mut s = String::new(); //buffer prep
	let mut stdout = BufReader::new(process.stdout.unwrap());

	let mut conn = pool.get_conn().unwrap();
	let statement = prepare_progress_updater(&mut conn);

	for line in &mut stdout.lines(){
		match line{
			Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
			Ok(text) => println!("{}",text),
		}
	}

	Ok(true)
}

fn prepare_progress_updater<'a>(conn: &'a mut MyPooledConn) -> Stmt<'a> {
	conn.prepare("UPDATE querydetails SET status = ? WHERE qid = ?").unwrap()
}

///updater called from the stdout progress
fn update_progress(stmt: &mut Stmt, progress: i32, qid: i64){
	stmt.execute(&[&progress,&qid]);
}