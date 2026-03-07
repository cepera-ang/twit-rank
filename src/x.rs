//! Direct X fetching via authenticated GraphQL endpoints.
//!
//! Implemented as a minimal subset to support:
//! - Background archiving (home timelines + list timelines)
//! - On-demand tweet fetch by ID (quotes/retweets) with write-through caching

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use fastrand::Rng;
use html_escape::{decode_html_entities, encode_text};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::config::SessionConfig;
use crate::model::Tweet;
use crate::time_util::{
    now_rfc3339_seconds, parse_x_created_at_to_rfc3339, unix_millis_to_rfc3339_seconds,
};

// Cookie-auth bearer tokens mirrored from the X web client.
const BEARER_TOKEN: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";
const BEARER_TOKEN2: &str = "Bearer AAAAAAAAAAAAAAAAAAAAAFXzAwAAAAAAMHCxpeSDG1gLNLghVe8d74hl6k4%3DRUMF4xAQLsbeBhTSRrCiQpJtxoGWeyHrDb5te2jpGskWDFW82F";

// GraphQL endpoints (hashed IDs change over time).
const GRAPH_HOME_TIMELINE: &str = "HCosKfLNW1AcOo3la3mMgg/HomeTimeline";
const GRAPH_HOME_LATEST_TIMELINE: &str = "U0cdisy7QFIoTfu3-Okw0A/HomeLatestTimeline";
const GRAPH_LIST_TWEETS: &str = "VQf8_XQynI3WzH6xopOMMQ/ListTimeline";
// ConversationTimeline has been more reliable for per-tweet fetches than TweetResultByIdQuery.
const GRAPH_TWEET_CONVERSATION: &str = "Y4Erk_-0hObvLpz0Iw3bzA/ConversationTimeline";
const GRAPH_TWEET_RESULT: &str = "nzme9KiYhfIOrrLrPP_XeQ/TweetResultByIdQuery";

const GRAPH_USER_TWEETS: &str = "6QdSuZ5feXxOadEdXa4XZg/UserWithProfileTweetsQueryV2";
const GRAPH_USER_BY_SCREEN_NAME: &str = "WEoGnYB0EG1yGwamDCF6zg/UserResultByScreenNameQuery";

const DEFAULT_TID_PAIRS_URL: &str =
    "https://raw.githubusercontent.com/fa0311/x-client-transaction-id-pair-dict/refs/heads/main/pair.json";

const DEFAULT_KEYWORD: &str = "obfiowerehiring";
const TID_EPOCH_OFFSET: i64 = 1_682_924_400; // 1682924400

// Raw GraphQL feature flag payload observed from the X web client.
// We minify once at startup to match the serialized `features` query parameter.
const GQL_FEATURES_RAW: &str = r#"{
  "android_ad_formats_media_component_render_overlay_enabled": false,
  "android_graphql_skip_api_media_color_palette": false,
  "android_professional_link_spotlight_display_enabled": false,
  "blue_business_profile_image_shape_enabled": false,
  "commerce_android_shop_module_enabled": false,
  "creator_subscriptions_subscription_count_enabled": false,
  "creator_subscriptions_tweet_preview_api_enabled": true,
  "freedom_of_speech_not_reach_fetch_enabled": true,
  "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
  "hidden_profile_likes_enabled": false,
  "highlights_tweets_tab_ui_enabled": false,
  "interactive_text_enabled": false,
  "longform_notetweets_consumption_enabled": true,
  "longform_notetweets_inline_media_enabled": true,
  "longform_notetweets_rich_text_read_enabled": true,
  "longform_notetweets_richtext_consumption_enabled": true,
  "mobile_app_spotlight_module_enabled": false,
  "responsive_web_edit_tweet_api_enabled": true,
  "responsive_web_enhance_cards_enabled": false,
  "responsive_web_graphql_exclude_directive_enabled": true,
  "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
  "responsive_web_graphql_timeline_navigation_enabled": true,
  "responsive_web_media_download_video_enabled": false,
  "responsive_web_text_conversations_enabled": false,
  "responsive_web_twitter_article_tweet_consumption_enabled": true,
  "unified_cards_destination_url_params_enabled": false,
  "responsive_web_twitter_blue_verified_badge_is_enabled": true,
  "rweb_lists_timeline_redesign_enabled": true,
  "spaces_2022_h2_clipping": true,
  "spaces_2022_h2_spaces_communities": true,
  "standardized_nudges_misinfo": true,
  "subscriptions_verification_info_enabled": true,
  "subscriptions_verification_info_reason_enabled": true,
  "subscriptions_verification_info_verified_since_enabled": true,
  "super_follow_badge_privacy_enabled": false,
  "super_follow_exclusive_tweet_notifications_enabled": false,
  "super_follow_tweet_api_enabled": false,
  "super_follow_user_api_enabled": false,
  "tweet_awards_web_tipping_enabled": false,
  "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
  "tweetypie_unmention_optimization_enabled": false,
  "unified_cards_ad_metadata_container_dynamic_card_content_query_enabled": false,
  "verified_phone_label_enabled": false,
  "vibe_api_enabled": false,
  "view_counts_everywhere_api_enabled": true,
  "premium_content_api_read_enabled": false,
  "communities_web_enable_tweet_community_results_fetch": true,
  "responsive_web_jetfuel_frame": true,
  "responsive_web_grok_analyze_button_fetch_trends_enabled": false,
  "responsive_web_grok_image_annotation_enabled": true,
  "responsive_web_grok_imagine_annotation_enabled": true,
  "rweb_tipjar_consumption_enabled": true,
  "profile_label_improvements_pcf_label_in_post_enabled": true,
  "creator_subscriptions_quote_tweet_preview_enabled": false,
  "c9s_tweet_anatomy_moderator_badge_enabled": true,
  "responsive_web_grok_analyze_post_followups_enabled": true,
  "rweb_video_timestamps_enabled": false,
  "responsive_web_grok_share_attachment_enabled": true,
  "articles_preview_enabled": true,
  "immersive_video_status_linkable_timestamps": false,
  "articles_api_enabled": false,
  "responsive_web_grok_analysis_button_from_backend": true,
  "rweb_video_screen_enabled": false,
  "payments_enabled": false,
  "responsive_web_profile_redirect_enabled": false,
  "responsive_web_grok_show_grok_translated_post": false,
  "responsive_web_grok_community_note_auto_translation_is_enabled": false,
  "profile_label_improvements_pcf_label_in_profile_enabled": false,
  "grok_android_analyze_trend_fetch_enabled": false,
  "grok_translations_community_note_auto_translation_is_enabled": false,
  "grok_translations_post_auto_translation_is_enabled": false,
  "grok_translations_community_note_translation_is_enabled": false,
  "grok_translations_timeline_user_bio_auto_translation_is_enabled": false,
  "subscriptions_feature_can_gift_premium": false,
  "responsive_web_twitter_article_notes_tab_enabled": false,
  "subscriptions_verification_info_is_identity_verified_enabled": false,
  "hidden_profile_subscriptions_enabled": false
}"#;

#[derive(Debug, Clone, Copy)]
pub enum HomeTimelineKind {
    Following,
    ForYou,
}

