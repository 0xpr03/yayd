# yayd-backend
### Yet another youtube downloader - backend for DB based downloading with proxy support. 
in short "yayd"
Supports playlists & mass downloads as zip  
  
This project was born out of ISP related connection problems with youtube.
It's purpose is to download, mainly, youtube videos in the quality wanted and
if optionally as audio only. It is supposed to run on a server, as it's reading
it's tasks from a database and also reports back to it.  
You can for example write a frontend: website which communicates over the DB with yayd.
By this one can A: surrogate the ISP peering problem by download over the server, B have all the 
advantages yayd has aside from this.

Yayd is capable of download whole playlists and zipping all files, converting audio & cutting
audio & video together, supporting youtube's DASH format.
Errors like undownloadable files & unavailable qualitys are reported back via codes. 
See [codes.md](codes.md) for more information. Complete failures are logged in 
  
One such GUI/Frontend/Website could look like this:
[GUI Example](***REMOVED***)
It is the current frontend used by the author.
  
## About quality, queries & the config
Each download task is an entry in the DB, this 'query' entry is containing the wished target, quality etc  
Youtube-Videos are consisting of two DASH-Files. One is only Video, in the quality you want.
The other one is a qualitatively bad video but audio containing DASH-File.  
These two are merged by yayd and thus if you specify the wanted quality [itag](https://en.wikipedia.org/wiki/YouTube#Quality_and_formats) in you query (queries.quality) 
yayd will merge this with an audio files as specified in the config.  
For a personal list of recommended quality itags to be used for the quality column see down below.
As youtube changes the available codecs it is recommended to verify your setup from time to time.
For example the current 1080p@60fps, mp4 (itag 299) is pixellated in certain circumstances, while the recently added
WebM (303) doesn't have this problem.  
  
(WebM is using VP9 as codec, MP4 h264)

The quality column (see db.md -> quality) is using positive values for youtube, as it changes it's formats over time. Negative values are thus reserved to static values like twitchs quality (which is not nummeric) or the codec for internal music conversion. This gives you the option to choose by yourself which
youtube quality you want to use.

### Recommended itags
140,251 AAC extraction (mq,hq)  
133,134,135,136,137,298,303: 240, 360, 480, 720, 1080p @30; 720, 1080p @60fps  
cut together with 140 (which is aac mp4 with very low video quality)  

# Config:
## db
Specify the credentials for a maria/mysql db connection
## lib
You can specify an executable/script which should be called, when the file is not available in your country  
The arguments from yayd are the following: `-q {quality} -r {rate} -f {file} -v {true/false} {url}`
Example for calling a java application (`/path/to/jre/java -jar /path/to/jar/application.jar [..]`) :  
```toml
lib_bin = "/path/to/jre/java"
lib_args = ["-jar", "application.jar"]
lib_dir = "/path/to/jar"
```
## Codecs
You can look up available codecs/qualities from youtube with youtube-dl via `youtube-dl -v -F [url]`
### General
`audio_mp3` codec id on which a mp3-conversion should be done  
`audio_raw` quality file which should be used for the audio download  
`audio_source_hq` same again for HQ audio downloads  
### Youtube (yt)
`audio_normal_mp4` itag for all MP4 (h264) downloads, DASH file to be merged with the video
`audio_normal_webm` itag for all webm (VP9) downloads, DASH file to be meged with the video
`audio_hq` itag for the source of HQ audio downloads

# Notes
Multithreading for downloads isn't planned as the one-by-one system is a natural limiter, preventing possible DDOS-Blocks (captcha) & saving bandwidth  
I'm open for other ideas or implementations but it's not my main goal at the moment.

## DB-Setup & internal quality code explanations see db.md