# yayd [![Build](https://github.com/0xpr03/yayd/actions/workflows/build.yml/badge.svg)](https://github.com/0xpr03/yayd/actions/workflows/build.yml)

## About:  
YAYD is intended as backend for [yt-dlp]. It's purpose is processing download jobs which are fetched from a Database. You probably want to
let it run over a webserver, using it as online service.
It was born out of ISP related problems and has multi-user support, can delete stored files from jobs after a time and supports proxies.
Currently there are only modules for youtube, twitch and soundcloud, feel free to extend them. (See [Hacking Yayd](#hacking-yayd))

## Installation

Required:  
* [FFMPEG] for conversions ([linux static builds](https://www.johnvansickle.com/ffmpeg/))
* [python] 2.6, 2.7, or 3.2+ for [yt-dlp], which is called & kept up to date by yayd
* [mariaDB]/MySQL

1. Use a [release] build or build yayd from source with [rust]: `cargo build --release`
2. Use and run [setup.sql] to create the tables according to your requirements.
3. Run yayd for a first time, edit the config file, see [Config](config.md)
4. Create your own logging configuration  
**yayd doesn't provide any sort of UI**, being a backend, see down below for an example.

## GUI / Frontend for yayd

A frontend example is vailable under [yayd-frontend] and [looks like this](docs/yayd_gui.png)

## Hacking Yayd

Every supported site has it's own handler module inside [src/handler](src/handler/)  
All library stuff, including [yt-dlp] and [ffmpeg] bindings are inside [src/lib](src/lib/)  
The Request struct used by every handler is inside [lib/mod.rs](src/lib/mod.rs#L36).
In general every request has an URL, quality code and information about whether it's an playlist request or not.
(A youtube URL for a single video can include a playlist tag.)

Handlers are consisting of a function called at program startup to register which URLs it's capable of handling
and a function that does the actual work. Handlers can register multiple modules, for example for split up playlist and file handling.

For an example see [youtube.rs](src/handler/youtube.rs)

### Testing

As yayd heavily relies on it's db connection, nearly all tests require an empty test DB to which yayd can connect. (Using temporary tables)
You can specify the connection parameters adapting the following command during tests:
```
db_test=true ip="127.0.0.1" port="3306" user=root pass="" db=ytdownl download_dir="/tmp" temp_dir="/tmp" RUST_BACKTRACE=1 mbps=100 ffmpeg_dir="/tmp/ffmpeg-3.0.2-64bit-static/" cargo test
```

## DB System and quality codes

The DB scheme can be seen in [this picture.](docs/rdm.svg)

## Status codes from yayd
`code` in `querydetails`, `status` although misleading is for step updates ("2|3")

| Code | Meaning |
|---|---|
| -1 | waiting |
| 0 | started |
| 1 | running |
| 2 | finished |
| 3 | finished, warnings |
| 10 | internal error |
| 11 | wrong quality |
| 12 | source unavailable |

## Quality Codes

These are the current quality codes per module:
column `quality` in `queries`

### Youtube Module

Code explanation see [itag](https://en.wikipedia.org/wiki/YouTube#Quality_and_formats).  
Audio and video files have to be cut together, thus [FFMPEG] is required.

| Code/iTag | Description |
| --- | --- |
| -1 | mp3 converted from source |
| -2 | AAC MQ general |
| -3 | AAC HQ general |
| 133 | 240p |
| 134 | 360p |
| 135 | 480p |
| 136 | 720p |
| 137 | 1080p |
| 298 | 720p, 60 fps |
| 303 | 1080p, 60 fps |

iTag explanation (code > 0):  
Youtube-Videos are consisting of two DASH-Files. One is only Video, (in the quality you want).
The other one is a bad resolution video, but audio containing DASH-File.  
These two are merged by yayd and thus if you specify the wanted quality [itag] in you query (queries.quality) 
yayd will merge this with an audio file as specified in the config.  
For a personal list of recommended quality itags to be used for the quality column see down below.
As youtube changes the available codecs it is recommended to verify your setup from time to time.
For example the current 1080p@60fps, mp4 (itag 299) is pixelated in certain circumstances, while the recently added
WebM (303) doesn't have this problem.  
  
(WebM is using VP9 as codec, MP4 h264)

### Twitch

| Code | Desciption |
| --- | --- |
| -10 | Mobile |
| -11 | Low |
| -12 | Medium |
| -13 | High |
| -14 | Source |

# Notes

There is currently no multithreading support, meaning one job at a time. This is intentional and prevents DOS-Blocks (captcha requests) by some sites.

   [yt-dlp]: <https://github.com/yt-dlp/yt-dlp>
   [FFMPEG]: <http://ffmpeg.org/>
   [mariadb]: <https://mariadb.org/>
   [rust]: <http://rust-lang.org/>
   [yayd-frontend]: <https://github.com/0xpr03/yayd-frontend>
   [release]: <https://github.com/0xpr03/yayd/releases>
   [setup.sql]: <setup.sql>
   [itag]: <https://en.wikipedia.org/wiki/YouTube#Quality_and_formats>
   [python]: <https://www.python.org/>
