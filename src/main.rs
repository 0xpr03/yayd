extern crate mysql;
extern crate toml;
extern crate rustc_serialize;
#[macro_use]
extern crate log;
extern crate log4rs;
#[macro_use]
extern crate lazy_static;

mod lib;
mod handler;

use lib::downloader::Downloader;
use lib::converter::Converter;
use handler::init_handlers;
use lib::config;
use lib::db;
use lib::logger;
use lib::Error;

const VERSION : &'static str = "0.6";
const CONFIG_PATH : &'static str = "config.cfg";
const LOG_CONFIG: &'static str = "log.conf";
const LOG_PATTERN: &'static str = "%d{%d-%m-%Y %H:%M:%S}\t[%l]\t%f:%L \t%m";
const CODE_STARTED: i8 = 0;
const CODE_IN_PROGRESS: i8 = 1;
const CODE_SUCCESS: i8 = 2;
const CODE_SUCCESS_WARNINGS: i8 = 3; // finished with warnings
const CODE_FAILED_INTERNAL: i8 = 10; // internal error
const CODE_FAILED_QUALITY: i8 = 11; // qualitz not available
const CODE_FAILED_UNAVAILABLE: i8 = 12; // source unavailable (private / removed)

lazy_static! {
    pub static ref CONFIG: config::Config = {
        println!("Starting yayd-backend v{}",&VERSION);
        config::init_config()
    };
    pub static ref SLEEP_TIME: std::time::Duration = {
        std::time::Duration::new(5,0)
    };
        
}

//#[allow(non_camel_case_types)]
//#[derive(Clone, Eq, PartialEq, Debug, Copy)]
//#[repr(i8)]// broken, enum not usable as of #10292
//enum StatusCodes {
//    QueryStarted = 0,
//    InProgress = 1,
//    Success = 2,
//    Finished_Warnings = 3,
//    FailedInternal = 10,
//    FailedQuality = 11,
//    FailedUnavailable = 12,
//}

