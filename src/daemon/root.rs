//! One served root: `ServedRoot` (state, engine locking, per-root db) plus
//! the `RootRecord` roots.json persistence (relocated from `daemon.rs`;
//! decomposition plan step 6).
use super::*;

// ---------- one served root ----------

/// One registered `.dl`-owning root served by the singleton, or the config-view
/// engine (`key == None`). Holds that root's warm `Engine` + parsed `Program` +
/// per-root watch filter. Byte-identical to the old per-root daemon's behavior;
/// only WHERE the db lives (a constant home position) changed.
pub struct ServedRoot {
    /// Engine/content/watch base. For the config view this is the XDG home (a
    /// benign "self" that scans nothing); the view comes from config repos.
    pub root: PathBuf,
    /// Registry key (blake3-16hex of the canonical root). `None` = config view.
    pub key: Option<String>,
    pub program_display: String,
    pub(crate) shared: Shared,
    /// Canonicalized absolute program-file paths the engine parsed.
    pub program_files: Mutex<Vec<PathBuf>>,
    /// True when this root loaded via `<root>/.dl/*.dl` discovery (vs an explicit
    /// program set): new/removed `.dl` files re-merge.
    pub discovery_mode: bool,
    pub prog: Mutex<Program>,
    pub eng: Mutex<Engine>,
    pub last_activity: Mutex<Instant>,
    pub tick_count: AtomicU64,
    /// Whether the last FULL tick left the program quiescent (the poll loop drives
    /// toward this; `await_quiescent` blocks on it).
    pub settled: AtomicBool,
    /// Cold-start staging in flight: the last full tick deferred the extract
    /// fan-out onto the queue and some `_cold_node` is still pending. Surfaced on
    /// the status RPC (`cold_start_pending`) so a client reads "warming", and
    /// `poll_idle` returns not-idle while true (await-settle blocks until warm).
    pub cold_pending: AtomicBool,
    /// The paths touched by the most recent tick (absolute). Empty after a full
    /// tick.
    pub last_changed_paths: Mutex<Vec<PathBuf>>,
    /// Set by `drop_root`; the watcher thread observes it and exits, dropping its
    /// `Arc<ServedRoot>` so the engine closes.
    pub stopped: Arc<AtomicBool>,
    /// `tick_count`'s value as of the end of the last FULL tick (`tick_full`).
    /// Only a full tick runs `rebuild_async` (queues fresh `@async`/`@stream`
    /// requests over the converged derived state); the watcher's incremental
    /// `tick_paths` never does. So `tick_count != last_full_tick_count` means
    /// a path-tick landed source motion since we last gave `rebuild_async` a
    /// chance to see it — `poll_idle`'s cheap "source changed" half. See
    /// `poll_idle` for the full gate (CPU-hog fix Part 1).
    last_full_tick_count: AtomicU64,
    /// The served root's on-disk db file (the writer engine's db). The lock-free
    /// read path (`crate::daemon_read`) opens READ-ONLY connections on it. `None`
    /// only for a hypothetical in-memory served root (none exist today), which
    /// sends every read to the engine-lock fallback.
    db_path: Option<PathBuf>,
    /// Shape snapshot for the lock-free read path, refreshed whenever the
    /// program (re)loads (`refresh_read_view`, called from `tick_full`). A read
    /// RPC clones this `Arc` under a short read lock and answers `query` /
    /// `query_rel` / `query_sql` / `schema` from committed SQLite state WITHOUT
    /// taking `lock_eng`, so read latency is independent of tick duration.
    read_view: RwLock<Arc<crate::daemon_read::ReadView>>,
}

