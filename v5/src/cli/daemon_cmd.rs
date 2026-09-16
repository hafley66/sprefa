//! `dl daemon <verb>` — the daemon-control subcommand family, plus the shared
//! output helpers. The old `--daemon`/`--stop`/`--restart`/... flags dispatch
//! through the same `crate::daemon` calls (see `super::run`) and stay hidden for
//! back-compat; new docs point at these verbs.

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::Path;

use super::root;

const VERBS: &str = "verbs: status start [--foreground] stop restart install uninstall drop <root> [--purge] load load-once rows jobs await-settle url why invocations [--limit N] events [--kind K] [--root R] [--limit N] health [--top N] [--root PATH] [--no-dupes] gc [--root PATH] [--apply]";

/// Route `start`/`stop`/`restart` through the OS service manager (plan
/// section 3.5.2: `dl daemon start/stop/restart` become thin launchctl/
/// systemctl wrappers once installed) instead of the bespoke `spawn_detached`
/// fallback (plan section 3.4) — but ONLY for the real, unsandboxed default
/// home. The installed LaunchAgent/systemd unit always serves
/// `daemon_home()`'s default resolution (no per-invocation env baked into the
/// unit); a `dl daemon` call under a sandboxed `XDG_STATE_HOME` (every
/// `tests/it/daemon.rs` case) is asking about a DIFFERENT daemon than the one
/// launchd/systemd supervises, so it must stay on the fallback path
/// regardless of whether a LaunchAgent happens to be installed for the real
/// user running the tests.
fn use_supervision() -> bool {
    std::env::var_os("XDG_STATE_HOME").is_none() && crate::supervise::is_installed()
}

