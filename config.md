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
Currently available itags/quality codes from youtube can be looked up via youtube-dl with `youtube-dl -v -F [url]`
### General
`audio_mp3` codec id on which a mp3-conversion should be done  
`audio_raw` quality file which should be used for the audio download  
`audio_source_hq` same again for HQ audio downloads  
### Youtube (yt)
`audio_normal_mp4` itag for all MP4 (h264) downloads, DASH file to be merged with the video
`audio_normal_webm` itag for all webm (VP9) downloads, DASH file to be meged with the video
`audio_hq` itag for the source of HQ audio downloads
### Twitch
```-10 = "Mobile"
-11 = "Low"
-12 = "Medium"
-13 = "High"
-14 = "Source"```
Left input numeric fom quality column, right used quality for twitch downloads.