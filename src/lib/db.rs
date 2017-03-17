extern crate regex;

use mysql::conn::pool::{PooledConn,Pool};
use mysql::conn::{Opts, OptsBuilder};
use mysql::conn::Stmt;
use mysql::value::{Value,from_row_opt};

use std::time::Duration;
use std::cell::RefCell;
use std::thread::sleep;
use std::path::PathBuf;
use std::convert::From;

use super::{Error,Request};

use lib;

use CODE_FAILED_INTERNAL;
use CODE_IN_PROGRESS;
use CODE_WAITING;
use CODE_STARTED;
use CONFIG;

///Move result value out, return with none on err & print
macro_rules! try_reoption { ($e:expr) => (match $e { Ok(x) => x, Err(e) => {warn!("{}",e);return None; },}) }
macro_rules! try_return { ($e:expr) => (match $e { Ok(x) => x, Err(e) => {warn!("{}",e);return; },}) }

macro_rules! try_option { ($e:expr) => (match $e { Some(x) => x, None => return None }) }
macro_rules! try_Eoption { ($e:expr) => (match $e { Some(x) => x, None => {error!("No value!");return None } }) }

/// Take value and return, log an error with the missing column otherwise
macro_rules! take_value {
    ($a:expr, $b:expr) => (match $a.take($b) {
            Some(x) => x,
            None => { error!("No value for column `{}`!",$b); return None }
    })
}

/// Get value and return, log an error with the missing column otherwise
macro_rules! get_value {
    ($a:expr, $b:expr) => (match $a.get($b) {
            Some(x) => x,
            None => { error!("No value for column `{}`!",$b); return None }
    })
}

/// from,to non playlist value
const DEFAULT_PLAYLIST_VAL: i16 = -2;

/// Required database tables to be checked on deletion
const REQ_DB_TABLES:[&'static str; 5]  = ["queries","querydetails","playlists","subqueries","query_files"];

pub enum DeleteRequestType<'a> {
    Marked, // delete = 1
    AgedMin(&'a u16) // valid = 1, age
}


/// Connection wrapper, allowing to get an Pool or an PooledConn
pub enum STConnection<'a> {
    Pool(&'a Pool),
    Conn(PooledConn)
}

impl<'a> From<PooledConn> for STConnection<'a> {
    fn from(x: PooledConn) -> STConnection<'a> {
        STConnection::Conn(x)
    }
}

impl<'a> From<&'a Pool> for STConnection<'a> {
    fn from(x: &Pool) -> STConnection {
        STConnection::Pool(x)
    }
}
/// Connect to DBMS, retry on failure.
/// `is_test` is only `true` for tests
#[cfg(not(test))]
pub fn db_connect(opts: Opts, sleep_time: Duration) -> Pool { 
    loop {
        match Pool::new(opts.clone()) {
            Ok(conn) => {return conn;},
            Err(err) => error!("Unable to establish a connection: {}",err),
        };
        sleep(sleep_time);
    }
}

#[cfg(test)]
pub fn db_connect(opts: Opts) -> Pool { 
    Pool::new(opts.clone()).unwrap()
}

/// Set state of query
pub fn set_query_state(conn: &mut PooledConn,qid: &u64 , state: &str){ // same here
    let mut stmt = try_return!(conn.prepare("UPDATE querydetails SET status = ? , progress = ? WHERE qid = ?"));
    let result = stmt.execute((&state,&0,qid)); // why is this var needed ?!
    match result {
        Ok(_) => (),
        Err(why) => error!("Error setting query state: {}",why),
    }
}

pub fn clear_query_states(conn: &mut PooledConn) {
    let affected = try_return!(conn.prep_exec("UPDATE `querydetails` SET `code` = ?, `status` = NULL WHERE `code` = ? OR `code` = ?",(CODE_FAILED_INTERNAL,CODE_STARTED, CODE_IN_PROGRESS))).affected_rows();
    if affected != 0 {
        info!("Cleaned {} entries.",affected);
    }else{
        info!("No entries to clean.");
    }
}

