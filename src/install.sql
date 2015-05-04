DELIMITER $$
DROP PROCEDURE IF EXISTS `crQuery`$$
CREATE DEFINER=`root`@`localhost` PROCEDURE `crQuery`(IN `parameter_url` VARCHAR(90) CHARSET utf8, /* paramter */
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
		
		SET @default_code = 0;
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
DROP TABLE IF EXISTS `files`;
CREATE TABLE IF NOT EXISTS `files` (
  `fid` int(10) unsigned NOT NULL,
  `name` varchar(60) CHARACTER SET ascii NOT NULL,
  `rname` varchar(60) CHARACTER SET utf8 COLLATE utf8_unicode_ci NOT NULL,
  `valid` int(11) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
DROP TABLE IF EXISTS `playlists`;
CREATE TABLE IF NOT EXISTS `playlists` (
  `qid` int(10) unsigned NOT NULL,
  `from` smallint(6) NOT NULL,
  `to` smallint(6) NOT NULL,
  `zip` tinyint(1) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
DROP TABLE IF EXISTS `queries`;
CREATE TABLE IF NOT EXISTS `queries` (
`qid` int(10) unsigned NOT NULL,
  `url` varchar(33) NOT NULL,
  `type` tinyint(4) NOT NULL,
  `quality` smallint(6) NOT NULL,
  `created` datetime NOT NULL,
  `uid` int(11) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
DROP TABLE IF EXISTS `querydetails`;
CREATE TABLE IF NOT EXISTS `querydetails` (
  `qid` int(10) unsigned NOT NULL,
  `code` tinyint(4) NOT NULL,
  `progress` double NOT NULL,
  `status` varchar(10) NOT NULL,
  `luc` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
DROP TABLE IF EXISTS `users`;
CREATE TABLE IF NOT EXISTS `users` (
`uid` int(11) NOT NULL,
  `name` varchar(15) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
ALTER TABLE `files`
 ADD PRIMARY KEY (`fid`);
 ALTER TABLE `playlists`
 ADD PRIMARY KEY (`qid`);
ALTER TABLE `queries`
 ADD PRIMARY KEY (`qid`);
ALTER TABLE `querydetails`
 ADD PRIMARY KEY (`qid`);
ALTER TABLE `users`
 ADD PRIMARY KEY (`uid`);
ALTER TABLE `queries`
MODIFY `qid` int(10) unsigned NOT NULL AUTO_INCREMENT;
ALTER TABLE `users`
MODIFY `uid` int(11) NOT NULL AUTO_INCREMENT;