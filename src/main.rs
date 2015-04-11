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

use std::collections::HashMap;

mod lib {
	pub mod config;
}

static VERSION : &'static str = "0.1"; // String not valid

lazy_static! {
	static ref CONFIG: Table = {
		println!("Starting yayd-backend v{}",&VERSION);
		lib::config::init_config() //return
	};

}


fn main() {
	for (key, value) in CONFIG.iter() {
	    println!("{}: {}", key, value);
	}
}

// Set options for the connection
// fn mysql_options() -> MyOpts {
//     MyOpts {
//     	tcp_addr: Some(CONFIG.get("ip").unwrap().clone()),
//     	tcp_port: 3306,
//     	//tcp_port: "3306"
//     	user: Some(CONFIG.get("user").unwrap().clone()),
//     	pass: Some(CONFIG.get("password").unwrap().clone()),
//     	db_name: Some(CONFIG.get("db").unwrap().clone()),
//     	..Default::default() // set other to default
//     }
// }