/// Set state of query to null & finished
///
/// Saves table space for finished downloads & sets progress to 100
pub fn set_null_state(conn: &mut PooledConn, qid: &u64){
    let mut stmt = try_return!(conn.prepare("UPDATE querydetails SET status = NULL, progress = 100 WHERE qid = ?"));
    let result = stmt.execute((qid,));
    match result {
        Ok(_) => (),
        Err(why) => error!("Error setting query sate: {}", why),
    }
}

/// Update query status code
/// Affecting querydetails.code
pub fn set_query_code(conn: &mut PooledConn, qid: &u64, code: &i8) { // same here
    trace!("Setting query code {} for id {}", code, qid);
    let mut stmt = conn.prepare("UPDATE querydetails SET code = ? WHERE qid = ?").unwrap();
    let result = stmt.execute((&code,&qid));
    match result {
        Ok(_) => (),
        Err(why) => error!("Error inserting querystatus: {}",why),
    }
}

/// Update progress steps for db entrys
pub fn update_steps(conn: &mut PooledConn, qid: &u64, ref step: i32,ref max_steps: i32){
    trace!("Updating steps to {} for id {}",step, qid);
    set_query_state(conn,qid, &format!("{}|{}", step, max_steps));
}

/// Prepares the progress update statement.
// MyPooledConn does only live when MyOpts is alive -> lifetime needs to be declared
pub fn prepare_progress_updater(conn: &mut PooledConn) -> Result<Stmt,Error> {
    match conn.prepare("UPDATE querydetails SET progress = ? WHERE qid = ?") {
        Ok(v) => Ok(v),
        Err(e) => Err(From::from(e)) // because implementing type conversion for non self declared types isn't allowed
    }
}

/// Add file to db including it's name & fid based on the qid
pub fn add_file_entry(conn: &mut PooledConn, qid: &u64, name: &str, real_name: &str) -> Result<u64, Error> {
    trace!("name: {}",name);
    let fid: u64;
    {
        let mut stmt = conn.prepare("INSERT INTO files (rname,name,valid) VALUES (?,?,?)").unwrap();
        let result = try!(stmt.execute((&real_name,&name,&true)));
        fid = result.last_insert_id();
    }
    {
        if CONFIG.general.link_files {
            let mut stmt = try!(conn.prepare("INSERT INTO `query_files` (qid,fid) VALUES(?,?)"));
               try!(stmt.execute((&qid,&fid)));
        }
    }
    Ok(fid)
}

/// Add query status msg for error reporting
pub fn add_query_error(conn: &mut PooledConn, qid: &u64, status: &str){
    let mut stmt = conn.prepare("INSERT INTO queryerror (qid,msg) VALUES (?,?)").unwrap();
    let result = stmt.execute((&qid,&status));
    match result {
        Ok(_) => (),
        Err(why) => error!("Error inserting query error: {}",why),
    }
}

/// Create new sub query, exmaple: for un-zipped playlist downloads, per-entry handle
pub fn add_sub_query(url: &str, request: &Request) -> Result<u64,Error> {
    let id: u64 = try!(insert_query(url, request));
    
    if CONFIG.general.link_subqueries {
        let mut conn = request.get_conn();
        let mut stmt = try!(conn.prepare("INSERT INTO `subqueries` (qid,origin_id) VALUES(?,?)"));
        try!(stmt.execute((&id,&request.qid)));
    }
    
    Ok(id)
}

/// Insert wrapper for requests, differing only url wise
fn insert_query(url: &str, req: &Request) -> Result<u64,Error> {
    let mut conn = req.get_conn();
    match _insert_query(&url, &req.quality,&req.uid,&req.r_type,&mut conn) {
        Err(e) => Err(e),
        Ok(v) => Ok(v)
    }
}

/// Inserts a new query
fn _insert_query(url: &str, quality: &i16, uid: &u32, r_type: &i16,conn: &mut PooledConn) -> Result<u64, Error> {
    let id: u64;
    {
        let mut stmt = try!(conn.prepare("INSERT INTO `queries` (url,quality,uid,created,`type`) VALUES(?,?,?,Now(),?)"));
        let result = try!(stmt.execute((url,quality, uid,r_type)));
        id = result.last_insert_id();
    }
    {
        let mut stmt = try!(conn.prepare("INSERT INTO `querydetails` (qid,`code`) VALUES(?,?)"));
           try!(stmt.execute((&id,&CODE_WAITING)));
    }
    Ok(id)
}

