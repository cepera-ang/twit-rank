use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::time_util::now_unix_timestamp;

pub struct SqliteCache {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct UserFeedbackStats {
    pub likes: i64,
    pub dislikes: i64,
}

impl SqliteCache {
    pub fn open(path: PathBuf) -> Result<Self> {
        // Ensure parent dir exists
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).ok();
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("open feedback db: {}", path.display()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS feedback (
                post_id TEXT PRIMARY KEY,
                value INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_feedback (
                user TEXT PRIMARY KEY,
                likes INTEGER NOT NULL,
                dislikes INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;

        // Migration: add reason column
        let _ = conn.execute("ALTER TABLE feedback ADD COLUMN reason TEXT DEFAULT ''", []);

        Ok(Self { path })
    }

    pub fn feedback_map_for_posts(&self, ids: &[i64]) -> Result<HashMap<i64, i64>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = Connection::open(&self.path)
            .with_context(|| format!("reopen feedback db: {}", self.path.display()))?;
        let mut map = HashMap::new();
        let mut stmt = conn.prepare("SELECT post_id, value FROM feedback WHERE post_id = ?1")?;
        for id in ids {
            let id_str = id.to_string();
            if let Some((_, value)) = stmt
                .query_row(params![id_str], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .optional()?
            {
                map.insert(*id, value);
            }
        }
        Ok(map)
    }

    pub fn user_feedback_map(&self) -> Result<HashMap<String, UserFeedbackStats>> {
        let conn = Connection::open(&self.path)
            .with_context(|| format!("reopen feedback db: {}", self.path.display()))?;
        let mut map = HashMap::new();
        let mut stmt = conn.prepare("SELECT user, likes, dislikes FROM user_feedback")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let user: String = row.get(0)?;
            let likes: i64 = row.get(1)?;
            let dislikes: i64 = row.get(2)?;
            map.insert(user.to_lowercase(), UserFeedbackStats { likes, dislikes });
        }
        Ok(map)
    }

    pub fn set_feedback(
        &self,
        post_id: i64,
        user: &str,
        value: i64,
        reason: Option<&str>,
    ) -> Result<()> {
        if value != 1 && value != -1 {
            return Ok(());
        }
        let post_id_str = post_id.to_string();
        let reason_str = reason.unwrap_or("");
        let mut conn = Connection::open(&self.path)
            .with_context(|| format!("reopen feedback db: {}", self.path.display()))?;
        let tx = conn.transaction()?;

        let prev: Option<i64> = tx
            .query_row(
                "SELECT value FROM feedback WHERE post_id = ?1",
                params![post_id_str],
                |row| row.get(0),
            )
            .optional()?;

        if prev == Some(value) {
            // Same value, just update reason
            let now = now_unix_timestamp();
            tx.execute(
                "UPDATE feedback SET reason = ?2, updated_at = ?3 WHERE post_id = ?1",
                params![post_id_str, reason_str, now],
            )?;
            tx.commit()?;
            return Ok(());
        }

        let now = now_unix_timestamp();
        tx.execute(
            "INSERT OR REPLACE INTO feedback (post_id, value, updated_at, reason) VALUES (?1, ?2, ?3, ?4)",
            params![post_id_str, value, now, reason_str],
        )?;

        let mut like_delta = 0i64;
        let mut dislike_delta = 0i64;
        if let Some(p) = prev {
            if p == 1 {
                like_delta -= 1;
            } else if p == -1 {
                dislike_delta -= 1;
            }
        }
        if value == 1 {
            like_delta += 1;
        } else if value == -1 {
            dislike_delta += 1;
        }

        tx.execute(
            "INSERT OR IGNORE INTO user_feedback (user, likes, dislikes, updated_at) VALUES (?1, 0, 0, ?2)",
            params![user, now],
        )?;
        tx.execute(
            "UPDATE user_feedback SET likes = likes + ?2, dislikes = dislikes + ?3, updated_at = ?4 WHERE user = ?1",
            params![user, like_delta, dislike_delta, now],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn delete_feedback(&self, post_id: i64, user: &str) -> Result<()> {
        let post_id_str = post_id.to_string();
        let mut conn = Connection::open(&self.path)
            .with_context(|| format!("reopen feedback db: {}", self.path.display()))?;
        let tx = conn.transaction()?;

        // Get previous value to update user stats
        let prev: Option<i64> = tx
            .query_row(
                "SELECT value FROM feedback WHERE post_id = ?1",
                params![post_id_str],
                |row| row.get(0),
            )
            .optional()?;

        if prev.is_none() {
            return Ok(()); // Nothing to delete
        }

        let now = now_unix_timestamp();
        tx.execute(
            "DELETE FROM feedback WHERE post_id = ?1",
            params![post_id_str],
        )?;

        // Update user stats
        let mut like_delta = 0i64;
        let mut dislike_delta = 0i64;
        if let Some(p) = prev {
            if p == 1 {
                like_delta -= 1;
            } else if p == -1 {
                dislike_delta -= 1;
            }
        }

        tx.execute(
            "UPDATE user_feedback SET likes = likes + ?2, dislikes = dislikes + ?3, updated_at = ?4 WHERE user = ?1",
            params![user, like_delta, dislike_delta, now],
        )?;

        tx.commit()?;
        Ok(())
    }
}
