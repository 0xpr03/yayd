# yayd-backend
Yet another youtube downloader-backend for the query & proxy downloading.  
Supports playlists & mass downloads as zip  
Using threads to split up the convertion - download work (planned)

*And my personal entry into rust*  
  
# DB-Backend:
## queries
qid | url | type | quality | created | uid
	type: -1 : nothing
		0: yt-video
		1: playlist
		
	quality:
		1: mp3
		140,22 AAC extraction (mq,hq)
		133,134,135,136,137,298,299: [240 360 480 720 1080 @30 720 1080 @60]youtube - encoded

url: utf8_bin

	
## querydetails
qid | code | status | luc
	code:
		0:waiting
		1:in progress
		2:finished
		3:failed

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
