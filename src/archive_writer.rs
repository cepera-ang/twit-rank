use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags};

use crate::model::Tweet;
use crate::x::{html_to_search_text, search_text_from_raw_json};

const INSERT_SQL: &str = r#"
    INSERT OR IGNORE INTO tweets
      (id, user_id, username, username_lc, fullname, text, search_text, created_at,
       reply_count, retweet_count, like_count, view_count,
        feed_kind, archived_at, user_pic, photos, quote_id, retweet_id,
        reply_to_id, conversation_id, entities_json, x_raw_json, videos)
    VALUES
      (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
       ?9, ?10, ?11, ?12,
        ?13, ?14, ?15, ?16, ?17, ?18,
        ?19, ?20, ?21, ?22, ?23)
    "#;

const UPDATE_SQL: &str = r#"
    UPDATE tweets SET
      user_id = CASE
        WHEN (user_id IS NULL OR user_id = '') AND ?2 <> '' THEN ?2
        ELSE user_id
      END,
      username = CASE
        WHEN (username IS NULL OR username = '') AND ?3 <> '' THEN ?3
        ELSE username
      END,
      username_lc = CASE
        WHEN ?4 <> '' THEN ?4
        ELSE username_lc
      END,
      fullname = CASE
        WHEN (fullname IS NULL OR fullname = '') AND ?5 <> '' THEN ?5
        ELSE fullname
      END,
      text = CASE
        WHEN ?6 <> '' AND (
          (text IS NULL OR text = '')
          OR (
            length(text) < length(?6)
            AND (
              substr(text, -3) = '...'
              OR substr(text, -1) = char(8230)
            )
          )
        ) THEN ?6
        ELSE text
      END,
      search_text = CASE
        WHEN ?7 <> '' THEN ?7
        ELSE search_text
      END,
      created_at = CASE
        WHEN (created_at IS NULL OR created_at = '') AND ?8 <> '' THEN ?8
        ELSE created_at
      END,
      reply_count = CASE
        WHEN reply_count = 0 AND ?9 > 0 THEN ?9
        ELSE reply_count
      END,
      retweet_count = CASE
        WHEN retweet_count = 0 AND ?10 > 0 THEN ?10
        ELSE retweet_count
      END,
      like_count = CASE
        WHEN like_count = 0 AND ?11 > 0 THEN ?11
        ELSE like_count
      END,
      view_count = CASE
        WHEN view_count = 0 AND ?12 > 0 THEN ?12
        ELSE view_count
      END,
      feed_kind = CASE
        WHEN (feed_kind IS NULL OR feed_kind = '' OR feed_kind LIKE 'ondemand%' OR feed_kind IN ('quote', 'retweet'))
         AND (?13 NOT LIKE 'ondemand%' AND ?13 <> '' AND ?13 IS NOT NULL)
        THEN ?13
        WHEN (feed_kind IS NULL OR feed_kind = '') AND (?13 <> '' AND ?13 IS NOT NULL)
        THEN ?13
        ELSE feed_kind
      END,
      archived_at = CASE
        WHEN (archived_at IS NULL OR archived_at = '') AND ?14 <> '' THEN ?14
        ELSE archived_at
      END,
      user_pic = CASE
        WHEN (user_pic IS NULL OR user_pic = '') AND ?15 <> '' THEN ?15
        ELSE user_pic
      END,
      photos = CASE
        WHEN (photos IS NULL OR photos = '' OR photos = '[]') AND (?16 <> '' AND ?16 <> '[]') THEN ?16
        ELSE photos
      END,
      quote_id = CASE
        WHEN (quote_id IS NULL OR quote_id = 0) AND ?17 > 0 THEN ?17
        ELSE quote_id
      END,
      retweet_id = CASE
        WHEN (retweet_id IS NULL OR retweet_id = 0) AND ?18 > 0 THEN ?18
        ELSE retweet_id
      END,
      reply_to_id = CASE
        WHEN (reply_to_id IS NULL OR reply_to_id = 0) AND ?19 > 0 THEN ?19
        ELSE reply_to_id
      END,
      conversation_id = CASE
        WHEN (conversation_id IS NULL OR conversation_id = 0) AND ?20 > 0 THEN ?20
        ELSE conversation_id
      END,
      entities_json = CASE
        WHEN (entities_json IS NULL OR entities_json = '') AND (?21 IS NOT NULL AND ?21 <> '') THEN ?21
        ELSE entities_json
      END,
      x_raw_json = CASE
        WHEN (x_raw_json IS NULL OR x_raw_json = '') AND (?22 IS NOT NULL AND ?22 <> '') THEN ?22
        ELSE x_raw_json
      END,
      videos = CASE
        WHEN (videos IS NULL OR videos = '' OR videos = '[]') AND (?23 <> '' AND ?23 <> '[]') THEN ?23
        ELSE videos
      END
    WHERE id = ?1
    "#;