/// Dispatch `dl daemon <verb> [args]`. Returns the process exit code.
pub fn run_cmd(args: &[String]) -> Result<i32> {
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        print_help();
        return Ok(0);
    }
    let target = root::daemon_target()?;
    let root_opt = target.root();
    match args.first().map(String::as_str).unwrap_or("status") {
        "status" => {
            // Class-13 stale-binary rail: `daemon status` is one of the three
            // named surfaces (docs/failure-modes.md:300-327) — a status probe
            // against a stale singleton should say so, not just report green.
            let stale_check_root = root_opt
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok());
            if let Some(check_root) = &stale_check_root {
                if use_supervision() {
                    // Re-pointed at the launchd world (plan section 4c):
                    // compare against the SUPERVISED daemon's own reported
                    // build mtime (`build_id` = "version:exe_mtime_secs"),
                    // not the `dl daemon status` CLI process's own exe —
                    // under launchd those can differ.
                    let reference = crate::daemon::status(None)
                        .ok()
                        .flatten()
                        .and_then(|info| {
                            info.get("build_id")
                                .and_then(|v| v.as_str().map(String::from))
                        })
                        .and_then(|build_id| build_id.rsplit_once(':').map(|(_, s)| s.to_string()))
                        .and_then(|secs| secs.parse::<u64>().ok())
                        .map(|secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));
                    if let Some(mtime) = reference {
                        crate::stale_binary::warn_if_stale_with_reference(
                            check_root,
                            "supervised daemon",
                            mtime,
                        );
                    }
                } else {
                    crate::stale_binary::warn_if_stale(check_root);
                }
            }
            print_status()
        }
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
            if let (Some(root), Some(outside)) =
                (root_opt, program_outside_unset_root(&programs, root_opt))
            {
                // @eprintln-ok: final user-facing refusal at CLI top level
                eprintln!(
                    "dl daemon start: {outside} is outside the resolved root {} \
                     (no DL_DAEMON_ROOT set, so the root fell back to the nearest \
                     `.dl/` ancestor of the current directory). Refusing: run this \
                     from inside the target repo, or set DL_DAEMON_ROOT=<dir>.",
                    root.display()
                ); // @eprintln-ok: final user-facing error print at CLI top level
                return Ok(2);
            }
            if foreground {
                match &init_root {
                    Some(r) => {
                        let root = r.display();
                        tracing::debug!(root = %root, "[daemon] starting singleton (foreground); registering {root}");
                    }
                    None => {
                        let home = crate::daemon::daemon_home();
                        let home_display = home.display();
                        tracing::debug!(home = %home_display, "[daemon] starting the rootless singleton (config view, foreground) at {home_display}");
                    }
                }
                crate::daemon::run_daemon(&programs, db, init_root, true, tray)?;
                Ok(0)
            } else {
                if use_supervision() {
                    crate::daemon::start_singleton_supervised()?;
                } else {
                    crate::daemon::start_singleton()?;
                }
                match &init_root {
                    Some(r) => {
                        crate::daemon::add_root(r)?;
                        let root = r.display();
                        tracing::debug!(root = %root, "daemon started (detached); serving {root} — `dl daemon status`");
                    }
                    None => {
                        let home = crate::daemon::daemon_home();
                        let home_display = home.display();
                        tracing::debug!(home = %home_display, "the rootless singleton started (detached, config view) at {home_display} — `dl daemon status`");
                    }
                }
                Ok(0)
            }
        }
        "stop" => {
            if use_supervision() {
                crate::supervise::stop()?;
            } else {
                crate::daemon::stop()?;
            }
            Ok(0)
        }
        "restart" => {
            if use_supervision() {
                crate::daemon::restart_supervised()?;
            } else {
                crate::daemon::restart()?;
            }
            Ok(0)
        }
        "install" => {
            let home = crate::daemon::daemon_home();
            let path = crate::supervise::install(&home)?;
            println!(
                "installed {} — registered, not started (`dl daemon start` to run it)",
                path.display()
            );
            Ok(0)
        }
        "uninstall" => {
            crate::supervise::uninstall()?;
            println!("uninstalled the dl supervision unit");
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
            crate::daemon::start_singleton()?;
            if let Some(r) = root_opt {
                crate::daemon::add_root(r)?;
            }
            print_load_response(crate::daemon::load(root_opt, path, "watched")?)?;
            Ok(0)
        }
        "load-once" => {
            let path = arg(args, 1, "dl daemon load-once <file.dl>")?;
            crate::daemon::start_singleton()?;
            if let Some(r) = root_opt {
                crate::daemon::add_root(r)?;
            }
            print_load_response(crate::daemon::load(root_opt, path, "once")?)?;
            Ok(0)
        }
        "rows" => {
            let rel = arg(args, 1, "dl daemon rows <rel>")?;
            let (cols, rows) = crate::daemon::query_rel(root_opt, rel)?;
            print_rows(&cols, &rows);
            Ok(0)
        }
        "url" => {
            // Print the daemon's standard HTTP base URL, read from http.json.
            println!("{}", crate::daemon_http::base_url()?);
            Ok(0)
        }
        "jobs" => print_jobs(),
        "why" => {
            // Reads only `<home>/why.jsonl` — no socket, no lock — so it
            // answers while the daemon is wedged and after a kill/crash.
            print!("{}", crate::why::report(&crate::daemon::daemon_home()));
            // Second witness (plan section 3.1): raw `launchctl print` text
            // when a supervision unit is installed for the real home — launchd
            // records last exit status and spawn/throttle history a SIGKILLed
            // daemon could not journal itself. Format is explicitly not
            // API-stable, so it is printed verbatim, never parsed.
            if use_supervision() {
                if let Some(supervision_text) = crate::supervise::debug_print() {
                    println!("--- supervision (launchctl print, second witness) ---");
                    print!("{supervision_text}");
                }
            }
            Ok(0)
        }
        "health" => {
            // Storage-health report over the file trail alone (roots.json +
            // read-only db opens) — like `why`, it answers regardless of
            // daemon state and never contends for the write lock.
            super::health::run(&args[1..])
        }
        "gc" => {
            // `_strings` intern-dictionary sweep. Unlike `health`, this WRITES
            // when `--apply` is given, so it opens a real read-write
            // connection and is expected to contend with a live daemon —
            // meant to be run with the daemon stopped, same convention as the
            // standing VACUUM maintenance step.
            crate::daemon::gc::run(&args[1..])
        }
        "invocations" => {
            // Reads only `<home>/invocations.db` — no engine lock — so it
            // answers regardless of daemon state. An open row whose pid is
            // dead is direct kill evidence (same idiom as `why`, one level up:
            // process, not daemon tick).
            let limit: usize = flag_value(args, "--limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(50);
            print!("{}", crate::invlog::report_recent(limit));
            Ok(0)
        }
        "events" => {
            // Reads only `<home>/events.jsonl[.1]` — no socket, no lock — so it
            // answers regardless of daemon state, same contract as `why` and
            // `invocations` one level up. Unlike `why` (which samples activity
            // every 2s and renders a human string), this replays the ARGUMENTS
            // of each discrete IO event — which paths changed, which file was
            // written — see `eventlog.rs`'s module doc for why that
            // distinction is the whole point.
            let kind = flag_value(args, "--kind");
            let root_filter = flag_value(args, "--root");
            let limit: usize = flag_value(args, "--limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);
            let rows =
                crate::eventlog::read(&crate::daemon::daemon_home(), kind, root_filter, limit);
            for row in rows {
                let ts_ms = row["ts"].as_i64().unwrap_or(0);
                let kind = row["kind"].as_str().unwrap_or("");
                let root_base = Path::new(row["root"].as_str().unwrap_or(""))
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let data = row["data"].to_string();
                println!("{} {kind} {root_base} {data}", hhmmss_local(ts_ms));
            }
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
            eprintln!("dl daemon: unknown verb `{other}`\n{VERBS}"); // @eprintln-ok: final user-facing error print at CLI top level
            Ok(2)
        }
    }
}

