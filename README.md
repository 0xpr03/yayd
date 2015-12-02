# yayd-backend
### Yet another youtube downloader-backend for DB based downloading with proxy support. 
Supports playlists & mass downloads as zip  
This is the backend for the downloader  

The purpose is to provide a system allowing you to download files, 
avoiding ISP related peering problems with youtube,
and providing all this server side & in the quality you want.

(Thanks at this point to the people on #rust & #rust-offtopic @ mozilla IRC)  
[GUI Example](***REMOVED***)

## About quality, queries & the config
Each download is an entry in the DB, this query is containing the wished target, quality etc  
Youtube-Videos are consisting of two DASH-Files. One is only Video, in the quality you want.
The other one is a bad quality video but audio containing DASH-File.  
These two are merged by yayd and thus if you specify the wanted quality [itag](https://en.wikipedia.org/wiki/YouTube#Quality_and_formats)
in you query,
the used audio-file is specified in the config of yayd and will be merged with the video.  
For a personal list of recommended quality itags see down below.
As youtube changes the available codecs it is recommended to verify your setup from time to time.
As an example the current 1080p@60fps, mp4 (itag 299) is pixellated in certain circumstances, while the recently added
WebM (303) doesn't have this problem.  
  
(WebM is using VP9 as codec, MP4 h264)

The quality column (see db.md -> quality) is using positive values for youtube, as it changes it's formats over time. Negative values are thus reserved to
static values like twitchs quality or the codec for internal music conversion. This gives you the option to choose by yourself which
youtube quality you want to use.

### Personal recommended itags
140,251 AAC extraction (mq,hq)  
133,134,135,136,137,298,303: 240, 360, 480, 720, 1080p @30; 720, 1080p @60fps  
cut together with 140 (which is aac mp4 with very low video quality)  

# Config:
## db
Specify the credentials for a maria/mysql db connection
## lib
You can specify an executable/script which should be called, when the file is not available in your country  
Example for calling a java application (`/path/to/jre/java -jar /path/to/jar/application.jar [..]`) :  
```toml
lib_bin = "/path/to/jre/java"
lib_args = ["-jar", "application.jar"]
lib_dir = "/path/to/jar"
```
## Codecs
`audio_mp3` codec id on which a mp3-conversion should be done  
`audio_raw` quality file which should be used for the audio download  
`audio_source_hq` same again for HQ audio downloads  

# Notes
Multithreading for downloads isn't planned as the one-by-one system is a natural limiter, preventing possible DDOS-Blocks (captcha) & saving bandwidth  
I'm open for other ideas or implementations but it's not my main goal at the moment.

## DB-Setup & internal quality code explanations see db.md