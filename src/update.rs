//! `dl update` — self-update to the latest published release, turnkey.
//!
//!   dl update           download + install the latest prebuilt release
//!   dl update --check    report the current vs latest version, install nothing
//!
//! Reuses the same cargo-dist release installer the one-line `install.sh` runs,
//! so `dl update` overwrites the binary in `$CARGO_HOME/bin` with the newest
//! release. No bundled updater dependency; the release pipeline is the source of
//! truth. (cargo-dist's `install-updater` stays off; this is the equivalent
//! surface without pinning axoupdater into the build.)

use anyhow::{bail, Result};
use std::process::Command;

const REPO: &str = "hafley66/sprefa";
const INSTALLER_URL: &str =
    "https://github.com/hafley66/sprefa/releases/latest/download/sprefa-dl-installer.sh";

const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, PartialEq)]
enum Action {
    Install,
    Check,
    Help,
    Unknown(String),
}

fn parse(args: &[String]) -> Action {
    for a in args {
        match a.as_str() {
            "--check" | "-n" => return Action::Check,
            "-h" | "--help" => return Action::Help,
            other => return Action::Unknown(other.to_string()),
        }
    }
    Action::Install
}

pub fn run(args: &[String]) -> Result<i32> {
    match parse(args) {
        Action::Help => {
            print_help();
            Ok(0)
        }
        Action::Unknown(u) => {
            eprintln!("dl update: unknown arg {u}");
            print_help();
            Ok(2)
        }
        Action::Check => {
            println!("[dl update] current: v{CURRENT}");
            match latest_tag() {
                Some(tag) => {
                    let latest = tag.trim_start_matches('v');
                    if latest == CURRENT {
                        println!("[dl update] latest:  {tag} (up to date)");
                    } else {
                        println!("[dl update] latest:  {tag} — run `dl update` to upgrade");
                    }
                    Ok(0)
                }
                None => {
                    eprintln!("[dl update] could not reach the release API ({REPO}); check your network.");
                    Ok(1)
                }
            }
        }
        Action::Install => {
            if which("curl").is_none() {
                bail!("`curl` is required for `dl update` (or use: cargo install --git https://github.com/{REPO} sprefa-dl --bin dl)");
            }
            println!("[dl update] current: v{CURRENT}");
            println!("[dl update] fetching the latest release installer…");
            // Same artifact the one-line install.sh runs; overwrites the binary
            // in $CARGO_HOME/bin. Piped through sh exactly as documented.
            let status = Command::new("sh")
                .arg("-c")
                .arg(format!("curl -LsSf {INSTALLER_URL} | sh"))
                .status()?;
            if !status.success() {
                bail!("release installer exited {status}");
            }
            println!("[dl update] done. Run `dl update --check` to confirm, or `dl --help`.");
            // Refresh the on-disk artifacts that embed version-pinned content
            // (the skill + the VSCode extension if installed). Project-level
            // wiring (.claude hooks, .githooks, AGENTS section) is per-repo;
            // re-run `dl setup --project` in any repo you bootstrapped.
            crate::setup::refresh_after_update();
            println!("[dl update] project repos: re-run `dl setup --project` in any bootstrapped repo to refresh its AGENTS/CLAUDE dl section + hooks.");
            Ok(0)
        }
    }
}

fn print_help() {
    eprintln!("usage: dl update [--check]");
    eprintln!("  (no args)   download + install the latest prebuilt release");
    eprintln!("  --check     report current vs latest version; install nothing");
}

/// The latest release tag from the GitHub API (`tag_name`), or None on any
/// failure (offline, rate-limited, malformed). Best-effort — `--check` degrades
/// to a clear message rather than an error.
fn latest_tag() -> Option<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let out = Command::new("curl")
        .args(["-LsSf", "-H", "User-Agent: dl-update", &url])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("tag_name")?.as_str().map(|s| s.to_string())
}

/// First executable named `bin` on PATH (best-effort `which`).
fn which(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(bin);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_to_install() {
        assert_eq!(parse(&[]), Action::Install);
    }

    #[test]
    fn parse_check_and_help() {
        assert_eq!(parse(&["--check".into()]), Action::Check);
        assert_eq!(parse(&["-n".into()]), Action::Check);
        assert_eq!(parse(&["-h".into()]), Action::Help);
        assert_eq!(parse(&["--help".into()]), Action::Help);
    }

    #[test]
    fn parse_unknown_is_reported() {
        assert_eq!(parse(&["--wat".into()]), Action::Unknown("--wat".into()));
    }

    #[test]
    fn current_version_is_the_crate_version() {
        // Guards against the const drifting from the package version.
        assert_eq!(CURRENT, env!("CARGO_PKG_VERSION"));
    }
}
