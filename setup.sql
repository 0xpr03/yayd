/* Copyright (c) 2015, Aron Heinecke
 * All rights reserved.
 * Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
 * 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

/*
 * Setup SQL for required tables
 * Leave the uid in queries even if you don't want multi-user support
 */
 
 
/*
 *Required table file format 'barracuda' for compression, and strict mode to sensure the correct creation of it.
 *MySQL won't notice you of a failed creation without compression otherwise.
 */

/*
 * Yayd job table
 */
CREATE TABLE `queries` (
 `qid` int(10) unsigned NOT NULL AUTO_INCREMENT,
 `url` varchar(100) NOT NULL,
 `quality` smallint(6) NOT NULL,
 `type` smallint(6) NOT NULL,
 `created` datetime NOT NULL,
 `uid` int(11) unsigned NOT NULL,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * If a query is an playlist job, all details are stored in this table.
 * Otherwise there's not entry in this table for this job
 */
CREATE TABLE `playlists` (
 `qid` int(10) unsigned NOT NULL,
 `from` smallint(6) NOT NULL,
 `to` smallint(6) NOT NULL,
 `compress` tinyint(1) NOT NULL,
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
 * Deleted files are marked as `valid: false`.
 */
CREATE TABLE `files` (
 `fid` int(10) unsigned NOT NULL AUTO_INCREMENT,
 `name` varchar(100) CHARACTER SET ascii NOT NULL,
 `rname` varchar(100) CHARACTER SET utf8 COLLATE utf8_unicode_ci NOT NULL,
 `valid` int(11) NOT NULL,
 PRIMARY KEY (`fid`) USING BTREE
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * Table with logged error messages for queries
 */
CREATE TABLE `queryerror` (
 `qid` int(10) unsigned NOT NULL,
 `msg` text NOT NULL,
 PRIMARY KEY (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * Used if log subqueries is enabled for non-zipped queries
 */
CREATE TABLE `subqueries` (
 `qid` int(10) unsigned NOT NULL,
 `origin_id` int(10) unsigned NOT NULL,
 PRIMARY KEY (`qid`,`origin_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 ROW_FORMAT=COMPRESSED;

/*
 * For file-query relations, if enabled in the config
 */
CREATE TABLE `query_files` (
 `qid` int(11) unsigned NOT NULL,
 `fid` int(10) unsigned NOT NULL,
 UNIQUE KEY `qid` (`qid`,`fid`),
 KEY `qid_2` (`qid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