/// Request an entry from the DB to handle
pub fn request_entry<'a, T: Into<STConnection<'a>>>(connection: T) -> Option<Request> {
    let mut db_conn: PooledConn = match connection.into() {
        STConnection::Pool(x) => try_reoption!(x.get_conn()),
        STConnection::Conn(x) => x
    };
    
    let mut row;
    {
        let mut stmt = try_reoption!(db_conn.prepare(
        "SELECT queries.qid,url,quality,`split`,`from`,`to`,uid,`type` FROM queries \
        JOIN querydetails ON queries.qid = querydetails.qid \
        LEFT JOIN playlists ON queries.qid = playlists.qid \
        WHERE querydetails.code = -1 \
        ORDER BY queries.created \
        LIMIT 1"));
        let mut result = try_reoption!(stmt.execute(()));
        row = try_reoption!(try_option!(result.next())); // result.next().'Some'->value.'unwrap'
    }
    
    trace!("row: {:?}", row);
    let from: i16;
    let to: i16;
    let split: bool;
    
    let temp: Value = get_value!(row,"from");
    let playlist: bool = temp != Value::NULL;
    debug!("playlist: {}",playlist);
    if playlist {
        split = take_value!(row,"split");
        from = take_value!(row,"from");
        to = take_value!(row,"to");
    } else {
        from = DEFAULT_PLAYLIST_VAL;
        to = DEFAULT_PLAYLIST_VAL;
        split = false;
    }
    let request = Request {
        url: take_value!(row,"url"),
        quality: take_value!(row,"quality"),
        qid: take_value!(row,"qid"),
        r_type: take_value!(row,"type"),
        conn: RefCell::new(db_conn),
        playlist: playlist,
        split: split,
        from: from,
        to: to,
        path: PathBuf::from(&CONFIG.general.download_dir),
        temp_path: PathBuf::from(&CONFIG.general.temp_dir),
        uid: take_value!(row,"uid")
    };
    Some(request)
}

/// Mark file as to be deleted via delete flag
pub fn set_file_delete_flag(conn: &mut PooledConn, fid: &u64, delete: bool) -> Result<(),Error> {
    let mut stmt = try!(conn.prepare("UPDATE files SET `delete` = ? WHERE fid = ?"));
    try!(stmt.execute((delete,fid)));
    Ok(())
}

/// (Auto) file deletion retriver
/// Returns a tuple of Vec<qid> and Vec<fid,file name> older then age
pub fn get_files_to_delete(conn: &mut PooledConn, del_type: DeleteRequestType) -> Result<(Vec<u64>,Vec<(u64,String)>),Error> {
    let sql = String::from("SELECT `query_files`.`qid`,`files`.`fid`,`name` FROM files \
            LEFT JOIN `query_files` ON files.fid = query_files.fid ");
    let sql = sql+&match del_type {
        DeleteRequestType::AgedMin(x) => String::from("WHERE `valid` = 1 AND `created` < (NOW() - INTERVAL %min% DAY_MINUTE)").replace("%min%", &x.to_string()),
        DeleteRequestType::Marked => String::from("WHERE files.`delete` = 1 AND `valid` = 1")
    };
    debug!("sql: {}",sql);
    let mut stmt = try!(conn.prepare(&sql));
    let mut qids = Vec::new();
    let mut files = Vec::new();
    for result in try!(stmt.execute(())) {
        let (qid,fid,name) = try!(from_row_opt::<(u64,u64,String)>(try!(result)));
        qids.push(qid);
        files.push((fid,name));
    }
    drop(stmt);
    qids.sort();
    qids.dedup();
    Ok((qids,files))
}

/// Set file valid flag
pub fn set_file_valid_flag(conn: &mut PooledConn, fid: &u64, valid: bool) -> Result<(),Error> {
    let mut stmt = try!(conn.prepare("UPDATE `files` SET `valid` = ? WHERE `fid` = ?"));
    if try!(stmt.execute((valid,fid))).affected_rows() != 1 {
        return Err(Error::InternalError(String::from(format!("Invalid affected lines count!"))));
    }
    Ok(())
}

