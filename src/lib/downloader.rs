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


#[derive(Debug)]
pub enum DownloadError {
	ConsoleError,
	RadError,
	DMCAError,
	InternalError,
}
///Regex: [download]  13.4% of 275.27MiB at 525.36KiB/s ETA 07:52
///
///Downloads a video, updates the DB
///TODO: get the sql statements out of the class
///TODO: wrap errors
///Doesn't care about DMCAs, will emit errors on them
pub fn download_video(url: &str, quality: i32, qid: i64, folderFormat: &str, pool: MyPool) -> Result<bool,DownloadError> {
	println!("{:?}", url);
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
	let mut statement = prepare_progress_updater(&mut conn);
	let mut conn = pool.get_conn().unwrap();
	let re = create_regex(&r"\d+\.\d%");

	let i = 0;

	set_query_code(&mut conn, &1, &qid);

	for line in &mut stdout.lines(){
		match line{
			Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
			Ok(text) => {
					println!("Out: {}",text);
					match re.find(&text) {
						Some(s) => { println!("Match at {}", s.0);
									println!("{}", &text[s.0..s.1]); // ONLY with ASCII chars makeable!
									update_progress(&mut statement, &text[s.0..s.1].to_string(), &qid);
								},
						None => println!("Detected no % match."),
					}
					//if re.is_match(&text) {println!("Match: {:?}", text);}
				},
		}
	}

	update_progress(&mut statement, &"Finished".to_string(), &qid);
	set_query_code(&mut conn, &3, &qid);

	Ok(true)
}

fn create_regex(expression: & str) -> regex::Regex {
	match regex::Regex::new(expression) {
	    Ok(re) => re,
	    Err(err) => panic!("regex {}", err),
	}
}

// MyPooledConn does only live when MyOpts is alive -> lifetime needs to be declared
fn prepare_progress_updater<'a>(conn: &'a mut MyPooledConn) -> Stmt<'a> {
	conn.prepare("UPDATE querydetails SET status = ? WHERE qid = ?").unwrap()
}

fn set_query_code<'a>(conn: &'a mut MyPooledConn, code: &i8, qid: &i64){
	let mut stmt = conn.prepare("UPDATE querydetails SET code = ? WHERE qid = ?").unwrap();
	stmt.execute(&[code,qid]);
}

///updater called from the stdout progress
fn update_progress(stmt: &mut Stmt, progress: &String, qid: &i64){
	stmt.execute(&[progress,qid]);
}