#[derive(Debug, Clone)]
pub struct TimelinePage {
    pub tweets: Vec<TweetPayload>,
    pub related: Vec<RelatedTweetPayload>,
    pub bottom_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum RelatedTweetKind {
    Quote,
    Retweet,
}

impl RelatedTweetKind {
    pub fn feed_kind(self) -> &'static str {
        match self {
            RelatedTweetKind::Quote => "quote",
            RelatedTweetKind::Retweet => "retweet",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RelatedTweetPayload {
    pub kind: RelatedTweetKind,
    pub tweet: TweetPayload,
}

#[derive(Debug, Clone)]
pub struct TweetBundle {
    pub tweet: TweetPayload,
    pub related: Vec<RelatedTweetPayload>,
}

#[derive(Debug, Clone)]
pub struct TweetPayload {
    pub id: i64,
    pub user_id: String,
    pub username: String,
    pub fullname: String,
    pub plain_text: String,
    pub html: String,
    pub created_at: String, // RFC3339
    pub reply_count: i64,
    pub retweet_count: i64,
    pub like_count: i64,
    pub view_count: i64,
    pub user_pic: Option<String>, // stripped pbs.twimg.com prefix (or full URL)
    pub photos: Vec<String>,      // stripped pbs.twimg.com prefix (or full URL)
    pub videos: Vec<VideoMedia>,
    pub quote_id: Option<i64>,
    pub retweet_id: Option<i64>,
    pub reply_to_id: Option<i64>,
    pub conversation_id: Option<i64>,
    pub entities_json: Option<String>,
    /// Raw per-tweet JSON payload extracted from the X response (tweet object only).
    pub x_raw_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoVariant {
    pub url: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub bitrate: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMedia {
    /// "video" or "animated_gif"
    #[serde(default)]
    pub kind: String,
    /// Poster image (pbs path or full URL)
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub variants: Vec<VideoVariant>,
}

impl TweetPayload {
    pub fn into_model(self, feed_kind: &str, archived_at: &str) -> Tweet {
        let username_lc = self.username.to_ascii_lowercase();
        let photos_json = if self.photos.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&self.photos).unwrap_or_else(|_| "[]".to_string()))
        };
        let videos_json = if self.videos.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&self.videos).unwrap_or_else(|_| "[]".to_string()))
        };
        Tweet {
            id: self.id,
            user_id: self.user_id,
            username: self.username,
            username_lc,
            fullname: self.fullname,
            text: self.html,
            search_text: normalize_search_text(&self.plain_text),
            created_at: self.created_at,
            reply_count: self.reply_count,
            retweet_count: self.retweet_count,
            like_count: self.like_count,
            view_count: self.view_count,
            feed_kind: feed_kind.to_string(),
            archived_at: archived_at.to_string(),
            user_pic: self.user_pic,
            photos: photos_json,
            videos: videos_json,
            quote_id: self.quote_id,
            retweet_id: self.retweet_id,
            reply_to_id: self.reply_to_id,
            conversation_id: self.conversation_id,
            entities_json: self.entities_json,
            x_raw_json: self.x_raw_json,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TidPair {
    #[serde(rename = "animationKey")]
    animation_key: String,
    verification: String,
}

#[derive(Debug, Clone)]
struct TidCache {
    pairs: Vec<TidPair>,
    last_fetch_epoch: i64,
}

#[derive(Debug, Clone)]
struct TidGenerator {
    pairs_url: String,
    cache: Arc<Mutex<TidCache>>,
    ttl: Duration,
}

impl TidGenerator {
    fn new(pairs_url: String) -> Self {
        Self {
            pairs_url,
            cache: Arc::new(Mutex::new(TidCache {
                pairs: vec![],
                last_fetch_epoch: 0,
            })),
            ttl: Duration::from_secs(60 * 60),
        }
    }

    async fn get_pair(&self, http: &reqwest::Client) -> Result<TidPair> {
        let now = epoch_seconds();
        {
            let cache = self.cache.lock().await;
            if !cache.pairs.is_empty() && (now - cache.last_fetch_epoch) < self.ttl.as_secs() as i64
            {
                let mut rng = Rng::new();
                return choose_cloned(&cache.pairs, &mut rng)
                    .ok_or_else(|| anyhow!("no tid pairs available"));
            }
        }

        let resp = http
            .get(&self.pairs_url)
            .header("User-Agent", "twit-rank")
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .with_context(|| format!("fetch tid pairs {}", self.pairs_url))?;

        let status = resp.status();
        let text = String::from_utf8(resp.bytes().await?.to_vec())
            .with_context(|| "tid pairs response is not utf-8")?;
        if !status.is_success() {
            return Err(anyhow!(
                "tid pairs fetch failed: status={status}, body_len={}",
                text.len()
            ));
        }

        let pairs: Vec<TidPair> =
            serde_json::from_str(&text).with_context(|| "parse tid pairs json")?;
        if pairs.is_empty() {
            return Err(anyhow!("tid pairs parse returned empty list"));
        }

        let mut cache = self.cache.lock().await;
        cache.pairs = pairs;
        cache.last_fetch_epoch = now;

        let mut rng = Rng::new();
        choose_cloned(&cache.pairs, &mut rng).ok_or_else(|| anyhow!("no tid pairs available"))
    }

    async fn gen_tid(&self, http: &reqwest::Client, path: &str) -> Result<String> {
        let pair = self.get_pair(http).await?;
        let now = epoch_seconds();
        let time_now = (now - TID_EPOCH_OFFSET) as i32;
        let time_bytes = time_now.to_le_bytes();

        let data = format!(
            "GET!{}!{}{}{}",
            path, time_now, DEFAULT_KEYWORD, pair.animation_key
        );
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let digest = hasher.finalize();

        let key_bytes = BASE64_STANDARD
            .decode(pair.verification.as_bytes())
            .with_context(|| "decode tid verification")?;

        let mut bytes = Vec::with_capacity(key_bytes.len() + 4 + 16 + 1);
        bytes.extend_from_slice(&key_bytes);
        bytes.extend_from_slice(&time_bytes);
        bytes.extend_from_slice(&digest[..16]);
        bytes.push(3u8);

        let mut rng = Rng::new();
        let r = rng.u8(..);
        let mut out = Vec::with_capacity(bytes.len() + 1);
        out.push(r);
        out.extend(bytes.into_iter().map(|b| b ^ r));

        Ok(BASE64_STANDARD
            .encode(out)
            .trim_end_matches('=')
            .to_string())
    }
}

#[derive(Debug, Clone, Copy)]
struct RateLimit {
    remaining: i64,
    reset: i64,
}

// Be optimistic: only treat a session as blocked once the endpoint is actually out of budget.
const RL_MIN_REMAINING: i64 = 0;

#[derive(Debug, Clone)]
struct SessionState {
    id: i64,
    auth_token: String,
    ct0: String,
    pending: usize,
    // Per-endpoint "do not use before" timestamps. Different GraphQL endpoints have different budgets.
    limited_until: HashMap<String, i64>,
    apis: HashMap<String, RateLimit>,
}

#[derive(Debug, Clone)]
struct SessionAuth {
    id: i64,
    auth_token: String,
    ct0: String,
}

#[derive(Debug, Clone)]
struct SessionLease {
    endpoint: String,
    session: SessionAuth,
}

#[derive(Debug, Default, Clone, Copy)]
struct SessionOutcome {
    invalidate: bool,
    limited: bool,
    rl: Option<RateLimit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionReadiness {
    Ready,
    Busy,
    RateLimited(Option<i64>),
}

#[derive(Debug)]
pub struct NoReadySessionsError {
    endpoint: String,
    busy: usize,
    rate_limited: usize,
    total: usize,
    next_ready_at: Option<i64>,
}

impl std::fmt::Display for NoReadySessionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.next_ready_at {
            Some(ts) => write!(
                f,
                "no sessions currently ready for endpoint={} (busy={}, rate_limited={}, total={}, next_ready_at={})",
                self.endpoint, self.busy, self.rate_limited, self.total, ts
            ),
            None => write!(
                f,
                "no sessions currently ready for endpoint={} (busy={}, rate_limited={}, total={})",
                self.endpoint, self.busy, self.rate_limited, self.total
            ),
        }
    }
}

impl std::error::Error for NoReadySessionsError {}

#[derive(Debug)]
pub struct GraphqlStatusError {
    pub endpoint: String,
    pub status: reqwest::StatusCode,
}

impl std::fmt::Display for GraphqlStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "graphql non-success status {} (endpoint={})",
            self.status, self.endpoint
        )
    }
}