fn main() {
    logger::initialize();
    let pool = db::db_connect(db::mysql_options(), *SLEEP_TIME, false);
    debug!("cleaning db");
    db::clear_query_states(&pool);
    
    let converter = Converter::new(&CONFIG.general.ffmpeg_bin_dir,&CONFIG.general.mp3_quality , pool.clone());
    let mut handler = init_handlers(Downloader::new(&CONFIG.general),converter);
    let mut print_pause = true;
    debug!("finished startup");
    
    loop {
        if let Some(request) = db::request_entry(& pool) {
            print_pause = true;
            let qid = request.qid.clone();
            db::set_query_code(&pool, &CODE_STARTED, &request.qid);
            db::set_query_state(&pool.clone(),&request.qid, "started", false);
            let code: i8 = match handler.handle(request) {
                Ok(_) => CODE_SUCCESS,
                Err(e) => {
                    match e {
                        Error::NotAvailable => CODE_FAILED_UNAVAILABLE,
                        Error::ExtractorError => CODE_FAILED_UNAVAILABLE,
                        Error::QualityNotAvailable => CODE_FAILED_QUALITY,
                        Error::HandlerWarn(_) => CODE_SUCCESS_WARNINGS,
                        _ => {
                            let details = match e {
                                Error::DBError(s) => format!("{:?}",s),
                                Error::DownloadError(s) => s,
                                Error::FFMPEGError(s) => s,
                                Error::InternalError(s) => s,
                                Error::InputError(s) => s,
                                _ => unreachable!(),
                            };
                            db::add_query_status(&pool,&qid, &details);
                            CODE_FAILED_INTERNAL
                        },
                    }
                }
            }; 
            db::set_query_code(&pool, &code,&qid);
            db::set_null_state(&pool, &qid);
            
        } else {
            if print_pause { debug!("Pausing.."); print_pause = false; }
            std::thread::sleep(*SLEEP_TIME);
        }
    }
}
/*
#[cfg(test)]
mod test {
    extern crate mysql;
    use mysql::error::MyResult;
    use mysql::error::MyError;
    use mysql::conn::{MyOpts};
    use mysql::conn::pool::MyPool;
    
    use super::handle_download;
    use lib;
    use lib::l_expect;
    use lib::config;
    use std::env;
    use std;
    use lib::db::db_connect;
    
    lazy_static! {
        pub static ref CONFIG: config::Config = {
            config::init_config()
        };
        pub static ref SLEEP_TIME: std::time::Duration = {
            std::time::Duration::new(0,0)
           };
    }
    
    macro_rules! println_stderr(
        ($($arg:tt)*) => (
            match writeln!(&mut ::std::io::stderr(), $($arg)* ) {
                Ok(_) => {},
                Err(x) => panic!("Unable to write to stderr: {}", x),
            }
        )
    );

    #[test]
    fn handle_db() {
        assert_eq!(env::var("db_test"),Ok("true".to_string()));
        lib::logger::initialize();
        let pool = connect_db();
        setup_db(&pool);
        info!("db is now set");
        let amount = 4;
        let mut file_db: Vec<String> = Vec::with_capacity(2);
        let converter = lib::converter::Converter::new(&CONFIG.general.ffmpeg_bin_dir, &CONFIG.general.mp3_quality, pool.clone());
        let mut r1;
        for i in 0..amount {
            r1 = lib::db::request_entry(&pool);
            assert!(r1.is_some());
            assert!(super::handle_download(&r1.unwrap(), &None, &converter, &mut file_db).is_ok());
        }
        
        
        
    }

    fn connect_db() -> MyPool {
        let myopts = MyOpts {
            tcp_addr: Some(env::var("db_ip").unwrap()),
            tcp_port: l_expect(env::var("db_port").unwrap().parse::<u16>(),"port"),
            user: Some(env::var("db_user").unwrap()),
            pass: Some(env::var("db_password").unwrap()),
            db_name: Some(env::var("db_db").unwrap()),
            ..Default::default() // set others to default
        };
        println!("{:?}",myopts);
        lib::db::db_connect(myopts, *super::SLEEP_TIME, true)
    }
    
    fn setup_db(pool: &MyPool) -> Result<(),MyError> {
        let setup = include_str!("../install.sql").to_string();
        let lines = setup.lines();
        let mut table_sql = String::new();
        let mut in_table = false;
        for line in lines {
            if in_table {
                table_sql = table_sql +"\n"+ line;
                if line.contains(";") {
                    in_table = false;
                    info!("Table:\n{}",table_sql);
                    l_expect(pool.prep_exec(&table_sql,()),"unable to create db!");
                    table_sql.clear();
                }
            }
            if line.starts_with("CREATE TABLE") {
                table_sql = table_sql +"\n"+ line;
                in_table = true;
            }
        }
        
        // create fake entries to monitor progress regressions leading to wrong updates
        let mut query_stmt = l_expect(pool.prepare("insert into `queries` (qid, url, type, quality, uid, created) VALUES (?,?,?,?,0,NOW())"),"prepare error");
        let mut querydetails_stmt = l_expect(pool.prepare("insert into `querydetails` (qid,code,progress,status) VALUES (?,?,?,?)"),"prepare error");
        let index_start = 10;
        let mut index = index_start;
        for i in 1..index_start {
            l_expect(query_stmt.execute((i,"",0,0)),"stmt exec");
            l_expect(querydetails_stmt.execute((i,-5,-5,"fake")), "stmt exec");
        }
        // shortest 60fps video I could find
        l_expect(query_stmt.execute((index,"https://www.youtube.com/watch?v=IOC_EoRSpUA",0,133)),"stmt exec");
        index += 1;
        l_expect(query_stmt.execute((index,"https://www.youtube.com/watch?v=IOC_EoRSpUA",0,303)),"stmt exec");
        index += 1;
        l_expect(query_stmt.execute((index,"https://www.youtube.com/watch?v=IOC_EoRSpUA",0,-1)),"stmt exec");
        index += 1;
        l_expect(query_stmt.execute((index,"https://www.youtube.com/watch?v=IOC_EoRSpUA",0,-2)),"stmt exec");
        index += 1;
        const code_waiting: i16 = -1;
        for i in index_start..index {
            l_expect(querydetails_stmt.execute((i,code_waiting,0,"waiting")),"stmt exec");
        }
        
        Ok(())
    }
}*/
