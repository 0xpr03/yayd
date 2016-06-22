/* Copyright (c) 2015, Aron Heinecke
 * All rights reserved.
 * Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
 * 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */
 
/*
 * Example SQL file for automatic query creations and user based systems
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
	IN `parameter_userID` VARCHAR(12) CHARSET utf8,
	IN `parameter_quality` INTEGER,
	IN `parameter_from` INTEGER,
	IN `parameter_to` INTEGER,
	IN `parameter_compress` BOOLEAN,
	IN `parameter_type` INTEGER,
	OUT `out_qid` INTEGER)
    MODIFIES SQL DATA
BEGIN
	DECLARE `var_qID` INTEGER;
	
	SET @no_playlist_from = -2;
	SET @default_code = -1;
	SET @default_status = 'waiting';
	SET @datetime = NOW();

	INSERT INTO `queries` (`url`,`quality`, `created`,`uid`,`type`) VALUES (parameter_url,parameter_quality,@datetime, parameter_userID,parameter_type);
	SET var_qID = LAST_INSERT_ID();
	
	INSERT INTO `querydetails` (qid,`code`,`status`) VALUES (var_qID,@default_code,@default_status);
	
	IF ( parameter_from != @no_playlist_from ) THEN
		INSERT INTO `playlists` (`qid`,`from`,`to`, `compress`) VALUES (var_qID,parameter_from,parameter_to,parameter_compress);
	END IF;
	
	SET out_qid = var_qID;
END$$
	
/*
 * Procedure WITHOUT multiuser support, use this if you don't want it and don't use the above then.
 */
DELIMITER $$
DROP PROCEDURE IF EXISTS `crQuery`$$
CREATE DEFINER=`root`@`localhost` PROCEDURE `crQuery`(IN `parameter_url` VARCHAR(100) CHARSET utf8, /* paramter */
	IN `parameter_quality` INTEGER,
	IN `parameter_from` INTEGER,
	IN `parameter_to` INTEGER,
	IN `parameter_compress` BOOLEAN,
	IN `parameter_type` INTEGER,
	OUT `out_qid` INTEGER)
    MODIFIES SQL DATA
BEGIN
	DECLARE `var_qID` INTEGER;
	
	SET @no_playlist_from = -2;
	SET @default_code = -1;
	SET @default_status = 'waiting';
	SET @datetime = NOW();
	SET @u_ID = 0;

	INSERT INTO `queries` (`url`,`quality`, `created`,`uid`,`type`) VALUES (parameter_url,parameter_quality,@datetime, @u_ID,parameter_type);
	SET var_qID = LAST_INSERT_ID();
	
	INSERT INTO `querydetails` (qid,`code`,`status`) VALUES (var_qID,@default_code,@default_status);
	
	IF ( parameter_from != @no_playlist_from ) THEN
		INSERT INTO `playlists` (`qid`,`from`,`to`, `compress`) VALUES (var_qID,parameter_from,parameter_to,parameter_compress);
	END IF;
	
	SET out_qid = var_qID;
END$$
 
 
/*
 * As example of how you can use the uid's
 */
CREATE TABLE `users` (
 `uid` int(11) unsigned NOT NULL AUTO_INCREMENT,
 `name` varchar(15) NOT NULL,
 PRIMARY KEY (`uid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;