impl std::error::Error for GraphqlStatusError {}

struct SessionPool {
    sessions: Mutex<Vec<SessionState>>,
    max_concurrent_reqs: usize,
}

impl SessionPool {
    fn new(sessions: Vec<SessionState>) -> Self {
        Self {
            sessions: Mutex::new(sessions),
            max_concurrent_reqs: 2,
        }
    }

    async fn acquire(&self, endpoint: &str) -> Result<SessionLease> {
        let start = std::time::Instant::now();

        loop {
            let now = epoch_seconds();

            let mut guard = self.sessions.lock().await;
            if guard.is_empty() {
                return Err(anyhow!("no sessions available"));
            }

            let mut idxs: Vec<usize> = (0..guard.len()).collect();
            let mut rng = Rng::new();
            shuffle(&mut idxs, &mut rng);

            let mut busy = 0usize;
            let mut rate_limited = 0usize;
            let mut next_ready_at: Option<i64> = None;

            for idx in idxs {
                let s = &mut guard[idx];
                match session_readiness(s, endpoint, now, self.max_concurrent_reqs) {
                    SessionReadiness::Ready => {
                        s.pending += 1;
                        return Ok(SessionLease {
                            endpoint: endpoint.to_string(),
                            session: SessionAuth {
                                id: s.id,
                                auth_token: s.auth_token.clone(),
                                ct0: s.ct0.clone(),
                            },
                        });
                    }
                    SessionReadiness::Busy => busy += 1,
                    SessionReadiness::RateLimited(until) => {
                        rate_limited += 1;
                        if let Some(until) = until {
                            next_ready_at =
                                Some(next_ready_at.map(|t| t.min(until)).unwrap_or(until));
                        }
                    }
                }
            }

            let total = guard.len();
            drop(guard);

            // If sessions are simply busy, wait briefly for in-flight requests to complete rather
            // than erroring immediately. This prevents spurious warnings when UI triggers a burst.
            if busy > 0 && start.elapsed() < Duration::from_millis(750) {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }

            return Err(NoReadySessionsError {
                endpoint: endpoint.to_string(),
                busy,
                rate_limited,
                total,
                next_ready_at,
            }
            .into());
        }
    }

    async fn finish(&self, lease: &SessionLease, outcome: SessionOutcome) {
        let mut guard = self.sessions.lock().await;
        let now = epoch_seconds();
        let idx = guard.iter().position(|s| s.id == lease.session.id);
        let Some(idx) = idx else { return };
        let s = &mut guard[idx];

        if s.pending > 0 {
            s.pending -= 1;
        }

        if let Some(rl) = outcome.rl {
            s.apis.insert(lease.endpoint.clone(), rl);
        }

        if outcome.limited {
            let until = outcome
                .rl
                .map(|rl| rl.reset)
                .filter(|&reset| reset > now)
                .unwrap_or(now + 60 * 60);
            let cur = s.limited_until.get(&lease.endpoint).copied().unwrap_or(0);
            if until > cur {
                s.limited_until.insert(lease.endpoint.clone(), until);
            }
        }

        if outcome.invalidate {
            guard.remove(idx);
        }
    }
}

fn session_readiness(
    s: &mut SessionState,
    endpoint: &str,
    now: i64,
    max_concurrent: usize,
) -> SessionReadiness {
    if s.pending >= max_concurrent {
        return SessionReadiness::Busy;
    }

    if let Some(until) = s.limited_until.get(endpoint).copied() {
        if now < until {
            return SessionReadiness::RateLimited(Some(until));
        }
        s.limited_until.remove(endpoint);
    }

    if let Some(rl) = s.apis.get(endpoint) {
        if rl.reset > now && rl.remaining <= RL_MIN_REMAINING {
            return SessionReadiness::RateLimited(Some(rl.reset));
        }
    }

    SessionReadiness::Ready
}

fn epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn now_rfc3339() -> String {
    now_rfc3339_seconds()
}

fn choose_cloned<T: Clone>(items: &[T], rng: &mut Rng) -> Option<T> {
    if items.is_empty() {
        None
    } else {
        Some(items[rng.usize(0..items.len())].clone())
    }
}

fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.usize(0..=i);
        items.swap(i, j);
    }
}

pub struct XClient {
    http: reqwest::Client,
    features: String,
    sessions: Arc<SessionPool>,
    tid: TidGenerator,
    tid_disable: bool,
}

impl XClient {
    pub fn new(
        sessions: &[SessionConfig],
        tid_pairs_url: Option<String>,
        tid_disable: bool,
    ) -> Result<Self> {
        let sessions = build_session_states(sessions);

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("build reqwest client")?;

        let features = GQL_FEATURES_RAW.replace([' ', '\n'], "");
        let tid_pairs_url = tid_pairs_url.unwrap_or_else(|| DEFAULT_TID_PAIRS_URL.to_string());

        Ok(Self {
            http,
            features,
            sessions: Arc::new(SessionPool::new(sessions)),
            tid: TidGenerator::new(tid_pairs_url),
            tid_disable,
        })
    }

    pub async fn home_timeline(
        &self,
        kind: HomeTimelineKind,
        count: i64,
        after: Option<&str>,
    ) -> Result<TimelinePage> {
        let endpoint = match kind {
            HomeTimelineKind::Following => GRAPH_HOME_LATEST_TIMELINE,
            HomeTimelineKind::ForYou => GRAPH_HOME_TIMELINE,
        };

        let mut vars = serde_json::json!({
            "count": count,
            "includePromotedContent": false,
            "latestControlAvailable": true,
            "requestContext": "launch",
            "withCommunity": true,
        });
        if let Some(cursor) = after {
            vars["cursor"] = Value::String(cursor.to_string());
        }
        let variables = serde_json::to_string(&vars)?;

        let js = self.fetch_graphql(endpoint, &variables, None).await?;
        Ok(parse_timeline(
            &js,
            &[
                "/data/home/home_timeline_urt/instructions",
                "/data/viewer/home_timeline/timeline/instructions",
            ],
        ))
    }

    pub async fn list_timeline(
        &self,
        list_rest_id: &str,
        count: i64,
        after: Option<&str>,
    ) -> Result<TimelinePage> {
        let mut vars = serde_json::json!({
            "rest_id": list_rest_id,
            "count": count,
        });
        if let Some(cursor) = after {
            vars["cursor"] = Value::String(cursor.to_string());
        }
        let variables = serde_json::to_string(&vars)?;

        let js = self
            .fetch_graphql(GRAPH_LIST_TWEETS, &variables, None)
            .await?;
        Ok(parse_timeline(
            &js,
            &[
                "/data/list/timeline_response/timeline/instructions",
                "/data/list/timeline/timeline/instructions",
            ],
        ))
    }

    /// Fetch a user's tweet timeline by their REST ID.
    /// Returns a TimelinePage just like home/list timelines.
    pub async fn user_timeline(
        &self,
        user_rest_id: &str,
        after: Option<&str>,
    ) -> Result<TimelinePage> {
        let mut vars = serde_json::json!({
            "rest_id": user_rest_id,
            "count": 20,
        });
        if let Some(cursor) = after {
            vars["cursor"] = Value::String(cursor.to_string());
        }
        let variables = serde_json::to_string(&vars)?;

        let js = self
            .fetch_graphql(GRAPH_USER_TWEETS, &variables, None)
            .await?;
        Ok(parse_timeline(
            &js,
            &[
                "/data/user_result/result/timeline_response/timeline/instructions",
                "/data/user/result/timeline/timeline/instructions",
            ],
        ))
    }

