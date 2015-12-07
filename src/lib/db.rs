use mysql::conn::MyOpts;
use mysql::conn::pool;
use mysql::conn::pool::{MyPool};
use mysql::value::from_value;

use std::thread::sleep_ms;

use lib::{DownloadError};
use std::error::Error;
use lib::downloader::DownloadDB;
use CONFIG;

///Move result value out, return with none on err & print
macro_rules! try_reoption { ($e:expr) => (match $e { Ok(x) => x, Err(e) => {warn!("{}",e);return None; },}) }
macro_rules! try_return { ($e:expr) => (match $e { Ok(x) => x, Err(e) => {warn!("{}",e);return; },}) }

macro_rules! try_option { ($e:expr) => (match $e { Some(x) => x, None => return None }) }

/// Connect to DBMS, retry on failure.
pub fn db_connect(opts: MyOpts, sleep_time: u32) -> MyPool { 
    loop {
        match pool::MyPool::new(opts.clone()) {
            Ok(conn) => {return conn;},
            Err(err) => error!("Unable to establish a connection: {}",err),
        };
        sleep_ms(sleep_time);
    }
}

/// Set state of query
pub fn set_query_state(pool: & pool::MyPool,qid: &i64 , state: &str, finished: bool){ // same here
    let progress: i32 = if finished {
        100
    }else{
        0
    };
    let mut conn = try_return!(pool.get_conn());
    let mut stmt = try_return!(conn.prepare("UPDATE querydetails SET status = ? , progress = ? WHERE qid = ?"));
    let result = stmt.execute((&state,&progress,qid)); // why is this var needed ?!
    match result {
        Ok(_) => (),
        Err(why) => error!("Error setting query state: {}",why),
    }
}

/// Set state of query to null
///
/// Saves table space for finished downloads
pub fn set_null_state(pool: & MyPool, qid: &i64){
    let mut conn = try_return!(pool.get_conn());
	let mut stmt = try_return!(conn.prepare("UPDATE querydetails SET status = NULL WHERE qid = ?"));
	let result = stmt.execute((qid,));
	match result {
	    Ok(_) => (),
	    Err(why) => error!("Error setting query sate: {}", why),
	}
}

/// Update query status code
/// Affecting querydetails.code
pub fn set_query_code(pool: & MyPool, code: &i8, qid: &i64) -> Result<(), DownloadError> { // same here
	let mut conn = try!(pool.get_conn());
    let mut stmt = conn.prepare("UPDATE querydetails SET code = ? WHERE qid = ?").unwrap();
    let result = stmt.execute((&code,&qid));
    match result {
        Ok(_) => Ok(()),
        Err(why) => Err(DownloadError::DBError(why.description().into())),
    }
}

/// Update progress steps for db entrys
pub fn update_steps(pool: & pool::MyPool, qid: &i64, step: i32, max_steps: i32, finished: bool){
    set_query_state(&pool,qid, &format!("{}|{}", step, max_steps), finished);
}

/// Add file to db including it's name & fid based on the qid
pub fn add_file_entry(pool: & MyPool, fid: &i64, name: &str, real_name: &str) -> Result<(), DownloadError> {
    trace!("name: {}",name);
    let mut conn = try!(pool.get_conn());
    let mut stmt = conn.prepare("INSERT INTO files (fid,rname,name,valid) VALUES (?,?,?,?)").unwrap();
    try!(stmt.execute((fid,&real_name,&name,&1))); // why is this var needed ?!
	Ok(())
}

/// Add query status msg for error reporting
pub fn add_query_status(pool: & MyPool, qid: &i64, status: &str){
    let mut conn = try_return!(pool.get_conn());
    let mut stmt = conn.prepare("INSERT INTO querystatus (qid,msg) VALUES (?,?)").unwrap();
    let result = stmt.execute((&qid,&status));
    match result {
        Ok(_) => (),
        Err(why) => error!("Error inserting querystatus: {}",why),
    }
}

/// Request an entry from the DB to handle
pub fn request_entry(pool: & MyPool) -> Option<DownloadDB> {
    let mut conn = try_reoption!(pool.get_conn());
    let mut stmt = try_reoption!(conn.prepare("SELECT queries.qid,url,`type`,quality,zip,`from`,`to` FROM querydetails \
                    INNER JOIN queries \
                    ON querydetails.qid = queries.qid \
                    LEFT JOIN playlists ON queries.qid = playlists.qid \
                    WHERE querydetails.code = -1 \
                    ORDER BY queries.created \
                    LIMIT 1"));
    let mut result = try_reoption!(stmt.execute(()));
    let result = try_reoption!(try_option!(result.next())); // result.next().'Some'->value.'unwrap'
    
    trace!("Result: {:?}", result[0]);
    trace!("result str: {}", result[1].into_str());
    let from;
    let to;
    let compress;
    let playlist = from_value::<Option<i16>>(result[4].clone()).is_some();
    if playlist {
        compress = from_value::<Option<i16>>(result[4].clone()).unwrap() == 1;
        from = from_value::<Option<i32>>(result[5].clone()).unwrap();
        to = from_value::<Option<i32>>(result[6].clone()).unwrap();
    } else {
        from = 0;
        to = 0;
        compress = false;
    }
    let download_db = DownloadDB { url: from_value::<String>(result[1].clone()),
                                    quality: from_value::<i16>(result[3].clone()),
                                    qid: from_value::<i64>(result[0].clone()),
                                    folder: CONFIG.general.temp_dir.clone(),
                                    pool: pool,
                                    playlist: playlist,
                                    compress: compress,
                                    to: to,
                                    from: from,
                                    source_type: from_value::<i16>(result[2].clone()) };
    Some(download_db)
}

/// Set DBMS connection settings
pub fn mysql_options() -> MyOpts {
    MyOpts {
        tcp_addr: Some(CONFIG.db.ip.clone()),
        tcp_port: CONFIG.db.port,
        user: Some(CONFIG.db.user.clone()),
        pass: Some(CONFIG.db.password.clone()),
        db_name: Some(CONFIG.db.db.clone()),
        ..Default::default() // set others to default
    }
}