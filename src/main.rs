#![feature(path_ext)]
#![feature(convert)]
extern crate mysql;
extern crate toml;
#[macro_use]
extern crate lazy_static;

mod lib;

use mysql::conn::MyOpts;
use std::default::Default;
use mysql::conn::pool;
use mysql::value::from_value;

use toml::Table;
use toml::Value;

use lib::config;
use lib::downloader::DownloadDB;

static VERSION : &'static str = "0.1"; // String not valid

lazy_static! {
    static ref CONFIG: Table = {
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
    	if let result = request_entry(& pool) {
    		
    	} else {
    		
    	}
    }
    
	// let downloader = downloader::Downloader::new(download_db);
	// downloader.download_video();
    

    println!("EOL!");
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

///Set options for the connection
fn mysql_options() -> MyOpts {
    let dbconfig = CONFIG.get("db").unwrap().clone();
    let dbconfig = dbconfig.as_table().unwrap(); // shadow binding to workaround borrow / lifetime problems

    MyOpts {
        //tcp_addr: Some(dbconfig.get("ip").unwrap().as_str().clone()),
        tcp_addr: get_option_string(dbconfig,"ip"),
        tcp_port: dbconfig.get("port").unwrap().as_integer().unwrap() as u16,
        //TODO: value does support Encodable -> set as encodable..
        user: get_option_string(dbconfig,"user"),
        pass: get_option_string(dbconfig, "password"),
        db_name: get_option_string(dbconfig, "db"),
        ..Default::default() // set other to default
    }
}

///Converts a toml::Value to a Option<String> for mysql::MyOpts
fn get_option_string(table: & Table,key: & str) -> Option<String> {
    let val: Value = table.get(key).unwrap().clone();
    if let toml::Value::String(s) = val {
        Some(s)
    } else { unreachable!() }
}

// fn get_option_int(table: & Table, key: & str) -> Option<int> {
//     let val: Value = table.get(key).unwrap().clone();
//     if let toml::Value::
// }