    /// Look up a user's REST ID by their screen name.
    pub async fn user_id_by_screen_name(&self, screen_name: &str) -> Result<String> {
        let vars = serde_json::json!({ "screen_name": screen_name });
        let variables = serde_json::to_string(&vars)?;

        let js = self
            .fetch_graphql(GRAPH_USER_BY_SCREEN_NAME, &variables, None)
            .await?;

        // Try multiple possible response paths.
        let rest_id = js
            .pointer("/data/user_result/result/rest_id")
            .or_else(|| js.pointer("/data/user/result/rest_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("user rest_id not found for @{screen_name}"))?;

        Ok(rest_id.to_string())
    }

    pub async fn tweet_by_id(&self, id: i64) -> Result<TweetBundle> {
        // Prefer ConversationTimeline here; TweetResultByIdQuery frequently 404s even for existing posts.
        let convo_err = match self.tweet_by_id_conversation(id).await {
            Ok(bundle) => return Ok(bundle),
            Err(e) => e,
        };

        // Fallback: direct tweet result query (kept for now, but unreliable on modern X).
        let vars = serde_json::json!({ "rest_id": id.to_string() });
        let variables = serde_json::to_string(&vars)?;

        let js = self.fetch_graphql(GRAPH_TWEET_RESULT, &variables, None).await.map_err(|e| {
            anyhow!(
                "tweet fetch failed via ConversationTimeline ({convo_err}) and TweetResultByIdQuery ({e})"
            )
        })?;
        let tw = js
            .pointer("/data/tweet_result/result")
            .ok_or_else(|| anyhow!("tweet_result missing in response"))?;
        parse_tweet_bundle(tw).ok_or_else(|| anyhow!("tweet parse failed (unavailable?)"))
    }

    async fn tweet_by_id_conversation(&self, id: i64) -> Result<TweetBundle> {
        let vars = serde_json::json!({
            "postId": id.to_string(),
            "includeHasBirdwatchNotes": false,
            "includePromotedContent": false,
            "withBirdwatchNotes": false,
            "withVoice": false,
            "withV2Timeline": true,
        });
        let variables = serde_json::to_string(&vars)?;

        let js = self
            .fetch_graphql(GRAPH_TWEET_CONVERSATION, &variables, None)
            .await?;

        let tw = find_tweet_result(&js, id)
            .ok_or_else(|| anyhow!("tweet {id} not found in conversation timeline"))?;
        parse_tweet_bundle(tw).ok_or_else(|| anyhow!("tweet parse failed (unavailable?)"))
    }

    async fn fetch_graphql(
        &self,
        endpoint: &str,
        variables: &str,
        field_toggles: Option<&str>,
    ) -> Result<Value> {
        let mut last_err: Option<anyhow::Error> = None;

        for _attempt in 0..3 {
            let lease = self.sessions.acquire(endpoint).await?;

            let mut url = Url::parse(&format!("https://x.com/i/api/graphql/{endpoint}"))
                .with_context(|| "build graphql url")?;
            url.query_pairs_mut()
                .append_pair("variables", variables)
                .append_pair("features", &self.features);
            if let Some(ft) = field_toggles {
                url.query_pairs_mut().append_pair("fieldToggles", ft);
            }

            let mut headers = base_headers();
            headers.insert(
                "x-csrf-token",
                HeaderValue::from_str(&lease.session.ct0)
                    .unwrap_or_else(|_| HeaderValue::from_static("")),
            );
            headers.insert(
                "cookie",
                HeaderValue::from_str(&format!(
                    "auth_token={}; ct0={}",
                    lease.session.auth_token, lease.session.ct0
                ))?,
            );
            headers.insert(
                "x-twitter-auth-type",
                HeaderValue::from_static("OAuth2Session"),
            );

            if self.tid_disable {
                headers.insert("authorization", HeaderValue::from_static(BEARER_TOKEN2));
            } else {
                headers.insert("authorization", HeaderValue::from_static(BEARER_TOKEN));
                let tid = self.tid.gen_tid(&self.http, url.path()).await?;
                headers.insert("x-client-transaction-id", HeaderValue::from_str(&tid)?);
            }

            let resp = self.http.get(url).headers(headers).send().await;
            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    let err = anyhow!(e).context("graphql request failed");
                    self.sessions
                        .finish(&lease, SessionOutcome::default())
                        .await;
                    last_err = Some(err);
                    continue;
                }
            };

            let status = resp.status();
            let rl = parse_rate_limit(resp.headers());
            let body = resp.bytes().await.unwrap_or_default();
            let parsed: Result<Value> =
                serde_json::from_slice(&body).map_err(|e| anyhow!(e).context("parse graphql json"));

            let mut outcome = SessionOutcome {
                rl,
                ..Default::default()
            };

            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                outcome.limited = true;
            }
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                outcome.invalidate = true;
            }

            if let Ok(ref js) = parsed {
                if let Some(errors) = js.get("errors").and_then(|v| v.as_array()) {
                    for e in errors {
                        if let Some(code) = e.get("code").and_then(|v| v.as_i64()) {
                            match code {
                                88 => outcome.limited = true,
                                89 | 239 | 326 => outcome.invalidate = true,
                                _ => {}
                            }
                        }
                    }
                }
            }

            self.sessions.finish(&lease, outcome).await;

            if outcome.invalidate || outcome.limited {
                last_err = Some(anyhow!(
                    "graphql request rejected: status={status}, invalidate={}, limited={}",
                    outcome.invalidate,
                    outcome.limited
                ));
                continue;
            }

            if !status.is_success() {
                last_err = Some(
                    GraphqlStatusError {
                        endpoint: endpoint.to_string(),
                        status,
                    }
                    .into(),
                );
                continue;
            }

            match parsed {
                Ok(js) => return Ok(js),
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("graphql request failed")))
    }
}

