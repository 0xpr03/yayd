/* from 0.6.1 */
ALTER TABLE
    `queries`
    CONVERT TO CHARACTER SET utf8mb4
    COLLATE utf8mb4_unicode_ci;
    
ALTER TABLE
    `playlists`
    CONVERT TO CHARACTER SET utf8mb4
    COLLATE utf8mb4_unicode_ci;

ALTER TABLE
    `querydetails`
    CONVERT TO CHARACTER SET utf8mb4
    COLLATE utf8mb4_unicode_ci;
    
ALTER TABLE
    `files`
    CONVERT TO CHARACTER SET utf8mb4
    COLLATE utf8mb4_unicode_ci;
    
ALTER TABLE
    `queryerror`
    CONVERT TO CHARACTER SET utf8mb4
    COLLATE utf8mb4_unicode_ci;
    
ALTER TABLE
    `subqueries`
    CONVERT TO CHARACTER SET utf8mb4
    COLLATE utf8mb4_unicode_ci;
    
ALTER TABLE
    `query_files`
    CONVERT TO CHARACTER SET utf8mb4
    COLLATE utf8mb4_unicode_ci;

/* from 0.6.4 */
ALTER TABLE queries KEY_BLOCK_SIZE=0;
ALTER TABLE playlists KEY_BLOCK_SIZE=0;
ALTER TABLE files KEY_BLOCK_SIZE=0;
ALTER TABLE queryerror KEY_BLOCK_SIZE=0;
ALTER TABLE subqueries KEY_BLOCK_SIZE=0;
