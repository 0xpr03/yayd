# DB-Backend:
See install.sql for a complete db-setup
## queries
qid | url | type | quality | created | uid   

| type | name |
|---|---|
| 0 | youtube |
| 1 | yt-playlist |
| 2 | twitch |
| 3 | soundcloud |
  
  
| id | quality |
|---|---|
| 0-500 | youtube itags reserved |
| -1 | mp3 converted from source |
| -2 | AAC MQ general |
| -3 | AAC HQ general |
| -10 | Twitch Mobile |
| -11 | Twitch Low |
| -12 | Twitch Medium |
| -13 | Twitch High |
| -14 | Twitch Source |

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