fn parse_rate_limit(headers: &reqwest::header::HeaderMap) -> Option<RateLimit> {
    let remaining = headers
        .get("x-rate-limit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok());
    let reset = headers
        .get("x-rate-limit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok());
    match (remaining, reset) {
        (Some(remaining), Some(reset)) => Some(RateLimit { remaining, reset }),
        _ => None,
    }
}

fn base_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("accept", HeaderValue::from_static("*/*"));
    h.insert("accept-encoding", HeaderValue::from_static("gzip"));
    h.insert(
        "accept-language",
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    h.insert("connection", HeaderValue::from_static("keep-alive"));
    h.insert("content-type", HeaderValue::from_static("application/json"));
    h.insert("origin", HeaderValue::from_static("https://x.com"));
    h.insert(
        "user-agent",
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36"),
    );
    h.insert("x-twitter-active-user", HeaderValue::from_static("yes"));
    h.insert("x-twitter-client-language", HeaderValue::from_static("en"));
    h.insert("priority", HeaderValue::from_static("u=1, i"));

    // Try to look like a browser request.
    h.insert(
        "sec-ch-ua",
        HeaderValue::from_static(
            "\"Google Chrome\";v=\"142\", \"Chromium\";v=\"142\", \"Not A(Brand\";v=\"24\"",
        ),
    );
    h.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
    h.insert("sec-ch-ua-platform", HeaderValue::from_static("Windows"));
    h.insert("sec-fetch-dest", HeaderValue::from_static("empty"));
    h.insert("sec-fetch-mode", HeaderValue::from_static("cors"));
    h.insert("sec-fetch-site", HeaderValue::from_static("same-site"));
    h
}

fn build_session_states(raw_sessions: &[SessionConfig]) -> Vec<SessionState> {
    let mut out = Vec::new();
    for raw in raw_sessions {
        let auth_token = raw.auth_token.trim();
        let ct0 = raw.ct0.trim();
        if auth_token.is_empty() || ct0.is_empty() {
            continue;
        }
        let id = raw.id.trim().parse::<i64>().unwrap_or(0);
        out.push(SessionState {
            id,
            auth_token: auth_token.to_string(),
            ct0: ct0.to_string(),
            pending: 0,
            limited_until: HashMap::new(),
            apis: HashMap::new(),
        });
    }
    out
}

fn parse_timeline(js: &Value, instruction_ptrs: &[&str]) -> TimelinePage {
    let mut out = TimelinePage {
        tweets: vec![],
        related: vec![],
        bottom_cursor: None,
    };
    let mut seen: HashSet<i64> = HashSet::new();

    let instructions = instruction_ptrs
        .iter()
        .find_map(|p| js.pointer(p))
        .and_then(|v| v.as_array());
    let Some(instructions) = instructions else {
        return out;
    };

    for instr in instructions {
        // Handle modules (used in some timelines).
        if let Some(module_items) = instr.get("moduleItems").and_then(|v| v.as_array()) {
            for item in module_items {
                if let Some(tr) = get_tweet_result(item, "item") {
                    collect_primary_and_related(tr, &mut out.tweets, &mut out.related, &mut seen);
                }
            }
            continue;
        }

        let typ = get_type_name(instr);
        if typ == "TimelineAddEntries" {
            let Some(entries) = instr.get("entries").and_then(|v| v.as_array()) else {
                continue;
            };
            for e in entries {
                let entry_id = get_entry_id(e).unwrap_or_default();

                if entry_id.starts_with("cursor-bottom") {
                    if let Some(c) = extract_cursor_value(e) {
                        out.bottom_cursor = Some(c.to_string());
                    }
                    continue;
                }

                for tr in extract_tweet_results_from_entry(e) {
                    collect_primary_and_related(tr, &mut out.tweets, &mut out.related, &mut seen);
                }
            }
        } else if typ == "TimelineReplaceEntry" {
            let replace_id = instr
                .get("entry_id_to_replace")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if replace_id.starts_with("cursor-bottom") {
                if let Some(entry) = instr.get("entry") {
                    if let Some(c) = extract_cursor_value(entry) {
                        out.bottom_cursor = Some(c.to_string());
                    }
                }
            }
        }
    }

    out
}

fn unwrap_visibility(js: &Value) -> &Value {
    if get_type_name(js) == "TweetWithVisibilityResults" {
        js.get("tweet").unwrap_or(js)
    } else {
        js
    }
}

fn extract_graph_tweet_id(js: &Value) -> Option<i64> {
    let js = unwrap_visibility(js);
    js.get("rest_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            js.get("legacy")
                .and_then(|l| l.get("id_str"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
        })
}

fn collect_primary_and_related(
    js: &Value,
    primary: &mut Vec<TweetPayload>,
    related: &mut Vec<RelatedTweetPayload>,
    seen: &mut HashSet<i64>,
) {
    let js = unwrap_visibility(js);

    // Collect related tweets for this entry into a local list first so we can
    // infer quote/retweet IDs even when GraphQL omits the *_id_str fields.
    let mut local_related: Vec<RelatedTweetPayload> = Vec::new();
    collect_related_tweets(js, &mut local_related, seen);

    if let Some(mut tw) = parse_graph_tweet(js) {
        if tw.retweet_id.is_none() {
            if let Some(rt) = local_related
                .iter()
                .find(|r| matches!(r.kind, RelatedTweetKind::Retweet))
            {
                tw.retweet_id = Some(rt.tweet.id);
            }
        }
        if tw.quote_id.is_none() {
            if let Some(q) = local_related
                .iter()
                .find(|r| matches!(r.kind, RelatedTweetKind::Quote))
            {
                tw.quote_id = Some(q.tweet.id);
            }
        }

        if seen.insert(tw.id) {
            primary.push(tw);
        }
    }

    related.extend(local_related);
}

fn collect_related_tweets(
    js: &Value,
    related: &mut Vec<RelatedTweetPayload>,
    seen: &mut HashSet<i64>,
) {
    let js = unwrap_visibility(js);

    // Retweet: check top-level first, then inside legacy (where the API actually puts it).
    if let Some(rt) = js
        .pointer("/retweeted_status_result/result")
        .or_else(|| js.pointer("/repostedStatusResults/result"))
        .or_else(|| js.pointer("/legacy/retweeted_status_result/result"))
    {
        collect_related_kind(rt, RelatedTweetKind::Retweet, related, seen);
    }

    // Quote: check top-level first, then inside legacy.
    if let Some(q) = js
        .pointer("/quoted_status_result/result")
        .or_else(|| js.pointer("/quotedPostResults/result"))
        .or_else(|| js.pointer("/legacy/quoted_status_result/result"))
    {
        collect_related_kind(q, RelatedTweetKind::Quote, related, seen);
    }

    // Fallback: V1-style retweeted_status nested inside legacy.
    if !related
        .iter()
        .any(|r| matches!(r.kind, RelatedTweetKind::Retweet))
    {
        if let Some(rt_legacy) = js.pointer("/legacy/retweeted_status") {
            if let Some(rt_id) = rt_legacy
                .get("id_str")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
            {
                if seen.insert(rt_id) {
                    if let Some(tw) = parse_v1_tweet(rt_legacy) {
                        related.push(RelatedTweetPayload {
                            kind: RelatedTweetKind::Retweet,
                            tweet: tw,
                        });
                    }
                }
            }
        }
    }
}

fn collect_related_kind(
    js: &Value,
    kind: RelatedTweetKind,
    related: &mut Vec<RelatedTweetPayload>,
    seen: &mut HashSet<i64>,
) {
    let js = unwrap_visibility(js);
    if js.is_null() {
        return;
    }

    // If we can't parse the tweet payload (unavailable/tombstone), still try to walk nested fields.
    let Some(tw) = parse_graph_tweet(js) else {
        collect_related_tweets(js, related, seen);
        return;
    };

    // Avoid repeatedly scanning/adding the same embedded tweet.
    if !seen.insert(tw.id) {
        return;
    }

    related.push(RelatedTweetPayload { kind, tweet: tw });

    // Quotes/retweets can be nested (e.g., a retweet of a quote tweet).
    collect_related_tweets(js, related, seen);
}

fn parse_tweet_bundle(js: &Value) -> Option<TweetBundle> {
    let js = unwrap_visibility(js);
    let mut tweet = parse_graph_tweet(js)?;

    let mut seen = HashSet::new();
    seen.insert(tweet.id);

    let mut related = Vec::new();
    collect_related_tweets(js, &mut related, &mut seen);

    if tweet.retweet_id.is_none() {
        if let Some(rt) = related
            .iter()
            .find(|r| matches!(r.kind, RelatedTweetKind::Retweet))
        {
            tweet.retweet_id = Some(rt.tweet.id);
        }
    }
    if tweet.quote_id.is_none() {
        if let Some(q) = related
            .iter()
            .find(|r| matches!(r.kind, RelatedTweetKind::Quote))
        {
            tweet.quote_id = Some(q.tweet.id);
        }
    }

    Some(TweetBundle { tweet, related })
}

fn find_tweet_result(js: &Value, id: i64) -> Option<&Value> {
    let instruction_ptrs = [
        "/data/timelineResponse/instructions",
        "/data/timeline_response/instructions",
        "/data/threaded_conversation_with_injections_v2/instructions",
    ];

    let instructions = instruction_ptrs
        .iter()
        .find_map(|p| js.pointer(p))
        .and_then(|v| v.as_array())?;

    for instr in instructions {
        // Some responses use moduleItems (same shape as other timelines).
        if let Some(module_items) = instr.get("moduleItems").and_then(|v| v.as_array()) {
            for item in module_items {
                if let Some(tr) = get_tweet_result(item, "item") {
                    if tweet_result_id(tr) == Some(id) {
                        return Some(tr);
                    }
                }
            }
            continue;
        }

        if get_type_name(instr) != "TimelineAddEntries" {
            continue;
        }

        let Some(entries) = instr.get("entries").and_then(|v| v.as_array()) else {
            continue;
        };
        for e in entries {
            // Fast-path: entry IDs include the tweet ID for focal tweets.
            if let Some(entry_id) = get_entry_id(e) {
                if entry_id.starts_with("tweet") {
                    if let Some(entry_tid) = entry_id_to_i64(entry_id) {
                        if entry_tid == id {
                            if let Some(tr) = get_tweet_result(e, "content") {
                                return Some(tr);
                            }
                        }
                    }
                }
            }

            for tr in extract_tweet_results_from_entry(e) {
                if tweet_result_id(tr) == Some(id) {
                    return Some(tr);
                }
            }
        }
    }

    None
}

fn tweet_result_id(js: &Value) -> Option<i64> {
    let js = unwrap_visibility(js);
    js.get("rest_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            js.get("legacy")
                .and_then(|v| v.get("id_str"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
        })
}

fn entry_id_to_i64(id: &str) -> Option<i64> {
    // Use the substring after the final '-'.
    let tail = id.rsplit_once('-').map(|(_, tail)| tail).unwrap_or(id);
    tail.parse::<i64>().ok()
}

fn get_type_name(v: &Value) -> &str {
    v.get("__typename")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("type").and_then(|x| x.as_str()))
        .unwrap_or("")
}

fn get_entry_id(v: &Value) -> Option<&str> {
    v.get("entryId")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("entry_id").and_then(|x| x.as_str()))
}

fn extract_cursor_value(v: &Value) -> Option<&str> {
    v.pointer("/content/value")
        .and_then(|x| x.as_str())
        .or_else(|| {
            v.pointer("/content/itemContent/value")
                .and_then(|x| x.as_str())
        })
        .or_else(|| v.pointer("/content/content/value").and_then(|x| x.as_str()))
}

fn get_tweet_result<'a>(js: &'a Value, root: &str) -> Option<&'a Value> {
    let root = js.get(root)?;
    root.pointer("/content/tweet_results/result")
        .or_else(|| root.pointer("/itemContent/tweet_results/result"))
        .or_else(|| root.pointer("/content/tweetResult/result"))
}

