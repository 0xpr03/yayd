#!/bin/bash
ip="192.168.91.1"
pw=""
user="root"
mbps=100
temp="/mnt/hgfs/VM-WORK/temp-downl"
downl="/mnt/hgfs/VM-WORK"
ffmpeg="/mnt/hgfs/VM-WORK/ffmpeg-release-32bit/"
mysql -h $ip -u $user -e 'drop database if exists test'
mysql -h $ip -u $user -e 'create database test'
db_test=true db_ip=$ip db_port="3306" db_user=yayd db_password=$pw db_db=test download_dir=$downl temp_dir=$temp mbps=$mbps ffmpeg_dir=$ffmpeg cargo test
