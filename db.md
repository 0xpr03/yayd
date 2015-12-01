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