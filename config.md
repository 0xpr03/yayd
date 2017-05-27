# Config:

## main

* `link_subqueries`  enabling activates linkage of split playlist downloads via the `subqueries` table
* `link_files` enabling activates linkage of fid to qid via `query_files` table
* `temp_dir` directory for temporary files, yayd should have write permission here
* `download_dir` directory for finished downloads, yayd and your webserver should have access, yayd write access
* `download_mbps` download limit in Mbit/s
* `youtube_dl_dir` directory of [yt-dl]
* `youtube_dl_auto_update` enable this to let yayd keeping [yt-dl] up to date, this is required as youtube changes its layout over time, requiring changes in yt-dl
  if this is disabled you have to provide [yt-dl] by yourself

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
Currently available itags/quality codes from youtube can be looked up via youtube-dl with `youtube-dl -v -F [url]`
### General
the following are ids for the `quality` column in `queries`, aka id of special jobs like conversion
* `audio_mp3` id for which an mp3-conversion should be done
* `audio_raw` id on which the source audio file should be retrieved
* `audio_source_hq` id for HQ audio downloads  
### Youtube (yt)
this are [itags] for the youtube handler, defining which itags to use for audio
`audio_normal_mp4` itag for all MP4 (h264) downloads, DASH file to be merged with the video
`audio_normal_webm` itag for all webm (VP9) downloads, DASH file to be meged with the video
`audio_hq` itag for the source of HQ audio downloads
### Twitch
twitch values for `quality` column - real values for [yt-dl]
```
-10 = "Mobile"
-11 = "Low"
-12 = "Medium"
-13 = "High"
-14 = "Source"
```

   [yt-dl]: <https://yt-dl.org>
   [itags]: <README.md#youtube-module>
