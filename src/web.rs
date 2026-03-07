use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{header, Method, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use crate::{
    archive::{Archive, SearchKind, SearchMode, SearchParams},
    archive_writer::ArchiveWriter,
    cache::SqliteCache,
    config::Config,
    model::Tweet,
    static_ui,
    time_util::now_rfc3339_seconds,
    x::{NoReadySessionsError, XClient},
};

/// API Post format matching frontend expectations
#[derive(Debug, Clone, Serialize)]
struct ApiPost {
    id: String,
    user: String,
    fullname: String,
    content: String,
    link: String,
    published: String,
    published_ts: i64,
    feedback: i64,
    likes: i64,
    retweets: i64,
    replies: i64,
    views: i64,
    feed_kind: String,
    // New fields
    user_pic: Option<String>,
    photos: Vec<String>,
    videos: Vec<ApiVideo>,
    quote_id: Option<String>,
    retweet_id: Option<String>,
    reply_to_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct StoredVideoVariant {
    url: String,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    bitrate: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct StoredVideoMedia {
    #[serde(default)]
    kind: String, // "video" | "animated_gif"
    #[serde(default)]
    poster: Option<String>, // pbs path (or full url)
    #[serde(default)]
    variants: Vec<StoredVideoVariant>,
}

#[derive(Debug, Clone, Serialize)]
struct ApiVideo {
    kind: String,         // "video" | "animated_gif"
    url: String,          // preferred source URL
    sources: Vec<String>, // ordered fallbacks (typically mp4 variants)
    poster: Option<String>,
}

const TWITTER_CDN: &str = "https://pbs.twimg.com/";

fn to_full_url(path: &str) -> String {
    if path.starts_with("http") {
        path.to_string()
    } else {
        format!("{}{}", TWITTER_CDN, path)
    }
}

fn variant_is_mp4(v: &StoredVideoVariant) -> bool {
    let ct = v.content_type.to_ascii_lowercase();
    if ct.starts_with("video/mp4") {
        return true;
    }
    v.url.to_ascii_lowercase().contains(".mp4")
}

impl ApiPost {
    fn from_tweet(tweet: Tweet, feedback: i64) -> Self {
        let ts = tweet.published_dt().unix_timestamp();
        let photos: Vec<String> = tweet
            .photos_vec()
            .into_iter()
            .map(|p| to_full_url(&p))
            .collect();

        let stored_videos: Vec<StoredVideoMedia> = tweet
            .videos
            .as_deref()
            .filter(|s| !s.trim().is_empty() && *s != "[]")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let videos: Vec<ApiVideo> = stored_videos
            .into_iter()
            .filter_map(|m| {
                // Prefer mp4 variants and order by highest bitrate first.
                // Client can step down through lower-bitrate fallbacks on error.
                let mut mp4_variants: Vec<StoredVideoVariant> = m
                    .variants
                    .iter()
                    .filter(|v| !v.url.is_empty() && variant_is_mp4(v))
                    .cloned()
                    .collect();
                mp4_variants.sort_by_key(|variant| std::cmp::Reverse(variant.bitrate.unwrap_or(0)));

                let mut any_variants: Vec<StoredVideoVariant> = m
                    .variants
                    .iter()
                    .filter(|v| !v.url.is_empty())
                    .cloned()
                    .collect();
                any_variants.sort_by_key(|variant| std::cmp::Reverse(variant.bitrate.unwrap_or(0)));

                let chosen = if mp4_variants.is_empty() {
                    any_variants
                } else {
                    mp4_variants
                };

                let mut sources: Vec<String> = Vec::new();
                for v in chosen {
                    if !sources.iter().any(|u| u == &v.url) {
                        sources.push(v.url);
                    }
                }
                let url = sources.first().cloned()?;
                let poster = m.poster.filter(|s| !s.is_empty()).map(|s| to_full_url(&s));
                Some(ApiVideo {
                    kind: if m.kind.is_empty() {
                        "video".to_string()
                    } else {
                        m.kind
                    },
                    url,
                    sources,
                    poster,
                })
            })
            .collect();

        Self {
            link: format!("https://x.com/{}/status/{}", tweet.username, tweet.id),
            id: tweet.id.to_string(),
            user: tweet.username,
            fullname: tweet.fullname,
            content: tweet.text,
            published: tweet.created_at,
            published_ts: ts,
            feedback,
            likes: tweet.like_count,
            retweets: tweet.retweet_count,
            replies: tweet.reply_count,
            views: tweet.view_count,
            feed_kind: tweet.feed_kind,
            user_pic: tweet
                .user_pic
                .filter(|s| !s.is_empty())
                .map(|s| to_full_url(&s)),
            photos,
            videos,
            quote_id: tweet.quote_id.filter(|&id| id > 0).map(|id| id.to_string()),
            retweet_id: tweet
                .retweet_id
                .filter(|&id| id > 0)
                .map(|id| id.to_string()),
            reply_to_id: tweet
                .reply_to_id
                .filter(|&id| id > 0)
                .map(|id| id.to_string()),
        }
    }
}

#[derive(Clone)]
struct AppState {
    archive: Arc<Archive>,
    writer: Arc<ArchiveWriter>,
    cache: Arc<SqliteCache>,
    x: Arc<XClient>,
    settings: Arc<tokio::sync::RwLock<Config>>,
    settings_path: Arc<PathBuf>,
}

#[derive(Deserialize)]
struct FeedQuery {
    limit: Option<usize>,
    q: Option<String>,
    feed: Option<String>,
}

#[derive(Deserialize)]
struct ApiPostsQuery {
    limit: Option<usize>,
    offset: Option<usize>,
    q: Option<String>,
    feed: Option<String>,
}

#[derive(Deserialize)]
struct ApiSearchQuery {
    limit: Option<usize>,
    offset: Option<usize>,
    q: Option<String>,
    mode: Option<String>,
    author: Option<String>,
    feed: Option<String>,
    created_from: Option<String>,
    created_to: Option<String>,
    min_likes: Option<i64>,
    min_retweets: Option<i64>,
    min_replies: Option<i64>,
    min_views: Option<i64>,
    has_photos: Option<bool>,
    has_videos: Option<bool>,
    has_media: Option<bool>,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct ApiFeedbackBody {
    id: String, // String to avoid JS number precision loss
    #[serde(default)]
    user: Option<String>,
    value: i64,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiSaveSettingsBody {
    archive_path: String,
    #[serde(default)]
    sessions: Vec<crate::config::SessionConfig>,
    #[serde(default)]
    list_ids: Vec<String>,
    poll_mins: u64,
    max_pages: usize,
    page_delay_ms: u64,
    feed_delay_ms: u64,
    tid_disable: bool,
    tid_pairs_url: String,
}

#[derive(Serialize)]
struct ApiFeedbackResponse {
    success: bool,
}

#[derive(Serialize)]
struct ApiPostsResponse {
    posts: Vec<ApiPost>,
    total: usize,
    has_more: bool,
    /// Pre-loaded related tweets (quotes/retweets) keyed by ID string.
    related: HashMap<String, ApiPost>,
}

#[derive(Serialize)]
struct ApiListsResponse {
    lists: Vec<ListInfo>,
}

#[derive(Serialize)]
struct ApiBuildInfo {
    build_id: String,
    build_epoch: Option<u64>,
    package_version: String,
}

#[derive(Serialize)]
struct ApiSettingsStatus {
    settings_file_exists: bool,
    has_sessions: bool,
    needs_setup: bool,
    session_count: usize,
    settings_path: String,
}

#[derive(Serialize)]
struct ApiSettingsResponse {
    archive_path: String,
    sessions: Vec<crate::config::SessionConfig>,
    list_ids: Vec<String>,
    poll_mins: u64,
    max_pages: usize,
    page_delay_ms: u64,
    feed_delay_ms: u64,
    tid_disable: bool,
    tid_pairs_url: String,
}

#[derive(Serialize)]
struct ApiSaveSettingsResponse {
    success: bool,
    restart_required: bool,
}

#[derive(Serialize)]
struct ListInfo {
    id: String,
    name: String,
}

fn server_build_id() -> &'static str {
    option_env!("TWIT_RANK_BUILD_ID").unwrap_or("unknown")
}

fn server_build_epoch() -> Option<u64> {
    option_env!("TWIT_RANK_BUILD_EPOCH").and_then(|s| s.parse::<u64>().ok())
}

pub async fn serve(
    bind: String,
    cfg: Config,
    settings_path: PathBuf,
    x: Arc<XClient>,
    writer: Arc<ArchiveWriter>,
    cache: Arc<SqliteCache>,
) -> Result<()> {
    let archive = Arc::new(Archive::open(&cfg.archive_path)?);

    let state = AppState {
        archive,
        writer,
        cache,
        x,
        settings: Arc::new(tokio::sync::RwLock::new(cfg.clone())),
        settings_path: Arc::new(settings_path),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT]);

    let app = Router::new()
        .route("/api/posts", get(api_posts))
        .route("/api/search", get(api_search))
        .route("/api/post/{id}", get(api_post))
        .route("/api/lists", get(api_lists))
        .route("/api/feeds", get(api_feeds))
        .route("/api/build", get(api_build))
        .route("/api/settings/status", get(api_settings_status))
        .route("/api/settings", get(api_settings).post(api_save_settings))
        .route("/api/ai/context", get(api_ai_context))
        .route("/api/feedback", post(api_feedback))
        .route("/", get(static_ui::index))
        .route("/{*path}", get(static_ui::asset))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "web ui listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn api_posts(
    State(state): State<AppState>,
    Query(query): Query<ApiPostsQuery>,
) -> Result<Json<ApiPostsResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    let tweets = state
        .archive
        .get_tweets(limit, offset, query.q.clone(), query.feed.clone())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = state
        .archive
        .count_tweets(query.q, query.feed)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    build_posts_response(&state, tweets, total, offset).map(Json)
}

async fn api_search(
    State(state): State<AppState>,
    Query(query): Query<ApiSearchQuery>,
) -> Result<Json<ApiPostsResponse>, (StatusCode, String)> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);
    let mode =
        parse_search_mode(query.mode.as_deref()).map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;
    let kind =
        parse_search_kind(query.kind.as_deref()).map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;

    let params = SearchParams {
        query: query.q,
        mode,
        author: query.author,
        feed: query.feed,
        created_from: query.created_from,
        created_to: query.created_to,
        min_likes: query.min_likes,
        min_retweets: query.min_retweets,
        min_replies: query.min_replies,
        min_views: query.min_views,
        has_photos: query.has_photos.unwrap_or(false),
        has_videos: query.has_videos.unwrap_or(false),
        has_media: query.has_media.unwrap_or(false),
        kind,
        limit,
        offset,
    };

    let result = state.archive.search_tweets(&params).map_err(|e| {
        let msg = e.to_string();
        let status = if msg.contains("invalid regex") || msg.contains("invalid date/time filter") {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, msg)
    })?;

    build_posts_response(&state, result.tweets, result.total, offset).map(Json)
}

