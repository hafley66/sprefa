//! `dl daemon <verb>` — the daemon-control subcommand family, plus the shared
//! output helpers. The old `--daemon`/`--stop`/`--restart`/... flags dispatch
//! through the same `crate::daemon` calls (see `super::run`) and stay hidden for
//! back-compat; new docs point at these verbs.

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::Path;

use super::root;

const VERBS: &str = "verbs: status start [--foreground] stop restart drop <root> [--purge] load load-once rows await-settle";

/// Dispatch `dl daemon <verb> [args]`. Returns the process exit code.
pub fn run_cmd(args: &[String]) -> Result<i32> {
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        print_help();
        return Ok(0);
    }
    let target = root::daemon_target()?;
    let root_opt = target.root();
    match args.first().map(String::as_str).unwrap_or("status") {
        "status" => print_status(),
        "serve" => {
            // Internal: the detached background singleton (idle timer ON). Runs in
            // THIS process, no cwd root — replays roots.json + the config view.
            // `spawn_detached` execs this; users say `dl daemon start`.
            let db = flag_value(args, "--db");
            crate::daemon::run_daemon(&[], db, None, false, false)?;
            Ok(0)
        }
        "start" => {
            // Detaches a background singleton by default; `--foreground` runs it
            // in this process (the debug path). `--tray` forces foreground (it
            // owns the macOS main thread). Trailing positionals are the initial
            // root's program(s); empty = discover `<root>/.dl/*.dl`.
            let tray = args.iter().any(|a| a == "--tray");
            let foreground = args.iter().any(|a| a == "--foreground") || tray;
            let db = flag_value(args, "--db");
            let programs: Vec<String> = args[1..]
                .iter()
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .collect();
            let init_root = root_opt.map(|p| p.to_path_buf());
            if let (Some(root), Some(outside)) = (root_opt, program_outside_unset_root(&programs, root_opt)) {
                eprintln!(
                    "dl daemon start: {outside} is outside the resolved root {} \
                     (no DL_DAEMON_ROOT set, so the root fell back to the nearest \
                     `.dl/` ancestor of the current directory). Refusing: run this \
                     from inside the target repo, or set DL_DAEMON_ROOT=<dir>.",
                    root.display()
                );
                return Ok(2);
            }
            if foreground {
                match &init_root {
                    Some(r) => eprintln!("[daemon] starting singleton (foreground); registering {}", r.display()),
                    None => eprintln!("[daemon] starting the rootless singleton (config view, foreground) at {}",
                        crate::daemon::daemon_home().display()),
                }
                crate::daemon::run_daemon(&programs, db, init_root, true, tray)?;
                Ok(0)
            } else {
                crate::daemon::ensure_singleton()?;
                match &init_root {
                    Some(r) => {
                        crate::daemon::add_root(r)?;
                        eprintln!("daemon started (detached); serving {} — `dl daemon status`", r.display());
                    }
                    None => eprintln!("the rootless singleton started (detached, config view) at {} — `dl daemon status`",
                        crate::daemon::daemon_home().display()),
                }
                Ok(0)
            }
        }
        "stop" => {
            crate::daemon::stop()?;
            Ok(0)
        }
        "restart" => {
            crate::daemon::restart()?;
            Ok(0)
        }
        "drop" => {
            let path = arg(args, 1, "dl daemon drop <root> [--purge]")?;
            let purge = args.iter().any(|a| a == "--purge");
            crate::daemon::drop_root(Path::new(path), purge)?;
            println!("dropped {path}{}", if purge { " (db purged)" } else { "" });
            Ok(0)
        }
        "load" => {
            // "serve this file reactively" in one command: start the singleton if
            // down, register the cwd root, then push the program as a watched set.
            let path = arg(args, 1, "dl daemon load <file.dl>")?;
            crate::daemon::ensure_singleton()?;
            if let Some(r) = root_opt { crate::daemon::add_root(r)?; }
            print_load_response(crate::daemon::load(root_opt, path, "watched")?)?;
            Ok(0)
        }
        "load-once" => {
            let path = arg(args, 1, "dl daemon load-once <file.dl>")?;
            crate::daemon::ensure_singleton()?;
            if let Some(r) = root_opt { crate::daemon::add_root(r)?; }
            print_load_response(crate::daemon::load(root_opt, path, "once")?)?;
            Ok(0)
        }
        "rows" => {
            let rel = arg(args, 1, "dl daemon rows <rel>")?;
            let (cols, rows) = crate::daemon::query_rel(root_opt, rel)?;
            print_rows(&cols, &rows);
            Ok(0)
        }
        "await-settle" => {
            let ms = flag_value(args, "--ms")
                .and_then(|s| s.parse().ok())
                .unwrap_or(30_000);
            let (settled, tick) = crate::daemon::await_quiescent(root_opt, ms)?;
            println!("settled={settled} tick={tick}");
            Ok(if settled { 0 } else { 3 })
        }
        other => {
            eprintln!("dl daemon: unknown verb `{other}`\n{VERBS}");
            Ok(2)
        }
    }
}

