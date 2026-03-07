use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::time_util::parse_rfc2822_or_rfc3339;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweet {
    pub id: i64,
    pub user_id: String,
    pub username: String,
    pub username_lc: String,
    pub fullname: String,
    pub text: String,
    pub search_text: String,
    pub created_at: String,
    pub reply_count: i64,
    pub retweet_count: i64,
    pub like_count: i64,
    pub view_count: i64,
    pub feed_kind: String,
    pub archived_at: String,
    // New fields
    pub user_pic: Option<String>,
    pub photos: Option<String>, // JSON array of photo URLs
    /// JSON array of video objects (variants + poster + kind).
    pub videos: Option<String>,
    pub quote_id: Option<i64>,
    pub retweet_id: Option<i64>,
    pub reply_to_id: Option<i64>,
    pub conversation_id: Option<i64>,
    pub entities_json: Option<String>,
    /// Raw per-tweet JSON payload extracted from the X GraphQL response.
    /// (Not the full timeline response body; this is the tweet object we parsed.)
    pub x_raw_json: Option<String>,
}

impl Tweet {
    pub fn published_dt(&self) -> OffsetDateTime {
        parse_rfc2822_or_rfc3339(&self.created_at).unwrap_or_else(OffsetDateTime::now_utc)
    }

    pub fn photos_vec(&self) -> Vec<String> {
        self.photos
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}
