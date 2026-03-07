use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-env-changed=TWIT_RANK_SKIP_FRONTEND_BUILD");
    print_frontend_rerun_hints();

    let build_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let pkg_version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "dev".to_string());
    let build_id = format!("{pkg_version}-{build_epoch}");
    println!("cargo:rustc-env=TWIT_RANK_BUILD_ID={build_id}");
    println!("cargo:rustc-env=TWIT_RANK_BUILD_EPOCH={build_epoch}");

    if should_skip_frontend_build() {
        println!("cargo:warning=Skipping frontend build due to TWIT_RANK_SKIP_FRONTEND_BUILD");
        return;
    }

    if !frontend_build_is_stale() {
        println!("cargo:warning=Reusing existing frontend/dist build");
        return;
    }

    ensure_npm_available();
    run_npm(&["ci"], &[]);
    let build_epoch_str = build_epoch.to_string();
    let frontend_env = [
        ("VITE_TWIT_RANK_BUILD_ID", build_id.as_str()),
        ("VITE_TWIT_RANK_BUILD_EPOCH", build_epoch_str.as_str()),
    ];
    run_npm(&["run", "build"], &frontend_env);

    let index_html = Path::new("frontend/dist/index.html");
    if !index_html.exists() {
        panic!(
            "frontend build completed but {} was not found",
            index_html.display()
        );
    }
}

fn should_skip_frontend_build() -> bool {
    match env::var("TWIT_RANK_SKIP_FRONTEND_BUILD") {
        Ok(raw) => {
            let v = raw.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        }
        Err(_) => false,
    }
}

fn ensure_npm_available() {
    let status = npm_status(&["--version"], None, &[])
        .unwrap_or_else(|e| panic!("failed to run npm --version: {e}. Install Node.js/npm first."));
    if !status.success() {
        panic!("npm --version failed; install Node.js/npm first.");
    }
}

fn frontend_build_is_stale() -> bool {
    let index_html = Path::new("frontend/dist/index.html");
    if !index_html.exists() {
        return true;
    }

    let dist_mtime = modified_time(index_html);
    if dist_mtime.is_none() {
        return true;
    }
    let dist_mtime = dist_mtime.unwrap();

    frontend_input_paths()
        .into_iter()
        .any(|path| modified_time(&path).is_none_or(|mtime| mtime > dist_mtime))
}

fn frontend_input_paths() -> Vec<std::path::PathBuf> {
    let mut paths = vec![
        "frontend/package.json".into(),
        "frontend/package-lock.json".into(),
        "frontend/vite.config.ts".into(),
        "frontend/tsconfig.json".into(),
        "frontend/tsconfig.app.json".into(),
        "frontend/tsconfig.node.json".into(),
        "frontend/index.html".into(),
    ];
    collect_tree_files(Path::new("frontend/src"), &mut paths);
    collect_tree_files(Path::new("frontend/public"), &mut paths);
    paths
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn run_npm(args: &[&str], extra_env: &[(&str, &str)]) {
    let status = npm_status(args, Some("frontend"), extra_env)
        .unwrap_or_else(|e| panic!("failed to run npm {:?}: {}", args, e));
    if !status.success() {
        panic!("npm {:?} failed in frontend/", args);
    }
}

fn npm_status(
    args: &[&str],
    current_dir: Option<&str>,
    extra_env: &[(&str, &str)],
) -> io::Result<std::process::ExitStatus> {
    let mut programs = vec!["npm"];
    if cfg!(windows) {
        programs.push("npm.cmd");
    }

    let mut last_not_found: Option<io::Error> = None;
    for program in programs {
        let mut cmd = Command::new(program);
        cmd.args(args);
        if let Some(dir) = current_dir {
            cmd.current_dir(dir);
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        match cmd.status() {
            Ok(status) => return Ok(status),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                last_not_found = Some(e);
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_not_found.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "npm executable was not found in PATH",
        )
    }))
}

fn print_frontend_rerun_hints() {
    for path in frontend_input_paths() {
        println!("cargo:rerun-if-changed={}", path.to_string_lossy());
    }
}

fn collect_tree_files(root: &Path, out: &mut Vec<std::path::PathBuf>) {
    if !root.exists() {
        return;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            let entries = fs::read_dir(&path).unwrap_or_else(|e| {
                panic!("failed to read frontend path {}: {}", path.display(), e)
            });
            for entry in entries {
                let entry = entry.unwrap_or_else(|e| panic!("failed to read directory entry: {e}"));
                stack.push(entry.path());
            }
        } else {
            out.push(path);
        }
    }
}
