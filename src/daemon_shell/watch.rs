//! Per-root file/git watcher, as a tokio task. The notify crate stays
//! callback/thread-based (its FSEvents/inotify thread invokes our closure); that
//! closure forwards each event into a tokio channel this task drains. The
//! gate/coalesce/dispatch ALGORITHM is unchanged from the old `watcher_loop`
//! thread — only the engine ops (`tick_full` / reload / git / config) run via
//! `spawn_blocking` so a slow tick never stalls a shell worker, and the hot
//! source-edit path enqueues a coalescing `tick:{root}` job for a dispatcher.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::ShellCtx;
use crate::config;
use crate::daemon::{enforce_fresh_binary, enforce_mem_limit, lock, served_repos, ExeStamp, ServedRoot};
use crate::watchgate::WatchGate;

/// One served root's watcher task. Retirement: `drop_root` sets `d.stopped`; the
/// shell cancellation token stops it on daemon shutdown. Both are observed
/// between events.
pub(crate) async fn watch_task(d: Arc<ServedRoot>, ctx: ShellCtx, launch_exe_stamp: Option<ExeStamp>) {
    use notify::{RecursiveMode, Watcher};
    let is_config = d.key.is_none();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(move |res| { let _ = tx.send(res); }) {
        Ok(w) => w,
        Err(e) => { tracing::error!("[{}] watcher init failed: {e}", d.root_label()); return; }
    };
    // Snapshot this engine's corpus once (a `spawn_blocking` read — it takes the
    // engine lock). Used for the initial watch set; expansion re-reads it.
    let corpus = snapshot_repos_blocking(&d).await;
    // The config view roots at the XDG home (which holds the per-root dbs); do NOT
    // watch it recursively. A registered root watches its own tree.
    let mut gate = if is_config {
        WatchGate::new(&[])
    } else {
        if let Err(e) = watcher.watch(&d.root, RecursiveMode::Recursive) {
            tracing::error!("[{}] watch root failed: {e}", d.root_label());
            return;
        }
        WatchGate::new(std::slice::from_ref(&d.root))
    };
    let mut watch_count: usize = if is_config { 0 } else { 1 };
    // Watch every folder in THIS ENGINE'S corpus so corpus edits react. A
    // hermetic served root's snapshot is empty (only its own `--root`, watched
    // above); the config view's snapshot is the config repos.
    for rc in &corpus {
        if rc.root.exists() && rc.root != d.root
            && watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
            watch_count += 1;
            gate.add_root(&rc.root);
        }
    }
    let cfg_path = config::SprfConfig::config_path()
        .and_then(|p| p.canonicalize().ok().or(Some(p)));
    if is_config {
        if let Some(cp) = &cfg_path {
            if let Some(dir) = cp.parent() {
                if dir.exists() && watcher.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                    watch_count += 1;
                }
            }
        }
        // Watch home/.dl for load'd programs (non-recursive; not the dbs).
        let dl = d.root.join(".dl");
        if dl.is_dir() { let _ = watcher.watch(&dl, RecursiveMode::NonRecursive); }
    }
    // Narrow-watch the `.git` dir of the root + this engine's corpus repos.
    if !is_config { watch_count += watch_git_narrow(&mut watcher, &mut gate, &d.root); }
    for rc in &corpus {
        if rc.root.exists() { watch_count += watch_git_narrow(&mut watcher, &mut gate, &rc.root); }
    }

    tracing::info!("[{}] watcher ready — {watch_count} watch(es)", d.root_label());
    let watcher_start = std::time::Instant::now();
    const STARTUP_GRACE: Duration = Duration::from_secs(1);
    let mut watched: std::collections::HashSet<PathBuf> = std::collections::HashSet::from_iter(
        [d.root.clone()].into_iter()
            .chain(corpus.iter().filter(|r| r.root.exists()).map(|r| r.root.clone())));
    loop {
        // Observe the drop flag + cancellation between events so `drop_root` and
        // daemon shutdown can retire us. The 1s timeout doubles as the guard
        // heartbeat (mem ceiling + fresh-binary check).
        let first = tokio::select! {
            _ = ctx.cancel.cancelled() => return,
            r = tokio::time::timeout(Duration::from_secs(1), rx.recv()) => r,
        };
        let first = match first {
            Ok(Some(ev)) => ev,
            Ok(None) => return, // watcher/channel closed
            Err(_) => {
                if d.stopped.load(Ordering::Relaxed) { return; }
                enforce_mem_limit(&d.root_label());
                enforce_fresh_binary(launch_exe_stamp);
                continue;
            }
        };
        if d.stopped.load(Ordering::Relaxed) { return; }
        if watcher_start.elapsed() < STARTUP_GRACE {
            while rx.try_recv().is_ok() {}
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        const QUIET: Duration = Duration::from_millis(120);
        const MAX_WINDOW: Duration = Duration::from_millis(600);
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut rescan = false;
        match first {
            Ok(ev) => {
                if ev.need_rescan() { rescan = true; } else { paths.extend(ev.paths); }
            }
            Err(e) => {
                tracing::warn!("[{}] watch error, forcing full tick: {e}", d.root_label());
                rescan = true;
            }
        }
        let window_start = Instant::now();
        loop {
            match tokio::time::timeout(QUIET, rx.recv()).await {
                Ok(Some(Ok(ev))) => {
                    if ev.need_rescan() { rescan = true; } else { paths.extend(ev.paths); }
                    if window_start.elapsed() > MAX_WINDOW { break; }
                }
                Ok(Some(Err(e))) => {
                    tracing::warn!("[{}] watch error, forcing full tick: {e}", d.root_label());
                    rescan = true;
                    if window_start.elapsed() > MAX_WINDOW { break; }
                }
                Ok(None) => return,
                Err(_) => break, // QUIET window elapsed
            }
        }
        if rescan {
            let tick_num = d.tick_count.load(Ordering::Relaxed);
            match run_tick_full(&d).await {
                Ok(()) => tracing::info!("[{}] tick #{tick_num} (rescan recovery) ok", d.root_label()),
                Err(e) => tracing::error!("[{}] tick #{tick_num} (rescan recovery) error: {e}", d.root_label()),
            }
            d.touch();
            enforce_mem_limit(&d.root_label());
            enforce_fresh_binary(launch_exe_stamp);
            continue;
        }
        let touches_git = gate.touches_git(&paths);
        let paths: Vec<PathBuf> = gate.filter(paths);
        if paths.is_empty() && !touches_git {
            continue;
        }
        d.touch();
        let touches_cfg = cfg_path.as_ref().is_some_and(|c|
            paths.iter().any(|p| p.canonicalize().ok().as_deref() == Some(c) || p == c));
        let mut paths = paths;
        let touches_program = d.program_in_paths(&paths);
        if touches_program {
            let names: Vec<String> = paths.iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
                .collect();
            // Singleton daemon: program edits ALWAYS hot-reload (never exit for
            // respawn — one exit would kill every served root).
            if d.discovery_mode {
                match run_reload_discovery(&d).await {
                    Ok(false) => {
                        tracing::info!("[{}] program edit ({}) — reloading (discovery)", d.root_label(), names.join(", "));
                        if let Err(e) = run_reload_program(&d).await {
                            tracing::warn!("[{}] reload failed, keeping old: {e}", d.root_label());
                        }
                    }
                    Ok(true) => {}
                    Err(e) => tracing::warn!("[{}] discovery reload: {e}", d.root_label()),
                }
            } else {
                tracing::info!("[{}] program edit ({}) — reloading", d.root_label(), names.join(", "));
                if let Err(e) = run_reload_program(&d).await {
                    tracing::warn!("[{}] reload failed, keeping old: {e}", d.root_label());
                }
            }
            continue;
        }
        if d.discovery_mode {
            let has_dl = paths.iter().any(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("dl")
                    && p.strip_prefix(&d.root)
                        .map(|r| r.starts_with(".dl"))
                        .unwrap_or(false)
            });
            if has_dl {
                tracing::info!("[{}] .dl discovery change — re-merging program", d.root_label());
                if let Err(e) = run_reload_discovery(&d).await {
                    tracing::warn!("[{}] discovery reload: {e}", d.root_label());
                }
                continue;
            }
        }
        if touches_git {
            let (n, changed) = run_on_git_event(&d).await;
            if n > 0 || !changed.is_empty() {
                tracing::info!("[{}] git change — {n} ref(s) advanced, {} worktree file(s)", d.root_label(), changed.len());
            }
            if changed.is_empty() { continue; }
            paths = changed;
        }
        let tick_label;
        let result = if touches_cfg {
            tick_label = "config change";
            run_config_change_tick(&d).await
        } else if paths.is_empty() {
            tick_label = "empty event";
            run_tick_full(&d).await
        } else {
            // J1: the hot source-edit path does not tick inline — it enqueues a
            // coalescing `tick:{root}` job that a dispatcher task drains (rapid
            // saves union into one job's paths).
            tick_label = "source change (queued)";
            run_enqueue_tick(&d, &paths).await
        };
        let n_paths = paths.len();
        let tick_num = d.tick_count.load(Ordering::Relaxed);
        let tick_ok = result.is_ok();
        match result {
            Ok(()) => tracing::info!("[{}] tick #{tick_num} ({tick_label}, {n_paths} paths) ok", d.root_label()),
            Err(e) => tracing::error!("[{}] tick #{tick_num} ({tick_label}, {n_paths} paths) error: {e}", d.root_label()),
        }
        // A tick is where the image grows (extract, closure, spine writes), so
        // check the ceiling here too, not only on the idle heartbeat.
        enforce_mem_limit(&d.root_label());
        enforce_fresh_binary(launch_exe_stamp);
        if tick_ok {
            let before = watch_count;
            for rc in snapshot_repos_blocking(&d).await {
                if rc.root.exists() && watched.insert(rc.root.clone())
                    && watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
                    watch_count += 1;
                    gate.add_root(&rc.root);
                    watch_count += watch_git_narrow(&mut watcher, &mut gate, &rc.root);
                    tracing::info!("[{}] watching (pulled) {} ({})", d.root_label(), rc.slug, rc.root.display());
                }
            }
            if watch_count != before {
                tracing::info!("[{}] watch count now {watch_count} (+{})", d.root_label(), watch_count - before);
            }
        }
    }
}