/// Set DBMS connection settings
pub fn mysql_options(conf: &lib::config::Config) -> Opts {
    let mut builder = OptsBuilder::new();
    builder.ip_or_hostname(Some(conf.db.ip.clone()))
    .tcp_port(conf.db.port)
    .user(Some(conf.db.user.clone()))
    .pass(Some(conf.db.password.clone()))
    .db_name(Some(conf.db.db.clone()));
    builder.into()
}

/// Delete request or file entry
/// If a qid is specified, all file entries will also be erased
/// For files to be erased the `link_files` config has to be enabled
/// On deletion error all is rolled back to avoid data inconsistency
pub fn delete_requests(conn: &mut PooledConn, qids: Vec<u64>,files: Vec<(u64,String)> ) -> Result<(),Error> {
    let mut transaction = try!(conn.start_transaction(false,None,None));
    
    {
        let mut stmt = try!(transaction.prepare("DELETE FROM files WHERE fid = ?"));
        for (fid,_) in files {
            try!(stmt.execute((&fid,)));
        }
    }
    
    let delete_sql_tmpl = "DELETE FROM %db% WHERE qid = ?";
    for db in REQ_DB_TABLES.iter() {
        let mut stmt = try!(transaction.prepare(delete_sql_tmpl.replace("%db%",db)));
        for qid in &qids {
            try!(stmt.execute((qid,)));
        }
    }
    try!(transaction.commit());
    Ok(())
}

/// Setup tables
/// Created as temporary if specified (valid for the current connection)
#[cfg(test)]
fn setup_db(conn: &mut PooledConn, temp: bool) -> Result<(),Error> {
    let tables = get_db_create_sql();
    for a in tables {
        conn.query(
            if temp {
                a.replace("CREATE TABLE","CREATE TEMPORARY TABLE")
            } else {
                a
            }
        ).unwrap();
    }
    Ok(())
}

/// Returns a vector of table setup sql
#[cfg(test)]
fn get_db_create_sql<'a>() -> Vec<String> {
    let raw_sql = include_str!("../../setup.sql");
    
    let reg = regex::Regex::new(r"(/\*(.|\s)*?\*/)").unwrap(); // https://regex101.com/r/bG6aF2/6, replace `\/` with `/`
    let raw_sql: String = reg.replace_all(raw_sql, "");
    
    let raw_sql = raw_sql.replace("\n","");
    let raw_sql = raw_sql.replace("\r","");
    
    debug!("\n\nSQL: {}\n\n",raw_sql);
    
    let split_sql:Vec<String> = raw_sql.split(";").filter_map(|x| // split at `;`, filter_map on iterator
        if x != "" { // check if it's an empty group (last mostly)
            Some(x.to_owned()) // &str to String
        } else {
            None
        }
        ).collect(); // collect back to vec
    
    debug!("\n\nGroups: {:?}\n\n",split_sql);
    
    split_sql
}

/// For all DB tests the DB itself has to be clear from any tables matching the names used here!
#[cfg(test)]
mod test {
    use std::path::PathBuf;
    
    use super::*;// import only public items
    use super::{DEFAULT_PLAYLIST_VAL,get_db_create_sql,REQ_DB_TABLES};
    use std::cell::RefCell;
    use mysql::conn::pool::{PooledConn,Pool};
    use mysql::value::from_row;
    use mysql;
    
    use chrono::naive::datetime::NaiveDateTime;
    use chrono::offset::local::Local;
    use chrono::datetime::DateTime;
    use chrono::duration::Duration;
    use chrono;
    
    use lib::logger;
    use lib::ReqCore;
    use lib::Request;
    use lib::Error;
    use lib;
    
    fn create_request(playlist: bool,config: &lib::config::Config) -> ReqCore {
        let mut req = ReqCore {
            url: String::from("test.com"),
            quality: 1,
            qid: 1,
            playlist: false,
            split: false,
            r_type: -2,
            from: DEFAULT_PLAYLIST_VAL,
            to: DEFAULT_PLAYLIST_VAL,
            path: PathBuf::from(&config.general.download_dir),
            temp_path: PathBuf::from(&config.general.temp_dir),
            uid: 1
        };
        
        if playlist {
            req.playlist = true;
            req.from = 0;
            req.to = 100;
            req.split = true;
        }
        req
    }
    
    fn connect() -> (lib::config::Config,Pool) {
        let config = lib::config::init_config();
        let pool = db_connect(mysql_options(&config));
        (config,pool)
    }
    
