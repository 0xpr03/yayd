# yayd-backend
### Yet another youtube downloader-backend for DB based downloading with proxy support. 
Supports playlists & mass downloads as zip  
This is the backend for the downloader  
  

(Thanks at this point to the people on #rust & #rust-offtopic @ mozilla IRC)  
[GUI Example](***REMOVED***)

## About quality, queries & the config
Each download is an entry in the DB, this query is containing the wished target, quality etc  
Youtube-Videos are consisting of two DASH-Files. One is only Video, in the quality you want.
The other one is a bad quality video but audio containing DASH-File.  
These two are merged by yayd and thus if you specify the wanted quality [itag](https://en.wikipedia.org/wiki/YouTube#Quality_and_formats)
in you query,
the used audio-file is specified in the config of yayd and will be merged with the video.  
For a personal list of recommended quality itags see queries: quality down below.  
As youtube changes the available codecs it is recommended to verify your setup from time to time.
As an example the current 1080p@60fps, mp4 (itag 299) is pixellated in certain circumstances, while the recently added
WebM (303) doesn't have this problem.  
  
(WebM is using VP9 as codec, MP4 h264)

# Config:
## db
Specify the credentials for a maria/mysql db connection
## lib
You can specify an executable/script which should be called, when the file is not available in your country  
Example for calling a java application:  
```toml
lib_bin = "/path/to/jre/java"
lib_args = ["-jar", "application.jar"]
lib_dir = "/path/to/jar"
```
## Codecs
`audio_mp3` codec id on which a mp3-conversion should be done  
`audio_raw` quality file which should be used for the audio download  
`audio_source_hq` same again for HQ audio downloads  

# DB-Backend:
See install.sql for a complete db-setup
## queries
qid | url | type | quality | created | uid   
	type:   
		0: yt-video  
		1: playlist  
		
	quality:  
		1: mp3  
		140,22 AAC extraction (mq,hq)  
		133,134,135,136,137,298,299: [240, 360, 480, 720, 1080p @30; 720, 1080p @60fps]youtube - video only  
		cut together with 140 (which is aac mp4 with very low video quality)  

url: utf8_bin

	
## querydetails
qid | code | status | luc  
	please see codes.md for a complete list of status codes

## playlists
qid | from | to | zip  

## files
file id | name | rname | valid  

rname:utf8_general  
name:utf8_unicode  

query id == file id

## users
uid | name  

insert:  
insert in query ids, insert in querydetails  
using users uid  
-> store it in a stored procedure  

## querystatus
qid | msg  

All errors occouring during downloads are stored in here