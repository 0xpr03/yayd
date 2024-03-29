extern crate regex;
use mysql::prelude::Queryable;
use mysql::PooledConn;
use mysql::Statement;

use std::convert::Into;
use std::io::prelude::*;
use std::io::BufReader;
#[cfg(not(target_os = "windows"))]
use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use std::sync::RwLock;

use crate::lib::config::ConfigGen;
use crate::lib::db::prep_progress_updater;
use crate::lib::Request;
use crate::lib::{Error, Result};

use crate::lib;

const UPDATE_VERSION_URL: &'static str =
    "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest"; // youtube-dl version check url
const UPDATE_SHA256_FILE: &'static str = "SHA2-256SUMS";
#[cfg(not(target_os = "windows"))]
const YTDL_NAME: &'static str = "yt-dlp"; // name of the python program file
#[cfg(target_os = "windows")]
const YTDL_NAME: &'static str = "yt-dlp.exe"; // name of the python program file
const UPDATE_ASSET_NAME: &'static str = YTDL_NAME;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

#[derive(Debug)]
pub struct Version {
    version: String,
    sha256: String,
    url: String,
}

lazy_static! {
    // [download]  13.4% of 275.27MiB at 525.36KiB/s ETA 07:52
    // we need to remove the / escaping!
    pub static ref REGEX_NAME: regex::Regex = regex!(r"(.*)\.([a-zA-Z0-9]+)\z");
    pub static ref REGEX_PROGRESS: regex::Regex = regex!(r"(\d+\.\d)%");
}

pub struct Downloader {
    defaults: &'static ConfigGen,
    lock: RwLock<()>,
    cmd_path: PathBuf,
}

#[derive(Deserialize)]
pub struct PlaylistEntry {
    pub url: String
}

/// Filename and extension storage
pub struct Filename {
    pub name: String,
    pub extension: String,
}

impl Filename {
    pub fn full_name(&self) -> String {
        format!("{}.{}", &self.name, &self.extension)
    }
}

impl Downloader {
    pub fn new(defaults: &'static ConfigGen) -> Downloader {
        Downloader {
            defaults: defaults,
            lock: RwLock::new(()),
            cmd_path: PathBuf::from(&defaults.youtube_dl_dir),
        }
    }

    /// Run a self-test checking for either yt-dl binaries or update failure
    /// depending on the config
    /// Returns true on success
    pub fn startup_test(&self) -> bool {
        info!("Testing yt-dl settings");
        let ytdl_path = self.ytdl_path();
        if ytdl_path.exists() {
            if let Err(e) = Self::check_ytdl_perm(&ytdl_path) {
                error!("Can't set required permissions on {:?}: {:?}", ytdl_path, e);
                return false;
            }
        } else {
            trace!("Ytdl not found.")
        }
        if self.defaults.youtube_dl_auto_update {
            match self.update_downloader() {
                Ok(_) => true,
                Err(e) => {
                    error!("Failed updating yt-dl {:?}", e);
                    false
                }
            }
        } else {
            match self.version() {
                Ok(_) => true,
                Err(e) => {
                    error!("Failed retrieving version of yt-dl {:?}", e);
                    false
                }
            }
        }
    }

    /// Returns the version
    /// Does not check for the guard!
    pub fn version(&self) -> Result<String> {
        trace!("Checking own version");
        let result = self.ytdl_base().arg("--version").output()?;
        if result.status.success() {
            Ok(String::from_utf8_lossy(&result.stdout).trim().to_string())
        } else {
            Err(Error::InternalError("Process errored".into()))
        }
    }