fn print_help() {
    eprintln!("usage: dl daemon <verb> [options]"); // @eprintln-ok: usage/help text
    eprintln!("  status                         show daemon and root status"); // @eprintln-ok: usage/help text
    eprintln!("  start [PROGRAMS] [--foreground] start the singleton (or run it in this process)"); // @eprintln-ok: usage/help text
    eprintln!("  stop                           stop the singleton"); // @eprintln-ok: usage/help text
    eprintln!("  restart                        restart the singleton"); // @eprintln-ok: usage/help text
    eprintln!("  install                        register a launchd LaunchAgent / systemd user unit (does not start it)"); // @eprintln-ok: usage/help text
    eprintln!("  uninstall                      deregister the LaunchAgent/unit (stops it first)"); // @eprintln-ok: usage/help text
    eprintln!("  drop <ROOT> [--purge]          unregister a root, optionally purge its db"); // @eprintln-ok: usage/help text
    eprintln!("  load <FILE.dl>                 load a watched program"); // @eprintln-ok: usage/help text
    eprintln!("  load-once <FILE.dl>            load a program for one run"); // @eprintln-ok: usage/help text
    eprintln!("  rows <REL>                     print live relation rows"); // @eprintln-ok: usage/help text
    eprintln!("  jobs                           list the tick/sink-drain job queue (newest first)"); // @eprintln-ok: usage/help text
    eprintln!("  await-settle [--ms N]          wait for the root to become quiescent"); // @eprintln-ok: usage/help text
    eprintln!("  url                            print the daemon's HTTP base URL (from http.json)"); // @eprintln-ok: usage/help text
    eprintln!("  health [--top N] [--root PATH] [--no-dupes]  storage report: per-db weights, orphan roots, dupe rels"); // @eprintln-ok: usage/help text
    eprintln!("  gc [--root PATH] [--apply]     sweep orphaned `_strings` intern rows (dry run unless --apply)"); // @eprintln-ok: usage/help text
    eprintln!("  events [--kind K] [--root R] [--limit N]  replay the IO event trail (args of each discrete event, not samples)"); // @eprintln-ok: usage/help text
    eprintln!("options: --db PATH, --tray (start); --ms N (await-settle)"); // @eprintln-ok: usage/help text
}

/// `dl watch <target>`: serve a program reactively (the daemon watches +
/// hot-reloads it). Sugar over `daemon load`, but takes the same input forms as
/// a one-shot — a `.dl` file, a folder (every `*.dl` in it), or inline source.
/// Starts the daemon if it is down.
pub fn run_watch(args: &[String]) -> Result<i32> {
    if args.is_empty() {
        eprintln!("usage: dl watch <file.dl | folder/ | 'head <- body.'>"); // @eprintln-ok: usage/help text
        return Ok(2);
    }
    let target = root::daemon_target()?;
    let root_opt = target.root();
    let expanded = super::inputs::expand(args)?;
    if expanded.files.is_empty() {
        eprintln!("dl watch: nothing to watch"); // @eprintln-ok: final user-facing error print at CLI top level
        return Ok(2);
    }
    if use_supervision() {
        crate::daemon::start_singleton_supervised()?;
    } else {
        crate::daemon::start_singleton()?;
    }
    if let Some(r) = root_opt {
        crate::daemon::add_root(r)?;
    }
    for file in &expanded.files {
        let resp = crate::daemon::load(root_opt, file, "watched")?;
        if let Some(err) = resp.error {
            eprintln!("{}", err.message); // @eprintln-ok: command's output contract for failure at CLI top level
            return Ok(1);
        }
    }
    // @eprintln-ok: command's output contract for a human at a TTY
    eprintln!(
        "[watch] {} program(s) joined the daemon serving {} — hot-reloading on edit.\n\
         inspect: dl daemon rows <rel>    status: dl daemon status    stop: dl daemon stop",
        expanded.files.len(),
        root_opt
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "the config view".to_string()),
    ); // @eprintln-ok: command's output contract for a human at a TTY
    Ok(0)
}

