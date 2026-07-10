//! `dl daemon <verb>` — the daemon-control subcommand family, plus the shared
//! output helpers. The old `--daemon`/`--stop`/`--restart`/... flags dispatch
//! through the same `crate::daemon` calls (see `super::run`) and stay hidden for
//! back-compat; new docs point at these verbs.

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::Path;

use super::root;

const VERBS: &str = "verbs: status start stop restart load load-once rows await-settle";

/// Dispatch `dl daemon <verb> [args]`. Returns the process exit code.
pub fn run_cmd(args: &[String]) -> Result<i32> {
    let root = root::daemon_target()?;
    let target = Some(root.as_path());
    match args.first().map(String::as_str).unwrap_or("status") {
        "status" => print_status(&root, target),
        "start" => {
            // Foreground serving daemon (the debug path, and what auto-attach
            // spawns detached — see `spawn_detached`). Trailing positionals are
            // the program(s) to serve; empty = discover `<root>/.dl/*.dl`.
            // `--tray` adds the macOS icon; `--db <path>` persists derived tables.
            let tray = args.iter().any(|a| a == "--tray");
            let db = flag_value(args, "--db");
            let programs: Vec<String> = args[1..]
                .iter()
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .collect();
            if let Some(outside) = program_outside_unset_root(&programs, &root) {
                eprintln!(
                    "dl daemon start: {outside} is outside the resolved root {} \
                     (no DL_DAEMON_ROOT set, so the root fell back to the nearest \
                     `.dl/` ancestor of the current directory). Refusing to start: \
                     this shape has silently bound a daemon.sock under an unrelated \
                     repo before. Run this from inside the target repo, or set \
                     DL_DAEMON_ROOT=<dir> explicitly.",
                    root.display()
                );
                return Ok(2);
            }
            crate::daemon::run_daemon(&programs, db, Some(root), true, tray)?;
            Ok(0)
        }
        "stop" => {
            crate::daemon::stop(target)?;
            Ok(0)
        }
        "restart" => {
            crate::daemon::restart(&root)?;
            Ok(0)
        }
        "load" => {
            // "serve this file reactively" in one command: start the daemon if
            // it is down, then push the program as a watched (hot-reloaded) set.
            let path = arg(args, 1, "dl daemon load <file.dl>")?;
            crate::daemon::ensure_daemon(&root, None)?;
            print_load_response(crate::daemon::load(target, path, "watched")?)?;
            Ok(0)
        }
        "load-once" => {
            let path = arg(args, 1, "dl daemon load-once <file.dl>")?;
            crate::daemon::ensure_daemon(&root, None)?;
            print_load_response(crate::daemon::load(target, path, "once")?)?;
            Ok(0)
        }
        "rows" => {
            let rel = arg(args, 1, "dl daemon rows <rel>")?;
            let (cols, rows) = crate::daemon::query_rel(target, rel)?;
            print_rows(&cols, &rows);
            Ok(0)
        }
        "await-settle" => {
            let ms = flag_value(args, "--ms")
                .and_then(|s| s.parse().ok())
                .unwrap_or(30_000);
            let (settled, tick) = crate::daemon::await_quiescent(target, ms)?;
            println!("settled={settled} tick={tick}");
            Ok(if settled { 0 } else { 3 })
        }
        other => {
            eprintln!("dl daemon: unknown verb `{other}`\n{VERBS}");
            Ok(2)
        }
    }
}

/// `dl watch <target>`: serve a program reactively (the daemon watches +
/// hot-reloads it). Sugar over `daemon load`, but takes the same input forms as
/// a one-shot — a `.dl` file, a folder (every `*.dl` in it), or inline source.
/// Starts the daemon if it is down.
pub fn run_watch(args: &[String]) -> Result<i32> {
    if args.is_empty() {
        eprintln!("usage: dl watch <file.dl | folder/ | 'head <- body.'>");
        return Ok(2);
    }
    let root = root::daemon_target()?;
    let target = Some(root.as_path());
    let expanded = super::inputs::expand(args)?;
    if expanded.files.is_empty() {
        eprintln!("dl watch: nothing to watch");
        return Ok(2);
    }
    crate::daemon::ensure_daemon(&root, None)?;
    for file in &expanded.files {
        let resp = crate::daemon::load(target, file, "watched")?;
        if let Some(err) = resp.error {
            eprintln!("{}", err.message);
            return Ok(1);
        }
    }
    eprintln!(
        "[watch] {} program(s) joined the daemon at {} — hot-reloading on edit.\n\
         inspect: dl daemon rows <rel>    status: dl daemon status    stop: dl daemon stop",
        expanded.files.len(),
        root.display()
    );
    Ok(0)
}

