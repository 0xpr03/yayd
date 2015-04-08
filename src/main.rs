#![feature(path_ext)]
extern crate mysql;
extern crate ini;
#[macro_use]
extern crate lazy_static;

use mysql::conn::MyOpts;
use std::default::Default;
use mysql::conn::pool;
use mysql::value::from_value;

use ini::Ini;

use std::env::current_dir;
use std::path::PathBuf;
use std::path::Path;
use std::fs::PathExt;

use std::collections::HashMap;

static VERSION : &'static str = "0.1"; // String not valid

lazy_static! {
	static ref CONFIG: HashMap< &'static str, String> = {
		let mut m = HashMap::new();
		m.insert("user", "root".to_string());
		m.insert("password", "".to_string());
		m.insert("db", "testdb".to_string());
		m.insert("ip", "127.0.0.1".to_string());
		m //return
	};

}


fn main() {
    println!("Starting yayd-backend v{}",&VERSION);

    init_config();

    let options = mysql_options();
    let pool = pool::MyPool::new(options).unwrap();


}

/// create PathBuf by getting the current working dir
/// set_file_name doesn't return smth -> needs to be run on mut path
fn init_config(){
    let mut path = current_dir().unwrap();
    path.set_file_name("config.cfg");
    // let  = path.as_path();
    println!("{:?}",path );
    if(path.as_path().is_file()) {
    	println!("Config file found.");
    }else{
    	println!("Config file not found.");
    	create_config(path.to_str().unwrap());
    }
}

fn create_config(file: &str){
	let mut conf = Ini::new();
	conf.begin_section("DB")
		.set("user","root")
		.set("password", "")
		.set("db","testdb")
		.set("ip", "127.0.0.1");
	conf.write_to_file(file);
}

/// Set options for the connection
fn mysql_options() -> MyOpts {
    MyOpts {
    	tcp_addr: Some(CONFIG.get("ip").unwrap().clone()),
    	tcp_port: 3306,
    	//tcp_port: "3306"
    	user: Some(CONFIG.get("user").unwrap().clone()),
    	pass: Some(CONFIG.get("password").unwrap().clone()),
    	db_name: Some(CONFIG.get("db").unwrap().clone()),
    	..Default::default() // set other to default
    }
}