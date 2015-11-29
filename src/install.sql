/*
Required table file format barracuda for compression & strict mode to sensure the correct creation of it
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

DELIMITER ;
CREATE TABLE `users` (
 `uid` int(11) NOT NULL AUTO_INCREMENT,
 `name` varchar(15) NOT NULL,
 PRIMARY KEY (`uid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
CREATE TABLE `playlists` (
 `qid` int(10) unsigned NOT NULL,
 `from` smallint(6) NOT NULL,
 `to` smallint(6) NOT NULL,
 `zip` tinyint(1) NOT NULL,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
CREATE TABLE `querydetails` (
 `qid` int(10) unsigned NOT NULL,
 `code` tinyint(4) NOT NULL,
 `progress` double DEFAULT NULL,
 `status` varchar(15) NOT NULL,
 `luc` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
CREATE TABLE `files` (
 `fid` int(10) unsigned NOT NULL,
 `name` varchar(100) CHARACTER SET ascii NOT NULL,
 `rname` varchar(100) CHARACTER SET utf8 COLLATE utf8_unicode_ci NOT NULL,
 `valid` int(11) NOT NULL,
 PRIMARY KEY (`fid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;
CREATE TABLE `queries` (
 `qid` int(10) unsigned NOT NULL AUTO_INCREMENT,
 `url` varchar(100) NOT NULL,
 `type` tinyint(4) NOT NULL,
 `quality` smallint(6) NOT NULL,
 `created` datetime NOT NULL,
 `uid` int(11) NOT NULL,
 `playlistid` int(11) DEFAULT NULL,
 PRIMARY KEY (`qid`),
 KEY `playlistid` (`playlistid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;
CREATE TABLE `querystatus` (
 `qid` int(10) unsigned NOT NULL,
 `msg` text NOT NULL,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;