impl ServedRoot {
    pub(crate) fn touch(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    /// Enqueue a tick/drain job for this root and ring the tokio doorbell. The
    /// shell's watcher/poll tasks call this (through `spawn_blocking`) instead of
    /// reaching the private `shared`/`Shared` handle directly.
    pub(crate) fn enqueue_job(&self, job: crate::jobq::JobRow) -> Result<()> {
        self.shared.enqueue(job)
    }

    /// Clone the current read-path shape snapshot — a cheap `Arc` clone under a
    /// short read lock that never contends with a tick.
    pub(crate) fn read_view(&self) -> Arc<crate::daemon_read::ReadView> {
        rlock(&self.read_view).clone()
    }

    /// Rebuild the read-path snapshot from the given engine + program. Called at
    /// the end of a full tick — the only path that can change rel shapes or the
    /// `?` query set — while the tick still holds `eng`+`prog`, so it is a
    /// straight clone under a short write lock.
    fn refresh_read_view(&self, eng: &Engine, prog: &Program) {
        let view = crate::daemon_read::ReadView::snapshot(&eng.rels, prog, self.db_path.clone());
        *self.read_view.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(view);
    }

    /// The job-queue identity for this root: its registry key, or `"config"`
    /// for the key-less config view. `tick:{id}` / `sink:{id}` job keys are
    /// built from this, and `Daemon::served_root_for_job` reverses it.
    pub(crate) fn job_root_id(&self) -> String {
        self.key.clone().unwrap_or_else(|| CONFIG_JOB_ID.to_string())
    }

    pub(crate) fn root_label(&self) -> String {
        self.root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned())
    }

    pub(crate) fn tick_full(&self, quiet: bool) -> Result<()> {
        let tick_next = self.tick_count.load(Ordering::Relaxed) + 1;
        crate::activity::begin_tick(tick_next, &self.program_display, &self.root, "full");
        let prog = lock(&self.prog);
        let mut eng = lock(&self.eng);
        let report = eng.tick_report(&prog, quiet)?;
        // A full tick is the only path that can change rel shapes or the `?`
        // query set (program reloads all end here) — refresh the read-path
        // snapshot while still holding eng+prog.
        self.refresh_read_view(&eng, &prog);
        drop(eng);
        drop(prog);
        crate::activity::end_tick();
        // Cold-start staging: a blank-slate seed or a resume re-enqueue defers the
        // extract fan-out onto the queue. Record the warming flag (status +
        // poll_idle read it) and enqueue the staged `ColdExtract` jobs.
        self.cold_pending.store(report.cold_pending, Ordering::Relaxed);
        if !report.cold_staged.is_empty() {
            let job_root_id = self.job_root_id();
            for (family, shard, priority) in &report.cold_staged {
                let job = crate::jobq::JobRow::cold_extract(&job_root_id, family, *shard, *priority);
                if let Err(e) = self.enqueue_job(job) {
                    tracing::warn!("[{}] cold-start enqueue {family}/{shard}: {e}", self.root_label());
                }
            }
        }
        self.settled.store(report.is_settled(), Ordering::Relaxed);
        let n = self.tick_count.fetch_add(1, Ordering::Relaxed) + 1;
        // This WAS a full tick, so it just ran `rebuild_async` over the
        // converged state — resync `last_full_tick_count` (`poll_idle`'s
        // "source changed since the last full tick" half goes false again).
        self.last_full_tick_count.store(n, Ordering::Relaxed);
        self.touch();
        *lock(&self.last_changed_paths) = Vec::new();
        // A no-op tick (nothing reconciled, no timer boundary, no digest move)
        // must not tell subscribers anything changed: every broadcast makes a
        // client (instant) re-query and re-render, and a churn of empty ticks
        // amplified into a webview render storm (2026-07-18). Timer-driven
        // ticks keep broadcasting — clock/every subscribers rely on them.
        if tick_warrants_broadcast(&report) {
            self.broadcast_diag_changed();
        }
        Ok(())
    }

    pub(crate) fn tick_paths(&self, paths: &[PathBuf], quiet: bool) -> Result<()> {
        let tick_next = self.tick_count.load(Ordering::Relaxed) + 1;
        crate::activity::begin_tick(tick_next, &self.program_display, &self.root, "paths");
        crate::activity::set(
            crate::activity::Phase::Reconcile,
            format!("{} changed path(s)", paths.len()),
        );
        let prog = lock(&self.prog);
        let mut eng = lock(&self.eng);
        eng.tick_paths(&prog, paths, quiet)?;
        drop(eng);
        drop(prog);
        crate::activity::end_tick();
        self.settled.store(false, Ordering::Relaxed);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        *lock(&self.last_changed_paths) = paths.to_vec();
        self.broadcast_diag_changed();
        Ok(())
    }