    /// Downloads the requested file.
    /// file_path specifies the download location.
    /// DMCA errors will get thrown.
    /// download_audio option: ignore the specified quality & download CONFIG.codecs.yt.audio_normal quality for split containers
    fn download_file_in(&self, request: &Request, file_path: &Path, quality: &str) -> Result<bool> {
        trace!("{:?}", request.url);

        trace!("quality: {}", quality);
        let mut child = self.run_download_process(file_path, &request.url, quality)?;
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let mut conn = request.get_conn();
        let statement = prep_progress_updater(&mut conn)?;

        for line in stdout.lines() {
            match line {
                Err(why) => {
                    error!("couldn't read cmd stdout: {}", why);
                    panic!();
                }
                Ok(text) => {
                    trace!("Out: {}", text);
                    match REGEX_PROGRESS.captures(&text) {
                        Some(cap) => {
                            //println!("Match at {}", s.0);
                            debug!("{}", cap.get(1).unwrap().as_str()); // ONLY with ASCII chars makeable!
                            self.update_progress(
                                &request.qid,
                                &mut conn,
                                &statement,
                                cap.get(1).unwrap().as_str(),
                            )?;
                        }
                        None => (),
                    }
                }
            }
        }

        child.wait()?; // waits for finish & then exists zombi process, fixes #10

        let mut stderr: String = String::new();
        stderr_buffer.read_to_string(&mut stderr)?;

        if stderr.is_empty() {
            Ok(true)
        } else if stderr.contains("requested format not available") {
            Err(Error::QualityNotAvailable)
        } else if stderr.contains("ExtractorError") {
            Err(Error::ExtractorError)
        } else {
            warn!("Unknown error at download");
            Err(Error::InternalError(stderr))
        }
    }

    /// Wrapper for download_file_fn to retry on Extract Error's, which are appearing randomly.
    pub fn download_file(
        &self,
        request: &Request,
        file_path: &Path,
        quality: &str,
    ) -> Result<bool> {
        let _guard = self.lock.read()?;
        for attempts in 0..2 {
            match self.download_file_in(&request, file_path, quality) {
                Ok(v) => return Ok(v),
                Err(e) => match e {
                    Error::ExtractorError => warn!("download try no {}", attempts),
                    _ => return Err(e),
                },
            }
        }
        Err(Error::ExtractorError)
    }

    /// Trys to get the original name of a file, while checking for availability
    /// As an ExtractError can appear randomly, bug 11, we're retrying again 2 times if it should occour
    /// Through specifying a quality it's possible to get extension specific for the format.
    pub fn get_file_name(&self, url: &str, quality: Option<String>) -> Result<Filename> {
        let _guard = self.lock.read()?;
        for attempts in 0..2 {
            let mut child = self.run_filename_process(url, quality.as_ref())?;
            let mut stdout_buffer = BufReader::new(child.stdout.take().unwrap());
            let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

            let mut stdout: String = String::new();
            stdout_buffer.read_to_string(&mut stdout)?;
            let mut stderr: String = String::new();
            stderr_buffer.read_to_string(&mut stderr)?;

            child.wait()?;
            let capture = REGEX_NAME.captures(&stdout.trim());
            if stderr.is_empty() && capture.is_some() {
                let caps = capture.unwrap();
                debug!("get_file_name: {:?}", stdout);
                return Ok(Filename {
                    name: caps[1].to_string(),
                    extension: caps[2].to_string(),
                });
            } else {
                if stderr.contains("not available in your country")
                    || stderr.contains("contains content from")
                    || stderr.contains("This video is available in")
                {
                    return Err(Error::DMCAError);
                } else if stderr.contains("Please sign in to view this video") {
                    return Err(Error::NotAvailable);
                } else if stderr.contains("ExtractorError") {
                    // #11
                    info!("ExtractorError on attempt {}", attempts + 1);
                } else {
                    return Err(Error::DownloadError(stderr));
                }
            }
        }
        Err(Error::ExtractorError)
    }

