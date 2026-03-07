use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default, alias = "authToken")]
    pub auth_token: String,
    #[serde(default)]
    pub ct0: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_archive_path")]
    pub archive_path: String,
    #[serde(default)]
    pub sessions: Vec<SessionConfig>,
    #[serde(default)]
    pub list_ids: Vec<String>,
    #[serde(default = "default_poll_mins")]
    pub poll_mins: u64,
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,
    #[serde(default = "default_page_delay_ms")]
    pub page_delay_ms: u64,
    #[serde(default = "default_feed_delay_ms")]
    pub feed_delay_ms: u64,
    #[serde(default)]
    pub tid_disable: bool,
    #[serde(default = "default_tid_pairs_url")]
    pub tid_pairs_url: String,
}

fn default_archive_path() -> String {
    "state/archive.sqlite".to_string()
}

fn default_poll_mins() -> u64 {
    15
}

fn default_max_pages() -> usize {
    20
}

fn default_page_delay_ms() -> u64 {
    2000
}

fn default_feed_delay_ms() -> u64 {
    30_000
}

fn default_tid_pairs_url() -> String {
    "https://raw.githubusercontent.com/fa0311/x-client-transaction-id-pair-dict/refs/heads/main/pair.json".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            archive_path: default_archive_path(),
            sessions: vec![],
            list_ids: vec![],
            poll_mins: default_poll_mins(),
            max_pages: default_max_pages(),
            page_delay_ms: default_page_delay_ms(),
            feed_delay_ms: default_feed_delay_ms(),
            tid_disable: false,
            tid_pairs_url: default_tid_pairs_url(),
        }
    }
}

impl Config {
    pub const DEFAULT_PATH: &'static str = "state/settings.toml";

    pub fn load(path: &Path) -> Result<Self> {
        let mut buf = std::fs::read_to_string(path)
            .with_context(|| format!("read settings {}", path.display()))?;
        if buf.starts_with('\u{feff}') {
            buf = buf.trim_start_matches('\u{feff}').to_string();
        }
        let mut cfg: Config = soml::from_str(&buf).with_context(|| "parse settings toml")?;
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create settings directory {}", parent.display()))?;
        }
        std::fs::write(path, self.to_toml_string())
            .with_context(|| format!("write settings {}", path.display()))
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("TWIT_RANK_ARCHIVE_PATH") {
            if !v.trim().is_empty() {
                self.archive_path = v;
            }
        }
        if let Ok(v) = std::env::var("TWIT_RANK_LIST_IDS") {
            let ids = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if !ids.is_empty() {
                self.list_ids = ids;
            }
        }
        if let Ok(v) = std::env::var("TWIT_RANK_POLL_MINS") {
            if let Ok(n) = v.trim().parse::<u64>() {
                self.poll_mins = n;
            }
        }
        if let Ok(v) = std::env::var("TWIT_RANK_MAX_PAGES") {
            if let Ok(n) = v.trim().parse::<usize>() {
                self.max_pages = n;
            }
        }
        if let Ok(v) = std::env::var("TWIT_RANK_PAGE_DELAY_MS") {
            if let Ok(n) = v.trim().parse::<u64>() {
                self.page_delay_ms = n;
            }
        }
        if let Ok(v) = std::env::var("TWIT_RANK_FEED_DELAY_MS") {
            if let Ok(n) = v.trim().parse::<u64>() {
                self.feed_delay_ms = n;
            }
        }
        if let Ok(v) = std::env::var("TWIT_RANK_TID_DISABLE") {
            let v = v.trim().to_ascii_lowercase();
            self.tid_disable = v == "1" || v == "true" || v == "yes";
        }
        if let Ok(v) = std::env::var("TWIT_RANK_TID_PAIRS_URL") {
            if !v.trim().is_empty() {
                self.tid_pairs_url = v;
            }
        }
    }

    pub fn has_sessions(&self) -> bool {
        self.sessions
            .iter()
            .any(|s| !s.auth_token.trim().is_empty() && !s.ct0.trim().is_empty())
    }

    pub fn sanitized_for_ui(&self) -> Self {
        let mut cfg = self.clone();
        for session in &mut cfg.sessions {
            session.auth_token.clear();
            session.ct0.clear();
        }
        cfg
    }

    fn to_toml_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "archive_path = {}\n",
            toml_string(&self.archive_path)
        ));
        out.push_str("list_ids = [\n");
        for list in &self.list_ids {
            out.push_str(&format!("  {},\n", toml_string(list)));
        }
        out.push_str("]\n");
        out.push_str(&format!("poll_mins = {}\n", self.poll_mins));
        out.push_str(&format!("max_pages = {}\n", self.max_pages));
        out.push_str(&format!("page_delay_ms = {}\n", self.page_delay_ms));
        out.push_str(&format!("feed_delay_ms = {}\n", self.feed_delay_ms));
        out.push_str(&format!("tid_disable = {}\n", self.tid_disable));
        out.push_str(&format!(
            "tid_pairs_url = {}\n",
            toml_string(&self.tid_pairs_url)
        ));

        for session in &self.sessions {
            out.push_str("\n[[sessions]]\n");
            if !session.id.trim().is_empty() {
                out.push_str(&format!("id = {}\n", toml_string(session.id.trim())));
            }
            if !session.username.trim().is_empty() {
                out.push_str(&format!(
                    "username = {}\n",
                    toml_string(session.username.trim())
                ));
            }
            out.push_str(&format!(
                "auth_token = {}\n",
                toml_string(session.auth_token.trim())
            ));
            out.push_str(&format!("ct0 = {}\n", toml_string(session.ct0.trim())));
        }

        out
    }
}

fn toml_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}
