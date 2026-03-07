use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use pico_args::Arguments;
use tokio::task::JoinHandle;
use tracing::{info, Level};

use twit_rank::archive_writer::ArchiveWriter;
use twit_rank::archiver;
use twit_rank::cache::SqliteCache;
use twit_rank::config::Config;
use twit_rank::web;
use twit_rank::x::XClient;

const DEFAULT_BIND: &str = "127.0.0.1:3030";
const STARTUP_MAINTENANCE_BATCH_SIZE: usize = 500;
const HELP_TEXT: &str = "\
twit-rank

Archive and serve local X timelines

USAGE:
  twit-rank [OPTIONS] [run|serve|archive] [--once]

OPTIONS:
  -c, --config <PATH>         Path to settings TOML
      --archive <PATH>        Override archive DB path
      --list-ids <IDS>        Override list IDs (comma-separated)
      --poll-mins <MINUTES>   Archiver poll interval
      --max-pages <COUNT>     Archiver max pages per feed/list per tick
      --tid-disable           Disable x-client-transaction-id generation
      --tid-pairs-url <URL>   Override TID pair dictionary URL
      --bind <ADDR>           Bind address for web UI/API (default: 127.0.0.1:3030)
  -h, --help                  Show this help
  -V, --version               Show version

COMMANDS:
  run      Start archiver + web UI/API in one process
  serve    Start web UI/API only
  archive  Start background archiver only

COMMAND FLAGS:
  --once   For run/archive: run one archiver tick and stop the archiver
";

#[derive(Debug, Clone, Default)]
struct Args {
    config: Option<PathBuf>,
    archive: Option<String>,
    list_ids: Option<String>,
    poll_mins: Option<u64>,
    max_pages: Option<usize>,
    tid_disable: bool,
    tid_pairs_url: Option<String>,
    bind: String,
    command: Option<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Run { once: bool },
    Serve,
    Archive { once: bool },
}

#[tokio::main]
async fn main() -> Result<()> {
    let startup_started = Instant::now();
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .init();

    let step_started = Instant::now();
    let args = parse_args_or_exit()?;
    log_startup_step("parse args", step_started, startup_started);

    let step_started = Instant::now();
    let settings_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(Config::DEFAULT_PATH));

    let mut cfg = if settings_path.exists() {
        Config::load(&settings_path)?
    } else {
        let mut cfg = Config::default();
        cfg.apply_env_overrides();
        cfg
    };
    log_startup_step("load settings", step_started, startup_started);

    let step_started = Instant::now();
    if let Some(path) = args.archive {
        cfg.archive_path = path;
    }
    if let Some(ids) = args.list_ids {
        let parsed = ids
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            cfg.list_ids = parsed;
        }
    }
    if let Some(n) = args.poll_mins {
        cfg.poll_mins = n;
    }
    if let Some(n) = args.max_pages {
        cfg.max_pages = n;
    }
    if args.tid_disable {
        cfg.tid_disable = true;
    }
    if let Some(u) = args.tid_pairs_url {
        cfg.tid_pairs_url = u;
    }

    normalize_and_validate_paths(&mut cfg)?;
    log_startup_step("normalize paths", step_started, startup_started);

    info!(
        archive = %cfg.archive_path,
        settings = %settings_path.display(),
        sessions = cfg.sessions.len(),
        "starting service"
    );

    let step_started = Instant::now();
    let _writer_lock = acquire_writer_lock(&cfg.archive_path)?;
    log_startup_step("acquire archive lock", step_started, startup_started);

    // Ensure archive DB schema exists and is writable.
    let step_started = Instant::now();
    let writer = Arc::new(ArchiveWriter::open(&cfg.archive_path)?);
    let schema_report = writer.init_schema()?;
    log_startup_step("init schema", step_started, startup_started);
    let step_started = Instant::now();
    let cache = Arc::new(SqliteCache::open(PathBuf::from(&cfg.archive_path))?);
    log_startup_step("open cache", step_started, startup_started);

    let step_started = Instant::now();
    let x = Arc::new(XClient::new(
        &cfg.sessions,
        Some(cfg.tid_pairs_url.clone()),
        cfg.tid_disable,
    )?);
    log_startup_step("build x client", step_started, startup_started);

    spawn_startup_maintenance(writer.clone(), schema_report);

    let cmd = args.command.unwrap_or(Command::Run { once: false });
    match cmd {
        Command::Serve => {
            tokio::select! {
                res = web::serve(args.bind, cfg, settings_path, x, writer, cache) => res,
                res = wait_for_ctrl_c() => {
                    res?;
                    tracing::info!(message = graceful_shutdown_line(), "shutdown requested, stopping web ui");
                    Ok(())
                }
            }
        }
        Command::Archive { once } => {
            if !cfg.has_sessions() {
                bail!(
                    "no X sessions configured; save {} from the setup page first",
                    settings_path.display()
                );
            }
            let arch_cfg = archiver_config_from_cfg(&cfg);
            tokio::select! {
                res = archiver::run_loop(x, writer, arch_cfg, once) => res,
                res = wait_for_ctrl_c() => {
                    res?;
                    tracing::info!(message = graceful_shutdown_line(), "shutdown requested, stopping archiver");
                    Ok(())
                }
            }
        }
        Command::Run { once } => {
            let mut archiver_task: Option<JoinHandle<()>> = None;
            if cfg.has_sessions() {
                let arch_cfg = archiver_config_from_cfg(&cfg);
                let x2 = x.clone();
                let w2 = writer.clone();
                archiver_task = Some(tokio::spawn(async move {
                    if let Err(e) = archiver::run_loop(x2, w2, arch_cfg, once).await {
                        tracing::error!("archiver exited with error: {}", e);
                    }
                }));
            } else {
                tracing::warn!(
                    "no X sessions configured; starting web UI only so setup can be completed"
                );
            }

            let result = tokio::select! {
                res = web::serve(args.bind, cfg, settings_path, x, writer, cache) => res,
                res = wait_for_ctrl_c() => {
                    res?;
                    tracing::info!(message = graceful_shutdown_line(), "shutdown requested, stopping web ui and archiver");
                    Ok(())
                }
            };

            if let Some(handle) = archiver_task {
                handle.abort();
                let _ = handle.await;
            }

            result
        }
    }
}

