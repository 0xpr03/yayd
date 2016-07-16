extern crate regex;

use mysql::conn::pool::{PooledConn,Pool};
use mysql::conn::{Opts, OptsBuilder};
use mysql::conn::Stmt;
use mysql::value::Value;

use std::time::Duration;
use std::cell::RefCell;
use std::thread::sleep;
use std::path::PathBuf;
use std::boxed::Box;

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

const DEFAULT_PLAYLIST_VAL: i16 = -2;

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
pub fn add_file_entry(conn: &mut PooledConn, qid: &u64, name: &str, real_name: &str) -> Result<(), Error> {
    trace!("name: {}",name);
    let fid: u64;
    {
        let mut stmt = conn.prepare("INSERT INTO files (rname,name,valid) VALUES (?,?,?)").unwrap();
        let result = try!(stmt.execute((&real_name,&name,&1)));
        fid = result.last_insert_id();
    }
    {
        if CONFIG.general.link_files {
            let mut stmt = try!(conn.prepare("INSERT INTO `query_files` (qid,fid) VALUES(?,?)"));
               try!(stmt.execute((&qid,&fid)));
        }
    }
    Ok(())
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
    
    if CONFIG.general.link_suberqueries {
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
        INNER JOIN queries \
        ON querydetails.qid = queries.qid \
        "SELECT queries.qid,url,quality,`split`,`from`,`to`,uid,`type` FROM queries \
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

/// Cleans all tables, only for testing
#[cfg(test)]
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
        println!("tbl: {}",tbl);
        conn.prep_exec(format!("TRUNCATE `{}`",tbl),()).unwrap();
    }
}

/// Setup tables
/// Create as temporary if specified
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
fn get_db_create_sql<'a>() -> Vec<String> {
    let raw_sql = include_str!("../../setup.sql");
    
    let reg = regex::Regex::new(r"(/\*(.|\s)*?\*/)").unwrap(); // https://regex101.com/r/bG6aF2/6, replace `\/` with `/`
    let raw_sql: String = reg.replace_all(raw_sql, "");
    
    let raw_sql = raw_sql.replace("\n","");
    let raw_sql = raw_sql.replace("\r","");
    
    debug!("\n\nSQL: {}\n\n",raw_sql);
    
    let mut split_sql:Vec<String> = raw_sql.split(";").filter_map(|x| // split at `;`, filter_map on iterator
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
    use super::{clean_db,DEFAULT_PLAYLIST_VAL,get_db_create_sql};
    use std::cell::RefCell;
    use mysql::conn::pool::{PooledConn,Pool};
    use mysql;
    
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
    
    /// Test wrapper, accepting ReqCore structs
    fn insert_query_core(req: &lib::ReqCore, conn: &mut PooledConn) -> Result<u64,Error> {
        let qid = try!(super::_insert_query(&req.url, &req.quality,&req.uid, &req.r_type,conn));
        if req.playlist {
            let mut stmt = try!(conn.prepare("INSERT INTO `playlists` (`qid`,`from`,`to`,`split`) VALUES(?,?,?,?)"));
            let _ = try!(stmt.execute((qid,req.from,req.to,req.split)));
        }
        Ok(qid)
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
        let request = create_request(false, &conf);
        setup(&mut conn);
        insert_query_core(&request, &mut conn).unwrap();
    }
    
    #[test]
    fn query_test() {
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