#[derive(Debug, Clone, Copy)]
pub struct UpsertOutcome {
    pub inserted_tweet: bool,
    pub inserted_feed: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InitSchemaReport {
    pub needs_username_lc_maintenance: bool,
    pub needs_search_text_maintenance: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MaintenanceSummary {
    pub username_lc_updated: usize,
    pub search_text_updated: usize,
}

/// Write access to the archive SQLite database (`tweets` table).
///
/// This acts as a write-through cache: anything we fetch from X on-demand is
/// persisted here, and the background archiver also writes into the same DB.
pub struct ArchiveWriter {
    path: PathBuf,
}

impl ArchiveWriter {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
        })
    }

    fn conn(&self) -> Result<Connection> {
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("open archive db for write {}", self.path.display()))?;

        // Allow concurrent readers while we write.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;

        Ok(conn)
    }

    pub fn init_schema(&self) -> Result<InitSchemaReport> {
        let conn = self.conn()?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tweets (
              id INTEGER PRIMARY KEY,
              user_id TEXT NOT NULL,
              username TEXT NOT NULL,
              username_lc TEXT NOT NULL DEFAULT '',
              fullname TEXT NOT NULL,
              text TEXT NOT NULL,
              search_text TEXT NOT NULL DEFAULT '',
              created_at TEXT NOT NULL,
              reply_count INTEGER DEFAULT 0,
              retweet_count INTEGER DEFAULT 0,
              like_count INTEGER DEFAULT 0,
              view_count INTEGER DEFAULT 0,
              feed_kind TEXT NOT NULL,
              archived_at TEXT NOT NULL,
              seen_at TEXT,
              user_pic TEXT DEFAULT '',
              photos TEXT DEFAULT '[]',
              videos TEXT DEFAULT '[]',
              quote_id INTEGER DEFAULT 0,
              retweet_id INTEGER DEFAULT 0,
              reply_to_id INTEGER DEFAULT 0,
              conversation_id INTEGER DEFAULT 0,
              entities_json TEXT DEFAULT '',
              x_raw_json TEXT DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_archived_at ON tweets(archived_at DESC);
            CREATE INDEX IF NOT EXISTS idx_feed_kind ON tweets(feed_kind);
            CREATE INDEX IF NOT EXISTS idx_created_at ON tweets(created_at DESC);

            CREATE TABLE IF NOT EXISTS tweet_feeds (
              tweet_id INTEGER NOT NULL,
              feed_kind TEXT NOT NULL,
              archived_at TEXT NOT NULL,
              PRIMARY KEY (tweet_id, feed_kind)
            );
            CREATE INDEX IF NOT EXISTS idx_tweet_feeds_kind ON tweet_feeds(feed_kind);
            CREATE INDEX IF NOT EXISTS idx_tweet_feeds_tweet_id ON tweet_feeds(tweet_id);
            CREATE INDEX IF NOT EXISTS idx_tweet_feeds_kind_tweet_desc ON tweet_feeds(feed_kind, tweet_id DESC);

            CREATE TABLE IF NOT EXISTS feedback (
              post_id TEXT PRIMARY KEY,
              value INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              reason TEXT DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS user_feedback (
              user TEXT PRIMARY KEY,
              likes INTEGER NOT NULL,
              dislikes INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS schema_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            "#,
        )?;

        // Migrations for older DBs. Ignore "duplicate column" errors.
        let _ = conn.execute("ALTER TABLE tweets ADD COLUMN user_pic TEXT DEFAULT ''", []);
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN username_lc TEXT NOT NULL DEFAULT ''",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN search_text TEXT NOT NULL DEFAULT ''",
            [],
        );
        let _ = conn.execute("ALTER TABLE tweets ADD COLUMN photos TEXT DEFAULT '[]'", []);
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN quote_id INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN retweet_id INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN reply_to_id INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN conversation_id INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN entities_json TEXT DEFAULT ''",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE tweets ADD COLUMN x_raw_json TEXT DEFAULT ''",
            [],
        );
        let _ = conn.execute("ALTER TABLE tweets ADD COLUMN videos TEXT DEFAULT '[]'", []);
        let _ = conn.execute("ALTER TABLE feedback ADD COLUMN reason TEXT DEFAULT ''", []);

        self.run_once_migration(&conn, "tweet_feeds_unique_v1", |conn| {
            conn.execute(
                "DELETE FROM tweet_feeds
                     WHERE rowid NOT IN (
                       SELECT MIN(rowid) FROM tweet_feeds GROUP BY tweet_id, feed_kind
                     )",
                [],
            )?;
            conn.execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_tweet_feeds_unique
                     ON tweet_feeds(tweet_id, feed_kind)",
                [],
            )?;
            Ok(())
        })?;
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tweets_username_lc ON tweets(username_lc)",
            [],
        );

        Ok(InitSchemaReport {
            needs_username_lc_maintenance: has_missing_username_lc(&conn)?,
            needs_search_text_maintenance: has_missing_search_text(&conn)?,
        })
    }

    pub fn run_startup_maintenance(&self, batch_size: usize) -> Result<MaintenanceSummary> {
        let conn = self.conn()?;
        let username_lc_updated = backfill_username_lc(&conn, batch_size)?;
        let search_text_updated = backfill_search_text(&conn, batch_size)?;
        Ok(MaintenanceSummary {
            username_lc_updated,
            search_text_updated,
        })
    }

    fn run_once_migration<F>(&self, conn: &Connection, key: &str, op: F) -> Result<()>
    where
        F: FnOnce(&Connection) -> Result<()>,
    {
        let already_done = conn.query_row(
            "SELECT 1 FROM schema_meta WHERE key = ?1 LIMIT 1",
            [key],
            |_| Ok(()),
        );
        if already_done.is_ok() {
            return Ok(());
        }

        op(conn)?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta(key, value) VALUES (?1, 'done')",
            [key],
        )?;
        Ok(())
    }

    pub fn tweet_exists(&self, id: i64) -> Result<bool> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT 1 FROM tweets WHERE id = ? LIMIT 1")?;
        let mut rows = stmt.query([id])?;
        Ok(rows.next()?.is_some())
    }

    pub fn missing_ids(&self, ids: &[i64]) -> Result<Vec<i64>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        // ids are small (quotes/retweets) so a simple per-id exists check is fine,
        // but reuse a single connection for efficiency.
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT 1 FROM tweets WHERE id = ? LIMIT 1")?;
        let mut missing = Vec::new();
        for &id in ids {
            if id <= 0 {
                continue;
            }
            let exists = stmt.exists([id])?;
            if !exists {
                missing.push(id);
            }
        }
        Ok(missing)
    }

    pub fn upsert_tweet(&self, tweet: &Tweet) -> Result<UpsertOutcome> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        let quote_id = tweet.quote_id.unwrap_or(0);
        let retweet_id = tweet.retweet_id.unwrap_or(0);
        let reply_to_id = tweet.reply_to_id.unwrap_or(0);
        let conversation_id = tweet.conversation_id.unwrap_or(0);
        let entities_json = tweet.entities_json.clone().unwrap_or_default();
        let x_raw_json = tweet.x_raw_json.clone().unwrap_or_default();
        let user_pic = tweet.user_pic.clone().unwrap_or_default();
        let photos = tweet.photos.clone().unwrap_or_else(|| "[]".to_string());
        let videos = tweet.videos.clone().unwrap_or_else(|| "[]".to_string());

        tx.execute(
            INSERT_SQL,
            params![
                tweet.id,
                tweet.user_id,
                tweet.username,
                tweet.username_lc,
                tweet.fullname,
                tweet.text,
                tweet.search_text,
                tweet.created_at,
                tweet.reply_count,
                tweet.retweet_count,
                tweet.like_count,
                tweet.view_count,
                tweet.feed_kind,
                tweet.archived_at,
                user_pic,
                photos,
                quote_id,
                retweet_id,
                reply_to_id,
                conversation_id,
                entities_json,
                x_raw_json,
                videos,
            ],
        )?;

        let inserted_tweet = tx.changes() > 0;

        // Merge enrichment for existing rows (write-through cache).
        if !inserted_tweet {
            tx.execute(
                UPDATE_SQL,
                params![
                    tweet.id,
                    tweet.user_id,
                    tweet.username,
                    tweet.username_lc,
                    tweet.fullname,
                    tweet.text,
                    tweet.search_text,
                    tweet.created_at,
                    tweet.reply_count,
                    tweet.retweet_count,
                    tweet.like_count,
                    tweet.view_count,
                    tweet.feed_kind,
                    tweet.archived_at,
                    user_pic,
                    photos,
                    quote_id,
                    retweet_id,
                    reply_to_id,
                    conversation_id,
                    entities_json,
                    x_raw_json,
                    videos,
                ],
            )?;
        }

        let mut inserted_feed = false;
        if !tweet.feed_kind.trim().is_empty() {
            tx.execute(
                "INSERT OR IGNORE INTO tweet_feeds(tweet_id, feed_kind, archived_at) VALUES (?1, ?2, ?3)",
                params![tweet.id, tweet.feed_kind, tweet.archived_at],
            )?;
            inserted_feed = tx.changes() > 0;
        }

        tx.commit()?;
        Ok(UpsertOutcome {
            inserted_tweet,
            inserted_feed,
        })
    }

    /// Find tweets with `retweet_id = 0` whose text starts with "RT @user:" (HTML)
    /// and try to resolve the original tweet ID from existing rows in the DB.
    /// Returns the number of rows patched.
    pub fn resolve_missing_retweet_ids(&self) -> Result<usize> {
        let conn = self.conn()?;

        // Collect broken retweet rows.
        let mut stmt = conn.prepare(
            "SELECT id, text FROM tweets WHERE (retweet_id IS NULL OR retweet_id = 0) AND text LIKE 'RT %'",
        )?;
        let broken: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        if broken.is_empty() {
            return Ok(0);
        }

        // Group by username for efficiency.
        let mut by_user: std::collections::HashMap<String, Vec<(i64, String)>> =
            std::collections::HashMap::new();
        for (rt_id, text) in broken {
            if let Some(username) = extract_rt_username(&text) {
                let content = extract_rt_content(&text, &username);
                by_user
                    .entry(username.to_lowercase())
                    .or_default()
                    .push((rt_id, content));
            }
        }

        let mut update_stmt = conn.prepare(
            "UPDATE tweets SET retweet_id = ?1 WHERE id = ?2 AND (retweet_id IS NULL OR retweet_id = 0)",
        )?;

        let mut patched = 0usize;
        for (username, rts) in &by_user {
            // Load all candidate tweets by this user once.
            let mut cand_stmt = conn.prepare(
                "SELECT id, text FROM tweets WHERE LOWER(username) = ?1 AND text <> '' AND text NOT LIKE 'RT %' ORDER BY created_at DESC",
            )?;
            let candidates: Vec<(i64, String)> = cand_stmt
                .query_map([username], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect();

            if candidates.is_empty() {
                continue;
            }

            // Pre-compute stripped text prefixes for candidates.
            let cand_prefixes: Vec<(i64, String)> = candidates
                .iter()
                .map(|(id, text)| {
                    let plain: String = strip_html_tags(text).chars().take(25).collect();
                    (*id, plain)
                })
                .collect();

            for (rt_id, content) in rts {
                let rt_plain: String = strip_html_tags(content).chars().take(25).collect();
                if rt_plain.is_empty() {
                    // No content to match; take the most recent candidate.
                    if let Some((orig_id, _)) = cand_prefixes.first() {
                        update_stmt.execute(params![orig_id, rt_id])?;
                        patched += 1;
                    }
                    continue;
                }
                // Find first candidate whose text prefix matches.
                for (orig_id, orig_prefix) in &cand_prefixes {
                    if !orig_prefix.is_empty() && orig_prefix == &rt_plain {
                        update_stmt.execute(params![orig_id, rt_id])?;
                        patched += 1;
                        break;
                    }
                }
            }
        }
        Ok(patched)
    }

    /// Return IDs of tweets that look like retweets (text starts with "RT ")
    /// but have no `retweet_id` set.
    pub fn broken_retweet_ids(&self, limit: usize) -> Result<Vec<i64>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM tweets WHERE (retweet_id IS NULL OR retweet_id = 0) AND text LIKE 'RT %' ORDER BY id DESC LIMIT ?1",
        )?;
        let ids = stmt
            .query_map([limit as i64], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    /// Return (username, user_id) pairs for the original authors of broken retweets.
    /// `user_id` comes from existing tweets by that user in the DB (may be empty).
    /// Results are ordered by frequency (most-retweeted users first).
    pub fn broken_retweet_users(&self, limit: usize) -> Result<Vec<(String, String)>> {
        let conn = self.conn()?;
        // Get all broken RT texts, extract usernames in Rust (SQL can't reliably parse HTML).
        let mut stmt = conn.prepare(
            "SELECT text FROM tweets WHERE (retweet_id IS NULL OR retweet_id = 0) AND text LIKE 'RT %'",
        )?;
        let texts: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        // Count occurrences per username.
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for text in &texts {
            if let Some(u) = extract_rt_username(text) {
                *counts.entry(u.to_lowercase()).or_default() += 1;
            }
        }

        // Sort by frequency descending.
        let mut pairs: Vec<_> = counts.into_iter().collect();
        pairs.sort_by_key(|pair| std::cmp::Reverse(pair.1));
        pairs.truncate(limit);

        // Look up user_id from existing tweets for each username.
        let mut lookup = conn.prepare(
            "SELECT user_id FROM tweets WHERE LOWER(username) = ?1 AND user_id <> '' LIMIT 1",
        )?;
        let result: Vec<(String, String)> = pairs
            .into_iter()
            .map(|(username, _count)| {
                let user_id: String = lookup
                    .query_row([&username], |row| row.get(0))
                    .unwrap_or_default();
                (username, user_id)
            })
            .collect();
        Ok(result)
    }

    pub fn upsert_tweets(&self, tweets: &[Tweet]) -> Result<usize> {
        if tweets.is_empty() {
            return Ok(0);
        }

        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        let mut inserted_count = 0usize;

        let mut insert_stmt = tx.prepare(INSERT_SQL)?;
        let mut update_stmt = tx.prepare(UPDATE_SQL)?;
        let mut feed_stmt = tx.prepare(
            "INSERT OR IGNORE INTO tweet_feeds(tweet_id, feed_kind, archived_at) VALUES (?1, ?2, ?3)",
        )?;

        for tweet in tweets {
            let quote_id = tweet.quote_id.unwrap_or(0);
            let retweet_id = tweet.retweet_id.unwrap_or(0);
            let reply_to_id = tweet.reply_to_id.unwrap_or(0);
            let conversation_id = tweet.conversation_id.unwrap_or(0);
            let user_pic = tweet.user_pic.clone().unwrap_or_default();
            let photos = tweet.photos.clone().unwrap_or_else(|| "[]".to_string());
            let entities_json = tweet.entities_json.clone().unwrap_or_default();
            let x_raw_json = tweet.x_raw_json.clone().unwrap_or_default();
            let videos = tweet.videos.clone().unwrap_or_else(|| "[]".to_string());

            insert_stmt.execute(params![
                tweet.id,
                tweet.user_id,
                tweet.username,
                tweet.username_lc,
                tweet.fullname,
                tweet.text,
                tweet.search_text,
                tweet.created_at,
                tweet.reply_count,
                tweet.retweet_count,
                tweet.like_count,
                tweet.view_count,
                tweet.feed_kind,
                tweet.archived_at,
                user_pic,
                photos,
                quote_id,
                retweet_id,
                reply_to_id,
                conversation_id,
                entities_json,
                x_raw_json,
                videos,
            ])?;

            let inserted_tweet = tx.changes() > 0;
            if !inserted_tweet {
                update_stmt.execute(params![
                    tweet.id,
                    tweet.user_id,
                    tweet.username,
                    tweet.username_lc,
                    tweet.fullname,
                    tweet.text,
                    tweet.search_text,
                    tweet.created_at,
                    tweet.reply_count,
                    tweet.retweet_count,
                    tweet.like_count,
                    tweet.view_count,
                    tweet.feed_kind,
                    tweet.archived_at,
                    user_pic,
                    photos,
                    quote_id,
                    retweet_id,
                    reply_to_id,
                    conversation_id,
                    entities_json,
                    x_raw_json,
                    videos,
                ])?;
            }

            if !tweet.feed_kind.trim().is_empty() {
                feed_stmt.execute(params![tweet.id, tweet.feed_kind, tweet.archived_at])?;
                let inserted_feed = tx.changes() > 0;
                if inserted_feed {
                    inserted_count += 1;
                }
            }
        }

        drop(insert_stmt);
        drop(update_stmt);
        drop(feed_stmt);
        tx.commit()?;
        Ok(inserted_count)
    }
}