    /// Run ONE cold-start extraction node (a `ColdExtract` job): the family's
    /// wholesale refresh under the engine mutex, marked `done` on the
    /// `_cold_node` row. When the last node lands, run the completion tick — a
    /// normal full tick that (cold no longer in progress) does the single
    /// blank-slate derived rebuild over the now-complete fact base. Budget caps
    /// (QoS/IOPOL/BG tier + rayon width) govern the tempo of the many nodes; this
    /// changes NONE of them (standing law "nothing seizes the machine").
    pub(crate) fn run_cold_node(&self, family: &str, shard: u32) -> Result<()> {
        let tick_now = self.tick_count.load(Ordering::Relaxed);
        crate::activity::begin_tick(tick_now, &self.program_display, &self.root, "cold-extract");
        crate::activity::set(
            crate::activity::Phase::ParseExtract,
            format!("cold-extract {family} shard {shard}"),
        );
        {
            let prog = lock(&self.prog);
            let mut eng = lock(&self.eng);
            eng.run_cold_node(&prog, family, shard)?;
        }
        crate::activity::end_tick();
        self.touch();
        let complete = lock(&self.eng).cold_nodes_complete()?;
        if complete {
            // The completion tick reads `cold_start_in_progress == false` (all
            // nodes done) → normal path → blank-slate `need_full` → the one full
            // `rebuild_derived`. Clears `cold_pending`, refreshes the read view,
            // broadcasts diag — all via `tick_full`.
            self.tick_full(true)?;
        }
        Ok(())
    }

    /// Re-parse the program files, swap the parsed `Program`, re-tick. A parse or
    /// type error keeps the last good program.
    pub(crate) fn reload_program(&self) -> Result<()> {
        let all = lock(&self.program_files).clone();
        let files: Vec<PathBuf> = all.iter().filter(|f| f.exists()).cloned().collect();
        if files.is_empty() {
            if !all.is_empty() {
                tracing::warn!("[{}] all {} watched program file(s) missing; keeping last-good program",
                    self.root_label(), all.len());
            }
            return Ok(());
        }
        let (new_prog, type_diags, _display) = crate::prepare_paths(&files)?;
        let n_err = type_diags
            .iter()
            .filter(|d| d.severity == crate::ast::Severity::Error)
            .count();
        if n_err > 0 {
            bail!("{n_err} type error(s) in reloaded program; keeping old");
        }
        crate::render_type_diags_eprintln(&type_diags);
        {
            let mut p = lock(&self.prog);
            *p = new_prog;
        }
        if let Err(e) = lock(&self.eng).save_program_meta(&all) {
            tracing::warn!("[{}] save_program_meta: {e}", self.root_label());
        }
        self.tick_full(false)?;
        Ok(())
    }

    /// Re-discover `.dl` files under `<root>/.dl/`, re-merge the program if the
    /// file set changed, re-tick. `Ok(true)` = set changed and re-merged;
    /// `Ok(false)` = set unchanged (content edit, or not in discovery mode).
    pub(crate) fn reload_discovery(&self) -> Result<bool> {
        if !self.discovery_mode {
            return Ok(false);
        }
        let files = crate::resolve_programs(&[], &self.root)?;
        let mut canon: Vec<PathBuf> = files
            .iter()
            .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
            .collect();
        canon.sort();
        {
            let pf = lock(&self.program_files);
            if canon == *pf {
                return Ok(false);
            }
        }
        let (new_prog, type_diags, _display) = crate::prepare_paths(&files)?;
        let n_err = type_diags
            .iter()
            .filter(|d| d.severity == crate::ast::Severity::Error)
            .count();
        if n_err > 0 {
            crate::render_type_diags_eprintln(&type_diags);
            tracing::warn!("[{}] discovery reload: {n_err} type error(s); keeping old", self.root_label());
            return Ok(false);
        }
        crate::render_type_diags_eprintln(&type_diags);
        {
            let mut pf = lock(&self.program_files);
            *pf = canon;
        }
        {
            let mut p = lock(&self.prog);
            *p = new_prog;
        }
        {
            let pf = lock(&self.program_files).clone();
            if let Err(e) = lock(&self.eng).save_program_meta(&pf) {
                tracing::warn!("[{}] save_program_meta: {e}", self.root_label());
            }
        }
        let n = lock(&self.program_files).len();
        tracing::info!("[{}] discovery reload: {n} file(s)", self.root_label());
        self.tick_full(false)?;
        Ok(true)
    }