fn log_startup_step(step: &str, step_started: Instant, startup_started: Instant) {
    tracing::info!(
        step,
        elapsed_ms = step_started.elapsed().as_millis() as u64,
        total_ms = startup_started.elapsed().as_millis() as u64,
        "startup step complete"
    );
}

async fn wait_for_ctrl_c() -> Result<()> {
    tokio::signal::ctrl_c()
        .await
        .context("wait for Ctrl+C shutdown signal")
}

fn graceful_shutdown_line() -> &'static str {
    const LINES: &[&str] = &[
        "Enough scrolling for one session.",
        "Archive closed. Go look at something farther away than a screen.",
        "That is enough timeline for today.",
        "Packets at rest. Touch grass if available.",
        "The feed can wait.",
    ];
    LINES[fastrand::usize(..LINES.len())]
}

fn spawn_startup_maintenance(
    writer: Arc<ArchiveWriter>,
    report: twit_rank::archive_writer::InitSchemaReport,
) {
    if !report.needs_username_lc_maintenance && !report.needs_search_text_maintenance {
        tracing::info!("startup maintenance already clean");
        return;
    }

    tracing::info!(
        needs_username_lc = report.needs_username_lc_maintenance,
        needs_search_text = report.needs_search_text_maintenance,
        batch_size = STARTUP_MAINTENANCE_BATCH_SIZE,
        "startup maintenance queued"
    );

    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        match writer.run_startup_maintenance(STARTUP_MAINTENANCE_BATCH_SIZE) {
            Ok(summary) => {
                tracing::info!(
                    username_lc_updated = summary.username_lc_updated,
                    search_text_updated = summary.search_text_updated,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "startup maintenance complete"
                );
            }
            Err(err) => {
                tracing::error!("startup maintenance failed: {}", err);
            }
        }
    });
}

