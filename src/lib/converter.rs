
extern crate regex;

use std::process::{Command, Stdio, Child};
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;
use std::io;
use std::ascii::AsciiExt;
use lib::config::ConfigGen;
use std::convert::Into;
use lib::downloader::DownloadError;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

pub struct Converter {
    pub ffmpeg_cmd: String,
    pub audio_quality: i32,
}

impl Converter {
    pub fn new(ffmpeg_cmd: String, audio_quality: i32) -> Converter {
        Converter{ffmpeg_cmd: ffmpeg_cmd, audio_quality: audio_quality}
    }
    ///Merges an audio & an video file together.
    ///Due to ffmpeg not giving out new lines we need to use tr, till the ffmpeg bindings are better
    ///This removes the option to use .arg() -> params must be handled carefully
    pub fn create_merge_cmd<'a>(&self, audio_file: &'a str, video_file: &'a str, output_file: &'a str) -> Result<Child,DownloadError> {
        match Command::new(self.create_ffmpeg_cmd(&audio_file, &video_file, &output_file))
                                        .stdin(Stdio::null())
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn() {
                Err(why) => Err(DownloadError::InternalError(Error::description(&why).into())),
                Ok(process) => Ok(process),
        }
    }

    ///Create a ffmpeg_cmd containing the path to ffmpeg, as defined in the config
    ///and all the needed arguments, which can't be set using .arg, see create_merge_cmd.
    fn create_ffmpeg_cmd(&self, audio_file: &str, video_file: &str, output_file: &str) -> String {
        format!(r#"{} -threads 0 -i "{}" -i "{}" -map 0 -map 1 -codec copy -shortest "{}" |& tr '\r' '\n'"#,
            self.ffmpeg_cmd,
            video_file,
            audio_file,
            output_file)
    }
}