/// Fetch a single tweet by ID (for quoted tweets)
/// First checks archive, then falls back to fetching from X and persists it (write-through cache).
async fn api_post(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<Option<ApiPost>>, (StatusCode, String)> {
    // First try the archive
    let tweet = state
        .archive
        .get_tweet_by_id(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(t) = tweet {
        let feedback_map = state
            .cache
            .feedback_map_for_posts(&[t.id])
            .unwrap_or_default();
        let fb = *feedback_map.get(&t.id).unwrap_or(&0);
        return Ok(Json(Some(ApiPost::from_tweet(t, fb))));
    }

    // Not in archive - fetch from X and persist into archive DB.
    match state.x.tweet_by_id(id).await {
        Ok(bundle) => {
            let archived_at = now_rfc3339_seconds();
            let row = bundle.tweet.into_model("ondemand:direct", &archived_at);
            state
                .writer
                .upsert_tweet(&row)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if !bundle.related.is_empty() {
                let related_rows = bundle
                    .related
                    .into_iter()
                    .map(|rt| rt.tweet.into_model(rt.kind.feed_kind(), &archived_at))
                    .collect::<Vec<_>>();
                state
                    .writer
                    .upsert_tweets(&related_rows)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }

            // Re-read from archive to return the canonical row format.
            let tweet = state
                .archive
                .get_tweet_by_id(id)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if let Some(t) = tweet {
                let feedback_map = state
                    .cache
                    .feedback_map_for_posts(&[t.id])
                    .unwrap_or_default();
                let fb = *feedback_map.get(&t.id).unwrap_or(&0);
                return Ok(Json(Some(ApiPost::from_tweet(t, fb))));
            }

            // Fallback: return the freshly fetched row even if we couldn't re-read it.
            let feedback_map = state
                .cache
                .feedback_map_for_posts(&[row.id])
                .unwrap_or_default();
            let fb = *feedback_map.get(&row.id).unwrap_or(&0);
            Ok(Json(Some(ApiPost::from_tweet(row, fb))))
        }
        Err(e) => {
            if e.downcast_ref::<NoReadySessionsError>().is_some() {
                tracing::debug!("Failed to fetch tweet {} from X: {}", id, e);
            } else {
                tracing::warn!("Failed to fetch tweet {} from X: {}", id, e);
            }
            Ok(Json(None))
        }
    }
}

async fn api_lists(
    State(state): State<AppState>,
) -> Result<Json<ApiListsResponse>, (StatusCode, String)> {
    let raw = state
        .archive
        .get_lists()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let lists = raw
        .into_iter()
        .map(|feed_kind| {
            // feed_kind is like "list:ml", extract slug
            let name = feed_kind
                .strip_prefix("list:")
                .unwrap_or(&feed_kind)
                .to_string();
            ListInfo {
                id: feed_kind,
                name,
            }
        })
        .collect();

    Ok(Json(ApiListsResponse { lists }))
}

/// Returns all available feed types
async fn api_feeds(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    // Always include the main feeds; order matters for UI defaults.
    // Put "forYou" last because it overlaps with everything else.
    let mut feeds = vec!["following".to_string()];

    // Add lists before "forYou"
    let lists = state
        .archive
        .get_lists()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    feeds.extend(lists);

    feeds.push("forYou".to_string());

    Ok(Json(feeds))
}

async fn api_build() -> Json<ApiBuildInfo> {
    Json(ApiBuildInfo {
        build_id: server_build_id().to_string(),
        build_epoch: server_build_epoch(),
        package_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn api_settings_status(State(state): State<AppState>) -> Json<ApiSettingsStatus> {
    let settings = state.settings.read().await;
    Json(ApiSettingsStatus {
        settings_file_exists: state.settings_path.exists(),
        has_sessions: settings.has_sessions(),
        needs_setup: !state.settings_path.exists() || !settings.has_sessions(),
        session_count: settings.sessions.len(),
        settings_path: state.settings_path.display().to_string(),
    })
}

async fn api_settings(State(state): State<AppState>) -> Json<ApiSettingsResponse> {
    let settings = state.settings.read().await;
    Json(ApiSettingsResponse {
        archive_path: settings.archive_path.clone(),
        sessions: settings.sessions.clone(),
        list_ids: settings.list_ids.clone(),
        poll_mins: settings.poll_mins,
        max_pages: settings.max_pages,
        page_delay_ms: settings.page_delay_ms,
        feed_delay_ms: settings.feed_delay_ms,
        tid_disable: settings.tid_disable,
        tid_pairs_url: settings.tid_pairs_url.clone(),
    })
}

async fn api_save_settings(
    State(state): State<AppState>,
    Json(body): Json<ApiSaveSettingsBody>,
) -> Result<Json<ApiSaveSettingsResponse>, (StatusCode, String)> {
    let cfg = Config {
        archive_path: body.archive_path.trim().to_string(),
        sessions: body
            .sessions
            .into_iter()
            .filter(|s| {
                !s.auth_token.trim().is_empty()
                    || !s.ct0.trim().is_empty()
                    || !s.username.trim().is_empty()
                    || !s.id.trim().is_empty()
            })
            .collect(),
        list_ids: body
            .list_ids
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        poll_mins: body.poll_mins,
        max_pages: body.max_pages,
        page_delay_ms: body.page_delay_ms,
        feed_delay_ms: body.feed_delay_ms,
        tid_disable: body.tid_disable,
        tid_pairs_url: body.tid_pairs_url.trim().to_string(),
    };
    if cfg.archive_path.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "archive_path is required".to_string(),
        ));
    }
    for (idx, session) in cfg.sessions.iter().enumerate() {
        let missing_auth = session.auth_token.trim().is_empty();
        let missing_ct0 = session.ct0.trim().is_empty();
        if missing_auth || missing_ct0 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("session {} requires both auth_token and ct0", idx + 1),
            ));
        }
    }

    cfg.save(&state.settings_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut settings = state.settings.write().await;
    *settings = cfg;

    Ok(Json(ApiSaveSettingsResponse {
        success: true,
        restart_required: true,
    }))
}

