use crate::model::Tweet;
use crate::time_util::parse_rfc2822_or_rfc3339;
use anyhow::{Context, Result};
use regex::Regex;
use rusqlite::{Connection, OpenFlags, Row, ToSql};
use std::path::PathBuf;
use time::macros::format_description;
use time::{Date, Duration, PrimitiveDateTime, UtcOffset};

const TWEET_SELECT: &str = "
    id, user_id, username, username_lc, fullname, text, search_text, created_at,
    reply_count, retweet_count, like_count, view_count,
    feed_kind, archived_at, user_pic, photos, quote_id, retweet_id,
    reply_to_id, conversation_id, entities_json, x_raw_json, videos
";

const TWEET_SELECT_FROM_T: &str = "
    t.id, t.user_id, t.username, t.username_lc, t.fullname, t.text, t.search_text, t.created_at,
    t.reply_count, t.retweet_count, t.like_count, t.view_count,
    t.feed_kind, t.archived_at, t.user_pic, t.photos, t.quote_id, t.retweet_id,
    t.reply_to_id, t.conversation_id, t.entities_json, t.x_raw_json, t.videos
";

const DATE_ONLY: &[time::format_description::BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

pub struct Archive {
    path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub query: Option<String>,
    pub mode: SearchMode,
    pub author: Option<String>,
    pub feed: Option<String>,
    pub created_from: Option<String>,
    pub created_to: Option<String>,
    pub min_likes: Option<i64>,
    pub min_retweets: Option<i64>,
    pub min_replies: Option<i64>,
    pub min_views: Option<i64>,
    pub has_photos: bool,
    pub has_videos: bool,
    pub has_media: bool,
    pub kind: SearchKind,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Literal,
    Regex,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchKind {
    #[default]
    Any,
    Original,
    Reply,
    Quote,
    Retweet,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResponse {
    pub tweets: Vec<Tweet>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LiteralToken {
    Term(String),
    Phrase(String),
}

#[derive(Debug, Clone)]
enum ParamValue {
    Text(String),
    Int(i64),
}

impl Archive {
    pub fn open(path: &str) -> Result<Self> {
        Ok(Self {
            path: PathBuf::from(path),
        })
    }

    fn conn(&self) -> Result<Connection> {
        Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("open archive db {}", self.path.display()))
    }

    pub fn get_tweets(
        &self,
        limit: usize,
        offset: usize,
        search_query: Option<String>,
        feed_kind: Option<String>,
    ) -> Result<Vec<Tweet>> {
        let conn = self.conn()?;
        let mut sql = String::from("SELECT ");
        let mut params = Vec::new();

        match (feed_kind.as_ref(), search_query.as_ref()) {
            (Some(kind), Some(q)) => {
                sql.push_str(TWEET_SELECT_FROM_T);
                sql.push_str(
                    " FROM tweet_feeds f
                      JOIN tweets t ON t.id = f.tweet_id
                      WHERE f.feed_kind = ? AND t.text LIKE ?
                      ORDER BY t.created_at DESC, t.id DESC
                      LIMIT ? OFFSET ?",
                );
                params.push(ParamValue::Text(kind.to_string()));
                params.push(ParamValue::Text(format!("%{}%", q)));
            }
            (Some(kind), None) => {
                sql.push_str(TWEET_SELECT_FROM_T);
                sql.push_str(
                    " FROM tweet_feeds f
                      JOIN tweets t ON t.id = f.tweet_id
                      WHERE f.feed_kind = ?
                      ORDER BY f.tweet_id DESC
                      LIMIT ? OFFSET ?",
                );
                params.push(ParamValue::Text(kind.to_string()));
            }
            (None, Some(q)) => {
                sql.push_str(TWEET_SELECT);
                sql.push_str(
                    " FROM tweets
                      WHERE text LIKE ?
                      ORDER BY created_at DESC, id DESC
                      LIMIT ? OFFSET ?",
                );
                params.push(ParamValue::Text(format!("%{}%", q)));
            }
            (None, None) => {
                sql.push_str(TWEET_SELECT);
                sql.push_str(" FROM tweets ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?");
            }
        }

        params.push(ParamValue::Int(
            i64::try_from(limit).context("limit exceeds SQLite integer range")?,
        ));
        params.push(ParamValue::Int(
            i64::try_from(offset).context("offset exceeds SQLite integer range")?,
        ));

        let mut stmt = conn.prepare(&sql)?;
        query_tweets(&mut stmt, &params)
    }

    pub fn search_tweets(&self, params: &SearchParams) -> Result<SearchResponse> {
        let conn = self.conn()?;
        match params.mode {
            SearchMode::Literal => self.search_literal(&conn, params),
            SearchMode::Regex => self.search_regex(&conn, params),
        }
    }

    pub fn get_lists(&self) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT feed_kind FROM tweet_feeds
             WHERE feed_kind LIKE 'list:%' AND feed_kind NOT LIKE '%:quote'",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut lists = Vec::new();
        for r in rows {
            lists.push(r?);
        }
        lists.sort();
        Ok(lists)
    }

    pub fn count_tweets(
        &self,
        search_query: Option<String>,
        feed_kind: Option<String>,
    ) -> Result<usize> {
        let conn = self.conn()?;
        let mut sql = String::new();
        let mut params = Vec::new();

        match (feed_kind.as_ref(), search_query.as_ref()) {
            (Some(kind), Some(q)) => {
                sql.push_str(
                    "SELECT COUNT(*) FROM tweet_feeds f JOIN tweets t ON t.id = f.tweet_id
                     WHERE f.feed_kind = ? AND t.text LIKE ?",
                );
                params.push(ParamValue::Text(kind.to_string()));
                params.push(ParamValue::Text(format!("%{}%", q)));
            }
            (Some(kind), None) => {
                sql.push_str("SELECT COUNT(*) FROM tweet_feeds WHERE feed_kind = ?");
                params.push(ParamValue::Text(kind.to_string()));
            }
            (None, Some(q)) => {
                sql.push_str("SELECT COUNT(*) FROM tweets WHERE text LIKE ?");
                params.push(ParamValue::Text(format!("%{}%", q)));
            }
            (None, None) => sql.push_str("SELECT COUNT(*) FROM tweets"),
        }

        let refs = param_refs(&params);
        let count: i64 = conn.query_row(&sql, refs.as_slice(), |row| row.get(0))?;
        usize::try_from(count).context("count exceeded usize")
    }

    pub fn get_tweets_by_ids(&self, ids: &[i64]) -> Result<Vec<Tweet>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn()?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT {TWEET_SELECT} FROM tweets WHERE id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let refs = ids.iter().map(|id| id as &dyn ToSql).collect::<Vec<_>>();
        let rows = stmt.query_map(refs.as_slice(), map_tweet_row)?;
        let mut tweets = Vec::new();
        for row in rows {
            tweets.push(row?);
        }
        Ok(tweets)
    }

    pub fn get_tweet_by_id(&self, id: i64) -> Result<Option<Tweet>> {
        let conn = self.conn()?;
        let sql = format!("SELECT {TWEET_SELECT} FROM tweets WHERE id = ?");
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_tweet_row(row)?))
        } else {
            Ok(None)
        }
    }

    fn search_literal(&self, conn: &Connection, params: &SearchParams) -> Result<SearchResponse> {
        let tokens = parse_literal_query(params.query.as_deref().unwrap_or_default());
        let mut where_sql = Vec::new();
        let mut values = Vec::new();
        build_metadata_filters(&mut where_sql, &mut values, params)?;
        for token in tokens {
            match token {
                LiteralToken::Term(term) | LiteralToken::Phrase(term) => {
                    where_sql.push("t.search_text LIKE ?".to_string());
                    values.push(ParamValue::Text(format!("%{}%", term)));
                }
            }
        }

        let where_clause = join_where(&where_sql);
        let count_sql = format!("SELECT COUNT(*) FROM tweets t{where_clause}");
        let total = query_count(conn, &count_sql, &values)?;

        let mut page_values = clone_params(&values);
        page_values.push(ParamValue::Int(
            i64::try_from(params.limit).context("limit exceeds SQLite integer range")?,
        ));
        page_values.push(ParamValue::Int(
            i64::try_from(params.offset).context("offset exceeds SQLite integer range")?,
        ));
        let sql = format!(
            "SELECT {TWEET_SELECT} FROM tweets t{where_clause}
             ORDER BY t.created_at DESC, t.id DESC
             LIMIT ? OFFSET ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        let tweets = query_tweets(&mut stmt, &page_values)?;
        Ok(SearchResponse { tweets, total })
    }

    fn search_regex(&self, conn: &Connection, params: &SearchParams) -> Result<SearchResponse> {
        let pattern = params.query.as_deref().unwrap_or_default().trim();
        let regex = Regex::new(pattern).with_context(|| "invalid regex")?;

        let mut where_sql = Vec::new();
        let mut values = Vec::new();
        build_metadata_filters(&mut where_sql, &mut values, params)?;
        let where_clause = join_where(&where_sql);
        let sql = format!(
            "SELECT {TWEET_SELECT} FROM tweets t{where_clause}
             ORDER BY t.created_at DESC, t.id DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let candidates = query_tweets(&mut stmt, &values)?;

        let mut total = 0usize;
        let mut tweets = Vec::new();
        for tweet in candidates {
            if regex.is_match(&tweet.search_text) {
                if total >= params.offset && tweets.len() < params.limit {
                    tweets.push(tweet);
                }
                total += 1;
            }
        }
        Ok(SearchResponse { tweets, total })
    }
}

fn query_tweets(stmt: &mut rusqlite::Statement<'_>, params: &[ParamValue]) -> Result<Vec<Tweet>> {
    let refs = param_refs(params);
    let rows = stmt.query_map(refs.as_slice(), map_tweet_row)?;
    let mut tweets = Vec::new();
    for row in rows {
        tweets.push(row?);
    }
    Ok(tweets)
}

fn map_tweet_row(row: &Row<'_>) -> rusqlite::Result<Tweet> {
    Ok(Tweet {
        id: row.get(0)?,
        user_id: row.get(1)?,
        username: row.get(2)?,
        username_lc: row.get(3)?,
        fullname: row.get(4)?,
        text: row.get(5)?,
        search_text: row.get(6)?,
        created_at: row.get(7)?,
        reply_count: row.get(8)?,
        retweet_count: row.get(9)?,
        like_count: row.get(10)?,
        view_count: row.get(11)?,
        feed_kind: row.get(12)?,
        archived_at: row.get(13)?,
        user_pic: row.get(14).ok(),
        photos: row.get(15).ok(),
        quote_id: row.get(16).ok(),
        retweet_id: row.get(17).ok(),
        reply_to_id: row.get(18).ok(),
        conversation_id: row.get(19).ok(),
        entities_json: row.get(20).ok(),
        x_raw_json: row.get(21).ok(),
        videos: row.get(22).ok(),
    })
}

fn build_metadata_filters(
    where_sql: &mut Vec<String>,
    values: &mut Vec<ParamValue>,
    params: &SearchParams,
) -> Result<()> {
    if let Some(feed) = params.feed.as_ref().filter(|s| !s.trim().is_empty()) {
        where_sql.push("t.feed_kind = ?".to_string());
        values.push(ParamValue::Text(feed.trim().to_string()));
    }

    if let Some(author) = params
        .author
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        where_sql.push("(t.username_lc = ? OR lower(t.fullname) LIKE ?)".to_string());
        let lc = author.to_ascii_lowercase();
        values.push(ParamValue::Text(lc.clone()));
        values.push(ParamValue::Text(format!("%{}%", lc)));
    }

    if let Some(from) = params
        .created_from
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        where_sql.push("t.created_at >= ?".to_string());
        values.push(ParamValue::Text(parse_search_bound(from, false)?));
    }
    if let Some(to) = params
        .created_to
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        where_sql.push("t.created_at <= ?".to_string());
        values.push(ParamValue::Text(parse_search_bound(to, true)?));
    }

    if let Some(min) = params.min_likes {
        where_sql.push("t.like_count >= ?".to_string());
        values.push(ParamValue::Int(min));
    }
    if let Some(min) = params.min_retweets {
        where_sql.push("t.retweet_count >= ?".to_string());
        values.push(ParamValue::Int(min));
    }
    if let Some(min) = params.min_replies {
        where_sql.push("t.reply_count >= ?".to_string());
        values.push(ParamValue::Int(min));
    }
    if let Some(min) = params.min_views {
        where_sql.push("t.view_count >= ?".to_string());
        values.push(ParamValue::Int(min));
    }

    if params.has_photos {
        where_sql.push("t.photos <> '[]'".to_string());
    }
    if params.has_videos {
        where_sql.push("t.videos <> '[]'".to_string());
    }
    if params.has_media {
        where_sql.push("(t.photos <> '[]' OR t.videos <> '[]')".to_string());
    }

    match params.kind {
        SearchKind::Any => {}
        SearchKind::Original => where_sql.push(
            "(coalesce(t.reply_to_id, 0) = 0 AND coalesce(t.quote_id, 0) = 0 AND coalesce(t.retweet_id, 0) = 0)"
                .to_string(),
        ),
        SearchKind::Reply => where_sql.push("coalesce(t.reply_to_id, 0) > 0".to_string()),
        SearchKind::Quote => where_sql.push("coalesce(t.quote_id, 0) > 0".to_string()),
        SearchKind::Retweet => where_sql.push("coalesce(t.retweet_id, 0) > 0".to_string()),
    }

    Ok(())
}