fn print_help() {
    eprintln!("usage: dl daemon <verb> [options]");
    eprintln!("  status                         show daemon and root status");
    eprintln!("  start [PROGRAMS] [--foreground] start the singleton (or run it in this process)");
    eprintln!("  stop                           stop the singleton");
    eprintln!("  restart                        restart the singleton");
    eprintln!("  drop <ROOT> [--purge]          unregister a root, optionally purge its db");
    eprintln!("  load <FILE.dl>                 load a watched program");
    eprintln!("  load-once <FILE.dl>            load a program for one run");
    eprintln!("  rows <REL>                     print live relation rows");
    eprintln!("  await-settle [--ms N]          wait for the root to become quiescent");
    eprintln!("options: --db PATH, --tray (start); --ms N (await-settle)");
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
    let target = root::daemon_target()?;
    let root_opt = target.root();
    let expanded = super::inputs::expand(args)?;
    if expanded.files.is_empty() {
        eprintln!("dl watch: nothing to watch");
        return Ok(2);
    }
    crate::daemon::ensure_singleton()?;
    if let Some(r) = root_opt { crate::daemon::add_root(r)?; }
    for file in &expanded.files {
        let resp = crate::daemon::load(root_opt, file, "watched")?;
        if let Some(err) = resp.error {
            eprintln!("{}", err.message);
            return Ok(1);
        }
    }
    eprintln!(
        "[watch] {} program(s) joined the daemon serving {} — hot-reloading on edit.\n\
         inspect: dl daemon rows <rel>    status: dl daemon status    stop: dl daemon stop",
        expanded.files.len(),
        root_opt.map(|p| p.display().to_string()).unwrap_or_else(|| "the config view".to_string()),
    );
    Ok(0)
}

/// Ping the singleton and print a status block: the process summary + every
/// registered root with its tick count. Exit 0 running, 1 not.
fn print_status() -> Result<i32> {
    match crate::daemon::status(None)? {
        None => {
            println!("daemon: not running  (home {})", crate::daemon::daemon_home().display());
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
            println!("daemon: running  (home {})", field("home"));
            println!("  build_id     {}", field("build_id"));
            println!("  config_tick  {}", field("config_tick_count"));
            println!("  roots        {}", field("root_count"));
            if let Some(roots) = info.get("roots").and_then(|v| v.as_array()) {
                for r in roots {
                    let get = |k: &str| r.get(k).map(|v| match v {
                        Value::String(s) => s.clone(), o => o.to_string(),
                    }).unwrap_or_default();
                    println!("    - {}  (tick {}, {}, {})",
                        get("root"), get("tick_count"),
                        if r.get("settled").and_then(|v| v.as_bool()).unwrap_or(false) { "settled" } else { "active" },
                        get("program"));
                }
            }
            let activity = info.get("activity");
            let phase = activity.and_then(|a| a.get("phase")).and_then(|v| v.as_str()).unwrap_or("idle");
            if phase == "idle" || phase.is_empty() {
                println!("  doing        idle");
            } else {
                let detail = activity.and_then(|a| a.get("detail")).and_then(|v| v.as_str()).unwrap_or("");
                let elapsed = activity.and_then(|a| a.get("elapsed_ms")).and_then(|v| v.as_u64()).unwrap_or(0);
                let tick = activity.and_then(|a| a.get("tick")).and_then(|v| v.as_u64()).unwrap_or(0);
                let what = if detail.is_empty() { phase.to_string() } else { format!("{phase} {detail}") };
                println!("  doing        {what}   ({}.{:0>1}s, tick {tick})",
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
fn program_outside_unset_root(programs: &[String], root: Option<&Path>) -> Option<String> {
    if std::env::var_os("DL_DAEMON_ROOT").is_some() {
        return None;
    }
    let root = root?;
    programs.iter().find_map(|program| {
        let program_path = Path::new(program);
        if !program_path.is_absolute() {
            return None;
        }
        let canon_program = program_path.canonicalize().unwrap_or_else(|_| program_path.to_path_buf());
        if canon_program.starts_with(root) { None } else { Some(program.clone()) }
    })
}