    /// Gets the playlist ids needed for furture download requests.
    /// The output is a vector of IDs
    pub fn get_playlist_entries(&self, request: &Request) -> Result<Vec<PlaylistEntry>> {

        let _guard = self.lock.read()?;
        let mut child = self.run_playlist_extract(request)?;
        trace!("retrieving playlist ids");
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut stderr_buffer = BufReader::new(child.stderr.take().unwrap());

        let re = regex!(r#""url": "([a-zA-Z0-9_-]+)""#);

        let mut entries: Vec<PlaylistEntry> = Vec::new();
        for line in stdout.lines() {
            match line {
                Err(why) => {
                    error!("couldn't read cmd stdout: {}", why);
                    panic!();
                }
                Ok(text) => {
                    match serde_json::from_str::<PlaylistEntry>(&text) {
                        Ok(entry) => {
                            trace!("Found list entry url {}",entry.url);
                            entries.push(entry);
                        },
                        Err(e) => {
                            warn!("Failed to parse playlist-extractor line: {}",e);
                            warn!("Reponse from ytdl: {}", text);
                        },
                    }
                }
            }
        }

        let mut stderr: String = String::new();
        stderr_buffer.read_to_string(&mut stderr)?;

        child.wait()?;

        if !stderr.is_empty() {
            warn!("stderr: {:?}", stderr);
            return Err(Error::InternalError(stderr));
        }

        Ok(entries)
    }

    /// Retrives the playlist name, will kill the process due to yt-dl starting detailed retrieval afterwards.
    pub fn get_playlist_name(&self, url: &str) -> Result<String> {
        let _guard = self.lock.read()?;
        let mut child = self.run_playlist_get_name(url)?;
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let re = regex!(r"\[download\] Downloading playlist: (.*)");

        let name: String;
        for line in stdout.lines() {
            match line {
                Err(why) => {
                    error!("couldn't read cmd stdout: {}", why);
                    panic!();
                }
                Ok(text) => {
                    println!("Out: {}", text);
                    match re.captures(&text) {
                        Some(cap) => {
                            trace!("{}", cap.get(1).unwrap().as_str()); // ONLY with ASCII chars makeable!
                            name = cap.get(1).unwrap().as_str().to_string();
                            child.wait()?;
                            trace!("done");
                            return Ok(name);
                        }
                        None => (),
                    }
                }
            }
        }

        child.wait()?; // waits for finish & then exists zombi process fixes #10

        Err(Error::DownloadError("no playlist name".to_string()))
    }

    /// This function does a 3rd party binding in case it's needed
    /// due to the country restrictions
    /// The returned value has to contain the original video name, the lib has to download & save
    /// the file to the given location
    pub fn lib_request_video(
        &self,
        current_steps: i32,
        max_steps: i32,
        file_path: &Path,
        request: &Request,
        quality: &str,
        get_video: bool,
    ) -> Result<Filename> {
        let _guard = self.lock.read()?;
        let mut child = self.lib_request_video_cmd(&request.url, file_path, quality, get_video)?;
        trace!("Requesting video via lib..");
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or(Error::InternalError("stdout socket error!".into()))?,
        );
        let mut stderr_buffer = BufReader::new(
            child
                .stderr
                .take()
                .ok_or(Error::InternalError("stderr socket error".into()))?,
        );

        let re = regex!(r"step (\d)");

        let mut last_line = String::new();
        for line in stdout.lines() {
            match line {
                Err(why) => {
                    error!("couldn't read cmd stdout: {}", why);
                    panic!();
                } // we'll abort, kinda the floor vanishing under the feet
                Ok(text) => {
                    trace!("Out: {}", text);
                    match re.captures(&text) {
                        Some(cap) => {
                            debug!("Match: {}", cap.get(1).unwrap().as_str()); // ONLY with ASCII chars makeable!
                            if !request.playlist {
                                lib::db::update_steps(
                                    &mut request.get_conn(),
                                    &request.qid,
                                    current_steps
                                        + &cap.get(1).unwrap().as_str().parse::<i32>().unwrap(),
                                    max_steps,
                                );
                            }
                        }
                        None => last_line = text.clone(),
                    }
                }
            }
        }

        trace!("reading stderr");

        let mut stderr: String = String::new();
        stderr_buffer.read_to_string(&mut stderr)?;

        child.wait()?;

        if !stderr.is_empty() {
            warn!("stderr: {:?}", stderr);
            return Err(Error::InternalError(stderr));
        }
        //this ONLY works because `filename: ` is ASCII..
        let mut out = last_line[last_line.find("filename: ").unwrap() + 9..]
            .trim()
            .to_string();
        out = lib::url_sanitize(&out);
        if let Some(caps) = REGEX_NAME.captures(&out) {
            Ok(Filename {
                name: caps[1].to_string(),
                extension: caps[2].to_string(),
            })
        } else {
            return Err(Error::InternalError(format!("no name match! {}", out)));
        }
    }

