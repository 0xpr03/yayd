# yayd [![Build Status](https://travis-ci.org/0xpr03/yayd.svg?branch=master)](https://travis-ci.org/0xpr03/yayd)
### Yet another youtube downloader - backend for DB based downloading with proxy support. 
Supports playlists & mass downloads as zip  
Backend for youtube-dl

## Features:  
* Download Playlists complete as zip from youtube
* Support newest youtube codecs without recompilation
* Convert to mp3
* Download original audio
* Download twitch videos (archived streams) 
* Multi-User support
* Own library embeddable for downloads not available from your country
  (Streaming those from other online services for example)
* Runnable from any VPS probably even from raspis
* Log any error occurring
* Extendable to support many more sites
* Bullet proof, no left over files on errors
* Terminable download rates
  
This project was born out of ISP related connection problems with youtube.
It's purpose is mainly to download youtube videos or convert them to audio files. 
It is supposed to run on a server, as it's using a DBMS like MySQL/mariaDB.
You can for example write a website which communicates over the DB with yayd.
By this, one can A: surrogate the ISP peering problem by download over the server, B have all the 
advantages yayd has aside from this. [GUI Example](yayd_gui.png)  
If you're too lazy or want a fast setup see below for a working, installable frontend.

All failures like undownloadable files & unavailable formats are reported back via codes. 
See [codes.md](codes.md) for more information. Complete failures are logged in the table querystatus.
  
One such GUI/Frontend/Website could look like in yayd_gui.png 
Please look into the repo [yayd-frontend](https://github.com/0xpr03/yayd-frontend) for an example.

## Installation
Needed: [youtube-dl](https://github.com/rg3/youtube-dl)  
FFMPEG optionally  
mariadb / mysql  
  
1. Build yayd with rust: `cargo build --release`  
2. Create the DB according to install.sql
3. Run it for a first time & correct the config.cfg.  
To run yayd with a GUI you'll need to write for example a website, or use the [example](https://github.com/0xpr03/yayd-frontend). Yayd itself doesn't provide any sort of UI.  
4. If everything is running fine, create your own log configuration if needed, see [here](https://github.com/sfackler/log4rs).
  
## About quality codes, queries & the config
Each download task is an entry in the DB, this 'query' entry is containing the wished target, quality etc  
Youtube-Videos are consisting of two DASH-Files. One is only Video, in the quality you want.
The other one is a qualitatively bad video but audio containing DASH-File.  
These two are merged by yayd and thus if you specify the wanted quality [itag](https://en.wikipedia.org/wiki/YouTube#Quality_and_formats) in you query (queries.quality) 
yayd will merge this with an audio file as specified in the config.  
For a personal list of recommended quality itags to be used for the quality column see down below.
As youtube changes the available codecs it is recommended to verify your setup from time to time.
For example the current 1080p@60fps, mp4 (itag 299) is pixelated in certain circumstances, while the recently added
WebM (303) doesn't have this problem.  
  
(WebM is using VP9 as codec, MP4 h264)

The quality column (see db.md -> quality) is using positive values for youtube, as it changes it's formats over time. Negative values are thus reserved to static values like twitchs quality (which is not numeric) or the codec for internal music conversion. This gives you the option to choose by yourself which
youtube quality you want to use.

### Recommended itags
140,251 AAC extraction (mq,hq)  
133,134,135,136,137,298,303: 240, 360, 480, 720, 1080p @30; 720, 1080p @60fps  
cut together with 140 (which is aac mp4 with very low video quality)  

# Notes
Multithreading for downloads isn't planned as the one-by-one system is a natural limiter, preventing possible DDOS-Blocks (captcha) & saving bandwidth  
I'm open for other ideas or implementations but it's not my main goal at the moment.

### DB-Setup & internal quality code explanations see db.md