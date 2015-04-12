#![feature(path_ext)]
#![feature(convert)]
extern crate mysql;
extern crate toml;
#[macro_use]
extern crate lazy_static;

use mysql::conn::MyOpts;
use std::default::Default;
use mysql::conn::pool;
use mysql::value::from_value;

use toml::Table;
use toml::Value;

mod lib {
	pub mod config;
	pub mod downloader;
}

static VERSION : &'static str = "0.1"; // String not valid

lazy_static! {
	static ref CONFIG: Table = {
		println!("Starting yayd-backend v{}",&VERSION);
		lib::config::init_config() //return
	};

}

fn main() {
	let opts = mysql_options();
	let pool = match pool::MyPool::new(opts) {
		Ok(conn) => { println!("Connected successfully."); conn},
		Err(err) => panic!("Unable to establish a connection!\n{}",err),
	};

	let mut conn = pool.get_conn().unwrap();
	let mut stmt = conn.prepare("SELECT queries.qid,url,type,quality FROM querydetails \
					INNER JOIN queries \
					ON querydetails.qid = queries.qid \
					WHERE querydetails.code = 0 \
					ORDER BY queries.created \
					LIMIT 1").unwrap();
	let mut result = stmt.execute(&[]).unwrap();
	let result = result.next().unwrap().unwrap();
	println!("Result: {:?}", result[0]);
	println!("result str: {}", result[1].into_str());
	//url: &str, quality: i32, qid: i64, folderFormat: &str, pool: MyPool
	lib::downloader::download_video(&from_value::<String>(&result[1]),
								from_value::<i32>(&result[3]),
								from_value::<i64>(&result[0]),
								"/home/dev/%(title)s-%(id)s.%(ext)s",
								pool);

	println!("EOL!");
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
// 	let val: Value = table.get(key).unwrap().clone();
// 	if let toml::Value::
// }