    /// Provides the base of the youtube-dl command
    fn ytdl_base(&self) -> Command {
        let mut cmd = Command::new(self.cmd_path.join(YTDL_NAME));
        cmd.current_dir(&self.defaults.youtube_dl_dir);
        cmd
    }

    /// Formats the download command.
    fn run_download_process(&self, file_path: &Path, url: &str, quality: &str) -> Result<Child> {
        Ok(self
            .ytdl_base()
            .arg("--newline")
            .arg("--no-warnings")
            .args(&["-r", &format!("{}M", self.defaults.download_mbps / 8)]) // yt-dl uses MB/s, we're using MBit/s
            .args(&["-f", &quality.to_string()])
            .arg("-o")
            .arg(file_path)
            .arg("--hls-prefer-native") // this is needed for twitch extraction
            .args(&["--ffmpeg-location", &self.defaults.ffmpeg_bin_dir]) // backup if internal converter fails
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?)
    }

    /// Runs the filename retrival process.
    fn run_filename_process(&self, url: &str, quality: Option<&String>) -> Result<Child> {
        let mut cmd = self.ytdl_base();
        cmd.arg("--get-filename")
            .arg("--no-warnings")
            .args(&["-o", "%(title)s.%(ext)s"]);
        if quality.is_some() {
            cmd.args(&["-f", &quality.unwrap()]);
        }
        Ok(cmd
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?)
    }

    /// Generate the lib command.
    /// binary [args] -q {quality} -r {rate} -f {file} -v {true/false} {url}
    fn lib_request_video_cmd(
        &self,
        url: &str,
        file_path: &Path,
        quality: &str,
        get_video: bool,
    ) -> Result<Child> {
        let java_path = Path::new(&self.defaults.lib_dir);

        debug!(
            "{} {:?} -q {} -r {}M -f {} -v {} {}",
            self.defaults.lib_bin,
            self.defaults.lib_args,
            quality,
            self.defaults.download_mbps,
            file_path.to_string_lossy(),
            get_video,
            url
        );
        Ok(Command::new(&self.defaults.lib_bin)
            .current_dir(&java_path)
            .args(&self.defaults.lib_args)
            .args(&["-q", quality])
            .args(&["-r", &format!("{}M", self.defaults.download_mbps)])
            .arg("-f")
            .arg(file_path)
            .args(&["-v", &(get_video).to_string()])
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?)
    }

