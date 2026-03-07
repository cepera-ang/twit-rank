use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::archive_writer::ArchiveWriter;
use crate::time_util::now_rfc3339_seconds;
use crate::x::{HomeTimelineKind, XClient};

#[derive(Debug, Clone)]
pub struct ListSpec {
    pub id: String,
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct ArchiverConfig {
    pub poll_mins: u64,
    pub max_pages: usize,
    pub tweets_per_page: i64,
    pub page_delay_ms: u64,
    pub feed_delay_ms: u64,
    pub lists: Vec<ListSpec>,
}

impl Default for ArchiverConfig {
    fn default() -> Self {
        Self {
            poll_mins: 15,
            max_pages: 20,
            tweets_per_page: 50,
            page_delay_ms: 2000,
            feed_delay_ms: 30_000,
            lists: vec![],
        }
    }
}

pub async fn run_loop(
    x: Arc<XClient>,
    writer: Arc<ArchiveWriter>,
    cfg: ArchiverConfig,
    once: bool,
) -> Result<()> {
    // In continuous mode, transient upstream errors should not kill the archiver task.
    // Keep retrying after a backoff so scraping resumes automatically.
    const ERROR_RETRY_SECS: u64 = 5 * 60;

    loop {
        if once {
            run_once(x.clone(), writer.clone(), cfg.clone()).await?;
            break;
        }

        match run_once(x.clone(), writer.clone(), cfg.clone()).await {
            Ok(()) => {
                tokio::time::sleep(Duration::from_secs(cfg.poll_mins * 60)).await;
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    retry_in_secs = ERROR_RETRY_SECS,
                    "archiver cycle failed; will retry"
                );
                tokio::time::sleep(Duration::from_secs(ERROR_RETRY_SECS)).await;
            }
        }
    }
    Ok(())
}

async fn run_once(x: Arc<XClient>, writer: Arc<ArchiveWriter>, cfg: ArchiverConfig) -> Result<()> {
    tracing::info!("archiver tick");

    collect_home(
        x.clone(),
        writer.clone(),
        cfg.clone(),
        HomeTimelineKind::Following,
        "following",
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(cfg.feed_delay_ms)).await;

    for list in cfg.lists.iter() {
        let label = format!("list:{}", list.slug);
        collect_list(x.clone(), writer.clone(), cfg.clone(), &list.id, &label).await?;
        tokio::time::sleep(Duration::from_millis(cfg.feed_delay_ms)).await;
    }

    collect_home(
        x.clone(),
        writer.clone(),
        cfg.clone(),
        HomeTimelineKind::ForYou,
        "forYou",
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(cfg.feed_delay_ms)).await;

    Ok(())
}

async fn collect_home(
    x: Arc<XClient>,
    writer: Arc<ArchiveWriter>,
    cfg: ArchiverConfig,
    kind: HomeTimelineKind,
    label: &str,
) -> Result<()> {
    tracing::info!(feed = label, "collecting home timeline");
    let mut cursor: Option<String> = None;

    for page in 1..=cfg.max_pages {
        let tl = x
            .home_timeline(kind, cfg.tweets_per_page, cursor.as_deref())
            .await?;

        if tl.tweets.is_empty() {
            tracing::info!(feed = label, page, "empty page, stopping");
            break;
        }

        let archived_at = now_rfc3339_seconds();
        let primary_rows = tl
            .tweets
            .into_iter()
            .map(|t| t.into_model(label, &archived_at))
            .collect::<Vec<_>>();
        let related_rows = tl
            .related
            .into_iter()
            .map(|rt| rt.tweet.into_model(rt.kind.feed_kind(), &archived_at))
            .collect::<Vec<_>>();

        // Count "new" only on primary timeline items so we don't change pagination behavior.
        let page_new = writer.upsert_tweets(&primary_rows)?;
        let related_new = writer.upsert_tweets(&related_rows)?;

        tracing::info!(
            feed = label,
            page,
            new = page_new,
            related_new,
            "page collected"
        );

        if page_new == 0 {
            tracing::info!(feed = label, page, "caught up, stopping");
            break;
        }

        cursor = tl.bottom_cursor;
        if cursor.is_none() {
            break;
        }

        tokio::time::sleep(Duration::from_millis(cfg.page_delay_ms)).await;
    }

    Ok(())
}

async fn collect_list(
    x: Arc<XClient>,
    writer: Arc<ArchiveWriter>,
    cfg: ArchiverConfig,
    list_id: &str,
    label: &str,
) -> Result<()> {
    tracing::info!(feed = label, "collecting list timeline");
    let mut cursor: Option<String> = None;

    for page in 1..=cfg.max_pages {
        let tl = x
            // Keep list timeline page size at 20 to match the current endpoint behavior.
            .list_timeline(list_id, 20, cursor.as_deref())
            .await?;

        if tl.tweets.is_empty() {
            tracing::info!(feed = label, page, "empty page, stopping");
            break;
        }

        let archived_at = now_rfc3339_seconds();
        let primary_rows = tl
            .tweets
            .into_iter()
            .map(|t| t.into_model(label, &archived_at))
            .collect::<Vec<_>>();
        let related_rows = tl
            .related
            .into_iter()
            .map(|rt| rt.tweet.into_model(rt.kind.feed_kind(), &archived_at))
            .collect::<Vec<_>>();

        // Count "new" only on primary timeline items so we don't change pagination behavior.
        let page_new = writer.upsert_tweets(&primary_rows)?;
        let related_new = writer.upsert_tweets(&related_rows)?;

        tracing::info!(
            feed = label,
            page,
            new = page_new,
            related_new,
            "page collected"
        );

        if page_new == 0 {
            tracing::info!(feed = label, page, "caught up, stopping");
            break;
        }

        cursor = tl.bottom_cursor;
        if cursor.is_none() {
            break;
        }

        tokio::time::sleep(Duration::from_millis(cfg.page_delay_ms)).await;
    }

    Ok(())
}