/// Ping the daemon and print a status block. Exit 0 running, 1 not.
fn print_status(root: &Path, target: Option<&Path>) -> Result<i32> {
    match crate::daemon::status(target)? {
        None => {
            println!("daemon: not running  (root {})", root.display());
            Ok(1)
        }
        Some(info) => {
            let field = |key: &str| {
                info.get(key)
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            };
            println!("daemon: running  (root {})", root.display());
            println!("  build_id   {}", field("build_id"));
            println!("  tick_count {}", field("tick_count"));
            println!("  settled    {}", field("settled"));
            println!("  program    {}", field("program"));
            let activity = info.get("activity");
            let phase = activity.and_then(|a| a.get("phase")).and_then(|v| v.as_str()).unwrap_or("idle");
            if phase == "idle" || phase.is_empty() {
                println!("  doing      idle");
            } else {
                let detail = activity.and_then(|a| a.get("detail")).and_then(|v| v.as_str()).unwrap_or("");
                let elapsed = activity.and_then(|a| a.get("elapsed_ms")).and_then(|v| v.as_u64()).unwrap_or(0);
                let tick = activity.and_then(|a| a.get("tick")).and_then(|v| v.as_u64()).unwrap_or(0);
                let what = if detail.is_empty() {
                    phase.to_string()
                } else {
                    format!("{phase} {detail}")
                };
                println!("  doing      {what}   ({}.{:0>1}s, tick {tick})",
                    elapsed / 1000, (elapsed % 1000) / 100);
            }
            Ok(0)
        }
    }
}

/// Print a `--load` / `--load-once` RPC response: the JSON result on success,
/// the error message (exit 1) on failure. Shared by the flag and subcommand
/// paths.
pub fn print_load_response(resp: crate::rpc::Response) -> Result<()> {
    if let Some(err) = resp.error {
        eprintln!("{}", err.message);
        std::process::exit(1);
    }
    if let Some(result) = resp.result {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Ok(())
}

/// Print a relation dump (header row + tab-separated rows + count) for
/// `dl daemon rows`.
pub fn print_rows(cols: &[String], rows: &[Vec<String>]) {
    if !cols.is_empty() {
        println!("{}", cols.join("\t"));
    }
    for row in rows {
        println!("{}", row.join("\t"));
    }
    println!("({} rows)", rows.len());
}

fn arg<'a>(args: &'a [String], idx: usize, usage: &str) -> Result<&'a str> {
    args.get(idx)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("usage: {usage}"))
}

/// Value following `name` in `args` (e.g. `--ms 5000`).
fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// The first `program` path that lies outside `root`, when `root` was NOT
/// pinned by an explicit `DL_DAEMON_ROOT` (an env-set root is trusted as-is —
/// the caller named it on purpose). `None` when `DL_DAEMON_ROOT` is set, no
/// program is absolute, or every absolute program canonicalizes under `root`.
///
/// Guards a real footgun: `dl daemon start <program>` derives its serving root
/// from `DL_DAEMON_ROOT`, else the nearest `.dl/` ancestor of the CURRENT
/// DIRECTORY — never from the program path. Running it from inside an
/// unrelated already-`.dl`'d repo (a habit, a copy-pasted command, a test
/// helper that forgot to set the env) silently binds THAT repo's
/// `daemon.sock` to whatever program was named, with no warning. Seen live:
/// `dl daemon start /tmp/disc2/p.dl` run from a real checkout bound the real
/// checkout's socket for a day.
fn program_outside_unset_root(programs: &[String], root: &Path) -> Option<String> {
    if std::env::var_os("DL_DAEMON_ROOT").is_some() {
        return None;
    }
    programs.iter().find_map(|program| {
        let program_path = Path::new(program);
        if !program_path.is_absolute() {
            return None;
        }
        let canon_program = program_path.canonicalize().unwrap_or_else(|_| program_path.to_path_buf());
        if canon_program.starts_with(root) { None } else { Some(program.clone()) }
    })
}
