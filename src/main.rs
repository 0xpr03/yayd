#![feature(path_ext)]
extern crate mysql;
extern crate ini;
extern crate toml;
#[macro_use]
extern crate lazy_static;

use mysql::conn::MyOpts;
use std::default::Default;
use mysql::conn::pool;
use mysql::value::from_value;

use toml::Table;
use toml::Encoder;

use std::io::Write;


use std::env::current_dir;
use std::path::PathBuf;
use std::path::Path;
use std::fs::PathExt;
use std::fs::File;

use std::collections::HashMap;

static VERSION : &'static str = "0.1"; // String not valid

lazy_static! {
	// static ref CONFIG: Table = {
	// 	println!("Starting yayd-backend v{}",&VERSION);
		
	// 	init_config() //return
	// };

}


fn main() {
    init_config();
	// println!("\nValue: {:?}", CONFIG.get("db").unwrap());

    // let options = mysql_options();
    // let pool = pool::MyPool::new(options).unwrap();
	
}

/// create PathBuf by getting the current working dir
/// set_file_name doesn't return smth -> needs to be run on mut path
fn init_config() -> Result<Table,ConfigError> {
    let mut path = current_dir().unwrap();
    path.set_file_name("config.cfg");
    // let  = path.as_path();
    println!("{:?}",path );
    //let conftbl: TomlTable = TomlTable(nul);
    let mut config : Option<Table> = None;
    if path.as_path().is_file() {
    	println!("Config file found.");
    	let mut file = try!(File::open(path.to_str().unwrap()).map_err(|e| ConfigError::OpenError));
    	let config = read_config(&mut file);
    }else{
    	println!("Config file not found.");
    	let mut file = try!(File::create(path.to_str().unwrap()));
    	//config = Some();
    	let config = try!(create_config(&mut file));
    }
    config
}

//makes it printable..
use std::{io,error};
#[derive(Debug)]
enum ConfigError { ParseError,OpenError(io::Error),ReadError(io::Error),WriteError(io::Error),CreateError(io::Error),ErrTypeUnkown }

impl From<ConfigError> for ConfigError {
    fn from(err: ConfigError) -> ConfigError {
        ConfigError(err)
    }
}

///wrap io::Error, for example usage
impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> ConfigError {
        ConfigError::ErrTypeUnkown
    }
}

fn read_config<T>(file: &mut File) -> Result<Table,ConfigError> {
	let mut f = File::open("foo.txt").map_err(|e| {ConfigError::OpenError(e)});
	let mut toml = String::new();
	f.read_to_string(&mut toml).map_err(|e| {ConfigError::ReadError(e)});
	toml::Parser::new(toml).parse().map_err(|e| {ConfigError::ParseError(e)});
}

fn create_config<T>(file: &mut File) -> Result<Table,ConfigError> {
	//TODO: replace with import_string
	let toml = r#"[db]
user = "root"
password = ""
db = "testdb"
port = 3306
ip = "127.0.0.1"
	"#;
	let mut parser = toml::Parser::new(toml);
	let config: Table = parser.parse().unwrap();
	println!("{:?}", config);
	file.write_all(toml.as_bytes()).map_err(|e| {ConfigError::WriteError(e)});
	config
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