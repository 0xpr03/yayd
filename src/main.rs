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
	let opts = mysql_options();
}

//Set options for the connection
fn mysql_options() -> MyOpts {
	let dbconfig = CONFIG.get("db").unwrap().clone();
	let dbconfig = dbconfig.as_table().unwrap(); // shadow binding to workaround borrow / lifetime problems


    MyOpts {
    	//tcp_addr: Some(dbconfig.get("ip").unwrap().as_str().clone()),
    	tcp_addr: get_option_string(dbconfig,"ip"),
    	tcp_port: 3306,
    	//tcp_port: "3306"
    	user: get_option_string(dbconfig,"user"),
    	pass: get_option_string(dbconfig, "password"),
    	db_name: get_option_string(dbconfig, "db"),
    	..Default::default() // set other to default
    }
}

fn get_option_string(table: & Table,key: & str) -> Option<String> {
	let val: Value = table.get(key).unwrap().clone();
	if let toml::Value::String(s) = val {
		println!("Value: {:?}", s);
		Some(s)
	} else { unreachable!() }
	//Some(table.get(key).unwrap().as_str().unwrap())
}