/// Ping the singleton and print a status block: the process summary + every
/// registered root with its tick count. Exit 0 running, 1 not.
fn print_status() -> Result<i32> {
    // Supervision line: best-effort, informational only (plan section 3.1 —
    // `launchctl print`'s own format is explicitly not API-stable, so this
    // never gates the exit code; liveness stays on the socket ping below).
    if crate::supervise::is_installed() {
        println!("supervised   installed (com.sprefa.dl)");
    }
    match crate::daemon::status(None)? {
        None => {
            println!(
                "daemon: not running  (home {})",
                crate::daemon::daemon_home().display()
            );
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
                    let get = |k: &str| {
                        r.get(k)
                            .map(|v| match v {
                                Value::String(s) => s.clone(),
                                o => o.to_string(),
                            })
                            .unwrap_or_default()
                    };
                    println!(
                        "    - {}  (tick {}, {}, {})",
                        get("root"),
                        get("tick_count"),
                        if r.get("settled").and_then(|v| v.as_bool()).unwrap_or(false) {
                            "settled"
                        } else {
                            "active"
                        },
                        get("program")
                    );
                    let fx_failed = r
                        .get("effects_failed")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let fx_orphaned = r
                        .get("effects_orphaned")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if fx_failed != 0 || fx_orphaned != 0 {
                        println!(
                            "      effects: {} failed, {} orphaned",
                            fx_failed, fx_orphaned
                        );
                    }
                }
            }
            let activity = info.get("activity");
            let phase = activity
                .and_then(|a| a.get("phase"))
                .and_then(|v| v.as_str())
                .unwrap_or("idle");
            if phase == "idle" || phase.is_empty() {
                println!("  doing        idle");
            } else {
                let detail = activity
                    .and_then(|a| a.get("detail"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let elapsed = activity
                    .and_then(|a| a.get("elapsed_ms"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let tick = activity
                    .and_then(|a| a.get("tick"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let what = if detail.is_empty() {
                    phase.to_string()
                } else {
                    format!("{phase} {detail}")
                };
                println!(
                    "  doing        {what}   ({}.{:0>1}s, tick {tick})",
                    elapsed / 1000,
                    (elapsed % 1000) / 100
                );
            }
            Ok(0)
        }
    }
}

/// `dl daemon jobs`: print the tick/sink-drain queue as a table, newest first.
/// Exit 1 if the daemon is not running.
fn print_jobs() -> Result<i32> {
    if crate::daemon::status(None)?.is_none() {
        println!(
            "daemon: not running  (home {})",
            crate::daemon::daemon_home().display()
        );
        return Ok(1);
    }
    let jobs = crate::daemon::jobs()?;
    let cell = |job: &Value, key: &str| -> String {
        job.get(key)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    };
    println!("KEY\tKIND\tSTATE\tATTEMPTS\tENQUEUED_AT\tSTARTED_AT");
    for job in &jobs {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            cell(job, "key"),
            cell(job, "kind"),
            cell(job, "state"),
            cell(job, "attempts"),
            cell(job, "enqueued_at"),
            cell(job, "started_at")
        );
    }
    println!("({} jobs)", jobs.len());
    Ok(0)
}

/// Print a `--load` / `--load-once` RPC response: the JSON result on success,
/// the error message (exit 1) on failure. Shared by the flag and subcommand
/// paths.
pub fn print_load_response(resp: crate::rpc::Response) -> Result<()> {
    if let Some(err) = resp.error {
        eprintln!("{}", err.message); // @eprintln-ok: final user-facing error print at CLI top level
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

/// Render an epoch-milliseconds timestamp (as `eventlog::read` rows carry) as
/// `HH:MM:SS` — plain seconds-of-day integer arithmetic, no `chrono`/`time`
/// dependency (none exists in this project; `dl daemon events` output does not
/// warrant adding one). UTC, not the host's zone offset: getting a true local
/// offset without a time crate means an unsafe `libc::localtime_r` call for a
/// one-line clock stamp, which is not worth it here — see the task's own
/// "keep it simple" allowance.
fn hhmmss_local(epoch_ms: i64) -> String {
    let epoch_s = epoch_ms.div_euclid(1000);
    let secs_of_day = epoch_s.rem_euclid(86_400);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{hour:02}:{minute:02}:{second:02}")
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
        let canon_program = program_path
            .canonicalize()
            .unwrap_or_else(|_| program_path.to_path_buf());
        if canon_program.starts_with(root) {
            None
        } else {
            Some(program.clone())
        }
    })
}
