
use lib::db;
use mysql::conn::pool::{MyPool};

/// Status update controller for handles
pub struct Status<'a> {
    pool: &'a MyPool,
    ignore_updates: bool,
    qid: &'a i64,
}

impl<'a> Status<'a> {
    /// Initialize a new status controller
    /// If ignoreUpdates is set, all update commands will be ignored.
    /// This is useful when a sub part of a playlist download is started, which shouldn't
    /// interfere with the status
    pub fn new(pool: &'a MyPool, ignore_updates: bool, qid: &'a i64) -> Status<'a>{
        Status { pool: pool, ignore_updates: ignore_updates, qid: qid}
    }
    
    /// Set status show next to the progress percentage
    pub fn set_status(&self, state: &str, finished: bool){
        if !self.ignore_updates {
       		db::set_query_state(self.pool, self.qid, state, finished);
        }
    }
    
    /// Set the status code, defining some fixed states
    pub fn set_status_code(&self, code: &i8){
        if !self.ignore_updates {
            db::set_query_code(self.pool, code, self.qid);
        }
    }
    
    /// Set status with step/maxsteps
    pub fn set_status_int(&self, step: i32, max_steps: i32){
        if !self.ignore_updates {
            db::update_steps(self.pool, self.qid, step, max_steps, false);
        }
    }
}