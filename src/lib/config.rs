
extern crate toml;

use toml::Table;

use std::io::Write;
use std::io::Read;

use std::env::current_dir;
use std::fs::PathExt;
use std::fs::File;

// Config section

#[derive(Debug)]
pub enum ConfigError {
    ReadError,
    WriteError,
    UnknownError,
    CreateError,
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
        config = read_config(&path.to_str().unwrap()).ok(); //result to option
    }else{
        println!("Config file not found.");
        //config = Some();
        config = create_config(&path.to_str().unwrap()).ok();
    }
    config.unwrap()
}

pub fn read_config(file: &str) -> Result<Table,ConfigError> {
    let mut f = try!(File::open(file).map_err(|_| ConfigError::ReadError));
    let mut toml = String::new();
    try!(f.read_to_string(&mut toml).map_err(|_| ConfigError::ReadError));
    match toml::Parser::new(toml.as_str()).parse() {
        None => Err(ConfigError::ParseError),
        Some(table) => Ok(table),
    }
}

pub fn create_config(path: &str) -> Result<Table,ConfigError> {
    //TODO: replace with import_string
    let toml = r#"[db]
user = "root"
password = ""
db = "ytdownl"
port = 3306
ip = "127.0.0.1"

[general]
save_dir = "~/downloads/"
    "#;
    let mut file = try!(File::create(path).map_err(|_| ConfigError::CreateError ));
    let mut parser = toml::Parser::new(toml);
    let config: Table = match parser.parse() {
        None => return Err(ConfigError::ParseError),
        Some(table) => table,
    };
    println!("Raw new config: {:?}", config);
    try!(file.write_all(toml.as_bytes()).map_err(|_| ConfigError::WriteError));
    Ok(config)
}