    /// Cheap idle probe for the poll cycle (CPU-hog fix, Part 1). `true` means
    /// this root's poll cycle has nothing to do: skip the whole thing (no
    /// `tick_full`, no drain, no corpus walk) rather than paying `tick_full`'s
    /// full source reconcile every `DEFAULT_POLL_SECS` regardless of state.
    ///
    /// Two probes, both O(1)/indexed — never a corpus walk:
    ///   (a) `pending_effect` COUNT (queued|running, any kind incl. `@stream`
    ///       subscriptions, which sit 'running' forever and need a continuing
    ///       drain) — non-zero means there is already drainable work.
    ///   (b) `tick_count != last_full_tick_count` — a path-tick (the watcher,
    ///       on a file change) landed source motion that no full tick has
    ///       run `rebuild_async` over yet, so a new `@async` request may be
    ///       owed (e.g. `watch-ext.dl`'s `ext_built`, gated on `ext_src`'s
    ///       content hash, not on wall-clock time).
    ///
    /// One case neither probe catches: an `@async`/`@stream` rule gated on
    /// `every`/`clock` fires purely off a wall-clock boundary crossing, with
    /// no associated file change the watcher would ever see — `rebuild_async`
    /// (the only place that evaluates the cadence and queues a fresh request)
    /// runs ONLY inside a full tick, so such a program genuinely needs the
    /// periodic full tick unconditionally, same as before this fix (see
    /// `gc_done_effects`'s doc comment on "a cadence-bucketed poll queues a
    /// fresh row every `clock` bucket forever" — a real, intentional pattern).
    /// `every_rels_used`/`clock_rels_used` scan the whole program (not just
    /// async rule bodies) — a derived rule elsewhere reading `every`/`clock`
    /// also opts a root out of the idle skip, which is conservative-correct,
    /// not a regression (such a root already relied on the always-full-tick
    /// poll before this fix).
    pub(crate) fn poll_idle(&self) -> Result<bool> {
        // Cold start in flight: never idle — the queue is draining `ColdExtract`
        // nodes, and `await-settle` must block until the corpus is warm.
        if self.cold_pending.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let cadence_driven = {
            let prog = lock(&self.prog);
            crate::engine::every_rels_used(&prog) || crate::engine::clock_rels_used(&prog)
        };
        if cadence_driven { return Ok(false); }
        // `self.settled` is the LAST full tick's `TickReport::is_settled()` —
        // quiescence can only be CONFIRMED by a tick that sees nothing move
        // (a tick that just landed a response is itself reported unsettled,
        // by design: is_settled() requires changed_rels to be timer-only).
        // So a not-yet-settled root owes one more full tick regardless of the
        // two cheap probes below — skipping it would freeze `settled` at
        // `false` forever the moment the queue empties, which is exactly the
        // state `dl daemon await-settle` blocks on.
        if !self.settled.load(Ordering::Relaxed) { return Ok(false); }
        let pending = lock(&self.eng).pending_effect_count()?;
        if pending > 0 { return Ok(false); }
        let dirty = self.tick_count.load(Ordering::Relaxed)
            != self.last_full_tick_count.load(Ordering::Relaxed);
        Ok(!dirty)
    }