fn parse_args_or_exit() -> Result<Args> {
    let mut pargs = Arguments::from_env();
    if pargs.contains(["-h", "--help"]) {
        print!("{HELP_TEXT}");
        std::process::exit(0);
    }
    if pargs.contains(["-V", "--version"]) {
        println!("{}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    let config = pargs
        .opt_value_from_os_str(["-c", "--config"], |s| {
            Ok::<PathBuf, std::convert::Infallible>(s.into())
        })
        .with_context(|| "parse --config")?;
    let archive = pargs
        .opt_value_from_str::<_, String>("--archive")
        .with_context(|| "parse --archive")?;
    let list_ids = pargs
        .opt_value_from_str::<_, String>("--list-ids")
        .with_context(|| "parse --list-ids")?;
    let poll_mins = pargs
        .opt_value_from_str::<_, u64>("--poll-mins")
        .with_context(|| "parse --poll-mins")?;
    let max_pages = pargs
        .opt_value_from_str::<_, usize>("--max-pages")
        .with_context(|| "parse --max-pages")?;
    let tid_disable = pargs.contains("--tid-disable");
    let tid_pairs_url = pargs
        .opt_value_from_str::<_, String>("--tid-pairs-url")
        .with_context(|| "parse --tid-pairs-url")?;
    let bind = pargs
        .opt_value_from_str::<_, String>("--bind")
        .with_context(|| "parse --bind")?
        .unwrap_or_else(|| DEFAULT_BIND.to_string());
    let once = pargs.contains("--once");
    let command = match pargs.subcommand().with_context(|| "parse subcommand")? {
        None => None,
        Some(cmd) if cmd == "run" => Some(Command::Run { once }),
        Some(cmd) if cmd == "serve" => {
            if once {
                bail!("--once is only valid with `run` or `archive`\n\n{HELP_TEXT}");
            }
            Some(Command::Serve)
        }
        Some(cmd) if cmd == "archive" => Some(Command::Archive { once }),
        Some(cmd) => bail!("unknown subcommand: {cmd}\n\n{HELP_TEXT}"),
    };

    let remaining = pargs.finish();
    if !remaining.is_empty() {
        let extras = remaining
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        bail!("unknown arguments: {extras}\n\n{HELP_TEXT}");
    }

    Ok(Args {
        config,
        archive,
        list_ids,
        poll_mins,
        max_pages,
        tid_disable,
        tid_pairs_url,
        bind,
        command,
    })
}

fn normalize_and_validate_paths(cfg: &mut Config) -> Result<()> {
    let cwd = std::env::current_dir().context("read current working directory")?;

    let archive_path = absolutize_path(&cwd, &cfg.archive_path);
    ensure_parent_dir_exists("archive db", &archive_path)?;
    ensure_not_directory("archive db", &archive_path)?;

    cfg.archive_path = archive_path.to_string_lossy().into_owned();

    Ok(())
}

fn absolutize_path(cwd: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

fn ensure_not_directory(label: &str, path: &Path) -> Result<()> {
    if path.exists() && path.is_dir() {
        bail!("{label} path points to a directory: {}", path.display());
    }
    Ok(())
}

fn ensure_parent_dir_exists(label: &str, path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{label} path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create parent directory for {label}: {}", parent.display()))?;
    Ok(())
}

fn acquire_writer_lock(archive_path: &str) -> Result<File> {
    let archive_path = PathBuf::from(archive_path);
    let lock_path = archive_lock_path(&archive_path);
    ensure_parent_dir_exists("archive lock", &lock_path)?;

    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open archive lock file {}", lock_path.display()))?;
    if file
        .metadata()
        .with_context(|| format!("stat archive lock file {}", lock_path.display()))?
        .len()
        == 0
    {
        file.set_len(1)
            .with_context(|| format!("initialize archive lock file {}", lock_path.display()))?;
    }

    if let Err(err) = file.try_lock() {
        match err {
            TryLockError::WouldBlock => {
                bail!(
                    "archive is already locked by another twit-rank process: {}",
                    lock_path.display()
                );
            }
            TryLockError::Error(err) => {
                return Err(err)
                    .with_context(|| format!("lock archive file {}", lock_path.display()));
            }
        }
    }

    Ok(file)
}

fn archive_lock_path(archive_path: &Path) -> PathBuf {
    let mut lock_path = archive_path.as_os_str().to_owned();
    lock_path.push(".lock");
    PathBuf::from(lock_path)
}

fn archiver_config_from_cfg(cfg: &Config) -> archiver::ArchiverConfig {
    let mut out = archiver::ArchiverConfig {
        poll_mins: cfg.poll_mins,
        max_pages: cfg.max_pages,
        page_delay_ms: cfg.page_delay_ms,
        feed_delay_ms: cfg.feed_delay_ms,
        ..Default::default()
    };

    out.lists = cfg
        .list_ids
        .iter()
        .filter_map(|s| parse_list_spec(s))
        .collect();

    out
}

fn parse_list_spec(s: &str) -> Option<archiver::ListSpec> {
    let raw = s.trim();
    if raw.is_empty() {
        return None;
    }

    // Accept URLs like https://x.com/i/lists/<id>
    let mut val = raw.to_string();
    if let Some(rest) = raw.strip_prefix("https://x.com/i/lists/") {
        val = rest.to_string();
    } else if let Some(rest) = raw.strip_prefix("https://twitter.com/i/lists/") {
        val = rest.to_string();
    }

    let (id, slug) = if let Some((id, slug)) = val.split_once(':') {
        (id.trim().to_string(), slug.trim().to_string())
    } else {
        (val.trim().to_string(), val.trim().to_string())
    };

    if id.is_empty() {
        return None;
    }
    let slug = if slug.is_empty() { id.clone() } else { slug };

    Some(archiver::ListSpec { id, slug })
}
