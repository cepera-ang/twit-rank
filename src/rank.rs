use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::model::Tweet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingConfig {
    /// Half-life in minutes for time decay
    #[serde(default = "RankingConfig::default_half_life_mins")]
    pub half_life_mins: f64,
    /// Case-insensitive text boosts: if substring appears in title/content, add to multiplier
    #[serde(default)]
    pub text_boosts: Vec<TextBoost>,
    /// Per-user multiplier boosts
    #[serde(default)]
    pub user_boosts: Vec<UserBoost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBoost {
    pub contains: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserBoost {
    pub user: String,
    pub weight: f64,
}

impl Default for RankingConfig {
    fn default() -> Self {
        Self {
            half_life_mins: Self::default_half_life_mins(),
            text_boosts: vec![],
            user_boosts: vec![],
        }
    }
}

impl RankingConfig {
    fn default_half_life_mins() -> f64 {
        240.0
    }
}

pub trait Ranker {
    fn score(&self, tweet: &Tweet) -> f64;
}

#[derive(Clone)]
pub struct BasicRanker {
    cfg: RankingConfig,
}

impl BasicRanker {
    pub fn from_config(cfg: RankingConfig) -> Self {
        Self { cfg }
    }

    fn time_decay(&self, published: &OffsetDateTime) -> f64 {
        let age_secs = (OffsetDateTime::now_utc() - *published)
            .whole_seconds()
            .max(0) as f64;
        let age_mins = age_secs / 60.0;
        let half_life = self.cfg.half_life_mins.max(1e-6);
        // exponential decay: factor = 2^(-age/half_life)
        (2.0_f64).powf(-age_mins / half_life)
    }
}

impl Ranker for BasicRanker {
    fn score(&self, tweet: &Tweet) -> f64 {
        let mut mult = 1.0;

        // user boost
        for ub in &self.cfg.user_boosts {
            if tweet.username.eq_ignore_ascii_case(&ub.user) {
                mult += ub.weight;
            }
        }

        // text boosts
        // Tweet doesn't have title, just text
        let text = tweet.text.to_lowercase();

        for tb in &self.cfg.text_boosts {
            if !tb.contains.is_empty() && text.contains(&tb.contains.to_lowercase()) {
                mult += tb.weight;
            }
        }

        let time_factor = self.time_decay(&tweet.published_dt());
        time_factor * mult
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FeedbackSignals {
    pub post_feedback: i64,
    pub user_bias: f64,
}

pub const FEEDBACK_WEIGHT: f64 = 0.35;
pub const USER_FEEDBACK_WEIGHT: f64 = 0.2;

pub fn apply_feedback(score: f64, signals: FeedbackSignals) -> f64 {
    let mut mult = 1.0 + FEEDBACK_WEIGHT * (signals.post_feedback as f64);
    mult += USER_FEEDBACK_WEIGHT * signals.user_bias;
    if mult < 0.1 {
        mult = 0.1;
    }
    score * mult
}

pub fn user_bias_from_counts(likes: i64, dislikes: i64) -> f64 {
    let total = (likes + dislikes) as f64;
    if total <= 0.0 {
        return 0.0;
    }
    (likes as f64 - dislikes as f64) / (total + 2.0)
}