    /// One poll cycle (the clock source for `@async`): advance the tick, then
    /// drain outstanding effects + external sinks. Returns the number drained.
    /// Skips entirely (see `poll_idle`) when there is nothing to integrate.
    pub(crate) fn poll_tick(&self) -> Result<usize> {
        if self.poll_idle()? { return Ok(0); }
        self.tick_full(true)?;
        let sinks_drained = {
            let prog = lock(&self.prog);
            let mut eng = lock(&self.eng);
            crate::activity::set(crate::activity::Phase::Effects, "external sinks");
            eng.drain_external_sinks(&prog).unwrap_or_else(|e| {
                tracing::warn!("[{}] drain_external_sinks: {e}", self.root_label());
                0
            })
        };
        let arity = {
            let prog = lock(&self.prog);
            crate::engine::async_effect_arity(&prog)
        };
        if arity.is_empty() { return Ok(sinks_drained); }
        let (templates, cwd) = {
            let mut m = {
                let prog = lock(&self.prog);
                crate::engine::shell_templates(&prog)
            };
            let eng = lock(&self.eng);
            let effect_cmd_txt = crate::lower::txt_tbl("effect_cmd");
            if let Ok(rows) = eng.query_sql(&format!("SELECT kind, template FROM {effect_cmd_txt}"), &[]) {
                for row in rows {
                    if let (Some(k), Some(t)) = (row.first().and_then(|v| v.as_str()),
                                                 row.get(1).and_then(|v| v.as_str())) {
                        m.insert(k.to_string(), t.to_string());
                    }
                }
            }
            (m, eng.root())
        };
        let exec = crate::engine::ShellEffectExec { templates, n_out: arity, cwd };
        let n = {
            let prog = lock(&self.prog);
            let mut eng = lock(&self.eng);
            crate::activity::set(crate::activity::Phase::Effects, "drain");
            let a = eng.drain_effects(&prog, &exec)?;
            let s = eng.drain_streams(&prog, &exec)?;
            a + s
        };
        let n = n + sinks_drained;
        self.touch();
        if n > 0 {
            self.tick_full(true)?;
            self.broadcast_diag_changed();
        }
        // The drain set Phase::Effects outside any tick; reset it so a settled
        // daemon's activity slot (and the why-trail samples) read idle, not a
        // forever-stale "effects drain".
        crate::activity::end_tick();
        Ok(n)
    }

    pub(crate) fn has_effects(&self) -> bool {
        let prog = lock(&self.prog);
        !crate::engine::async_effect_arity(&prog).is_empty()
    }

    fn broadcast_diag_changed(&self) {
        let paths: Vec<String> = lock(&self.last_changed_paths).iter()
            .map(|p| p.to_string_lossy().into_owned()).collect();
        let note = json!({"jsonrpc": "2.0", "method": "diag_changed", "params": {
            "root": self.root.to_string_lossy(),
            "tick": self.tick_count.load(Ordering::Relaxed),
            "paths": paths,
        }});
        let body = match serde_json::to_string(&note) {
            Ok(s) => s,
            Err(_) => return,
        };
        // Push through the async pump (best-effort); no socket write on the tick
        // thread anymore, so a slow subscriber can never stall a tick.
        self.shared.push_frame(body);
    }

    /// The git refs to watch for advance: always `HEAD`, plus every non-WORK rev
    /// literal the loaded program scans.
    fn watched_ref_names(&self) -> Vec<String> {
        let mut names = vec!["HEAD".to_string()];
        let prog = lock(&self.prog);
        for item in &prog.items {
            if let crate::ast::Item::Rule(r) = item {
                for b in &r.body {
                    if let crate::ast::BodyItem::Scan { rev: crate::ast::Term::Str(s), .. } = b {
                        if s.as_str() != "WORK" && !names.contains(s) {
                            names.push(s.clone());
                        }
                    }
                }
            }
        }
        names
    }

