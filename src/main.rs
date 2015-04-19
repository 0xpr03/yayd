

#![feature(path_ext)]
#![feature(convert)]
extern crate mysql;
extern crate toml;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;

mod lib;

use mysql::conn::MyOpts;
use std::default::Default;
use mysql::conn::pool;
use mysql::error::MyError;
use mysql::conn::pool::MyPooledConn;
use mysql::value::from_value;

use std::error::Error;

use toml::Table;
use toml::Value;

use lib::config;
use lib::downloader::DownloadDB;
use lib::downloader::Downloader;
use lib::downloader::DownloadError;
use lib::socket;

static VERSION : &'static str = "0.1"; // String not valid

lazy_static! {
    static ref CONFIG: config::Config = {
        println!("Starting yayd-backend v{}",&VERSION);
        config::init_config() //return
    };

}

fn main() {
    let opts = mysql_options();
    let pool = match pool::MyPool::new(opts) {
        Ok(conn) => { println!("Connected successfully."); conn},
        Err(err) => panic!("Unable to establish a connection!\n{}",err),
    };
    loop {
    	if let Some(result) = request_entry(& pool) {
    		if result.playlist {
    			println!("Playlist not supported atm!");
    			//TODO: set playlist entry to err
    		}
    		let qid = result.qid.clone();
    		set_query_code(&mut pool.get_conn().unwrap(), &1, &result.qid).ok().expect("Failed to set query code!");
    		
    		let code = if handle_download(result) {
    			2
    		} else {
    			3
    		};
    		set_query_code(&mut pool.get_conn().unwrap(), &code,&qid).ok().expect("Failed to set query code!");
    	} else {
    		println!("Pausing..");
    		std::thread::sleep_ms(5000);
    	}
    }
    
	// let downloader = downloader::Downloader::new(download_db);
	// downloader.download_video();

    println!("EOL!");
}

///handles 1 url
fn handle_download(downl_db: DownloadDB) -> bool{
	let dbcopy = downl_db.clone(); //copy, all implement copy & no &'s in use
	let download = Downloader::new(downl_db);
	let name = match download.get_file_name() {
		Ok(v) => v,
		Err(DownloadError::DMCAError) => {
			println!("DMCA error!");
			let offlrequest = match socket::request_video(&dbcopy.url, &CONFIG.general.jar_folder, &CONFIG.general.jar_cmd) {
				Err(err) => { println!("Offliberty-call error {:?}", err); return false; },
				Ok(v) => v,
			};
			println!("Output: {}", offlrequest);
			"default-debug-cover".to_string()
		},
		Err(e) => {
			println!("Unknown error: {:?}", e);
			//TODO: add error descr (change enum etc)
			set_query_state(&dbcopy.pool.clone(),&dbcopy.qid, "Error!");
			return false;
		},
	};
	println!("Filename: {}", name);
	true
}

fn set_query_state(pool: & pool::MyPool,qid: &i64 , state: &str){ // same here
	let mut conn = pool.get_conn().unwrap();
    let mut stmt = conn.prepare("UPDATE querydetails SET status = ? WHERE qid = ?").unwrap();
    let result = stmt.execute(&[&state,qid]); // why is this var needed ?!
    match result {
    	Ok(_) => (),
    	Err(why) => println!("Error setting query state: {}",why),
    }
}

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
    //url: &str, quality: i16, qid: i64, folderFormat: &str, pool: MyPool+
    let download_db = DownloadDB { url: from_value::<String>(&result[1]),
												quality: from_value::<i16>(&result[3]),
												qid: from_value::<i64>(&result[0]),
												folder_format: "/home/dev".into(),
												pool: pool.clone(),
												download_limit: 6,
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
        //tcp_addr: Some(dbconfig.get("ip").unwrap().as_str().clone()),
        tcp_addr: Some(CONFIG.db.ip.clone()),
        //tcp_port: dbconfig.get("port").unwrap().as_integer().unwrap() as u16,
        tcp_port: CONFIG.db.port,
        //TODO: value does support Encodable -> set as encodable..
        //user: get_option_string(dbconfig,"user"),
        user: Some(CONFIG.db.user.clone()),
        pass: Some(CONFIG.db.password.clone()),
        db_name: Some(CONFIG.db.db.clone()),
        ..Default::default() // set other to default
    }
}

///Converts a toml::Value to a Option<String> for mysql::MyOpts
#[allow(deprecated)]
fn get_option_string(table: & Table,key: & str) -> Option<String> {
    let val: Value = table.get(key).unwrap().clone();
    if let toml::Value::String(s) = val {
        Some(s)
    } else { unreachable!() }
}