fn parse_literal_query(query: &str) -> Vec<LiteralToken> {
    let mut tokens = Vec::new();
    let mut chars = query.chars().peekable();

    while let Some(ch) = chars.peek().copied() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch == '"' {
            chars.next();
            let mut phrase = String::new();
            for next in chars.by_ref() {
                if next == '"' {
                    break;
                }
                phrase.push(next);
            }
            let phrase = phrase.trim();
            if !phrase.is_empty() {
                tokens.push(LiteralToken::Phrase(phrase.to_ascii_lowercase()));
            }
            continue;
        }

        let mut term = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_whitespace() {
                break;
            }
            term.push(next);
            chars.next();
        }
        let term = term.trim();
        if !term.is_empty() {
            tokens.push(LiteralToken::Term(term.to_ascii_lowercase()));
        }
    }

    tokens
}

fn parse_search_bound(input: &str, end_of_day: bool) -> Result<String> {
    let trimmed = input.trim();
    if let Some(dt) = parse_rfc2822_or_rfc3339(trimmed) {
        return Ok(dt
            .to_offset(UtcOffset::UTC)
            .format(&time::format_description::well_known::Rfc3339)
            .expect("rfc3339 formatting should succeed"));
    }

    let date = Date::parse(trimmed, DATE_ONLY)
        .with_context(|| format!("invalid date/time filter: {trimmed}"))?;
    let base = PrimitiveDateTime::new(date, time::Time::MIDNIGHT).assume_utc();
    let dt = if end_of_day {
        base + Duration::days(1) - Duration::seconds(1)
    } else {
        base
    };
    Ok(dt
        .format(&time::format_description::well_known::Rfc3339)
        .expect("rfc3339 formatting should succeed"))
}