/// Narrow-watch a repo's `.git` dir. Returns the number of watch registrations
/// installed. (Runs `git rev-parse` synchronously; rare — only at watcher
/// startup and when a new corpus repo is pulled in — so it stays inline.)
fn watch_git_narrow(watcher: &mut notify::RecommendedWatcher, gate: &mut WatchGate, root: &Path) -> usize {
    use notify::{RecursiveMode, Watcher};
    let out = match std::process::Command::new("git").arg("-C").arg(root)
        .args(["rev-parse", "--git-dir"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return 0,
    };
    let gd = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let gdp = if Path::new(&gd).is_absolute() { PathBuf::from(&gd) } else { root.join(&gd) };
    if !gdp.exists() { return 0; }
    gate.add_git_dir(&gdp);
    let mut added = 0;
    for (path, recursive) in gate.git_watch_targets(&gdp) {
        if !path.exists() { continue; }
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        if watcher.watch(&path, mode).is_ok() { added += 1; }
    }
    added
}

// ---------- engine-op wrappers (all on the blocking pool) ----------
//
// Each hop into the engine (`tick_full`, reload, git observe, config re-repo,
// enqueue) is `spawn_blocking`: the watcher task itself is async, but the engine
// stays strictly sync. A `JoinError` (a tick that panicked) is mapped to a
// tick error rather than taking down the watcher task.

async fn snapshot_repos_blocking(d: &Arc<ServedRoot>) -> Vec<config::RepoConfig> {
    let d = d.clone();
    tokio::task::spawn_blocking(move || lock(&d.eng).snapshot_repos()).await.unwrap_or_default()
}

async fn run_tick_full(d: &Arc<ServedRoot>) -> Result<()> {
    let d = d.clone();
    tokio::task::spawn_blocking(move || d.tick_full(true)).await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("tick task panicked")))
}