fn extract_tweet_results_from_entry(e: &Value) -> Vec<&Value> {
    if let Some(tr) = get_tweet_result(e, "content") {
        return vec![tr];
    }

    let mut out = Vec::new();
    if let Some(items) = e.pointer("/content/items").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(tr) = get_tweet_result(item, "item") {
                out.push(tr);
            }
        }
    }
    out
}

/// Parse a V1-style tweet object (e.g. from `legacy/retweeted_status`).
///
/// Unlike GraphQL tweets, these have `id_str`, `full_text`, `user` etc.
/// directly at the top level — there is no `legacy` wrapper or `rest_id`.
fn parse_v1_tweet(js: &Value) -> Option<TweetPayload> {
    let id = js
        .get("id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())?;

    let created_at = js
        .get("created_at")
        .and_then(|v| v.as_str())
        .and_then(parse_created_at)
        .unwrap_or_else(now_rfc3339);

    let user = js.get("user");
    let username = user
        .and_then(|u| u.get("screen_name"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let fullname = user
        .and_then(|u| u.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let user_id = user
        .and_then(|u| u.get("id_str"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let user_pic = user
        .and_then(|u| u.get("profile_image_url_https"))
        .and_then(|v| v.as_str())
        .map(strip_pbs_url);

    let text = js
        .get("full_text")
        .and_then(|v| v.as_str())
        .map(decode_legacy_text_entities)
        .unwrap_or_default();

    // Build url_map from entities — the V1 object uses the same legacy structure.
    let url_map = extract_url_entities(js);
    let html = render_tweet_html(&text, &url_map);

    let reply_count = js.get("reply_count").and_then(|v| v.as_i64()).unwrap_or(0);
    let retweet_count = js
        .get("retweet_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let like_count = js
        .get("favorite_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let view_count = js.get("views_count").and_then(|v| v.as_i64()).unwrap_or(0);

    let (photos, videos) = extract_media(js);

    let quote_id = js
        .get("quoted_status_id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok());

    let reply_to_id = js
        .get("in_reply_to_status_id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok());

    let conversation_id = js
        .get("conversation_id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok());

    let entities_json = js.get("entities").map(|e| e.to_string());
    let x_raw_json = Some(js.to_string());

    Some(TweetPayload {
        id,
        user_id,
        username,
        fullname,
        plain_text: text,
        html,
        created_at,
        reply_count,
        retweet_count,
        like_count,
        view_count,
        user_pic,
        photos,
        videos,
        quote_id,
        retweet_id: None,
        reply_to_id,
        conversation_id,
        entities_json,
        x_raw_json,
    })
}

fn parse_graph_tweet(js: &Value) -> Option<TweetPayload> {
    if js.is_null() {
        return None;
    }

    let typ = get_type_name(js);
    match typ {
        "TweetWithVisibilityResults" => return js.get("tweet").and_then(parse_graph_tweet),
        "TweetUnavailable" | "TweetTombstone" | "TweetPreviewDisplay" => return None,
        _ => {}
    }

    let legacy = js.get("legacy")?;

    let id = js
        .get("rest_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            legacy
                .get("id_str")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
        })?;

    let created_at = legacy
        .get("created_at")
        .and_then(|v| v.as_str())
        .and_then(parse_created_at)
        .or_else(|| {
            legacy
                .get("created_at_ms")
                .and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.parse::<i64>().ok())
                        .or(v.as_i64())
                })
                .and_then(unix_millis_to_rfc3339_seconds)
        })
        .unwrap_or_else(now_rfc3339);

    let (user_id, username, fullname, user_pic) = parse_graph_user(js)
        .unwrap_or_else(|| ("".to_string(), "".to_string(), "".to_string(), None));

    let (text, url_map) = select_text_and_url_map(js, legacy);
    let html = render_tweet_html(&text, &url_map);

    let reply_count = legacy
        .get("reply_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let retweet_count = legacy
        .get("retweet_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let like_count = legacy
        .get("favorite_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let view_count = js
        .pointer("/views/count")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| legacy.get("views_count").and_then(|v| v.as_i64()))
        .unwrap_or(0);

    let quote_id = legacy
        .get("quoted_status_id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            js.pointer("/quoted_status_result/result")
                .or_else(|| js.pointer("/quotedPostResults/result"))
                .and_then(extract_graph_tweet_id)
        });

    let retweet_id = legacy
        .get("retweeted_status_id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            // The API nests retweeted_status_result inside legacy, not at the top level.
            legacy
                .pointer("/retweeted_status_result/result")
                .or_else(|| js.pointer("/retweeted_status_result/result"))
                .or_else(|| js.pointer("/repostedStatusResults/result"))
                .and_then(extract_graph_tweet_id)
        })
        .or_else(|| {
            legacy
                .pointer("/retweeted_status/id_str")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
        });

    let reply_to_id = legacy
        .get("in_reply_to_status_id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            js.pointer("/reply_to_results/rest_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
        });

    let conversation_id = legacy
        .get("conversation_id_str")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok());

    let (photos, videos) = extract_media(legacy);

    // Keep legacy entities around for possible future UI improvements/debugging.
    let entities_json = legacy.get("entities").map(|e| e.to_string());
    let x_raw_json = Some(js.to_string());

    let user_id = if user_id.is_empty() {
        legacy
            .get("user_id_str")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    } else {
        user_id
    };

    Some(TweetPayload {
        id,
        user_id,
        username,
        fullname,
        plain_text: text,
        html,
        created_at,
        reply_count,
        retweet_count,
        like_count,
        view_count,
        user_pic,
        photos,
        videos,
        quote_id,
        retweet_id,
        reply_to_id,
        conversation_id,
        entities_json,
        x_raw_json,
    })
}

fn parse_graph_user(js: &Value) -> Option<(String, String, String, Option<String>)> {
    // Primary paths: core/user_result(s)/result (standard GraphQL tweet structure).
    let user = js
        .pointer("/core/user_result/result")
        .or_else(|| js.pointer("/core/user_results/result"))
        // Fallback: some embedded tweets (quotes/retweets) nest user data directly
        // under user_result(s) without a core wrapper.
        .or_else(|| js.pointer("/user_result/result"))
        .or_else(|| js.pointer("/user_results/result"));

    if let Some(user) = user {
        if let Some(legacy) = user.get("legacy") {
            let user_id = user
                .get("rest_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let username = legacy
                .get("screen_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let fullname = legacy
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let user_pic = legacy
                .get("profile_image_url_https")
                .and_then(|v| v.as_str())
                .map(|s| strip_pbs_url(s).replace("_normal", ""));

            if !username.is_empty() {
                return Some((
                    user_id.to_string(),
                    username.to_string(),
                    fullname.to_string(),
                    user_pic.filter(|s| !s.is_empty()),
                ));
            }
        }
    }

    // Last resort: extract screen_name from the legacy.entities mentions or
    // the core object's flat fields (some newer API shapes put it there).
    let screen_name = js.pointer("/core/screen_name").and_then(|v| v.as_str());
    let name = js.pointer("/core/name").and_then(|v| v.as_str());
    if let Some(sn) = screen_name {
        return Some((
            String::new(),
            sn.to_string(),
            name.unwrap_or(sn).to_string(),
            None,
        ));
    }

    None
}

fn strip_pbs_url(url: &str) -> String {
    let mut s = url.strip_prefix("https://").unwrap_or(url).to_string();
    s = s.strip_prefix("http://").unwrap_or(&s).to_string();
    s = s.strip_prefix("pbs.twimg.com/").unwrap_or(&s).to_string();
    s
}

fn extract_media(legacy: &Value) -> (Vec<String>, Vec<VideoMedia>) {
    let mut photos = Vec::new();
    let mut videos = Vec::new();

    if let Some(media) = legacy
        .pointer("/extended_entities/media")
        .and_then(|v| v.as_array())
    {
        for m in media {
            let typ = m
                .get("type")
                .and_then(|v| v.as_str())
                .or_else(|| m.get("__typename").and_then(|v| v.as_str()))
                .unwrap_or("");

            match typ {
                "photo" => {
                    if let Some(u) = m.get("media_url_https").and_then(|v| v.as_str()) {
                        photos.push(strip_pbs_url(u));
                    }
                }
                "video" | "animated_gif" => {
                    let poster = m
                        .get("media_url_https")
                        .and_then(|v| v.as_str())
                        .map(strip_pbs_url)
                        .filter(|s| !s.is_empty());

                    let mut variants: Vec<VideoVariant> = Vec::new();
                    if let Some(arr) = m.pointer("/video_info/variants").and_then(|v| v.as_array())
                    {
                        for vv in arr {
                            let url = vv.get("url").and_then(|v| v.as_str()).unwrap_or_default();
                            if url.is_empty() {
                                continue;
                            }
                            let content_type = vv
                                .get("content_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let bitrate = vv.get("bitrate").and_then(|v| v.as_i64());
                            variants.push(VideoVariant {
                                url: url.to_string(),
                                content_type,
                                bitrate,
                            });
                        }
                    }

                    if !variants.is_empty() {
                        videos.push(VideoMedia {
                            kind: typ.to_string(),
                            poster,
                            variants,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    (photos, videos)
}

fn parse_created_at(s: &str) -> Option<String> {
    parse_x_created_at_to_rfc3339(s)
}

/// Extract the t.co → expanded URL mapping from `legacy.entities.urls`.
fn extract_url_entities(legacy: &Value) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(urls) = legacy.pointer("/entities/urls").and_then(|v| v.as_array()) {
        for u in urls {
            let short = u.get("url").and_then(|v| v.as_str()).unwrap_or_default();
            let expanded = u
                .get("expanded_url")
                .and_then(|v| v.as_str())
                .unwrap_or(short);
            if !short.is_empty() && !expanded.is_empty() {
                map.insert(short.to_string(), expanded.to_string());
            }
        }
    }
    // Media URLs (t.co links to images/video) — we hide these since we render media separately.
    if let Some(media) = legacy.pointer("/entities/media").and_then(|v| v.as_array()) {
        for m in media {
            if let Some(short) = m.get("url").and_then(|v| v.as_str()) {
                // Map media t.co links to empty string so they get removed from text.
                map.entry(short.to_string()).or_default();
            }
        }
    }
    // Some payloads only include media URLs under extended_entities (no entities.media).
    if let Some(media) = legacy
        .pointer("/extended_entities/media")
        .and_then(|v| v.as_array())
    {
        for m in media {
            if let Some(short) = m.get("url").and_then(|v| v.as_str()) {
                map.entry(short.to_string()).or_default();
            }
        }
    }
    map
}

fn merge_url_entities_from_urls_array(map: &mut HashMap<String, String>, urls: &Value) {
    let Some(arr) = urls.as_array() else {
        return;
    };

    for u in arr {
        let short = u.get("url").and_then(|v| v.as_str()).unwrap_or_default();
        if short.is_empty() {
            continue;
        }

        let expanded = u
            .get("expanded_url")
            .or_else(|| u.get("expandedUrl"))
            .and_then(|v| v.as_str())
            .unwrap_or(short);

        if expanded.is_empty() {
            continue;
        }

        map.insert(short.to_string(), expanded.to_string());
    }
}

fn merge_media_tco_as_hidden(map: &mut HashMap<String, String>, media: &Value) {
    let Some(arr) = media.as_array() else {
        return;
    };
    for m in arr {
        if let Some(short) = m.get("url").and_then(|v| v.as_str()) {
            map.entry(short.to_string()).or_default();
        }
    }
}

/// Pick the best available text (note_tweet for long tweets) and build a t.co → expanded map
/// so the UI shows full URLs instead of the shortener.
fn select_text_and_url_map(js: &Value, legacy: &Value) -> (String, HashMap<String, String>) {
    let note_text = js
        .pointer("/note_tweet/note_tweet_results/result/text")
        .and_then(|v| v.as_str());

    let text = match note_text {
        Some(text) => text.to_string(),
        None => legacy
            .get("full_text")
            .and_then(|v| v.as_str())
            .map(decode_legacy_text_entities)
            .unwrap_or_default(),
    };

    // Start with legacy entities, then layer note_tweet entity_set on top.
    let mut url_map = extract_url_entities(legacy);

    if let Some(urls) = js.pointer("/note_tweet/note_tweet_results/result/entity_set/urls") {
        merge_url_entities_from_urls_array(&mut url_map, urls);
    }
    if let Some(media) = js.pointer("/note_tweet/note_tweet_results/result/entity_set/media") {
        // Media t.co links should be removed from rendered text; images are rendered separately.
        merge_media_tco_as_hidden(&mut url_map, media);
    }

    (text, url_map)
}

fn decode_legacy_text_entities(text: &str) -> String {
    if text.contains('&') {
        decode_html_entities(text).into_owned()
    } else {
        text.to_string()
    }
}

pub fn normalize_search_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = true;
    for ch in text.chars().flat_map(|c| c.to_lowercase()) {
        let normalized = match ch {
            '\r' | '\n' | '\t' => ' ',
            _ => ch,
        };
        if normalized.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(normalized);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

pub fn html_to_search_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut prev_space = true;
    let mut chars = html.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            '&' if !in_tag => {
                let mut entity = String::from("&");
                while let Some(&next) = chars.peek() {
                    entity.push(next);
                    chars.next();
                    if next == ';' || entity.len() >= 16 {
                        break;
                    }
                }
                let decoded = decode_html_entities(&entity);
                for dc in decoded.chars() {
                    if dc.is_whitespace() {
                        if !prev_space {
                            out.push(' ');
                            prev_space = true;
                        }
                    } else {
                        out.push(dc);
                        prev_space = false;
                    }
                }
            }
            _ if !in_tag => {
                if ch.is_whitespace() {
                    if !prev_space {
                        out.push(' ');
                        prev_space = true;
                    }
                } else {
                    out.push(ch);
                    prev_space = false;
                }
            }
            _ => {}
        }
    }

    normalize_search_text(&out)
}

pub fn search_text_from_raw_json(raw: &str) -> Option<String> {
    let js: Value = serde_json::from_str(raw).ok()?;
    parse_graph_tweet(&js)
        .or_else(|| parse_v1_tweet(&js))
        .map(|tweet| normalize_search_text(&tweet.plain_text))
}

fn render_tweet_html(text: &str, url_map: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut last = 0;
    while let Some((start, end)) = find_next_url(text, last) {
        let seg = &text[last..start];
        out.push_str(&linkify_mentions_hashtags(seg));

        let url = &text[start..end];
        if let Some(expanded) = url_map.get(url) {
            if expanded.is_empty() {
                // Media URL — strip from text entirely.
            } else {
                let href = encode_text(expanded);
                let display = encode_text(expanded);
                out.push_str(&format!("<a href=\"{}\">{}</a>", href, display));
            }
        } else {
            let url_esc = encode_text(url);
            out.push_str(&format!("<a href=\"{}\">{}</a>", url_esc, url_esc));
        }
        last = end;
    }
    let rest = &text[last..];
    out.push_str(&linkify_mentions_hashtags(rest));

    out.replace('\n', "<br/>")
}

fn linkify_mentions_hashtags(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len() + 16);
    let chars: Vec<(usize, char)> = seg.char_indices().collect();
    let mut last = 0;
    let mut i = 0;

    while i < chars.len() {
        let (idx, ch) = chars[i];
        let prev_is_word = i > 0 && is_social_word(chars[i - 1].1);
        if (ch == '@' || ch == '#') && !prev_is_word {
            let max_len = if ch == '@' { 15 } else { usize::MAX };
            let mut j = i + 1;
            let mut len = 0usize;
            while j < chars.len() && is_social_word(chars[j].1) && len < max_len {
                j += 1;
                len += 1;
            }
            if len > 0 {
                let end = if j < chars.len() {
                    chars[j].0
                } else {
                    seg.len()
                };
                out.push_str(&encode_text(&seg[last..idx]));
                let token = &seg[idx + ch.len_utf8()..end];
                if ch == '@' {
                    out.push_str(&format!("<a href=\"https://x.com/{token}\">@{token}</a>"));
                } else {
                    out.push_str(&format!(
                        "<a href=\"https://x.com/search?q=%23{token}\">#{token}</a>"
                    ));
                }
                last = end;
                i = j;
                continue;
            }
        }
        i += 1;
    }

    out.push_str(&encode_text(&seg[last..]));
    out
}

fn find_next_url(text: &str, from: usize) -> Option<(usize, usize)> {
    let rest = &text[from..];
    let rel_start = match (rest.find("https://"), rest.find("http://")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }?;
    let start = from + rel_start;
    let mut end = text.len();
    for (offset, ch) in text[start..].char_indices() {
        if offset > 0 && ch.is_whitespace() {
            end = start + offset;
            break;
        }
    }
    Some((start, end))
}

fn is_social_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::{
        decode_legacy_text_entities, html_to_search_text, normalize_search_text, render_tweet_html,
        search_text_from_raw_json, select_text_and_url_map,
    };

    #[test]
    fn decodes_legacy_full_text_entities_before_rendering() {
        let html = render_tweet_html(
            &decode_legacy_text_entities("A &amp; B\n&gt; quote"),
            &HashMap::new(),
        );

        assert_eq!(html, "A &amp; B<br/>&gt; quote");
        assert!(!html.contains("&amp;amp;"));
        assert!(!html.contains("&amp;gt;"));
    }

    #[test]
    fn select_text_uses_note_tweet_text_when_available() {
        let js = json!({
            "note_tweet": {
                "note_tweet_results": {
                    "result": {
                        "text": "Fish & Chips > Fries"
                    }
                }
            }
        });
        let legacy = json!({
            "full_text": "Fish &amp; Chips &gt; Fries"
        });

        let (text, url_map) = select_text_and_url_map(&js, &legacy);

        assert_eq!(text, "Fish & Chips > Fries");
        assert!(url_map.is_empty());
    }

    #[test]
    fn select_text_decodes_legacy_full_text_when_note_tweet_missing() {
        let js = json!({});
        let legacy = json!({
            "full_text": "Fish &amp; Chips &gt; Fries"
        });

        let (text, _) = select_text_and_url_map(&js, &legacy);

        assert_eq!(text, "Fish & Chips > Fries");
    }

    #[test]
    fn render_tweet_html_linkifies_mentions_hashtags_and_urls_without_regex() {
        let mut url_map = HashMap::new();
        url_map.insert(
            "https://t.co/abc".to_string(),
            "https://example.com/a".to_string(),
        );

        let html = render_tweet_html("Hi @alice check #rust https://t.co/abc", &url_map);

        assert_eq!(
            html,
            "Hi <a href=\"https://x.com/alice\">@alice</a> check <a href=\"https://x.com/search?q=%23rust\">#rust</a> <a href=\"https://example.com/a\">https://example.com/a</a>"
        );
    }

    #[test]
    fn normalize_search_text_collapses_whitespace_and_case() {
        assert_eq!(
            normalize_search_text("  Fish\tAND\nChips  "),
            "fish and chips"
        );
    }

    #[test]
    fn html_to_search_text_strips_tags_and_decodes_entities() {
        assert_eq!(
            html_to_search_text("Fish<br/> &amp; <a href=\"https://x.com\">Chips</a>"),
            "fish & chips"
        );
    }

    #[test]
    fn search_text_from_raw_json_extracts_plain_text() {
        let raw = json!({
            "rest_id": "123",
            "core": {
                "user_result": {
                    "result": {
                        "rest_id": "7",
                        "legacy": {
                            "screen_name": "chef",
                            "name": "Chef"
                        }
                    }
                }
            },
            "legacy": {
                "id_str": "123",
                "created_at": "Sat Mar 07 14:22:03 +0000 2026",
                "full_text": "Fish &amp; Chips",
                "reply_count": 0,
                "retweet_count": 0,
                "favorite_count": 0,
                "entities": { "urls": [] }
            }
        })
        .to_string();

        assert_eq!(
            search_text_from_raw_json(&raw).as_deref(),
            Some("fish & chips")
        );
    }
}