fn has_missing_username_lc(conn: &Connection) -> Result<bool> {
    let exists = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM tweets WHERE username_lc IS NULL OR username_lc = '' LIMIT 1
         )",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(exists != 0)
}

fn has_missing_search_text(conn: &Connection) -> Result<bool> {
    let exists = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM tweets WHERE search_text IS NULL OR search_text = '' LIMIT 1
         )",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(exists != 0)
}

fn backfill_username_lc(conn: &Connection, batch_size: usize) -> Result<usize> {
    if batch_size == 0 {
        return Ok(0);
    }

    let mut total_changed = 0usize;
    let mut last_id = 0i64;

    loop {
        let mut stmt = conn.prepare(
            "SELECT id, username
             FROM tweets
             WHERE id > ?1
               AND (username_lc IS NULL OR username_lc = '')
             ORDER BY id
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![last_id, batch_size as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut updates = Vec::new();
        let mut max_id = last_id;
        for row in rows {
            let (id, username) = row?;
            max_id = id;
            if !username.trim().is_empty() {
                updates.push((id, username.to_lowercase()));
            }
        }
        drop(stmt);

        if max_id == last_id {
            break;
        }

        if !updates.is_empty() {
            let tx = conn.unchecked_transaction()?;
            let mut update = tx.prepare("UPDATE tweets SET username_lc = ?1 WHERE id = ?2")?;
            for (id, username_lc) in &updates {
                total_changed += update.execute(params![username_lc, id])?;
            }
            drop(update);
            tx.commit()?;
        }

        last_id = max_id;
    }

    Ok(total_changed)
}

fn backfill_search_text(conn: &Connection, batch_size: usize) -> Result<usize> {
    if batch_size == 0 {
        return Ok(0);
    }

    let mut total_changed = 0usize;
    let mut last_id = 0i64;

    loop {
        let mut stmt = conn.prepare(
            "SELECT id, text, x_raw_json
             FROM tweets
             WHERE id > ?1
               AND (search_text IS NULL OR search_text = '')
             ORDER BY id
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![last_id, batch_size as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2).unwrap_or_default(),
            ))
        })?;

        let mut updates = Vec::new();
        let mut max_id = last_id;
        for row in rows {
            let (id, html, raw_json) = row?;
            max_id = id;
            let search_text = if !raw_json.trim().is_empty() {
                search_text_from_raw_json(&raw_json).unwrap_or_else(|| html_to_search_text(&html))
            } else {
                html_to_search_text(&html)
            };
            if !search_text.is_empty() {
                updates.push((id, search_text));
            }
        }
        drop(stmt);

        if max_id == last_id {
            break;
        }

        if !updates.is_empty() {
            let tx = conn.unchecked_transaction()?;
            let mut update = tx.prepare("UPDATE tweets SET search_text = ?1 WHERE id = ?2")?;
            for (id, search_text) in &updates {
                total_changed += update.execute(params![search_text, id])?;
            }
            drop(update);
            tx.commit()?;
        }

        last_id = max_id;
    }

    Ok(total_changed)
}