    /// React to a `.git` change: diff each watched ref old→new against `_file` and
    /// broadcast `rev_advanced`. Returns (refs advanced, worktree files changed).
    pub(crate) fn on_git_event(&self) -> (usize, Vec<PathBuf>) {
        let mut repos: Vec<(String, PathBuf)> = vec![(self.root_label(), self.root.clone())];
        // This engine's corpus (hermetic served root => just its own root; the
        // config view => the config repos), not every ambient config repo.
        for rc in lock(&self.eng).snapshot_repos() {
            if rc.root.exists() && !repos.iter().any(|(s, _)| s == &rc.slug) {
                repos.push((rc.slug, rc.root));
            }
        }
        let names = self.watched_ref_names();
        let mut advances: Vec<(String, String, String, String, Vec<String>)> = Vec::new();
        let mut changed: Vec<PathBuf> = Vec::new();
        {
            let eng = lock(&self.eng);
            for (slug, root) in &repos {
                for name in &names {
                    match eng.observe_ref(slug, root, name) {
                        Ok(Some((old, new))) => {
                            let files = eng
                                .files_changed_between(slug, root, old.as_deref().unwrap_or(""), &new)
                                .unwrap_or_default();
                            for f in &files {
                                let abs = root.join(f);
                                if abs.exists() && !changed.contains(&abs) {
                                    changed.push(abs);
                                }
                            }
                            advances.push((slug.clone(), name.clone(),
                                old.unwrap_or_default(), new, files));
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("[{}] observe_ref {slug}/{name}: {e}", self.root_label()),
                    }
                }
            }
            if !advances.is_empty() {
                if let Err(e) = eng.refresh_daemon_rels() {
                    tracing::warn!("[{}] refresh_daemon_rels: {e}", self.root_label());
                }
            }
        }
        self.touch();
        if !advances.is_empty() {
            self.broadcast_rev_advanced(&advances);
        }
        (advances.len(), changed)
    }

    fn broadcast_rev_advanced(&self, advances: &[(String, String, String, String, Vec<String>)]) {
        for (repo, name, old, new, files) in advances {
            let note = json!({"jsonrpc": "2.0", "method": "rev_advanced", "params": {
                "root": self.root.to_string_lossy(),
                "repo": repo, "ref": name, "old": old, "new": new, "paths": files,
            }});
            let body = match serde_json::to_string(&note) { Ok(s) => s, Err(_) => continue };
            self.shared.push_frame(body);
        }
    }

    pub(crate) fn program_in_paths(&self, paths: &[PathBuf]) -> bool {
        let pf = lock(&self.program_files);
        for p in paths {
            let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
            if pf.iter().any(|f| f == &canon) {
                return true;
            }
        }
        false
    }

    /// Open a served root: open its db, load its program (`<root>/.dl/*.dl`
    /// discovery, or an explicit program set), cold-tick, and build the struct.
    /// `key == None` is the config view: the engine roots at the XDG home, scans
    /// nothing, and draws facts from the config repos.
    pub(crate) fn open(
        root: Option<PathBuf>,
        key: Option<String>,
        programs: &[String],
        db_path: Option<&str>,
        shared: Shared,
    ) -> Result<Arc<ServedRoot>> {
        let is_config = key.is_none();
        let eng_root = root.clone().unwrap_or_else(daemon_home);
        let files = if programs.is_empty() {
            crate::resolve_programs(&[], &eng_root).unwrap_or_default()
        } else {
            programs.iter().map(PathBuf::from).collect()
        };
        let (prog, type_diags, display) = if files.is_empty() {
            (Program { items: vec![] }, vec![], "<serving>".to_string())
        } else {
            crate::prepare_paths(&files)?
        };
        crate::render_type_diags_eprintln(&type_diags);
        let n_err = type_diags.iter().filter(|d| d.severity == crate::ast::Severity::Error).count();
        if n_err > 0 { bail!("{n_err} type error(s) in program; root not served"); }

        // Ensure the db's parent dir exists before opening it (the per-root db
        // lives under <home>/roots/<key>/, which won't exist on first register).
        if let Some(k) = &key { let _ = std::fs::create_dir_all(root_db_dir(k)); }
        let conn = db::open(db_path)?;
        let mut eng = Engine::new(conn, eng_root.clone());
        eng.poll_loop = true;
        if is_config { eng.set_root_implicit(true); }
        eng.set_repos(served_repos(is_config));
        // `begin_tick` (not the bare `set_root` this replaced) both pushes the
        // root AND opens the tick-level span for this root's very first cold
        // tick (tick 0, no `ServedRoot`/`tick_count` yet) — the boot-time
        // counterpart to `tick_full`'s "full" / `tick_paths`'s "paths".
        crate::activity::begin_tick(0, &display, &eng_root, "cold-boot");
        crate::activity::set(crate::activity::Phase::ColdTick, display.as_str());
        let cold_report = eng.tick_report(&prog, false)?;
        crate::activity::end_tick();
        // Cold-start staging: the cold tick may have deferred the extract
        // fan-out onto the queue (blank-slate seed, or a resume re-enqueue after
        // a `kill -9`). Enqueue its `ColdExtract` jobs now — `shared` is in scope
        // here, before the `ServedRoot` exists, so this is the first enqueue
        // point on a fresh boot.
        let cold_start_pending = cold_report.cold_pending;
        if !cold_report.cold_staged.is_empty() {
            let job_root_id = key.clone().unwrap_or_else(|| CONFIG_JOB_ID.to_string());
            for (family, shard, priority) in &cold_report.cold_staged {
                let job = crate::jobq::JobRow::cold_extract(&job_root_id, family, *shard, *priority);
                if let Err(e) = shared.enqueue(job) {
                    tracing::warn!("[daemon] cold-start enqueue {family}/{shard}: {e}");
                }
            }
        }
        let canon_files: Vec<PathBuf> = files
            .iter()
            .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
            .collect();
        if let Err(e) = eng.save_repos_meta() { tracing::warn!("[daemon] save_repos_meta: {e}"); }
        if let Err(e) = eng.save_program_meta(&canon_files) { tracing::warn!("[daemon] save_program_meta: {e}"); }

        let label = eng_root.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| eng_root.to_string_lossy().into_owned());
        // `program {prog_display}`, not `{display}`: the tracing macro pulls its
        // own `display` field-helper into scope, which shadows the local in the
        // format capture. The rendered text is byte-identical.
        let prog_display = display.as_str();
        tracing::info!("[{label}] served ({} type diag(s), program {prog_display})", type_diags.len());

        // Initial read-path snapshot from the cold-tick engine + program.
        let db_path_buf = db_path.map(PathBuf::from);
        let read_view = crate::daemon_read::ReadView::snapshot(&eng.rels, &prog, db_path_buf.clone());

        // Class-17 db-ratio rail (docs/failure-modes.md:407-441): once per
        // root at daemon boot (this fn also runs for a later `add_root`,
        // which is the same "a corpus just got read into a db" moment for
        // that root). The config view (`is_config`, root:None) scans
        // nothing of its own, so it has no corpus to ratio against.
        if !is_config {
            if let Some(db_path_ref) = db_path_buf.as_deref() {
                crate::db_ratio::emit_verdict(&eng_root, db_path_ref);
            }
        }

        Ok(Arc::new(ServedRoot {
            root: eng_root,
            key,
            program_display: display,
            shared,
            program_files: Mutex::new(canon_files),
            discovery_mode: programs.is_empty(),
            prog: Mutex::new(prog),
            eng: Mutex::new(eng),
            last_activity: Mutex::new(Instant::now()),
            tick_count: AtomicU64::new(1),
            settled: AtomicBool::new(false),
            cold_pending: AtomicBool::new(cold_start_pending),
            last_changed_paths: Mutex::new(Vec::new()),
            stopped: Arc::new(AtomicBool::new(false)),
            // The cold tick just above (`eng.tick(&prog, false)`) IS a full
            // tick — it already ran `rebuild_async` once — so start in sync
            // with `tick_count` (both 1), not dirty.
            last_full_tick_count: AtomicU64::new(1),
            db_path: db_path_buf,
            read_view: RwLock::new(Arc::new(read_view)),
        }))
    }
}