    fn setup(conn: &mut PooledConn){
        use super::setup_db;
        setup_db(conn,true);
    }
    
    fn get_status(conn: &mut PooledConn, qid: &u64) -> (i8,Option<f64>,Option<String>) {
        let mut stmt = conn.prepare("SELECT `code`,`progress`,`status` FROM `querydetails` WHERE `qid`=?").unwrap();
        let mut result = stmt.execute((qid,)).unwrap();
        mysql::from_row(result.next().unwrap().unwrap())
    }
    
    fn get_error(conn: &mut PooledConn, qid: &u64) -> Option<String> {
        let mut stmt = conn.prepare("SELECT `msg` FROM `queryerror` WHERE `qid`=?").unwrap();
        let mut result = stmt.execute((qid,)).unwrap();
        result.next().unwrap().unwrap().take("msg")
    }
    
    /// Cleans all tables, only for testing
    fn clean_db(conn: &mut PooledConn) {
        use std::env;
        let mut tables: Vec<String> = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT `TABLE_NAME` FROM information_schema.`TABLES` WHERE `TABLE_SCHEMA` = ?").unwrap();
            
            for row in stmt.execute((env::var("db").unwrap(),)).unwrap(){
                let mut row_u = row.unwrap();
                tables.push(row_u.take("TABLE_NAME").unwrap());
            }
        }
        
