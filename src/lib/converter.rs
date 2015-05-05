
extern crate regex;

use std::process::{Command, Stdio, Child};
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;
use std::convert::Into;

use lib::downloader::DownloadError;

use mysql::conn::pool::{MyPool,MyPooledConn};
use mysql::conn::Stmt;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

pub struct Converter<'a> {
    pub ffmpeg_cmd: &'a str,
    pub audio_quality: &'a i16,
    pub pool: MyPool,
}

impl<'a> Converter<'a> {
    pub fn new(ffmpeg_cmd: &'a str, audio_quality: &'a i16, pool: MyPool) -> Converter<'a> {
        Converter{ffmpeg_cmd: ffmpeg_cmd, audio_quality: audio_quality, pool: pool}
    }

    ///Merge audo & video file to one, using ffmpeg, saving directly at the dest. folder
    pub fn merge_files(&self, qid: &i64, video_file: &'a str,audio_file: &'a str, output_file: &'a str) -> Result<(), DownloadError>{
        let max_frames: i64 = try!(self.get_max_frames(video_file));
        println!("Total frames: {}",max_frames);

        let process = try!(self.create_merge_cmd(audio_file, video_file, output_file));
        let stdout = BufReader::new(process.stdout.unwrap());
        let mut stderr_buffer = BufReader::new(process.stderr.unwrap());

        let mut conn = self.pool.get_conn().unwrap();
        let mut statement = self.prepare_progress_updater(&mut conn);
        let re = regex!(r"(frame=\s*(\d+))");

        for line in stdout.lines(){
            match line{
                Err(why) => panic!("couldn't read cmd stdout: {}", Error::description(&why)),
                Ok(text) => {
                        println!("Out: {}",text);
                        if re.is_match(&text) {
                            let cap = re.captures(&text).unwrap();
                            println!("frame: {}", cap.at(2).unwrap());
                            try!(self.update_progress(&mut statement, self.caclulate_progress(&max_frames, &cap.at(2).unwrap()).to_string(), qid));
                        }
                    },
            }
        }

        let mut stderr: String = String::new();
        try!(stderr_buffer.read_to_string(&mut stderr));
        println!("Stderr: {}", stderr);

        Ok(())
    }

    fn caclulate_progress<'b>(&self, max_frames: &i64, current_frame: &str) -> i64 {
        let frame = current_frame.parse::<i64>().unwrap();
        frame / max_frames * 100
    }

    ///retrives the max frames from a video file, needed a percentual progress calculation
    fn get_max_frames(&self, video_file: &str) -> Result<i64,DownloadError> {
        let process = try!(self.create_fps_get_cmd(video_file));
        let mut stdout_buffer = BufReader::new(process.stdout.unwrap());
        let mut stdout: String = String::new();
        try!(stdout_buffer.read_to_string(&mut stdout));
        println!("total frames stdout: {:?}", stdout.trim());
        match stdout.trim().parse::<i64>() {
            Ok(v) => Ok(v),
            Err(why) => Err(DownloadError::FFMPEGError(format!("Couldn't get max frames {}",stdout))),
        }
    }

    ///Merges an audio & an video file together.
    ///Due to ffmpeg not giving out new lines we need to use tr, till the ffmpeg bindings are better
    ///This removes the option to use .arg() -> params must be handled carefully
    fn create_merge_cmd(&self, audio_file: &str, video_file: &str, output_file: &str) -> Result<Child,DownloadError> {
        self.create_bash_cmd(self.format_ffmpeg_cmd(audio_file, video_file, output_file))
    }

    ///Creates a cmd to gain the amount of frames in a video, for progress calculation
    fn create_fps_get_cmd(&self, video_file: &str) -> Result<Child, DownloadError> {
        self.create_bash_cmd(self.format_frame_get_cmd(video_file))
    }

    ///Create an bash cmd
    fn create_bash_cmd(&self, cmd: String) -> Result<Child, DownloadError> {
        match Command::new("bash")
                            .arg("-c")
                            .arg(cmd)
                            .stdin(Stdio::null())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .spawn() {
                Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
                Ok(process) => Ok(process),
        }
    }

    ///Formats a command to gain the total amount of frames in a video file
    ///which will be used for the progress calculation
    fn format_frame_get_cmd(&self, video_file: &str) -> String {
        let a = format!(r#"{} -i {} -vcodec copy -acodec copy -f null /dev/null 2>&1 | grep 'frame=' | cut -f 2 -d ' '"#,self.ffmpeg_cmd, video_file);
        println!("ffmpeg-fps cmd: {}", a);
        a
    }

    ///Creates a ffmpeg_cmd containing the path to ffmpeg, as defined in the config
    ///and all the needed arguments, which can't be set using .arg, see create_merge_cmd.
    fn format_ffmpeg_cmd(&self, audio_file: &str, video_file: &str, output_file: &str) -> String {
        let a = format!(r#"{} -stats -threads 0 -i "{}" -i "{}" -map 0 -map 1 -codec copy -shortest "{}" 2>&1 |& tr '\r' '\n'"#,
            self.ffmpeg_cmd,
            video_file,
            audio_file,
            output_file);
        println!("ffmpeg cmd: {}", a);
        a
    }

    // MyPooledConn does only live when MyOpts is alive -> lifetime needs to be declared
    //TODO: think about generalizing this, while using the local pool
    fn prepare_progress_updater(&'a self,conn: &'a mut MyPooledConn) -> Stmt<'a> { // no livetime needed: struct livetime used
        conn.prepare("UPDATE querydetails SET progress = ? WHERE qid = ?").unwrap()
    }

    ///updater called from the stdout progress
    fn update_progress(&self,stmt: &mut Stmt, progress: String, qid: &i64) -> Result<(),DownloadError>{
        try!(stmt.execute(&[&progress,qid]).map(|_| Ok(())))
    }
}