    /// Runs the playlist extraction process.
    fn run_playlist_extract(&self, request: &Request) -> Result<Child> {
        let mut cmd = self.ytdl_base();
        cmd.arg("-s")
            .arg("--dump-json")
            .arg("--flat-playlist")
            .arg("--no-warnings");
        if request.from > 0 {
            cmd.arg("--playlist-start");
            cmd.arg(request.from.to_string());
        }
        if request.to > 0 {
            cmd.arg("--playlist-end");
            cmd.arg(request.to.to_string());
        }
        Ok(cmd
            .arg(&request.url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?)
    }

    /// Runs the playlist name retrival process.
    fn run_playlist_get_name(&self, url: &str) -> Result<Child> {
        Ok(self
            .ytdl_base()
            .arg("-s")
            .arg("--no-warnings")
            .args(&["--playlist-start", "1"])
            .args(&["--playlist-end", "1"])
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?)
    }

    /// Executes the progress update statement.
    fn update_progress(
        &self,
        qid: &u64,
        conn: &mut PooledConn,
        stmt: &Statement,
        progress: &str,
    ) -> Result<()> {
        conn.exec_drop(stmt, (progress, qid))?;
        Ok(())
        //-> only return errors, ignore the return value of stmt.execute
    }

    /// Returns the latest upstream version number and sha256
    pub fn get_latest_version() -> Result<Version> {
        let release: GHRelease = lib::http::http_json_get(UPDATE_VERSION_URL)?;
        let version = release.tag_name;
        let hashes = {
            let sha256_asset = match release.assets.iter().find(|v| v.name == UPDATE_SHA256_FILE) {
                Some(v) => v,
                None => return Err(Error::InternalError("SHA256 asset not found!".into())),
            };
            lib::http::http_text_get(&sha256_asset.browser_download_url)?
        };
        let sha256 = parse_hashfile(&hashes)?;
        let asset_url = match release
            .assets
            .into_iter()
            .find(|v| v.name == UPDATE_ASSET_NAME)
        {
            Some(v) => v.browser_download_url,
            None => return Err(Error::InternalError("yt-dlp asset not found!".into())),
        };

        Ok(Version {
            version: version,
            sha256: sha256.to_owned(),
            url: asset_url,
        })
    }

    /// Update youtube-dl
    /// Check for version, download update and check for sha2
    /// W-Lcok
    pub fn update_downloader(&self) -> Result<()> {
        use std::fs::{remove_file, rename};

        let guard_ = self.lock.write()?;
        // check for existence of lib
        let download_file = self.ytdl_path();
        let backup_file = self.cmd_path.join("ytdl_backup");
        let r_version = Downloader::get_latest_version()?;
        debug!("Latest version: {}", r_version.version);
        if download_file.exists() {
            let version = self.version()?;
            debug!("Current version: {}", version);
            if version != r_version.version {
                match self.inner_update(&download_file, &r_version) {
                    Ok(_) => {}
                    Err(v) => {
                        // rollback to old version
                        info!("Update failed, doing rollback");
                        if download_file.exists() {
                            remove_file(&download_file)?;
                        }
                        rename(&backup_file, &download_file)?;
                        return Err(v);
                    }
                }
            } else {
                trace!("equal version");
            }
        } else {
            self.inner_update(&download_file, &r_version)?;
        }
        drop(guard_);
        Ok(())
    }

    /// download & verify update
    /// does NOT lock!
    fn inner_update(&self, file_path: &Path, version: &Version) -> Result<()> {
        use crate::lib::http;

        http::http_download(&version.url, &file_path)?;
        debug!("yt-dl updated");
        if !lib::check_SHA256(&file_path, &version.sha256)? {
            return Err(Error::InternalError("Hash mismatch".into()));
        }

        Self::check_ytdl_perm(file_path)?;

        Ok(())
    }

    fn ytdl_path(&self) -> PathBuf {
        self.cmd_path.join(YTDL_NAME)
    }

    fn check_ytdl_perm(path: &Path) -> Result<()> {
        #[cfg(not(target_os = "windows"))]
        {
            std::fs::set_permissions(path, PermissionsExt::from_mode(0o700))?;
            trace!("Set file permissions for yt-dlp");
        }
        Ok(())
    }
}

fn parse_hashfile(input: &str) -> Result<&str> {
    input
        .split('\n')
        .find(|c| c.ends_with(UPDATE_ASSET_NAME))
        .map(|v| v.split(" ").next().unwrap().trim())
        .ok_or(Error::InternalError("Hash entry not found!".into()))
}

#[derive(Debug, Deserialize)]
struct GHRelease {
    tag_name: String,
    assets: Vec<GHAsset>,
}

#[derive(Debug, Deserialize)]
struct GHAsset {
    name: String,
    browser_download_url: String,
    // content_type: String,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn filenames() {
        assert!(REGEX_NAME.is_match("A#B\"C.ABCÜ02.mp4"));
        assert!(REGEX_NAME.is_match("A#B\"C.ABCÜ02.webm"));
        assert!(!REGEX_NAME.is_match("A#B\"C.ABCÜ02."));
    }

    #[test]
    fn latest_version() {
        let version = Downloader::get_latest_version().unwrap();
        dbg!(version);
    }

    #[test]
    fn hash_parsing() {
        let data = include_str!("../../tests/SHA2-256SUMS.txt");
        let expected = "5c37c8f9aaf8cc12faea034de96deb5794b7177f071425ce69dad3f315335559";
        assert_eq!(expected, parse_hashfile(data).unwrap());
    }
}