        conn.prep_exec("SET FOREIGN_KEY_CHECKS=0;",()).unwrap(); // disable key checks, avoiding theoretical problems
        for tbl in tables {
            conn.prep_exec(format!("TRUNCATE `{}`",tbl),()).unwrap();
        }
    }
    
    /// Test wrapper, accepting ReqCore structs, with additional playlist insertion over _insert_query
    fn insert_query_core(req: &lib::ReqCore, conn: &mut PooledConn) -> Result<u64,Error> {
        let qid = try!(super::_insert_query(&req.url, &req.quality,&req.uid, &req.r_type,conn));
        if req.playlist {
            let mut stmt = try!(conn.prepare("INSERT INTO `playlists` (`qid`,`from`,`to`,`split`) VALUES(?,?,?,?)"));
            let _ = try!(stmt.execute((qid,req.from,req.to,req.split)));
        }
        Ok(qid)
    }
    
    /// Set last update check date, used for deletion checks
    fn set_file_created(conn: &mut PooledConn,qid: &u64, date: NaiveDateTime) {
        let mut stmt = conn.prepare("UPDATE files SET `created`= ? WHERE fid = ?").unwrap();
        assert!(stmt.execute((date,qid)).is_ok());
    }
    
    /// Retrieve NaiveDateTime LUC from querydetails
    fn get_luc(conn: &mut PooledConn,qid: &u64) -> NaiveDateTime {
        let mut stmt = conn.prepare("SELECT luc from querydetails WHERE qid = ?").unwrap();
        let mut result = stmt.execute((qid,)).unwrap();
        result.next().unwrap().unwrap().take("luc").unwrap()
    }
    
    /// Get fid,name, r_name of files for qid to test against an insertion
    /// Retrusn an Vec<(fid,name,rname)>
    fn get_files(conn: &mut PooledConn, qid: &u64) -> Vec<(u64,String,String)> {
        let mut stmt = conn.prepare("SELECT files.fid,name, rname FROM files \
            JOIN `query_files` ON files.fid = query_files.fid \
            WHERE query_files.qid = ? ORDER BY fid").unwrap();
        let result = stmt.execute((qid,)).unwrap();
        let a: Vec<(u64,String,String)> = result.map( |row| {
            from_row(row.unwrap())
        }).collect();
        a
    }
    
    #[test]
    fn sql_test() {
        get_db_create_sql();
    }
    
    #[test]
    fn connect_setup_test() {
        let (cfg,pool) = connect();
        setup(&mut pool.get_conn().unwrap());
    }
    
    #[test]
    fn insert_query_test() {
        let (conf,pool) = connect();
        let mut conn = pool.get_conn().unwrap();
        let request = create_request(true, &conf);
        setup(&mut conn);
        insert_query_core(&request, &mut conn).unwrap();
    }
    
    #[test]
    fn file_test() {
        lib::config::init_config();
        
        let (conf,pool) = connect();
        let mut conn = pool.get_conn().unwrap();
        let mut request = create_request(false, &conf);
        setup(&mut conn);
        request.qid = insert_query_core(&request, &mut conn).unwrap();
        
        let f_name = "f_test";
        let f_r_name = "f_r_test";
        let n_fid = add_file_entry(&mut conn, &request.qid, &f_name,&f_r_name).unwrap();
        let (fid,ref retr_name,ref retr_r_name) = get_files(&mut conn,&request.qid)[0];
        assert_eq!(retr_name,f_name);
        assert_eq!(retr_r_name,f_r_name);
        assert_eq!(n_fid,fid);
        assert!(set_file_valid_flag(&mut conn, &fid,false).is_ok());
    }
    
    #[test]
    fn query_delete_test() {
        lib::config::init_config();
        let (conf,pool) = connect();
        let mut conn = pool.get_conn().unwrap();
        setup(&mut conn);
        
        let mut request = create_request(true, &conf);
        request.qid = insert_query_core(&request, &mut conn).unwrap();
        
        let fid = add_file_entry(&mut conn, &request.qid, &"test", &"test").unwrap();
        
        let mut files = Vec::new();
        files.push((fid,"asd".to_string()));
        let mut qids = Vec::new();
        qids.push(request.qid.clone());
        
        delete_requests(&mut conn,qids,files).unwrap();
        
        let SQL = "SELECT COUNT(*) as amount FROM %db% WHERE 1";
        for db in REQ_DB_TABLES.iter() {
            let mut res = conn.prep_exec(SQL.replace("%db%",db),()).unwrap();
            let amount: i32 = res.next().unwrap().unwrap().take("amount").unwrap();
            assert_eq!(amount,0);
        }
    }
    
    #[test]
    fn file_delete_sql_test() {
        lib::config::init_config();
        
        const age: u16 = 60 * 25; // minutes, age subtracted per iter
        const max_age_diff: u16 = age - 10;
        const affected_invalid: i16 = 2; // i count file which will be invalidated
        const AMOUNT_FILES: i16 = 16;
        const AGE_DEL_RATIO: i16 = 50;
        
        let (conf,pool) = connect();
        let mut conn = pool.get_conn().unwrap();
        setup(&mut conn);
        
        let start_time = Local::now();
        let mut requests = Vec::new();
        {
            let mut time = start_time.naive_local();
            let subtr_time = Duration::days(1);
            //let deleteSwitchTime = Duration::days()
            let req_template = create_request(false, &conf);
            
            let treshold = AGE_DEL_RATIO * AMOUNT_FILES / 100;
            let mut amount_flagged_delete = 0;
            
            for i in 0..AMOUNT_FILES { // create AMOUNT_FILES files, affected_invalid of them are marked
                                       // as deleted, AGE_DEL_RATIO of them are marked with the delete flag
                let mut req_new = req_template.clone();
                req_new.qid = insert_query_core(&req_new, &mut conn).unwrap();
                let f_name = format!("f_{}",i);
                let f_r_name = format!("f_r_{}", i);
                let fid = add_file_entry(&mut conn, &req_new.qid, &f_name,&f_r_name).unwrap();
                let delete = amount_flagged_delete < treshold;
                
                let valid = match i == affected_invalid {
                    true => {assert!(set_file_valid_flag(&mut conn, &fid,false).is_ok()); false},
                    false => true,
                };
                set_file_created(&mut conn, &fid,time);
                
                if delete && valid {
                    assert!(set_file_delete_flag(&mut conn, &fid, true).is_ok());
                    amount_flagged_delete += 1;
                }
                
                requests.push((req_new.qid,fid,time.clone(),f_name,f_r_name,valid,delete));
                time = time - subtr_time;
            }
            
            
        }
        
        assert!((Local::now() - start_time).num_milliseconds() < 1_000); // took too long to be accurate at retrieving
        
        { // get aged files-test
        let (qids,files) = get_files_to_delete(&mut conn,DeleteRequestType::AgedMin(&max_age_diff)).unwrap();
        // Vec<u64>,Vec<(u64,String)>
        assert_eq!(files.is_empty(),false);
        for (fid,name) in files { // check file for file that all data is correct
            let mut iter = requests.iter().filter(|&&(_,ref r_fid,_,_,_,_,_)| r_fid == &fid);
            let &(ref r_qid,ref r_fid,ref time,ref f_name,_,ref r_valid,ref r_delete) = iter.next().unwrap();
            assert_eq!(f_name,&name);
            assert_eq!(r_valid,&true);
            assert_eq!(r_fid,&fid);
            let diff = start_time - Duration::minutes(max_age_diff as i64);
            assert!(time <= &diff.naive_local() );
            assert!(qids.contains(&r_qid));
            assert!(iter.next().is_none());
            assert!(set_file_valid_flag(&mut conn, &fid,false).is_ok());
        }
        // re-check that no results remain
        let (qids,files) = get_files_to_delete(&mut conn,DeleteRequestType::AgedMin(&max_age_diff)).unwrap();
        assert!(qids.is_empty());
        assert!(files.is_empty());
        }
        
        { // delete marked test
        let (qids,files) = get_files_to_delete(&mut conn,DeleteRequestType::Marked).unwrap();
        // Vec<u64>,Vec<(u64,String)>
        assert_eq!(files.is_empty(),false);
        for (fid,name) in files { // check file for file that all data is correct
            let mut iter = requests.iter().filter(|&&(_,ref r_fid,_,_,_,_,_)| r_fid == &fid);
            let &(ref r_qid,ref r_fid,ref time,ref f_name,_,ref r_valid,ref r_delete) = iter.next().unwrap();
            assert_eq!(f_name,&name);
            assert_eq!(r_valid,&true);
            assert_eq!(r_delete,&true);
            assert_eq!(r_fid,&fid);
            let diff = start_time - Duration::minutes(max_age_diff as i64);
            assert!(time >= &diff.naive_local() );
            assert!(qids.contains(&r_qid));
            assert!(iter.next().is_none());
            assert!(set_file_valid_flag(&mut conn, &fid,false).is_ok());// set as invalid: deleted
            assert!(set_file_delete_flag(&mut conn, &fid,false).is_ok()); // set to be deleted: false
        }
        // re-check that no results remain
        let (qids,files) = get_files_to_delete(&mut conn,DeleteRequestType::Marked).unwrap();
        assert!(qids.is_empty());
        assert!(files.is_empty());
        }
    }
    
    #[test]
    fn query_test() {
        logger::init_config();
        lib::config::init_config();
        {
            let (conf,pool) = connect();
            let mut conn = pool.get_conn().unwrap();
            let mut request = create_request(false, &conf);
            setup(&mut conn);
            let id = insert_query_core(&request, &mut conn).unwrap();
            request.qid = id;
            
            let out_req = request_entry(conn).unwrap();
            request.verify(&out_req);
        }
        
        {
            let (conf,pool) = connect();
            let mut conn = pool.get_conn().unwrap();
            let mut request = create_request(true, &conf);
            setup(&mut conn);
            let id = insert_query_core(&request, &mut conn).unwrap();
            request.qid = id;
            
            let out_req = request_entry(conn).unwrap();
            request.verify(&out_req);
        }
    }
    
    #[test]
    fn query_update_test(){
        let (conf, pool) = connect();
        let mut conn = pool.get_conn().unwrap();
        
        let request = create_request(false, &conf);
        setup(&mut conn);
        let id = insert_query_core(&request, &mut conn).unwrap();
        
        let new_code = -9;
        let new_state = String::from("asd");
        super::set_query_code(&mut conn, &id, &new_code);
        super::set_query_state(&mut conn, &id, &new_state);
        let (code,progr,state) = get_status(&mut conn,&id);
        assert_eq!(code,new_code);
        assert!(state.is_some());
        assert_eq!(new_state,state.unwrap());
    }
    
    #[test]
    fn add_query_error_test(){
        let (conf, pool) = connect();
        let mut conn = pool.get_conn().unwrap();
        
        let request = create_request(false, &conf);
        setup(&mut conn);
        let id = insert_query_core(&request, &mut conn).unwrap();
        
        let new_error = String::from("asd");
        super::add_query_error(&mut conn, &id, &new_error);
        let error = get_error(&mut conn,&id);
        assert!(error.is_some());
        assert_eq!(new_error,error.unwrap());
    }
}