async fn run_reload_program(d: &Arc<ServedRoot>) -> Result<()> {
    let d = d.clone();
    tokio::task::spawn_blocking(move || d.reload_program()).await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("reload task panicked")))
}

async fn run_reload_discovery(d: &Arc<ServedRoot>) -> Result<bool> {
    let d = d.clone();
    tokio::task::spawn_blocking(move || d.reload_discovery()).await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("discovery reload task panicked")))
}

async fn run_on_git_event(d: &Arc<ServedRoot>) -> (usize, Vec<PathBuf>) {
    let d = d.clone();
    tokio::task::spawn_blocking(move || d.on_git_event()).await.unwrap_or((0, Vec::new()))
}

async fn run_config_change_tick(d: &Arc<ServedRoot>) -> Result<()> {
    let d = d.clone();
    tokio::task::spawn_blocking(move || {
        {
            let mut eng = lock(&d.eng);
            eng.set_repos(served_repos(d.key.is_none()));
        }
        if let Err(e) = lock(&d.eng).save_repos_meta() {
            tracing::warn!("[{}] save_repos_meta: {e}", d.root_label());
        }
        d.tick_full(true)
    })
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("config-change tick task panicked")))
}

async fn run_enqueue_tick(d: &Arc<ServedRoot>, paths: &[PathBuf]) -> Result<()> {
    let d = d.clone();
    let paths = paths.to_vec();
    tokio::task::spawn_blocking(move || {
        d.enqueue_job(crate::jobq::JobRow::tick(&d.job_root_id(), &paths))
    })
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("enqueue task panicked")))
}
