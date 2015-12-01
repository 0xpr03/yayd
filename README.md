# yayd-backend
### Yet another youtube downloader-backend for DB based downloading with proxy support. 
Supports playlists & mass downloads as zip  
This is the backend for the downloader  
  

(Thanks at this point to the people on #rust & #rust-offtopic @ mozilla IRC)  
[GUI Example](***REMOVED***)

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

# Notes
Multithreading for downloads isn't planned as one-by-one is a natural limiter, preventing possible DDOS-Blocks & saving bandwidth  
I'm open for other ideas or implementations but it's not my main goal at the moment.

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
		133,134,135,136,137,298,299: [240 360 480 720 1080 @30 720 1080 @60]youtube - video only  
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