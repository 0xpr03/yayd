#![feature(path_ext)]
#![feature(convert)]
extern crate toml;

use toml::Table;
use toml::Encoder;

use std::io::Write;
use std::io::Read;

use std::env::current_dir;
use std::path::PathBuf;
use std::path::Path;
use std::fs::PathExt;
use std::fs::File;

// Config section

#[derive(Debug)]
pub enum ConfigError {
	ReadError,
	WriteError,
	UnknownError,
	ParseError,
}

/// create PathBuf by getting the current working dir
/// set_file_name doesn't return smth -> needs to be run on mut path
pub fn init_config() -> Table {
    let mut path = current_dir().unwrap();
    path.set_file_name("config.cfg");
    // let  = path.as_path();
    println!("{:?}",path );
    //let conftbl: TomlTable = TomlTable(nul);
    let mut config : Option<Table> = None;
    if path.as_path().is_file() {
    	println!("Config file found.");
    	let mut file = File::open(path.to_str().unwrap()).unwrap();
    	let config = read_config(&mut file).unwrap();
    }else{
    	println!("Config file not found.");
    	let mut file = File::create(path.to_str().unwrap()).unwrap();
    	//config = Some();
    	let config = create_config(&mut file).unwrap();
    }
    config.unwrap()
}

pub fn read_config(file: &mut File) -> Result<Table,ConfigError> {
	let mut f = try!(File::open("foo.txt").map_err(|_| ConfigError::ReadError));
	let mut toml = String::new();
	try!(f.read_to_string(&mut toml).map_err(|_| ConfigError::ReadError));
	match toml::Parser::new(toml.as_str()).parse() {
		None => Err(ConfigError::ParseError),
		Some(table) => Ok(table),
	}
}

pub fn create_config(file: &mut File) -> Result<Table,ConfigError> {
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
	try!(file.write_all(toml.as_bytes()).map_err(|_| ConfigError::WriteError));
	Ok(config)
}