
extern crate regex;

use std::process::{Command, Stdio, Child};
use std::io::prelude::*;
use std::io::BufReader;
use std::convert::Into;
use std::path::PathBuf;
use std::str;

use lib::DownloadError;

use mysql::conn::pool::{MyPool,MyPooledConn};
use mysql::conn::Stmt;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

/// Converter containing all necessary methods to merge, extract and convert audio with/from video files
pub struct Converter<'a> {
    ffmpeg_dir: PathBuf,
    mp3_quality: &'a i16,
    pool: MyPool,
}

/// Struct containing file information needed for progress calculation
struct FileInfo {
	duration: f32, // seconds
	frames: f32,
}

impl<'a> Converter<'a> {
    pub fn new(ffmpeg_dir: &'a str,mp3_quality: &'a i16, pool: MyPool) -> Converter<'a> {
    	debug!("ffmpeg dir: {}",ffmpeg_dir);
        Converter{ffmpeg_dir: PathBuf::from(ffmpeg_dir),mp3_quality: mp3_quality, pool: pool}
    }

    /// Merge audo & video files to one
    /// As ffmpeg uses \r for progress updates, we'll have to read untill this delimiter
    /// ffmpeg prints only to stderr
    pub fn merge_files(&self, qid: &i64, video_file: &'a str,audio_file: &'a str, output_file: &'a str) -> Result<(), DownloadError>{
        let file_info = try!(self.get_file_info(video_file));
        trace!("Total frames: {}",file_info.frames);

        let mut child = try!(self.run_merge_cmd(audio_file, video_file, output_file));
        trace!("started merge process");
        let mut stdout = BufReader::new(child.stderr.take().unwrap());

        let mut conn = self.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"frame=\s*(\d+)");
		
		let mut buf = vec![];
		buf.reserve(128);
		
		let mut cur_frame: f32;
		
		loop {
			match stdout.read_until(b'\r', &mut buf) {
				Ok(0) => break,
				Ok(_) => {},
				Err(why) => {error!("couldn't read cmd stdout: {}", why); panic!(); }
			}
			
			{
				let line = str::from_utf8(&buf).unwrap();
				debug!("ffmpeg: {}",line);
				if let Some(cap) = re.captures(&line) {
	                debug!("frame: {}", cap.at(1).unwrap());
	                cur_frame = cap.at(1).unwrap().parse::<f32>().unwrap();
	                try!(self.update_progress(&mut statement, format!("{:.2}",(cur_frame / file_info.frames ) * 100.0), qid)
	                );
				}
			}
			
			buf.clear();

		}

        try!(child.wait());

        Ok(())
    }
    
    /// Extract audio from video files
    /// If set audio will be converted to mp3 on the fly
    pub fn extract_audio(&self, qid: &i64, video_file: &'a str, output_file: &'a str, convert_mp3: bool) -> Result<(), DownloadError> {
		let file_info = try!(self.get_file_info(video_file));
		debug!("duration: {}",file_info.duration);
        
        let mut child;
        if convert_mp3 {
            child = try!(self.run_audio_extract_to_mp3(video_file, output_file));
        }else {
            child = try!(self.run_audio_extract(video_file, output_file));
        }
        
        let mut stdout = BufReader::new(child.stderr.take().unwrap());
        
        let mut conn = self.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"time=(\d+):(\d+):(\d+.?\d*)");
        
		let mut buf = vec![];
		buf.reserve(128);
		
		loop {
			match stdout.read_until(b'\r', &mut buf) {
				Ok(0) => break,
				Ok(_) => {},
				Err(why) => {error!("couldn't read cmd stdout: {}", why); panic!(); }
			}
			
			{
				let line = str::from_utf8(&buf).unwrap();
				debug!("ffmpeg: {}",line);
				if let Some(cap) = re.captures(&line) {
					let mut seconds: i32 = cap.at(2).unwrap().parse::<i32>().unwrap() * 60; // minutes
		            seconds += cap.at(1).unwrap().parse::<i32>().unwrap() * 60 * 60 ; // hours
		            
		            let seconds: f32 = seconds as f32 + cap.at(3).unwrap().parse::<f32>().unwrap();
		            try!(self.update_progress(&mut statement, format!("{:.2}",(seconds / file_info.duration ) * 100.0), qid)
	                );
				}
				
			}
			
			buf.clear();

		}

        try!(child.wait());

        Ok(())
    }

    /// Retrive file information
    fn get_file_info(&self, video_file: &str) -> Result<FileInfo,DownloadError> {
        let stdout = try!(self.run_file_probe(video_file));
		
		let regex_duration = regex!(r"duration=(\d+\.?\d*)");
		let regex_fps = regex!(r"r_frame_rate=(\d+)/(\d+)");
		
		if let Some(cap) = regex_duration.captures(&stdout) {
			let mut file_info = FileInfo { duration: -1.0, frames: -1.0 };
			file_info.duration = cap.at(1).unwrap().parse::<f32>().unwrap();
			
			if let Some(cap) = regex_fps.captures(&stdout) {
				let x: f32 = cap.at(1).unwrap().parse::<f32>().unwrap();
				let y: f32 = cap.at(2).unwrap().parse::<f32>().unwrap();
				file_info.frames =  ( x / y ) * file_info.duration;
			}
		
			Ok(file_info)
		}else{
			Err(DownloadError::FFMPEGError(format!("Couldn't get max frames: {}",stdout)))
		}
    }
    
    /// Runs a file probe and returns its output, used in progress calculation
    fn run_file_probe(&self, video_file: &str) -> Result<String, DownloadError> {
    	let mut command = self.create_ffmpeg_base("ffprobe");
    	command.args(&["-select_streams","0"]);
    	command.args(&["-show_entries","stream=duration,r_frame_rate"]);
    	command.args(&["-of","default=noprint_wrappers=1"]);
    	command.arg(video_file);
    	
    	match command.spawn() {
    		Err(why) => Err(why.into()),
            Ok(mut process) => {
            	let mut stdout_buffer = BufReader::new(process.stdout.take().unwrap());
	        	let mut stdout: String = String::new();
	      		try!(stdout_buffer.read_to_string(&mut stdout));
	      		try!(process.wait());
	      		debug!("ffprobe: {}",stdout);
	      		Ok(stdout)
            },
    	}
    }

    /// Merges an audio & an video file together.
    /// ffmpeg uses \r as \n
    fn run_merge_cmd(&self, audio_file: &str, video_file: &str, output_file: &str) -> Result<Child,DownloadError> {
    	let mut command = self.create_ffmpeg_base("ffmpeg");
    	command.args(&["-threads","0"]);
    	command.args(&["-i",video_file]);
    	command.args(&["-i",audio_file]);
    	command.args(&["-map","0"]);
    	command.args(&["-map","1"]);
    	command.args(&["-codec","copy"]);
    	command.arg("-shortest");
    	command.arg(output_file);
    	//-stats -threads 0 -i "{}" -i "{}" -map 0 -map 1 -codec copy -shortest "{}"
    	match command.spawn() {
    		Err(why) => Err(why.into()),
            Ok(process) => Ok(process),
    	}
    }
    
    ///Create a ffmpeg instance with the audio extract cmd
    fn run_audio_extract(&self, video_file: &str, output_file: &str) -> Result<Child, DownloadError> {
        let mut command = self.create_ffmpeg_base("ffmpeg");
        command.args(&["-threads", "0"]);
        command.args(&["-i",video_file]);
        command.args(&["-vn", "-acodec","copy"]);
        command.arg(output_file);
        
        match command.spawn() {
    		Err(why) => Err(why.into()),
            Ok(process) => Ok(process),
    	}
    }
    
    fn run_audio_extract_to_mp3(&self, video_file: &str, output_file: &str) -> Result<Child, DownloadError> {
        let mut command = self.create_ffmpeg_base("ffmpeg");
        command.args(&["-threads", "0"]);
        command.args(&["-i",video_file]);
        command.args(&["-codec:a", "libmp3lame"]);
        command.args(&["-qscale:a",&self.mp3_quality.to_string()]);
        command.arg(output_file);
        
        match command.spawn() {
    		Err(why) => Err(why.into()),
            Ok(process) => Ok(process),
    	}
    }
    
    /// Create FFMPEG basic command
    /// executable is the called ffmpeg binary
    fn create_ffmpeg_base(&self, executable: &'static str) -> Command {
    	let mut cmd = Command::new(self.ffmpeg_dir.join(executable));
    	cmd.args(&["-v","error"]);
    	cmd.stdin(Stdio::null());
    	cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    	cmd
    }

    // MyPooledConn does only live when MyOpts is alive -> lifetime needs to be declared
    //TODO: think about generalizing this, while using the local pool
    fn prepare_progress_updater(&'a self,conn: &'a mut MyPooledConn) -> Stmt<'a> { // no livetime needed: struct livetime used
        conn.prepare("UPDATE querydetails SET progress = ? WHERE qid = ?").unwrap()
    }

    ///updater called from the stdout progress
    fn update_progress(&self,stmt: &mut Stmt, progress: String, qid: &i64) -> Result<(),DownloadError>{
    	trace!("updating progress {}",progress);
        try!(stmt.execute((&progress,qid)).map(|_| Ok(())))
    }

}