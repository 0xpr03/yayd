
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

pub struct Converter<'a> {
    ffmpeg_dir: PathBuf,
    mp3_quality: &'a i16,
    pool: MyPool,
}

impl<'a> Converter<'a> {
    pub fn new(ffmpeg_dir: &'a str,mp3_quality: &'a i16, pool: MyPool) -> Converter<'a> {
    	debug!("ffmpeg dir: {}",ffmpeg_dir);
        Converter{ffmpeg_dir: PathBuf::from(ffmpeg_dir),mp3_quality: mp3_quality, pool: pool}
    }

    /// Merge audo & video files to one
    /// As ffmpeg uses \r for progress updates, we'll have to read untill this delimiter
    /// ffmpeg prints only to stderr
    pub fn merge_files(&self, qid: &i64, video_file: &'a str,audio_file: &'a str, output_file: &'a str, show_progress: bool) -> Result<(), DownloadError>{
        trace!("progress {}",show_progress);
        let max_frames: f64 = try!(self.get_max_frames(video_file));
        trace!("Total frames: {}",max_frames);

        let mut child = try!(self.create_merge_cmd(audio_file, video_file, output_file));
        trace!("started merge process");
        let mut stdout = BufReader::new(child.stderr.take().unwrap());

        let mut conn = self.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"frame=\s*(\d+)");
		
		let mut buf = vec![];
		buf.reserve(128);
		
		let mut cur_frame: f64;
		
		loop {
			match stdout.read_until(b'\r', &mut buf) {
				Ok(0) => break,
				Ok(_) => {},
				Err(why) => {error!("couldn't read cmd stdout: {}", why); panic!(); }
			}
			
			{
				let line = str::from_utf8(&buf).unwrap();
				debug!("ffmpeg: {}",line);
				if re.is_match(&line) {
					let cap = re.captures(&line).unwrap();
	                debug!("frame: {}", cap.at(1).unwrap());
	                cur_frame = cap.at(1).unwrap().parse::<f64>().unwrap();
	                try!(self.update_progress(&mut statement, format!("{:.2}",(cur_frame / max_frames) * 100.0), qid)
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
    pub fn extract_audio(&self, video_file: &'a str, output_file: &'a str, convert_mp3: bool) -> Result<(), DownloadError> {
//        let duration = 
        
        let mut child;
        if convert_mp3 {
            child = try!(self.create_audio_extrac_mp3_convert_cmd(video_file, output_file));
        }else {
            child = try!(self.create_audio_extract_cmd(video_file, output_file));
        }
        
        let mut stdout = BufReader::new(child.stderr.take().unwrap());
        
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
				
			}
			
			buf.clear();

		}

        try!(child.wait());

        Ok(())
    }

    /// Retrive the max frames from video file for percentual progress calculation
    fn get_max_frames(&self, video_file: &str) -> Result<f64,DownloadError> {
        let mut child = try!(self.create_fps_get_cmd(video_file));
        let mut stdout_buffer = BufReader::new(child.stderr.take().unwrap());
        let mut stdout: String = String::new();
        try!(stdout_buffer.read_to_string(&mut stdout));
        println!("total frames stdout: {:?}", stdout.trim());
        try!(child.wait());
		
        let regex_duration = regex!(r"Duration: ((\d\d):(\d\d):(\d\d)\.\d\d)");
        let regex_fps = regex!(r"(\d+)\sfps");

        if regex_duration.is_match(&stdout) && regex_fps.is_match(&stdout) {
            let cap_fps = regex_fps.captures(&stdout).unwrap();
            
            let cap_duration = regex_duration.captures(&stdout).unwrap();
            trace!("Found duration: {}",cap_duration.at(0).unwrap());
            let fps = cap_fps.at(1).unwrap().parse::<i64>().unwrap();
            let mut seconds = cap_duration.at(4).unwrap().parse::<i64>().unwrap();
            seconds += cap_duration.at(3).unwrap().parse::<i64>().unwrap() * 60;
            seconds += cap_duration.at(2).unwrap().parse::<i64>().unwrap() * 60 * 60 ;
            Ok((seconds * fps) as f64)
        }else{
            Err(DownloadError::FFMPEGError(format!("Couldn't get max frames {}",stdout)))
        }
    }

    /// Merges an audio & an video file together.
    /// ffmpeg uses \r as \n
    fn create_merge_cmd(&self, audio_file: &str, video_file: &str, output_file: &str) -> Result<Child,DownloadError> {
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

    ///Creates a cmd to gain the amount of frames in a video, used in progress calculation
    fn create_fps_get_cmd(&self, video_file: &str) -> Result<Child, DownloadError> {
    	let mut command = self.create_ffmpeg_base("ffprobe");
    	command.args(&["-i",video_file]);
    	
    	match command.spawn() {
    		Err(why) => Err(why.into()),
            Ok(process) => Ok(process),
    	}
    }
    
    ///Create a ffmpeg instance with the audio extract cmd
    fn create_audio_extract_cmd(&self, video_file: &str, output_file: &str) -> Result<Child, DownloadError> {
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
    
    fn create_audio_extrac_mp3_convert_cmd(&self, video_file: &str, output_file: &str) -> Result<Child, DownloadError> {
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
    	cmd.stdin(Stdio::null());
    	cmd.stdout(Stdio::null());
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