// ---------- registered-root persistence ----------

/// One line in `roots.json`.
#[derive(Clone)]
pub(crate) struct RootRecord {
    pub(crate) root: PathBuf,
    pub(crate) key: String,
    pub(crate) added_at: u64,
}

pub(crate) fn read_roots_json() -> Vec<RootRecord> {
    let txt = match std::fs::read_to_string(roots_json_path()) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let v: Value = match serde_json::from_str(&txt) { Ok(v) => v, Err(_) => return Vec::new() };
    v.as_array().map(|arr| arr.iter().filter_map(|r| {
        let root = r.get("root").and_then(|x| x.as_str())?;
        let key = r.get("key").and_then(|x| x.as_str())?;
        let added_at = r.get("added_at").and_then(|x| x.as_u64()).unwrap_or(0);
        Some(RootRecord { root: PathBuf::from(root), key: key.to_string(), added_at })
    }).collect()).unwrap_or_default()
}

pub(crate) fn write_roots_json(records: &[RootRecord]) {
    let arr: Vec<Value> = records.iter().map(|r| json!({
        "root": r.root.to_string_lossy(), "key": r.key, "added_at": r.added_at,
    })).collect();
    let _ = std::fs::create_dir_all(daemon_home());
    if let Ok(s) = serde_json::to_string_pretty(&Value::Array(arr)) {
        let _ = std::fs::write(roots_json_path(), s);
    }
}