async fn api_ai_context(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<String, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50);
    let tweets = state
        .archive
        .get_tweets(limit, 0, query.q, query.feed)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut out = String::new();
    for t in tweets {
        let clean_text = t.text.replace('\n', " ");
        out.push_str(&format!(
            "[{}] @{} ({}): {}\n",
            t.created_at, t.username, t.fullname, clean_text
        ));
    }
    Ok(out)
}

async fn api_feedback(
    State(state): State<AppState>,
    Json(body): Json<ApiFeedbackBody>,
) -> Result<Json<ApiFeedbackResponse>, (StatusCode, String)> {
    // Parse ID from string (to avoid JS number precision loss)
    let id: i64 = body
        .id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid ID".to_string()))?;

    // Get user from body or look up from tweet
    let user = if let Some(u) = body.user {
        u
    } else {
        // Look up the tweet to get username
        state
            .archive
            .get_tweet_by_id(id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map(|t| t.username)
            .unwrap_or_else(|| "unknown".to_string())
    };

    // Handle delete (value = 0) vs set (value = 1 or -1)
    if body.value == 0 {
        state
            .cache
            .delete_feedback(id, &user)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        state
            .cache
            .set_feedback(id, &user, body.value, body.reason.as_deref())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    Ok(Json(ApiFeedbackResponse { success: true }))
}

fn build_posts_response(
    state: &AppState,
    tweets: Vec<Tweet>,
    total: usize,
    offset: usize,
) -> Result<ApiPostsResponse, (StatusCode, String)> {
    let ids: Vec<i64> = tweets.iter().map(|t| t.id).collect();
    let feedback_map = state
        .cache
        .feedback_map_for_posts(&ids)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let posts = tweets
        .into_iter()
        .map(|t| {
            let fb = *feedback_map.get(&t.id).unwrap_or(&0);
            ApiPost::from_tweet(t, fb)
        })
        .collect::<Vec<_>>();

    let has_more = offset + posts.len() < total;
    let mut related_ids = Vec::new();
    for p in &posts {
        if let Some(ref qid) = p.quote_id {
            if let Ok(id) = qid.parse::<i64>() {
                related_ids.push(id);
            }
        }
        if let Some(ref rid) = p.retweet_id {
            if let Ok(id) = rid.parse::<i64>() {
                related_ids.push(id);
            }
        }
        if let Some(ref rid) = p.reply_to_id {
            if let Ok(id) = rid.parse::<i64>() {
                related_ids.push(id);
            }
        }
    }
    related_ids.sort_unstable();
    related_ids.dedup();

    let mut related = HashMap::new();
    if !related_ids.is_empty() {
        if let Ok(related_tweets) = state.archive.get_tweets_by_ids(&related_ids) {
            let rel_ids: Vec<i64> = related_tweets.iter().map(|t| t.id).collect();
            let rel_fb = state
                .cache
                .feedback_map_for_posts(&rel_ids)
                .unwrap_or_default();
            for t in related_tweets {
                let fb = *rel_fb.get(&t.id).unwrap_or(&0);
                related.insert(t.id.to_string(), ApiPost::from_tweet(t, fb));
            }
        }
    }

    Ok(ApiPostsResponse {
        posts,
        total,
        has_more,
        related,
    })
}

fn parse_search_mode(mode: Option<&str>) -> Result<SearchMode, String> {
    match mode
        .unwrap_or("literal")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "literal" => Ok(SearchMode::Literal),
        "regex" => Ok(SearchMode::Regex),
        other => Err(format!("invalid search mode: {other}")),
    }
}

fn parse_search_kind(kind: Option<&str>) -> Result<SearchKind, String> {
    match kind.unwrap_or("any").trim().to_ascii_lowercase().as_str() {
        "" | "any" => Ok(SearchKind::Any),
        "original" => Ok(SearchKind::Original),
        "reply" => Ok(SearchKind::Reply),
        "quote" => Ok(SearchKind::Quote),
        "retweet" => Ok(SearchKind::Retweet),
        other => Err(format!("invalid search kind: {other}")),
    }
}