fn join_where(filters: &[String]) -> String {
    if filters.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", filters.join(" AND "))
    }
}

fn clone_params(values: &[ParamValue]) -> Vec<ParamValue> {
    values.to_vec()
}

fn param_refs(values: &[ParamValue]) -> Vec<&dyn ToSql> {
    values
        .iter()
        .map(|value| match value {
            ParamValue::Text(v) => v as &dyn ToSql,
            ParamValue::Int(v) => v as &dyn ToSql,
        })
        .collect()
}

fn query_count(conn: &Connection, sql: &str, values: &[ParamValue]) -> Result<usize> {
    let refs = param_refs(values);
    let count: i64 = conn.query_row(sql, refs.as_slice(), |row| row.get(0))?;
    usize::try_from(count).context("count exceeded usize")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::archive_writer::ArchiveWriter;
    use crate::model::Tweet;

    use super::{
        parse_literal_query, parse_search_bound, Archive, LiteralToken, SearchKind, SearchMode,
        SearchParams,
    };

    #[test]
    fn parse_literal_query_splits_terms_and_phrases() {
        assert_eq!(
            parse_literal_query("rust async \"quoted phrase\""),
            vec![
                LiteralToken::Term("rust".to_string()),
                LiteralToken::Term("async".to_string()),
                LiteralToken::Phrase("quoted phrase".to_string())
            ]
        );
    }

    #[test]
    fn parse_search_bound_supports_date_only() {
        assert_eq!(
            parse_search_bound("2026-03-07", false).unwrap(),
            "2026-03-07T00:00:00Z"
        );
        assert_eq!(
            parse_search_bound("2026-03-07", true).unwrap(),
            "2026-03-07T23:59:59Z"
        );
    }

    #[test]
    fn search_tweets_supports_literal_and_regex_filters() {
        let path = temp_db_path("search");
        let wal_path = PathBuf::from(format!("{}-wal", path.display()));
        let shm_path = PathBuf::from(format!("{}-shm", path.display()));
        let writer = ArchiveWriter::open(&path).unwrap();
        let _ = writer.init_schema().unwrap();
        writer
            .upsert_tweets(&[
                test_tweet(1, "alice", "Rust async search", "2026-03-07T10:00:00Z"),
                test_tweet(2, "bob", "Regex powered filters", "2026-03-06T10:00:00Z"),
                test_tweet(3, "alice", "No match here", "2026-03-05T10:00:00Z"),
            ])
            .unwrap();

        let archive = Archive::open(path.to_str().unwrap()).unwrap();
        let literal = archive
            .search_tweets(&SearchParams {
                query: Some("rust async".to_string()),
                mode: SearchMode::Literal,
                kind: SearchKind::Any,
                limit: 10,
                offset: 0,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(literal.total, 1);
        assert_eq!(literal.tweets[0].id, 1);

        let regex = archive
            .search_tweets(&SearchParams {
                query: Some("regex|rust".to_string()),
                mode: SearchMode::Regex,
                author: Some("alice".to_string()),
                kind: SearchKind::Any,
                limit: 10,
                offset: 0,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(regex.total, 1);
        assert_eq!(regex.tweets[0].id, 1);

        drop(archive);
        drop(writer);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(wal_path);
        let _ = std::fs::remove_file(shm_path);
    }

    fn test_tweet(id: i64, username: &str, search_text: &str, created_at: &str) -> Tweet {
        Tweet {
            id,
            user_id: id.to_string(),
            username: username.to_string(),
            username_lc: username.to_ascii_lowercase(),
            fullname: username.to_string(),
            text: format!("<p>{search_text}</p>"),
            search_text: search_text.to_ascii_lowercase(),
            created_at: created_at.to_string(),
            reply_count: 0,
            retweet_count: 0,
            like_count: 0,
            view_count: 0,
            feed_kind: "following".to_string(),
            archived_at: created_at.to_string(),
            user_pic: None,
            photos: None,
            videos: None,
            quote_id: None,
            retweet_id: None,
            reply_to_id: None,
            conversation_id: None,
            entities_json: None,
            x_raw_json: None,
        }
    }

    fn temp_db_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "twit-rank-{label}-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
