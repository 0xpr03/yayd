/* Copyright (c) 2015, Aron Heinecke
 * All rights reserved.
 * Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
 * 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */
 
/*
 *Required table file format 'barracuda' for compression, and strict mode to sensure the correct creation of it.
 *MySQL won't notice you of a failed creation without compression otherwise.
 */


/*
 * This stored procedure inserts a new job for yayd into queries,querydetails & playlists
 * If playlist data is existing it'll create an playlist table entry, otherwise not.
 * It's main goal is to search for the ID of the user or creating a new one for the job creation.
 * If you don't want multi user support, use the procedure down below.
 */
DELIMITER $$
DROP PROCEDURE IF EXISTS `crQuery`$$
CREATE DEFINER=`root`@`localhost` PROCEDURE `crQuery`(IN `parameter_url` VARCHAR(100) CHARSET utf8, /* paramter */
	IN `parameter_user` VARCHAR(12) CHARSET utf8,
	IN `parameter_type` TINYINT,
	IN `parameter_quality` INTEGER,
	IN `parameter_from` INTEGER,
	IN `parameter_to` INTEGER,
	IN `parameter_zip` BOOLEAN,
	OUT `out_qid` INTEGER)
    MODIFIES SQL DATA
BEGIN
		DECLARE `var_uID` INTEGER;
		DECLARE `var_qID` INTEGER;
		
		SET @default_code = -1;
		SET @playlist_code = 1;
		SET @default_status = 'waiting';
		SET @datetime = NOW();
		
		SELECT `uid` INTO var_uID FROM `users` WHERE `name` LIKE parameter_user;

		IF FOUND_ROWS() = 0
		THEN
			INSERT INTO `users` (`name`) VALUES (parameter_user);
			SET var_uID = LAST_INSERT_ID();
		END IF;

		INSERT INTO `queries` (`url`,`type`,`quality`, `created`,`uid`) VALUES (parameter_url,parameter_type,parameter_quality,@datetime, var_uID);
		SET var_qID = LAST_INSERT_ID();
		
		INSERT INTO `querydetails` (qid,`code`,`status`) VALUES (var_qID,@default_code,@default_status);
		
		IF parameter_type = @playlist_code
		THEN
			INSERT INTO `playlists` (`qid`,`from`,`to`, `zip`) VALUES (var_qID,parameter_from,parameter_to,parameter_zip);
		END IF;
		
		SET out_qid = var_qID;
	END$$
	
/*
 * Procedure WITHOUT multiuser support, use this if you don't want it and don't use the above then.
 */
CREATE DEFINER=`root`@`localhost` PROCEDURE `crQuery`(IN `parameter_url` VARCHAR(100) CHARSET utf8, /* paramter */
	IN `parameter_type` TINYINT,
	IN `parameter_quality` INTEGER,
	IN `parameter_from` INTEGER,
	IN `parameter_to` INTEGER,
	IN `parameter_zip` BOOLEAN,
	OUT `out_qid` INTEGER)
    MODIFIES SQL DATA
BEGIN
		DECLARE `var_qID` INTEGER;
		
		SET @default_code = -1;
		SET @playlist_code = 1;
		SET @default_status = 'waiting';
		SET @datetime = NOW();
		
		SELECT `uid` INTO var_uID FROM `users` WHERE `name` LIKE parameter_user;

		INSERT INTO `queries` (`url`,`type`,`quality`, `created`) VALUES (parameter_url,parameter_type,parameter_quality,@datetime);
		SET var_qID = LAST_INSERT_ID();
		
		INSERT INTO `querydetails` (qid,`code`,`status`) VALUES (var_qID,@default_code,@default_status);
		
		IF parameter_type = @playlist_code
		THEN
			INSERT INTO `playlists` (`qid`,`from`,`to`, `zip`) VALUES (var_qID,parameter_from,parameter_to,parameter_zip);
		END IF;
		
		SET out_qid = var_qID;
	END$$
DELIMITER ;

/*
 * Yayd job table
 */
CREATE TABLE `queries` (
 `qid` int(10) unsigned NOT NULL AUTO_INCREMENT,
 `url` varchar(100) NOT NULL,
 `type` tinyint(4) NOT NULL,
 `quality` smallint(6) NOT NULL,
 `created` datetime NOT NULL,
 `uid` int(11) NOT NULL, /* comment this line out if you don't want multiuser support */
 `playlistid` int(11) DEFAULT NULL,
 PRIMARY KEY (`qid`),
 KEY `playlistid` (`playlistid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * Create only if you want multi user support.
 */
CREATE TABLE `users` (
 `uid` int(11) NOT NULL AUTO_INCREMENT,
 `name` varchar(15) NOT NULL,
 PRIMARY KEY (`uid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * If a query is an playlist job, all details are stored in this table.
 */
CREATE TABLE `playlists` (
 `qid` int(10) unsigned NOT NULL,
 `from` smallint(6) NOT NULL,
 `to` smallint(6) NOT NULL,
 `zip` tinyint(1) NOT NULL,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * Table for progress information which are changing rapidly.
 */
CREATE TABLE `querydetails` (
 `qid` int(10) unsigned NOT NULL,
 `code` tinyint(4) NOT NULL,
 `progress` double DEFAULT NULL,
 `status` varchar(10) DEFAULT NULL,
 `luc` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;

/*
 * Table storing the name of a file, and `rname` for the name actually used on the HDD (ASCII sanitized).
 * Deleted files are marked as `valid=false`, though this isn't relevant for yayd.
 */
CREATE TABLE `files` (
 `fid` int(10) unsigned NOT NULL,
 `name` varchar(100) CHARACTER SET ascii NOT NULL,
 `rname` varchar(100) CHARACTER SET utf8 COLLATE utf8_unicode_ci NOT NULL,
 `valid` int(11) NOT NULL,
 PRIMARY KEY (`fid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * Table with logged error messages for queries
 */
CREATE TABLE `querystatus` (
 `qid` int(10) unsigned NOT NULL,
 `msg` text NOT NULL,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;