/// Extract the retweeted username from text that starts with "RT @user:" or
/// "RT <a href=...>@user</a>:".
fn extract_rt_username(text: &str) -> Option<String> {
    let text = text.strip_prefix("RT ")?;
    // HTML form: <a href="...">@username</a>:
    if text.starts_with("<a ") {
        let at_pos = text.find('@')?;
        let end = text[at_pos + 1..].find(|c: char| !c.is_alphanumeric() && c != '_')?;
        let username = &text[at_pos + 1..at_pos + 1 + end];
        if !username.is_empty() {
            return Some(username.to_string());
        }
    }
    // Plain text form: @username:
    let text = text.strip_prefix('@')?;
    let end = text.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    let username = &text[..end];
    if !username.is_empty() {
        Some(username.to_string())
    } else {
        None
    }
}

/// Extract the content after the "RT @user: " prefix (handles both HTML and plain text).
fn extract_rt_content(text: &str, username: &str) -> String {
    // Try HTML form: RT <a ...>@username</a>: content
    if let Some(pos) = text.find(&format!("@{}</a>: ", username)) {
        let start = pos + format!("@{}</a>: ", username).len();
        return text[start..].to_string();
    }
    if let Some(pos) = text.find(&format!("@{}</a>:", username)) {
        let start = pos + format!("@{}</a>:", username).len();
        return text[start..].trim_start().to_string();
    }
    // Plain text form: RT @username: content
    if let Some(pos) = text.find(&format!("@{}: ", username)) {
        let start = pos + format!("@{}: ", username).len();
        return text[start..].to_string();
    }
    String::new()
}

/// Naive HTML tag stripper for text comparison.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out
}
