//! The reactive tick orchestrator: `tick` (full) and `tick_paths` (incremental),
//! lifted out of `engine/mod.rs` to shrink the file an AI re-reads each session
//! (engine breakdown Stage 6). Pure relocation — the bodies are unchanged. As a
//! child module of `engine`, this file reaches `Engine`'s private fields,
//! helpers (reconcile_sources / rebuild_derived / refresh_* / ...), and types
//! directly; both methods stay `pub` (the daemon + CLI drive them), so no
//! visibility change is needed for the lift.

use super::*;
use crate::db::Db;

/// RAII I/O-mode switch for a FULL derived rebuild, scoped to the two full-tick
/// rebuild spans (`need_full` and the `extract_changed` rerun). For the duration
/// it sets `PRAGMA synchronous=OFF` and `PRAGMA wal_autocheckpoint=0`, then on
/// drop restores the prior values and runs `PRAGMA wal_checkpoint(TRUNCATE)` —
/// on the normal path, an early `?` error, or a panic alike, since `Drop` runs
/// regardless.
///
/// This is safe ONLY because the derived tables live in the CACHE db: a process
/// kill (SIGKILL) under WAL still leaves a consistent, recoverable database —
/// synchronous=OFF only widens the window in which an OS-level crash or power
/// loss could roll the WAL back, and the worst outcome there is a stale/partial
/// derived cache, which the next boot's `derived_incomplete_rels` check detects
/// and rebuilds. Never wrap a write to un-recomputable state in this guard.
struct BulkRebuildIo<'a> {
    db: &'a Db,
    prior_synchronous: i64,
    prior_wal_autocheckpoint: i64,
}

impl<'a> BulkRebuildIo<'a> {
    fn enter(db: &'a Db) -> Result<Self> {
        let prior_synchronous: i64 = db
            .conn()
            .query_row("PRAGMA synchronous", [], |row| row.get(0))?;
        let prior_wal_autocheckpoint: i64 = db
            .conn()
            .query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))?;
        db.conn()
            .execute_batch("PRAGMA synchronous=OFF; PRAGMA wal_autocheckpoint=0;")?;
        Ok(Self {
            db,
            prior_synchronous,
            prior_wal_autocheckpoint,
        })
    }
}

impl Drop for BulkRebuildIo<'_> {
    fn drop(&mut self) {
        // Best-effort restore + checkpoint; a failure here cannot unwind out of
        // Drop, and the next tick would re-apply these pragmas anyway.
        let _ = self.db.conn().execute_batch(&format!(
            "PRAGMA synchronous={}; PRAGMA wal_autocheckpoint={}; PRAGMA wal_checkpoint(TRUNCATE);",
            self.prior_synchronous, self.prior_wal_autocheckpoint
        ));
    }
}

/// What a full `tick` moved this pass, for settle/quiescence detection
/// (`plans/2026-07-06-settle-quiescence.md`). Every field is surfaced from a
/// local the tick already computes; `tick` (the `()` wrapper) drops it.
#[derive(Default, Clone, Debug)]
pub struct TickReport {
    /// The tick's internal `changed` flag (reconcile delta ∪ carry ∪ clock ∪
    /// family/effect-digest moves). Coarse — a clock boundary sets it.
    pub changed: bool,
    /// `derived:program` digest moved (a derived rule's shape/inputs changed).
    pub derived_moved: bool,
    /// Source/family rels attributed as changed this tick (the affected-derived
    /// seed set). `every`/`clock` here are the steady-state timers.
    pub changed_rels: Vec<String>,
    /// A `@next` carry staged at tx+1 differs from the live rel — next tick will
    /// move (Phase 2, non-destructive peek).
    pub staged_next: bool,
    /// `pending_effect` rows queued|running whose kind is NOT a `@stream`
    /// subscription — an off-tick drain still owes a response (Phase 3).
    pub inflight_effects: usize,
    /// Cold-start staging: this tick deferred the extract fan-out onto the queue
    /// (a blank-slate seed, or a resume re-enqueue after `kill -9`) and did NOT
    /// rebuild derived. `is_settled()` is forced false while true, so
    /// `await-settle` / the poll loop keep waiting until the corpus is warm.
    pub cold_pending: bool,
    /// The `(family, shard, priority)` cold nodes the caller (daemon shell) must
    /// enqueue as `ColdExtract` jobs. Empty on a normal tick.
    pub cold_staged: Vec<(String, u32, i64)>,
}

/// Whether a path-scoped tick may widen into the existing full-tick fallback.
/// The default `Engine::tick_paths` API uses `Allow`; measurement harnesses can
/// use `Forbid` to prove that a scenario stayed on the incremental path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathTickFallbackPolicy {
    Allow,
    Forbid,
}

/// The execution path taken by `Engine::tick_paths_with_policy`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathTickOutcome {
    Incremental,
    FullFallback {
        reason: &'static str,
        scope: &'static str,
    },
    ForbiddenFallback {
        reason: &'static str,
        scope: &'static str,
    },
}

impl PathTickOutcome {
    pub fn fallback_reason(self) -> Option<&'static str> {
        match self {
            Self::Incremental => None,
            Self::FullFallback { reason, .. } | Self::ForbiddenFallback { reason, .. } => {
                Some(reason)
            }
        }
    }

    pub fn fallback_scope(self) -> Option<&'static str> {
        match self {
            Self::Incremental => None,
            Self::FullFallback { scope, .. } | Self::ForbiddenFallback { scope, .. } => Some(scope),
        }
    }
}

impl TickReport {
    /// Settled = this tick produced no NON-timer motion and nothing is pending.
    /// `every`/`clock`/`@stream` are steady-state and excluded, so a
    /// timer-driven program still reports settled at a quiet point instead of
    /// spinning forever.
    pub fn is_settled(&self) -> bool {
        !self.cold_pending
            && !self.derived_moved
            && self.changed_rels.iter().all(|r| is_timer_rel(r) || crate::rels::is_bookkeeping_rel(r))
            && !self.staged_next
            && self.inflight_effects == 0
    }
}

/// The rels whose motion is a recurring timer, not real progress. Bookkeeping
/// rels (`stmt_ms`/`rel_count`/`query_log`, see `RelKind::bookkeeping`) get the
/// same settle treatment in `is_settled`: the tick writes their inputs itself,
/// so a tick whose only motion is its own bookkeeping is quiescent — counting
/// it kept the daemon's poll loop scheduling full ticks forever (the sink-loop
/// that wrote GBs per boot). Their derived dependents still rebuild on the one
/// trailing tick after real motion, so perf rails stay fresh.
pub fn is_timer_rel(rel: &str) -> bool {
    rel == "every" || rel == "clock"
}

impl Engine {
    /// The single slowest derived rel's INSERT-statement wall time from
    /// `_stmt_ms` (written batched by `rebuild_derived`), for the `[tick]`
    /// verdict line. `("none", 0)` when no rebuild has landed yet (a
    /// one-shot's first tick, before any derived rel writes `_stmt_ms`).
    fn slowest_stmt_ms(&self) -> (String, i64) {
        self.db.conn()
            .query_row("SELECT rel, ms FROM _stmt_ms ORDER BY ms DESC LIMIT 1", [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap_or_else(|_| ("none".to_string(), 0))
    }

    /// Force each named extraction-tied builtin rel family (module/type/call/
    /// dataflow/doc/comment/template/unresolved/spine — the exact strings are
    /// `ExtractFamily::name()`'s: `"module-rels"`, `"type-rels"`,
    /// `"call-rels"`, `"dataflow-rels"`, `"doc-rels"`, `"comment-rels"`,
    /// `"template-rels"`, `"unresolved-rels"`, `"spine-rels"`) to refresh
    /// RIGHT NOW, bypassing the `used(prog)` gate a normal `tick`/`run`
    /// applies. For a caller that needs a family's rows present (e.g.
    /// `type_edge` from `"type-rels"`) without running a full tick over a
    /// program that happens to mention the rel.
    ///
    /// Does NOT run a derived-rule rebuild (`rebuild_derived`): a caller that
    /// also needs derived rels populated still needs a `tick`/`run`
    /// afterward. Does NOT run `prime_analysis_bundles`'s shared-parse
    /// optimization (that needs a `Program` to know which families it may
    /// skip priming for), so requesting several of type/call/dataflow here
    /// costs one parse pass each instead of one shared pass. DOES run the
    /// CST `node` refresh first when `"spine-rels"` is requested: spine
    /// projects the `_strings`/`_where_bytes` tables the node walk writes,
    /// so it would otherwise read stale/empty meta tables (mirrors the
    /// pre-node/post-node split `tick_report` applies below). Every other
    /// family is independent of the others. Errors (rather than silently
    /// no-opping) on a name that matches no registered family.
    pub fn ensure_families(&mut self, names: &[&str]) -> Result<()> {
        let families = crate::rels::extract_families();
        for name in names {
            if !families.iter().any(|fam| fam.name() == *name) {
                let available: Vec<&str> = families.iter().map(|fam| fam.name()).collect();
                bail!("ensure_families: unknown family {name:?}; available: {}", available.join(", "));
            }
        }
        if names.contains(&"spine-rels") {
            self.refresh_node_rels()?;
        }
        for fam in families {
            if names.contains(&fam.name()) {
                fam.refresh(self)?;
            }
        }
        Ok(())
    }

    /// One reactive tick, discarding the settle report (the common path). See
    /// `tick_report` for the settle/quiescence driver.
    #[tracing::instrument(skip_all, level = "info")]
    pub fn tick(&mut self, prog: &Program, quiet: bool) -> Result<()> {
        self.tick_report(prog, quiet).map(|_| ())
    }

    /// One reactive tick: declare, reconcile sources incrementally, rebuild
    /// derived only if a source fact changed, then run queries. Returns a
    /// `TickReport` of what moved (`dl --settle` drives this to a fixpoint).
    // ARCH {"url":"engine/40-tick","role":"orchestrator"}
    #[tracing::instrument(skip_all, level = "info")]
    pub fn tick_report(&mut self, prog: &Program, quiet: bool) -> Result<TickReport> {
        let t_tick = std::time::Instant::now();
        self.rev_cache.clear();
        self.extraction_drops.clear();
        self.shape_diags.clear();
        CMD_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        self.db.tick_begin();
        self.db.clear_write_ledger();
        self.write_ledger.borrow_mut().clear();
        let tick = self.tick_seq.get() + 1;
        self.tick_seq.set(tick);
        self.adjacency_cache.borrow_mut().take();
        // Rewrite any rel headed by both a source/extract rule and a derived
        // rule into hidden twins + a synthesized union, BEFORE rule
        // classification below ever sees the program — so every downstream
        // pass (reconcile, rebuild_derived, closures, `?` queries) treats the
        // twins as ordinary rels and the visible rel as ordinary-derived.
        // `None` (the common case: nothing mixed) keeps borrowing the
        // original `prog`, no clone paid.
        let desugared = desugar::desugar_mixed_rels(prog)?;
        let prog: &Program = desugared.as_ref().map(|(p, _)| p).unwrap_or(prog);
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i {
            Item::Rule(r) => Some(r), _ => None,
        }).collect();
        let closures = closure_map(&rules);
        crate::activity::set(crate::activity::Phase::Declare, "");
        // Meta tables (incl. `_shapes`) before declare: `resolve_derived_shapes`
        // inside `declare_all` reads `_shapes` (Phase 5).
        self.ensure_meta()?;
        self.declare_all(prog, &closures)?;
        // Retire `_cold_node` rows for families no longer used (mirrors the
        // `_derived_complete` declare-time cleanup); a fresh db has none.
        self.sweep_cold_nodes(prog)?;

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        // `repo`-sink + `checkout`-sink rules are NOT drained here: they hit the
        // network and rewrite checkouts, so they live in `drain_external_sinks`
        // and are run by the daemon poll loop / explicit `--apply` (a `?` query
        // is a pure read). Their shape validation also moved there.
        // `@next` carry rules: staged into carry_<rel> at tx+1 after the tick
        // converges, NOT derived into the head rel this tick. `@async` rules emit
        // a `pending_effect` request per body solution; the off-tick daemon runs
        // the executor and lands the response in the head rel at a later tick.
        // Neither is derived this tick (both excluded from all_derived below).
        let next_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_next()).collect();
        // `@async` (one-shot request/response) and `@stream` (`sh*` subscription)
        // share the emission path: both bind request args over the converged tick
        // and queue a `pending_effect` row. They diverge at DRAIN time — an @async
        // row runs once (`drain_effects`), a @stream row stays 'running' and fans
        // output lines into the head rel each drain (`drain_streams`). `is_effect`
        // is the union used for emission and the response-rel conflict checks.
        let async_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.is_async() || r.is_stream()).collect();
        // An effect body fires over already-derived relations; a source op
        // (scan/match/...) in the body has no meaning here (the effect, not the
        // file system, is the IO). Reject it like a repo-sink.
        for r in &async_rules {
            if r.is_source() {
                bail!("@async/@stream rule (rel `{}`) must be derived-style (no scan/match/ast/...); \
                       its body binds the request args over already-derived relations", r.head.rel);
            }
        }
        // non-source, non-closure rules. A rule whose body reads a closure head in
        // the seedable shape is split out: it can't go through `lower_rule` (the
        // closure head is a VIEW that isn't populated mid-fixpoint), so it is
        // evaluated by a seeded BFS in the query phase instead. Repo-sinks are
        // excluded: they are drained, not derived. `@next` rules are excluded:
        // their head lands in the next tick's carry buffer, not this tick.
        // A TERM-form json/jsonp rule (`page(..), jsonp(body, "x", n)`) joins
        // relations in SQL then extracts from a bound string value — tree-sitter
        // can't run inside the SQL fixpoint, so it is evaluated by the hybrid
        // `eval_extract_rules` pass (after sources/responses are present, before
        // the derived fixpoint) and excluded from `all_derived` here.
        let extract_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.has_term_extract() && !r.is_source()
                && self.rels.contains_key(&r.head.rel)).collect();
        let all_derived: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && !r.is_repo_sink() && !r.is_next() && !r.is_async()
                && !r.is_stream() && !r.has_term_extract()
                && r.closure_edge().is_none() && r.scc_edge().is_none()
                && r.node2vec_edge().is_none()
                // A rule heading a still-PENDING derived-shape rel (its `rel name:
                // shape.` has no persisted columns yet) has no table to write —
                // skip it this tick; it runs once the shape resolves (Phase 5).
                && self.rels.contains_key(&r.head.rel)).collect();
        // `head(..) <- scc(edge).` rules: materialize (rep, member) from the
        // closure condensation in the query phase (after refresh_cond_cache).
        // Excluded from all_derived — the Scc body item can't lower to SQL, and
        // the head is filled from the already-computed Tarjan condensation.
        let scc_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.scc_edge().is_some()).collect();
        // `head(..) <- node2vec(edge).` rules: same exclusion shape as scc. The
        // edge rel is an ordinary derived rel; after it materializes we read its
        // rows, learn node vectors, and fill the head with KNN pairs.
        let node2vec_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.node2vec_edge().is_some()).collect();
        check_stratification(&all_derived, &closures)?;
        let (seed_rules, derived_rules) = split_seed_and_derived(&all_derived, &closures)?;

        // source rels are heads of source rules; they get incremental retraction.
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules {
            if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); }
        }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules {
            if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); }
        }
        // Split the derived layer around the operator boundary: pre-stratum rules
        // run in the main fixpoint, post-stratum rules (those transitively reading
        // an scc/node2vec head) run AFTER the operator evals fill those heads.
        let strata = partition_derived_strata(&derived_rules, &derived_rels, &scc_rules, &node2vec_rules)?;
        // A rel written by BOTH a source rule (scan/match/ast/sg/json/cmd/comment)
        // and a derived rule cannot share one table: `reconcile_sources` fills the
        // source rows incrementally (tracked in `_prov`), then `rebuild_derived`
        // does a full `DELETE FROM rel` + recompute, which silently drops every
        // scanned row. Bail loudly instead — split into two rels and union them in
        // a third derived rule (see examples/anim-self.dl's pin/fpin -> span_of).
        for rel in &source_rels {
            if derived_rels.contains(rel) {
                bail!("relation '{rel}' is written by both a source rule (scan/match/ast/...) \
                       and a derived rule; the scanned rows would be dropped on rebuild. Put \
                       the source rule and the derived rule in two separate relations and union \
                       them in a third derived rule.");
            }
        }
        // An `@in(class)` port rel is EDB injected by the serving loop (--mcp);
        // a source or derived rule heading it would wipe/collide with the
        // injected requests. Rules READ an in-port; they head an @out rel.
        let in_ports: Vec<&str> = prog.items.iter().filter_map(|i| match i {
            crate::ast::Item::Rel(d)
                if matches!(&d.port, Some(p) if p.dir == crate::ast::PortDir::In) =>
                Some(d.name.as_str()),
            _ => None,
        }).collect();
        for rel in source_rels.iter().chain(derived_rels.iter()) {
            if in_ports.contains(&rel.as_str()) {
                bail!("relation '{rel}' is an @in port (rows are injected by the serving loop); \
                       rules read it, never head it — head your handler output in an @out rel \
                       (or an ordinary relation) instead.");
            }
        }
        // Same hazard for a rel headed by BOTH a term-extract rule (json/jsonp body
        // form) and a plain derived rule: `eval_extract_rules` fills the extract
        // rows, then `rebuild_derived` runs AFTER it (so derived rules can read the
        // extract output) and its `DELETE FROM rel` + recompute silently drops them.
        // Extract must precede the derived fixpoint, so the same rel cannot be both;
        // split the extract into its own rel and union it in a third derived rule.
        let mut extract_rels: Vec<String> = Vec::new();
        for r in &extract_rules {
            if !extract_rels.contains(&r.head.rel) { extract_rels.push(r.head.rel.clone()); }
        }
        for rel in &extract_rels {
            if derived_rels.contains(rel) {
                bail!("relation '{rel}' is written by both a term-extract rule (json/jsonp \
                       body form) and a derived rule; the extracted rows would be dropped when \
                       the derived rule rebuilds. Put the extract in its own relation and union \
                       it with the derived rule in a third relation.");
            }
        }
        // `@next` carry relations: a head rel staged for the next tick must be
        // written ONLY by @next rules — it is loaded from carry as EDB this tick,
        // so a source/derived rule heading it would be wiped (rebuild_derived) or
        // collide (reconcile). Its dedup head set drives the carry load/stage.
        let mut next_rels: Vec<String> = Vec::new();
        for r in &next_rules {
            if !next_rels.contains(&r.head.rel) { next_rels.push(r.head.rel.clone()); }
        }
        for rel in &next_rels {
            if source_rels.contains(rel) || derived_rels.contains(rel) {
                bail!("relation '{rel}' is headed by a @next rule and also by a source/derived \
                       rule; a @next (carry) relation must be written only by @next rules.");
            }
        }
        // `@async` response relations: like carry, an @async head rel is written
        // only by the off-tick drain (a persisted source-style rel). A source or
        // derived rule heading it would be wiped each tick. Bail loudly.
        let mut async_rels: Vec<String> = Vec::new();
        for r in &async_rules {
            if !async_rels.contains(&r.head.rel) { async_rels.push(r.head.rel.clone()); }
        }
        for rel in &async_rels {
            if source_rels.contains(rel) || derived_rels.contains(rel) {
                bail!("relation '{rel}' is headed by a @async rule and also by a source/derived \
                       rule; an @async (response) relation is written only by the effect drain.");
            }
            if next_rels.contains(rel) {
                bail!("relation '{rel}' is headed by both @next and @async rules; pick one.");
            }
        }
        // Load each carry rel's rows staged for the current generation into its
        // live table BEFORE reconcile/derive, so same-tick rules read the carried
        // state as an ordinary relation. tx stays at `cur_tx` until staging.
        let cur_tx = self.current_tx()?;
        let mut carry_changed = false;
        for rel in &next_rels {
            let meta = self.rels.get(rel)
                .ok_or_else(|| anyhow::anyhow!("@next relation {rel} is not declared (add `rel {rel}(...)`)"))?
                .clone();
            self.ensure_carry_table(rel, &meta)?;
            carry_changed |= self.load_carry(rel, &meta, cur_tx)?;
        }
        let edges: Vec<&str> = dedup_edges(&closures);
        self.create_auto_indexes(&derived_rules, &closures)?;

        let t_src = std::time::Instant::now();
        // In profile mode each sub-phase reports its own wall time, so a hung or
        // crawling tick says WHERE it is spending the wait.
        let phase = |label: &'static str, t: std::time::Instant| {
            if crate::db::profiling() {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                tracing::debug!(label, ms, "[profile] {label}: {ms:.1}ms");
            }
        };
        let t = std::time::Instant::now();
        // Rels read as a body predicate anywhere (source or derived): a scan whose
        // head feeds a rule is "consumed", so an empty tick softens rather than
        // shouting (see the zero-match diagnostic in reconcile_sources).
        let consumed: std::collections::HashSet<String> = derived_rules.iter()
            .chain(source_rules.iter())
            .flat_map(|r| r.body.iter().filter_map(|b| match b {
                crate::ast::BodyItem::Pos(a) | crate::ast::BodyItem::Neg(a) => Some(a.rel.clone()),
                _ => None,
            }))
            .collect();
        crate::activity::set(crate::activity::Phase::Reconcile, "");
        // Guard: a program with NO scan rules must not treat its empty scan scope
        // as authoritative and wipe an existing db's file-scoped source relations.
        // Scan rules that matched zero files still run reconcile normally (a real
        // empty corpus legitimately reconciles down). Only the no-scan-rules case
        // is skipped, and only when the db already has scanned files.
        let recon = if source_rules.is_empty() {
            let file_count: i64 = self
                .db
                .conn()
                .query_row("SELECT COUNT(*) FROM _file", [], |r| r.get(0))
                .unwrap_or(0);
            if file_count > 0 {
                tracing::warn!(
                    file_count,
                    "[reconcile] program has no scan rules; leaving {file_count} existing files untouched"
                );
                Reconcile {
                    changed: false,
                    extracted: 0,
                    retracted: 0,
                    parsed: 0,
                    total: 0,
                }
            } else {
                self.reconcile_sources(&source_rules, &source_rels, &consumed)?
            }
        } else {
            self.reconcile_sources(&source_rules, &source_rels, &consumed)?
        };
        phase("reconcile-sources", t);
        // A carried-in @next rel that moved is an EDB change for this tick's
        // derived rules (e.g. a `poll` rule that reads the carried `etag`).
        let mut changed = recon.changed || carry_changed;        // Per-rel change attribution for the scoped rebuild below (perf gap B):
        // every source/built-in relation whose rows moved this tick lands here,
        // and only the derived rels dependency-reachable from the set re-derive.
        // Baselining each source relation's content digest doubles as the
        // attribution for extraction rels (the digests were already computed
        // here for tick_paths' bytes-moved-rows-didn't prune).
        // Source-attribution digest saves are DEFERRED to after the rebuild
        // lands (flushed at the `derived:program` deferral point below): a tick
        // killed mid-rebuild must leave every `_reldigest` baseline unmoved, so
        // the next boot re-detects the change and re-scopes the rebuild. Before
        // the crash-window fix this was safe only because the whole-pass
        // derived-missing full rebuild re-attributed anything lost; `rebuild_derived`
        // now marks completion per component, so that backstop is gone and the
        // saves must not race ahead of the rows they describe.
        let mut pending_digests: Vec<(String, [u8; 32])> = Vec::new();
        let (seed_moved, seed_pending) = self.seed_rel_digests(&source_rels)?;
        pending_digests.extend(seed_pending);
        let mut changed_source_rels: HashSet<String> = seed_moved.into_iter().collect();
        // refresh built-in repo/rev/content/file from the updated _file cache,
        // before derived rules that may join them are rebuilt.
        let t = std::time::Instant::now();
        self.refresh_builtin_rels()?;
        if recon.changed { for b in BUILTIN_RELS { changed_source_rels.insert(b.to_string()); } }
        phase("builtin-rels", t);
        // The clock can move with no file change (a boundary crossing, or the
        // clear after one), so feed its change into `changed` — else the full tick
        // below skips rebuild_derived and a rule gated by `every` keeps stale rows.
        if every_rels_used(prog) && self.refresh_every(&every_intervals(prog))? {
            changed = true;
            changed_source_rels.insert("every".to_string());
        }
        if clock_rels_used(prog) && self.refresh_clock(&clock_periods(prog))? {
            changed = true;
            changed_source_rels.insert("clock".to_string());
        }
        // D5.5 rev-retraction sweep: `_file` above is this tick's live rev
        // set, so a rev that just stopped being scanned is retractable now.
        // Runs BEFORE the extraction families below, not after: each family's
        // own legacy rebuild is gated behind its per-rev digest skip, which
        // never fires for a rev that disappeared (see `sweep_gone_revs`'s
        // doc), so the sweep does the legacy rebuild itself rather than
        // relying on a family running this tick.
        self.sweep_gone_revs()?;
        // The extraction-tied builtin rel families (module/type/call/dataflow/
        // doc/spine): `trait ExtractFamily` + registry (src/rels/extract_family.rs),
        // replacing six hand-written used-gate/refresh blocks. Contract
        // preserved per family: refresh() returns whether to mark its rels()
        // changed — a real input-digest diff for type/call/dataflow/doc
        // (perf gaps A/C), unconditional true for the wholesale module/spine
        // rebuilds (conservative mark). None of these feed `changed`
        // directly, only the `changed_source_rels` attribution set.
        // `node` (CST) is not a member (it must run BEFORE spine — its walk
        // writes the `_strings`/`_where_bytes` meta tables spine projects),
        // so it stays hand-dispatched between the pre/post-node slices.
        let any_extract = crate::rels::extract_families_pre_node().iter().any(|f| f.used(prog))
            || crate::rels::extract_families_post_node().iter().any(|f| f.used(prog))
            || node_rels_used(prog);
        if any_extract {
            crate::activity::set(crate::activity::Phase::ParseExtract, "extract");
        }
        // Cold-start staging (daemon poll loop only, D1): on a blank slate defer
        // the extract-family fan-out + derived rebuild onto the durable queue
        // instead of running every family over the whole corpus in this one
        // lock-held tick. Under MB-chunking (plan Addendum 2026-07-18) the SCIP
        // pre-extract and the analysis prime also move OFF this tick: scip is its
        // own `scip-index` node (drains first, feeds the resolution ladder) and
        // each family node primes its own caches, so the seed/resume tick stays
        // short. A resume tick (`kill -9` mid-start) re-enqueues only the still-
        // pending nodes. See `engine/cold_stage.rs`.
        let cold_should_seed = self.cold_start_should_seed(prog)?;
        let cold_resume =
            !cold_should_seed && self.poll_loop && self.cold_start_in_progress()?;
        let cold_staging_active = cold_should_seed || cold_resume;
        // Pre-extract RelKinds (scip): the index load is an INPUT to the extract
        // families below (the type/call resolvers read `scip_ref`), so it must
        // land first — otherwise a fresh db's first tick extracts index-blind and
        // only heals on tick 2 via the digest fold. When cold-staging, scip is
        // deferred to its own node and prime to the family nodes.
        if !cold_staging_active {
            for k in crate::rels::rel_kinds() {
                if k.pre_extract() && k.used(prog) && k.refresh(self)? && !(self.poll_loop && k.bookkeeping()) {
                    changed = true;
                    for r in k.rels() { changed_source_rels.insert(r.to_string()); }
                }
            }
            // Type/call/dataflow share a parsed representation in language front
            // ends that implement `extract_bundle` (currently Rust). Prime their
            // existing fact caches once before the independent family refreshers;
            // those refreshers retain ownership of digests, resolution, and rows.
            super::extract::prime_analysis_bundles(self, prog)?;
        }
        let cold_staged: Vec<(String, u32, i64)> = if cold_should_seed {
            self.seed_cold_nodes(prog)?
                .into_iter()
                .map(|j| (j.family, j.shard, j.priority))
                .collect()
        } else if cold_resume {
            self.pending_cold_jobs(prog)?
                .into_iter()
                .map(|j| (j.family, j.shard, j.priority))
                .collect()
        } else {
            Vec::new()
        };
        if !cold_staged.is_empty() {
            // Short tick: facts intentionally incomplete, derived NOT rebuilt.
            // Report reads UNSETTLED (`cold_pending`) so await-settle waits.
            self.last_n1 = self.db.tick_end();
            self.flush_write_ledger(tick)?;
            self.clear_exe_identity_cache();
            return Ok(TickReport {
                cold_pending: true,
                cold_staged,
                ..Default::default()
            });
        }
        for fam in crate::rels::extract_families_pre_node() {
            if !fam.used(prog) { continue; }
            crate::activity::detail(fam.name());
            let t = std::time::Instant::now();
            if fam.refresh(self)?.moved() {
                for r in fam.rels() {
                    changed_source_rels.insert(r.to_string());
                }
            }
            phase(fam.name(), t);
        }
        if node_rels_used(prog) {
            crate::activity::detail("node");
            let t = std::time::Instant::now();
            if self.refresh_node_rels()? {
                changed = true;
                for n in NODE_RELS { changed_source_rels.insert(n.to_string()); }
            }
            phase("node-rels", t);
        }
        for fam in crate::rels::extract_families_post_node() {
            if !fam.used(prog) { continue; }
            crate::activity::detail(fam.name());
            let t = std::time::Instant::now();
            // Spine is corpus-gated: its output is a pure function of file
            // content, so skip it (and its changed mark) when no file moved AND
            // its rels are already populated. Before this gate spine returned
            // Ok(true) every tick it was used, forcing every dependent to
            // re-derive on every clock-driven full tick even with zero file
            // changes.
            let run = if fam.corpus_gated() {
                recon.changed || self.any_rel_empty(fam.rels())?
            } else {
                true
            };
            let moved = if run {
                fam.refresh(self)?.moved()
            } else {
                false
            };
            if moved {
                for r in fam.rels() { changed_source_rels.insert(r.to_string()); }
            }
            phase(fam.name(), t);
        }
        // The git-derived/analysis/scip/propose/embed families behind RelKind.
        // The diff can move without any file content changing (a commit moves
        // HEAD under an identical worktree), so the refresh result feeds
        // `changed` directly rather than riding the reconcile delta. A full tick
        // always refreshes every used family (`dirty` is consulted only by the
        // incremental `tick_paths`), so the scip index reload runs here too.
        for k in crate::rels::rel_kinds() {
            if k.pre_extract() { continue; } // ran before the extract families
            if k.used(prog) && k.refresh(self)? {
                // Under a repeated-tick scheduler, bookkeeping families
                // (stmt_ms/rel_count/query_log) move on every tick by
                // construction: the tick writes their inputs, and rebuilding
                // their dependents re-jitters the timings they report, so
                // seeding the scoped rebuild from them can never converge —
                // derived_moved stays true and the poll loop schedules full
                // ticks forever (75GB/2.7h measured 2026-07-17). One-shot
                // ticks keep the seed: they cannot loop, and the perf rails'
                // second-invocation contract needs it (see Engine::poll_loop).
                if self.poll_loop && k.bookkeeping() { continue; }
                changed = true;
                for r in k.rels() { changed_source_rels.insert(r.to_string()); }
            }
        }
        // `program`/`head`/`rev_advanced` and `effect_log` are wholesale
        // wipe+repopulate PROJECTIONS of state written out of band (the
        // daemon's meta tables; the `pending_effect` queue the async drain
        // mutates BETWEEN ticks) — their content can move without any other
        // signal in this function noticing. Before P1, this went unnoticed:
        // these two blocks never set `changed = true` themselves, so a
        // derived rule reading `effect_log` (like the `queued`/`landed`
        // rails) only re-derived because `any_derived_empty` forced a full
        // rebuild whenever that rule's rel happened to still read zero rows
        // from the PRIOR tick. Content-digest gate them the same way as
        // `async:`/`hook:`/`port:` above, so their own change is what
        // attributes their dependents, not an accident of table emptiness.
        if daemon_rels_used(prog) {
            self.refresh_daemon_rels()?;
            for rel in DAEMON_RELS {
                let Some(meta) = self.rels.get(rel).cloned() else { continue };
                let d = self.rel_content_digest(rel, &meta)?;
                let key = format!("daemon:{rel}");
                if self.load_rel_digest(&key)? != Some(d) {
                    pending_digests.push((key, d));
                    changed = true;
                    changed_source_rels.insert(rel.to_string());
                }
            }
        }
        if effect_rels_used(prog) {
            self.refresh_effect_rels()?;
            for rel in EFFECT_RELS {
                let Some(meta) = self.rels.get(rel).cloned() else { continue };
                let d = self.rel_content_digest(rel, &meta)?;
                let key = format!("effect:{rel}");
                if self.load_rel_digest(&key)? != Some(d) {
                    pending_digests.push((key, d));
                    changed = true;
                    changed_source_rels.insert(rel.to_string());
                }
            }
        }
        // @async/@stream response rels are written by the OFF-TICK drain, so
        // none of the source-phase machinery above attributes them. Digest
        // their content here (an `async:` key in the same `_reldigest` store):
        // a drain that landed rows re-derives their dependents; a quiet tick
        // leaves them out of the scoped rebuild. First-ever seeding counts as
        // moved, like the source-rel baseline.
        for rel in &async_rels {
            let Some(meta) = self.rels.get(rel).cloned() else { continue };
            let d = self.rel_content_digest(rel, &meta)?;
            let key = format!("async:{rel}");
            if self.load_rel_digest(&key)? != Some(d) {
                pending_digests.push((key, d));
                changed = true;
                changed_source_rels.insert(rel.clone());
            }
        }
        // `hook_event` rows are written out-of-tick by `dl --hook` (the
        // `hook_event` RPC / the in-process feed), accumulating harness facts, so
        // none of the source-phase machinery above attributes them. Digest the
        // rel's content (a `hook:` key in `_reldigest`) so a new event re-derives
        // its dependents; a tick with no new event leaves them scoped out. Lazy —
        // a program that never reads hook_event pays nothing.
        if hook_rels_used(prog) {
            for rel in HOOK_RELS {
                let Some(meta) = self.rels.get(rel).cloned() else { continue };
                let d = self.rel_content_digest(rel, &meta)?;
                let key = format!("hook:{rel}");
                if self.load_rel_digest(&key)? != Some(d) {
                    pending_digests.push((key, d));
                    changed = true;
                    changed_source_rels.insert(rel.to_string());
                }
            }
        }
        // `@in(class)` port rels (e.g. `--mcp`'s request rel) are EDB injected
        // directly by the serving loop (`inject_rpc`), bypassing every
        // source-rule/family digest above — an "unattributable EDB change"
        // (see the `need_full` comment below). Before the P1 fix, this went
        // unnoticed because draining the paired `@out` rel to empty forced a
        // full rebuild on the NEXT tick via `any_derived_empty`'s "any derived
        // rel has zero rows" probe. The P1 fix (a completion marker, not a row
        // count) removes that accidental prop, so the in-port's own change
        // must be attributed here, the same `port:` key pattern as `async:`/
        // `hook:` above — a freshly injected request row re-derives its
        // `@out`-port dependents; a tick with no new injection stays scoped out.
        let in_port_rels: Vec<String> = prog.items.iter().filter_map(|i| match i {
            Item::Rel(d) if matches!(&d.port, Some(p) if p.dir == crate::ast::PortDir::In) =>
                Some(d.name.clone()),
            _ => None,
        }).collect();
        for rel in &in_port_rels {
            let Some(meta) = self.rels.get(rel).cloned() else { continue };
            let d = self.rel_content_digest(rel, &meta)?;
            let key = format!("port:{rel}");
            if self.load_rel_digest(&key)? != Some(d) {
                pending_digests.push((key, d));
                changed = true;
                changed_source_rels.insert(rel.clone());
            }
        }
        let src_ms = t_src.elapsed().as_secs_f64() * 1000.0;

        let t_der = std::time::Instant::now();
        let der_digest = derived_program_digest(&derived_rules, &seed_rules, &edges);
        let prior_der_digest = self.load_rel_digest("derived:program")?;
        let derived_moved = prior_der_digest != Some(der_digest);
        // #13 write-storm fix, dirty-rel scoping: when the program digest moved,
        // the per-rel `drv:` shape digests name WHICH heads' rules moved. If
        // every motion is attributable to current derived heads, the program-
        // edit full rebuild below downgrades to the scoped arm seeded with
        // those heads — byte-identical derived tables downstream of nothing
        // are never wiped and rewritten (the GB-boot cost was whole-layer
        // DELETE+INSERT for a one-rule edit). Diff computed only on a moved
        // tick; its pending saves flush at the deferred-digest point below.
        let shape_diff = if derived_moved {
            Some(self.derived_rule_diff(&derived_rules, &seed_rules, &edges)?)
        } else {
            None
        };
        // A blank slate, a program-shape change, or an unattributable EDB change
        // (a carried @next rel has no per-rel entry above) rebuilds everything.
        // Otherwise a changed tick scopes the rebuild to the derived rels
        // dependency-reachable from what actually moved (perf gap B — the full
        // tick's twin of tick_paths' affected_derived scoping).
        // P1: `derived_incomplete_rels`/`first_empty_closure_edge` replace the
        // old "any derived rel has zero rows" probe (`any_derived_empty`),
        // which forced a full rebuild of every derived rel on every tick
        // whenever ANY of them was legitimately, durably empty (an inert rail,
        // a diff view with nothing to report) — 34/154 derived rels on a real
        // db, so effectively every tick.
        let incomplete_derived = self.derived_incomplete_rels(&derived_rels)?;
        let empty_closure_edge = self.first_empty_closure_edge(&edges)?;
        let mut need_full = derived_moved || carry_changed
            || !incomplete_derived.is_empty() || empty_closure_edge.is_some();
        // P3: name WHY a full rebuild happened, in tick record priority order
        // matching the `need_full` OR above (a blank slate/program-shape
        // change is the more useful diagnosis when several conditions coincide,
        // e.g. the very first tick on a fresh db has both `derived_moved` and
        // every rel "incomplete").
        let mut full_reason: Option<String> = if !need_full { None }
            else if derived_moved && prior_der_digest.is_none() { Some("blank-slate".to_string()) }
            else if derived_moved { Some("program-edit".to_string()) }
            else if carry_changed { Some("carry-changed".to_string()) }
            else if !incomplete_derived.is_empty() { Some(format!("derived-missing:{}", incomplete_derived.join(","))) }
            else { empty_closure_edge.as_ref().map(|edge| format!("closure-missing:{edge}")) };
        // The #13 downgrade: ONLY the pure program-edit trigger is scopable — a
        // blank slate has nothing to keep, carry/closure triggers mean table
        // state (not rule shape) is untrustworthy. `full_reason` is priority-
        // ordered and can mask coincident triggers, so each is re-checked
        // explicitly; incomplete rels are tolerable only when the scope
        // rebuilds every one of them anyway (a newly declared rel is both
        // "incomplete" and "moved"). Unattributable or empty diffs keep the
        // full rebuild (safety net: the monolithic digest moved but no head
        // owns the motion).
        let mut program_scope: HashSet<String> = HashSet::new();
        if full_reason.as_deref() == Some("program-edit")
            && !carry_changed && empty_closure_edge.is_none()
        {
            if let Some(diff) = &shape_diff {
                if diff.attributable && !diff.moved.is_empty()
                    && incomplete_derived.iter().all(|rel| diff.moved.contains(rel))
                {
                    need_full = false;
                    full_reason = None;
                    program_scope = diff.moved.clone();
                    if !quiet {
                        let mut names: Vec<&str> =
                            program_scope.iter().map(|s| s.as_str()).collect();
                        names.sort_unstable();
                        let names_str = names.join(", ");
                        tracing::debug!(
                            rel_count = names.len(),
                            names = %names_str,
                            "[derived-scope] program edit scoped to {} rel(s): {names_str}",
                            names.len()
                        );
                    }
                }
            }
        }
        let mut affected: HashSet<String> = HashSet::new();
        let mut dirty_edges: HashSet<&str> = HashSet::new();
        self.last_derived_rebuilt = Vec::new();
        if changed || need_full || !program_scope.is_empty() {
            crate::activity::set(crate::activity::Phase::Derived, "");
        }
        if need_full {
            // A full derived rebuild is the choke this arc targets. On the cache
            // db, drop fsync + autocheckpoint for its duration (RAII restores +
            // truncates on the way out, error paths included) so the 30s+ rebuild
            // is not also paying a WAL fsync per statement.
            {
                let _io = BulkRebuildIo::enter(&self.db)?;
                self.rebuild_derived(&strata.pre_rules, &strata.pre_rels)?;
                self.rebuild_closures(&edges)?;
            }
            dirty_edges = edges.iter().copied().collect();
            self.last_derived_rebuilt = strata.pre_rels.clone();
        } else if changed || !program_scope.is_empty() {
            let mut scope_seed = changed_source_rels.clone();
            scope_seed.extend(program_scope.iter().cloned());
            affected = affected_derived(&derived_rules, &scope_seed);
            let sub_rules: Vec<&Rule> = strata.pre_rules.iter().copied()
                .filter(|r| affected.contains(&r.head.rel)).collect();
            let sub_rels: Vec<String> = strata.pre_rels.iter()
                .filter(|r| affected.contains(*r)).cloned().collect();
            self.rebuild_derived(&sub_rules, &sub_rels)?;
            let aff_edges: Vec<&str> = edges.iter().copied()
                .filter(|e| affected.contains(*e) || changed_source_rels.contains(*e)).collect();
            self.rebuild_closures(&aff_edges)?;
            dirty_edges = aff_edges.into_iter().collect();
            self.last_derived_rebuilt = sub_rels;
        }
        // The hybrid join+extract pass (term-form json/jsonp) reads its inputs —
        // facts/source/derived/response rels — AFTER the fixpoint populates them,
        // then extracts from each bound string. If its output moved, the consumers
        // of the extract head rels must re-derive, so a second fixpoint pass runs.
        // (No feedback INTO the extracted inputs, so one extra pass converges.)
        let extract_changed = self.eval_extract_rules(&extract_rules)?;
        if extract_changed {
            // Same full-rebuild span as the `need_full` arm — same cache-db I/O
            // relaxation for the same reason.
            {
                let _io = BulkRebuildIo::enter(&self.db)?;
                self.rebuild_derived(&strata.pre_rules, &strata.pre_rels)?;
                self.rebuild_closures(&edges)?;
            }
            dirty_edges = edges.iter().copied().collect();
            self.last_derived_rebuilt = strata.pre_rels.clone();
        }
        // Deferred saves land only after the rebuild lands, so a failed tick
        // retries: the `derived:program` shape digest AND every source-attribution
        // digest (`seed_rel_digests` + the daemon/effect/async/hook/port keys)
        // collected above. A tick killed before this point leaves the baselines
        // unmoved and the change is re-detected next boot.
        if derived_moved {
            self.save_rel_digest("derived:program", &der_digest)?;
            if let Some(diff) = &shape_diff {
                self.save_rel_digests(&diff.pending)?;
                self.delete_rel_digests(&diff.stale)?;
            }
        }
        for (key, digest) in &pending_digests {
            self.save_rel_digest(key, digest)?;
        }
        let der_ms = t_der.elapsed().as_secs_f64() * 1000.0;

        if !quiet {
            let (slowest_rel, slowest_ms) = self.slowest_stmt_ms();
            let reason = full_reason.as_deref().unwrap_or("-");
            let parsed = recon.parsed;
            let total = recon.total;
            let extracted = recon.extracted;
            let retracted = recon.retracted;
            let derived = if changed || !program_scope.is_empty() { "rebuilt" } else { "unchanged" };
            tracing::debug!(
                parsed,
                total,
                extracted,
                retracted,
                derived,
                src_ms,
                der_ms,
                slowest_rel = %slowest_rel,
                slowest_ms,
                reason = %reason,
                "[tick] files {parsed}/{total} parsed, +{extracted} -{retracted} source facts, derived {derived} | source {src_ms:.1}ms, derived {der_ms:.1}ms, slowest {slowest_rel}={slowest_ms}ms, trigger=full, reason={reason}"
            );
            crate::verdict::verdict(
                "tick-verdict",
                &format!("[tick-verdict] full derived_ms={der_ms:.1} slowest_rel={slowest_rel} slowest_ms={slowest_ms} trigger=full reason={reason}"),
                &[("kind", "full"), ("derived_ms", &format!("{der_ms:.1}")),
                  ("slowest_rel", &slowest_rel), ("slowest_ms", &slowest_ms.to_string()),
                  ("trigger", "full"), ("reason", reason)],
            );
        }
        // Only the edges actually rebuilt this tick are dirty for the cond
        // cache (scoped or full); the digest check inside still skips the
        // Tarjan for edges whose rows didn't move.
        let cond_edges = cond_edges_for(&edges, &scc_rules);
        if !scc_rules.is_empty() || !node2vec_rules.is_empty() || !edges.is_empty() {
            crate::activity::set(crate::activity::Phase::Operators, "");
        }
        self.refresh_cond_cache(&cond_edges, &dirty_edges)?;
        for (r, cs) in &seed_rules { self.eval_closure_seed_rule(r, cs)?; }
        for r in &scc_rules { self.eval_scc_rule(r)?; }
        for r in &node2vec_rules { self.eval_node2vec_rule(r)?; }
        // Post-stratum: derived rules that read an operator head. The heads just
        // filled (scc/node2vec evals above), so these now lower correctly. On a
        // warm tick no derived rel moved, so no operator edge moved, so the heads
        // (deterministic) are unchanged and the post rels already hold the right
        // rows. On a scoped tick, redo only post rels whose inputs moved — a
        // changed source rel they read, OR an operator head whose input edge was
        // rebuilt (same seed logic as tick_paths).
        if !strata.post_rels.is_empty() {
            if need_full || extract_changed {
                self.rebuild_derived(&strata.post_rules, &strata.post_rels)?;
            } else if changed || !program_scope.is_empty() {
                let mut seed = changed_source_rels.clone();
                seed.extend(program_scope.iter().cloned());
                for r in scc_rules.iter().chain(node2vec_rules.iter()) {
                    let edge = r.scc_edge().or_else(|| r.node2vec_edge())
                        .expect("operator rule has an scc/node2vec edge");
                    if affected.contains(edge) || changed_source_rels.contains(edge) {
                        seed.insert(r.head.rel.clone());
                    }
                }
                let aff_post = affected_derived(&derived_rules, &seed);
                let sub_post_rules: Vec<&Rule> = strata.post_rules.iter().copied()
                    .filter(|r| aff_post.contains(&r.head.rel)).collect();
                let sub_post_rels: Vec<String> = strata.post_rels.iter()
                    .filter(|r| aff_post.contains(*r)).cloned().collect();
                if !sub_post_rels.is_empty() {
                    self.rebuild_derived(&sub_post_rules, &sub_post_rels)?;
                }
            }
        }
        // Phase 5: the `type_decl_row` sink is now fully derived, so persist its
        // rows to `_shapes` (digest-guarded). A computed `rel name: shape.` that
        // referenced it resolves at the NEXT tick's declare — the one-tick delay.
        self.persist_type_decl_shapes(prog)?;
        // The priming tick skips `?` evaluation: it exists only to derive the
        // coordinates a data-driven scan / repo-sink reads on the real tick.
        // A quiet tick (the daemon's reactive path) also skips PRINTING the
        // query tables — the RPC `query` capture is the daemon's read path, so
        // re-rendering every `?` table to daemon.log on each tick is pure noise
        // that grows the log without bound. Foreground `dl prog.dl` / `--watch`
        // pass quiet=false and still print.
        if !self.prime_tick && !quiet {
            crate::activity::set(crate::activity::Phase::Query, "");
            for item in &prog.items {
                // Each `?` query is independent: a failed or malformed query
                // (unknown rel, bad point-query shape) reports and the rest still
                // run. Aborting the whole chain on the first failure hid every
                // later answer behind one broken question.
                if let Item::Query(q) = item {
                    if let Err(e) = self.run_query(q, &closures) {
                        let rel = q.head.rel.as_str();
                        tracing::warn!(rel = %rel, error = %e, "[dl] query `{rel}` failed: {e}");
                    }
                }
            }
        }
        self.run_gens(prog, quiet)?;
        // External sinks (`repo` pulls + `checkout` sweeps) used to drain here.
        // They hit the network + rewrite checkouts, so a `?` / --check / LSP /
        // MCP read tick must NOT run them. The daemon's poll loop calls
        // `drain_external_sinks` off-tick on its cadence; one-shot CLI runs opt
        // in via `--apply` / `DL_APPLY_SINKS=1`.
        if self.dropped > 0 {
            let dropped = self.dropped;
            tracing::warn!(
                dropped,
                "[checked-type] dropped {dropped} rows failing file/dir/path checks"
            );
            self.dropped = 0;
        }
        if tick_audit() {
            let mut counts: Vec<(String, i64)> = Vec::new();
            for rel in self.rels.keys() {
                let n: i64 = self.db.conn().query_row(
                    &format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?;
                counts.push((rel.clone(), n));
            }
            counts.sort_by(|a, b| a.0.cmp(&b.0));
            let rel_count = counts.len();
            tracing::debug!(rel_count, "[audit] {rel_count} relation(s)");
            for (rel, n) in &counts {
                tracing::debug!(rel = %rel, n, "[audit]   {rel}: {n}");
            }
        }
        // @next staging: now that the tick has converged, each @next rule's body
        // is evaluated over tick-T's relations and its head rows land in
        // carry_<rel> at tx = cur_tx + 1. Then the clock advances. The carried
        // rows surface as the live rel at the START of the next tick (Edit 2).
        if !next_rules.is_empty() {
            self.rebuild_next(&next_rules, &next_rels, cur_tx)?;
        }
        // @async request emission: now that the tick converged, each @async rule's
        // body binds the request args; one `pending_effect` row per solution lands
        // (idempotent on its digest id). The daemon's `drain_effects` runs them
        // off-tick and the response surfaces in the head rel at a later tick.
        if !async_rules.is_empty() {
            self.rebuild_async(prog, &async_rules, cur_tx)?;
        }
        // The carry clock advances once per tick that has any temporal rule, so
        // `req_tx` and the next carry generation track the same coordinate.
        if !next_rules.is_empty() || !async_rules.is_empty() {
            self.set_tx(cur_tx + 1)?;
        }
        self.last_n1 = self.db.tick_end();
        self.flush_write_ledger(tick)?;

        // Settle report: peek whether any @next carry just staged at cur_tx+1
        // differs from the live rel (non-destructively — load_carry that applies
        // it runs at the START of the next tick), and count non-stream effects
        // still owed a drain.
        let mut staged_next = false;
        for rel in &next_rels {
            if let Some(meta) = self.rels.get(rel).cloned() {
                staged_next |= self.carry_differs(rel, &meta, cur_tx + 1)?;
            }
        }
        let changed_rels: Vec<String> = changed_source_rels.into_iter().collect();
        let inflight_effects = self.inflight_nonstream(prog)?;
        // One perf-log record per tick: what moved, what re-derived, the whole
        // tick's wall cost. `derived_strategy` names WHY everything re-ran
        // ("full" = program/blank-slate/carry; "scoped" = only reachable rels;
        // "unchanged" = nothing propagated) — the fact the spike hunter wants.
        let derived_strategy = if need_full { "full" }
            else if changed || !program_scope.is_empty() { "scoped" }
            else { "unchanged" };
        crate::perflog::emit_tick(&crate::perflog::TickRec {
            tick: crate::activity::snapshot().tick,
            kind: "full",
            files_parsed: recon.parsed,
            files_total: recon.total,
            extracted: recon.extracted,
            retracted: recon.retracted,
            derived_strategy,
            derived_rebuilt: &self.last_derived_rebuilt,
            changed_rels: &changed_rels,
            full_reason: full_reason.as_deref(),
            effects_inflight: inflight_effects,
            total_ms: t_tick.elapsed().as_millis() as u64,
        });
        self.clear_exe_identity_cache();
        Ok(TickReport {
            changed,
            derived_moved,
            changed_rels,
            staged_next,
            inflight_effects,
            cold_pending: false,
            cold_staged: Vec::new(),
        })
    }

    /// Reactive tick driven by a known set of changed paths (from the file
    /// watcher): reconciles only those paths, never walking or statting the
    /// tree. Only WORK source rules participate; route git-rev changes to `tick`.
    // ARCH {"url":"engine/41-tick-paths","role":"orchestrator"}
    #[tracing::instrument(skip_all, fields(n_changed = changed.len()), level = "info")]
    pub fn tick_paths(&mut self, prog: &Program, changed: &[PathBuf], quiet: bool) -> Result<()> {
        self.tick_paths_with_policy(prog, changed, quiet, PathTickFallbackPolicy::Allow)
            .map(|_| ())
    }

    /// Path-scoped tick with an explicit widening policy and structured result.
    /// `Forbid` reports the fallback boundary without executing the full tick.
    pub fn tick_paths_with_policy(
        &mut self,
        prog: &Program,
        changed: &[PathBuf],
        quiet: bool,
        fallback_policy: PathTickFallbackPolicy,
    ) -> Result<PathTickOutcome> {
        let _tick_started = std::time::Instant::now();
        self.rev_cache.clear();
        self.extraction_drops.clear();
        self.shape_diags.clear();
        CMD_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        self.db.tick_begin();
        self.adjacency_cache.borrow_mut().take();
        // Every `self.tick(...)` fallback below MUST pass the ORIGINAL,
        // pre-desugar program: `tick_report` runs its own
        // `desugar_mixed_rels` pass, and re-desugaring an already-rewritten
        // program would see its synthesized `__src`/`__drv` decls and trip
        // the reserved-twin-suffix guard. Keep the untouched reference under
        // its own name before shadowing `prog` below.
        let original_prog = prog;
        let desugared = desugar::desugar_mixed_rels(prog)?;
        let prog: &Program = desugared.as_ref().map(|(p, _)| p).unwrap_or(prog);
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let closures = closure_map(&rules);
        // Meta tables (incl. `_shapes`) before declare (Phase 5, see tick_report).
        self.ensure_meta()?;
        self.declare_all(prog, &closures)?;

        // A `repo`-sink program pulls dynamically; the drain runs only on the
        // full-tick path, so an incremental tick defers to the full tick. (The
        // sink's body depends on derived relations whose churn is otherwise
        // invisible to the path-scoped reconcile.) A data-driven scan (variable
        // repo/rev) reads last tick's coordinate relation at reconcile time too,
        // so it also defers.
        if rules
            .iter()
            .any(|r| r.is_repo_sink() || r.is_checkout_sink() || scan_has_var_coords(r))
        {
            tracing::debug!(
                "full-tick fallback: program has a repo-sink, checkout-sink, or data-driven scan"
            );
            return self.path_tick_fallback(
                original_prog,
                quiet,
                fallback_policy,
                "dynamic-scan-or-repo-sink",
                "full-tick",
            );
        }

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        let all_derived: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && r.closure_edge().is_none() && r.scc_edge().is_none()
                && r.node2vec_edge().is_none()
                && self.rels.contains_key(&r.head.rel)).collect();
        let scc_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.scc_edge().is_some()).collect();
        let node2vec_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.node2vec_edge().is_some()).collect();
        check_stratification(&all_derived, &closures)?;
        let (seed_rules, derived_rules) = split_seed_and_derived(&all_derived, &closures)?;
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules { if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); } }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules { if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); } }
        let strata = partition_derived_strata(&derived_rules, &derived_rels, &scc_rules, &node2vec_rules)?;
        let edges: Vec<&str> = dedup_edges(&closures);
        self.create_auto_indexes(&derived_rules, &closures)?;

        // An edited source rule invalidates extractions everywhere, not just at
        // the delta paths: fall back to the full tick, whose reconcile
        // re-extracts the dirty relation's entire file set and persists the new
        // rule digests.
        if !self.source_rule_digests(&source_rules)?.0.is_empty() {
            tracing::debug!("full-tick fallback: source rule edited");
            return self.path_tick_fallback(
                original_prog,
                quiet,
                fallback_policy,
                "source-rule-edit",
                "full-source-reconcile",
            );
        }
        // Same for the derived layer: an edited derived rule or ground fact
        // rebuilds everything, which is the full tick's job.
        let der_digest = derived_program_digest(&derived_rules, &seed_rules, &edges);
        if self.load_rel_digest("derived:program")? != Some(der_digest) {
            tracing::debug!("full-tick fallback: derived rule/ground-fact edited");
            return self.path_tick_fallback(
                original_prog,
                quiet,
                fallback_policy,
                "derived-program-edit",
                "full-derived-rebuild",
            );
        }
        // A changed path outside self.root (a config or dynamically-registered
        // repo's source edit) can't be reconciled by the path-scoped loop below,
        // which strips against self.root only. Fall back to the full tick so
        // every folder in view stays reactive, not just the self worktree.
        if changed.iter().any(|p| !p.starts_with(&self.root)) {
            tracing::debug!("full-tick fallback: changed path outside self.root (#6a)");
            return self.path_tick_fallback(
                original_prog,
                quiet,
                fallback_policy,
                "changed-path-outside-root",
                "full-tick",
            );
        }
        if self.path_delta_needs_full_source_reconcile(&source_rules, changed)? {
            tracing::debug!(
                "full-tick fallback: changed path feeds a keyed/merged source relation"
            );
            return self.path_tick_fallback(
                original_prog,
                quiet,
                fallback_policy,
                "keyed-or-merged-source",
                "full-source-reconcile",
            );
        }

        self.db.clear_write_ledger();
        self.write_ledger.borrow_mut().clear();
        let tick = self.tick_seq.get() + 1;
        self.tick_seq.set(tick);

        // Win D: `module_rels_needed` (not the narrower `module_rels_used`) so
        // a program reading only type_link/call_edge still gets the module
        // family's incremental refresh. Its resolver narrowing depends on
        // `module_edge_rev` even when the program never names a module_* rel.
        let wants_module_rels = module_rels_needed(prog);
        let path_reconcile =
            self.reconcile_source_paths(&source_rules, &source_rels, changed, wants_module_rels)?;
        let crate::engine::path_reconcile::PathSourceReconcile {
            mut changed_facts,
            mut changed_source_rels,
            module_delta_paths,
            module_full_work,
            node_delta_paths,
            seen,
            extracted,
            retracted,
            npaths,
        } = path_reconcile;

        // A changed file's bytes moved, but did its extracted rows? Prune the
        // source rels whose content digest is unchanged (comment/format edits),
        // so they do not propagate a rebuild. v4's `Replay` at relation grain.
        let files_changed = changed_facts;
        // Pre-extract RelKinds (scip), mirroring the full tick's ordering: when
        // index.scip moved in the same delta as source files, its reload must
        // precede the extract families inside the guard below. An
        // index.scip-only delta keeps the old semantics (rows load and mark
        // changed; extraction re-reads them on its next run).
        for k in crate::rels::rel_kinds() {
            if k.pre_extract() && k.used(prog) && k.dirty(self, &seen) && k.refresh(self)? {
                for r in k.rels() { changed_source_rels.insert(r.to_string()); }
                changed_facts = true;
            }
        }
        // The file set itself (path/hash/rev) is the built-in `file`/`content`/
        // `rev` relations, so any file change makes them changed inputs: refresh
        // them and mark them changed so rules that join `file` re-derive. This is
        // separate from the per-source-rel digest prune above (a comment edit
        // leaves `fn` unchanged but does change the file's content hash).
        if files_changed {
            self.refresh_builtin_rels()?;
            for b in BUILTIN_RELS { changed_source_rels.insert(b.to_string()); }
            // D5.5 rev-retraction sweep — same seam and rationale as the full
            // tick (see `tick`'s call site): only reachable when `_file`
            // actually moved this tick, since a rev can only disappear from
            // it as part of a file/rev delta.
            self.sweep_gone_revs()?;
        }
        let module_dependency_before = self.module_resolution_dependency_digest("WORK");
        let module_outcome = if wants_module_rels {
            crate::rels::ModuleFamily
                .refresh_delta(self, module_full_work, &module_delta_paths)?
        } else {
            crate::rels::RefreshOutcome::Unchanged
        };
        let module_dependency_after = self.module_resolution_dependency_digest("WORK");
        let module_dependency_changed = module_dependency_before != module_dependency_after;
        if module_outcome.moved() {
            for m in MODULE_RELS {
                changed_source_rels.insert(m.to_string());
            }
            changed_facts = true;
        }
        let path_refresh_context = crate::rels::PathRefreshContext {
            changed_paths: &node_delta_paths,
            module_dependency_changed,
            module_full_refresh: module_full_work,
        };
        if files_changed || module_outcome.moved() {
            // The ExtractFamily registry, minus module (dispatched below via
            // `ModuleFamily::refresh_delta` — it must also fire on a
            // manifest-only change, outside this files-changed guard). Each
            // family reports whether its input digest moved (perf gap C): a
            // changed file OUTSIDE a family's corpus (e.g. an edited .dl or
            // .md under a type-graph program) no longer marks the family's
            // rels changed, so their derived dependents stay put. `spine`
            // (post-node) reports true unconditionally — conservative mark,
            // as before.
            // Prime the same generation-local bundle used by the full tick so
            // daemon/LSP edits also parse a changed Rust file once for every
            // requested analysis family.
            super::extract::prime_analysis_bundles(self, prog)?;
            for fam in crate::rels::extract_families_paths_pre_node() {
                if fam.used(prog) && fam.refresh_paths(self, &path_refresh_context)?.moved() {
                    for r in fam.rels() {
                        changed_source_rels.insert(r.to_string());
                    }
                }
            }
            // Node rels write into the spine meta tables, so refresh them BEFORE
            // the spine projection (else this tick's `ref`/`string` miss node spans).
            // Path-scoped: re-walk ONLY this tick's changed files; the other
            // files' node/child rows are untouched.
            if files_changed && node_rels_used(prog) && self.refresh_node_rels_delta(&node_delta_paths)? {
                for n in NODE_RELS {
                    changed_source_rels.insert(n.to_string());
                }
                changed_facts = true;
            }
            if files_changed {
                for fam in crate::rels::extract_families_post_node() {
                    if fam.used(prog) && fam.refresh(self)?.moved() {
                        for r in fam.rels() {
                            changed_source_rels.insert(r.to_string());
                        }
                    }
                }
            }
        }
        // The clock fires on time, not on file change, so refresh it outside the
        // `files_changed` guard. It only re-derives dependents on a tick where the
        // row set actually moves (a boundary crossing or the clear after one), so a
        // quiet poll tick stays a no-op.
        if every_rels_used(prog) && self.refresh_every(&every_intervals(prog))? {
            changed_source_rels.insert("every".to_string());
            changed_facts = true;
        }
        if clock_rels_used(prog) && self.refresh_clock(&clock_periods(prog))? {
            changed_source_rels.insert("clock".to_string());
            changed_facts = true;
        }
        // The RelKind families. Most self-diff and re-run every incremental tick
        // (a no-op returns false); `dirty(&seen)` lets a family opt out — scip
        // gates its index reload on `index.scip` being in the changed set so a
        // source edit never forces a full SCIP reload. Every save can move the
        // worktree diff, so the `changed` family re-reads whenever the program
        // joins it; the false-on-no-op result keeps the rebuild scope tight.
        for k in crate::rels::rel_kinds() {
            if k.pre_extract() { continue; } // ran before the extract families
            if k.used(prog) && k.dirty(self, &seen) && k.refresh(self)? {
                for r in k.rels() { changed_source_rels.insert(r.to_string()); }
                changed_facts = true;
            }
        }
        if daemon_rels_used(prog) { self.refresh_daemon_rels()?; }
        if changed_source_rels.is_empty() { changed_facts = false; }

        // Cold start (or a derived rel that never completed a rebuild pass)
        // needs a full rebuild; otherwise rebuild only the derived rels
        // dependency-reachable from what changed, plus the closures over
        // affected edges. Untouched chains are left intact. `tick_paths` falls
        // back to the full `tick()` whenever the derived program digest moved
        // (see the guard above), so unlike the full tick, only the P1
        // completion-marker check and the closure check can force `need_full`
        // here.
        let incomplete_derived = self.derived_incomplete_rels(&derived_rels)?;
        let empty_closure_edge = self.first_empty_closure_edge(&edges)?;
        let need_full = !incomplete_derived.is_empty() || empty_closure_edge.is_some();
        let full_reason: Option<String> = if !incomplete_derived.is_empty() {
            Some(format!("derived-missing:{}", incomplete_derived.join(",")))
        } else {
            empty_closure_edge.as_ref().map(|edge| format!("closure-missing:{edge}"))
        };
        let mut rebuilt: Vec<String> = Vec::new();
        // Edges whose source/derived relation was rebuilt this tick; only these
        // are re-considered by the cond cache (the rest are reused untouched).
        let mut dirty_edges: HashSet<&str> = HashSet::new();
        // Pre-stratum rebuild only (the post-stratum, which reads operator heads,
        // runs after the operator evals below). `affected` carries the changed
        // derived set forward so the post-stratum knows which operator inputs moved.
        let mut affected: HashSet<String> = HashSet::new();
        if need_full {
            self.rebuild_derived(&strata.pre_rules, &strata.pre_rels)?;
            self.rebuild_closures(&edges)?;
            rebuilt = strata.pre_rels.clone();
            dirty_edges = edges.iter().copied().collect();
        } else if changed_facts {
            affected = affected_derived(&derived_rules, &changed_source_rels);
            let sub_rules: Vec<&Rule> = strata.pre_rules.iter().copied()
                .filter(|r| affected.contains(&r.head.rel)).collect();
            let sub_rels: Vec<String> = strata.pre_rels.iter()
                .filter(|r| affected.contains(*r)).cloned().collect();
            self.rebuild_derived(&sub_rules, &sub_rels)?;
            let aff_edges: Vec<&str> = edges.iter().copied()
                .filter(|e| affected.contains(*e) || changed_source_rels.contains(*e)).collect();
            self.rebuild_closures(&aff_edges)?;
            dirty_edges = aff_edges.iter().copied().collect();
            rebuilt = sub_rels;
        }

        if !quiet {
            let what = if need_full { "ALL".to_string() }
                       else if rebuilt.is_empty() { "none".to_string() }
                       else { rebuilt.join(",") };
            let (slowest_rel, slowest_ms) = self.slowest_stmt_ms();
            let trigger = format!("file-event({npaths})");
            tracing::debug!(
                npaths,
                extracted,
                retracted,
                what = %what,
                slowest_rel = %slowest_rel,
                slowest_ms,
                trigger = %trigger,
                "[tick] {npaths} path(s) changed, +{extracted} -{retracted} source facts, rebuilt derived: {what}, slowest {slowest_rel}={slowest_ms}ms, trigger={trigger}"
            );
            crate::verdict::verdict(
                "tick-summary",
                &format!("[tick-verdict] incremental slowest_rel={slowest_rel} slowest_ms={slowest_ms} trigger={trigger}"),
                &[("kind", "incremental"), ("slowest_rel", &slowest_rel),
                  ("slowest_ms", &slowest_ms.to_string()), ("trigger", &trigger)],
            );
        }
        let cond_edges = cond_edges_for(&edges, &scc_rules);
        self.refresh_cond_cache(&cond_edges, &dirty_edges)?;
        for (r, cs) in &seed_rules { self.eval_closure_seed_rule(r, cs)?; }
        for r in &scc_rules { self.eval_scc_rule(r)?; }
        for r in &node2vec_rules { self.eval_node2vec_rule(r)?; }
        // Post-stratum rebuild (rules reading an operator head). The heads filled
        // just above. On a full rebuild, redo every post rel; on an incremental
        // tick, redo only those whose inputs moved — a changed source/derived rel
        // they read, OR an operator head whose input edge was rebuilt this tick.
        if !strata.post_rels.is_empty() {
            if need_full {
                self.rebuild_derived(&strata.post_rules, &strata.post_rels)?;
            } else if changed_facts {
                let mut seed = changed_source_rels.clone();
                for r in scc_rules.iter().chain(node2vec_rules.iter()) {
                    let edge = r.scc_edge().or_else(|| r.node2vec_edge())
                        .expect("operator rule has an scc/node2vec edge");
                    if affected.contains(edge) || changed_source_rels.contains(edge) {
                        seed.insert(r.head.rel.clone());
                    }
                }
                let aff_post = affected_derived(&derived_rules, &seed);
                let sub_post_rules: Vec<&Rule> = strata.post_rules.iter().copied()
                    .filter(|r| aff_post.contains(&r.head.rel)).collect();
                let sub_post_rels: Vec<String> = strata.post_rels.iter()
                    .filter(|r| aff_post.contains(*r)).cloned().collect();
                if !sub_post_rels.is_empty() {
                    self.rebuild_derived(&sub_post_rules, &sub_post_rels)?;
                }
            }
        }
        // Independent `?` queries: one failure reports and the chain continues.
        // Quiet (daemon reactive) ticks skip PRINTING — the RPC `query` capture
        // is the read path; re-rendering the tables to daemon.log every tick is
        // unbounded noise (see the full-tick loop above).
        if !quiet {
            for item in &prog.items {
                if let Item::Query(q) = item {
                    if let Err(e) = self.run_query(q, &closures) {
                        let rel = q.head.rel.as_str();
                        tracing::warn!(rel = %rel, error = %e, "[dl] query `{rel}` failed: {e}");
                    }
                }
            }
        }
        // Phase 5: persist the derived-shape sink (digest-guarded), same as the
        // full tick — a computed `rel name: shape.` resolves next tick.
        self.persist_type_decl_shapes(prog)?;
        self.run_gens(prog, quiet)?;
        if self.dropped > 0 {
            let dropped = self.dropped;
            tracing::warn!(dropped, "[checked-type] dropped {dropped} rows");
            self.dropped = 0;
        }
        self.last_n1 = self.db.tick_end();
        self.flush_write_ledger(tick)?;
        // Surface a slow incremental tick in the LSP server log so live
        // dogfooding catches a perf regression (e.g. a CST refresh that
        // silently went full-corpus). Gated on `tick_log_ms()` (env
        // `DL_TICK_LOG_MS`, default 250ms) so a normal fast tick is silent.
        let tick_ms = _tick_started.elapsed().as_secs_f64() * 1000.0;
        if tick_ms >= tick_log_ms() {
            let changed_paths = changed.len();
            tracing::debug!(
                tick_ms,
                changed_paths,
                "[tick] incremental tick took {tick_ms:.1}ms over {changed_paths} changed path(s)"
            );
        }
        self.last_derived_rebuilt = rebuilt.clone();
        crate::perflog::emit_tick(&crate::perflog::TickRec {
            tick: crate::activity::snapshot().tick,
            kind: "incremental",
            files_parsed: npaths,
            files_total: npaths,
            extracted,
            retracted,
            // `need_full` (not just `rebuilt.is_empty()`) so a full rebuild
            // triggered by a missing completion marker/closure edge is
            // labeled "full", not mislabeled "scoped" just because `rebuilt`
            // happened to be non-empty.
            derived_strategy: if need_full { "full" } else if rebuilt.is_empty() { "unchanged" } else { "scoped" },
            derived_rebuilt: &rebuilt,
            changed_rels: &[],
            full_reason: full_reason.as_deref(),
            effects_inflight: 0,
            total_ms: tick_ms as u64,
        });
        self.clear_exe_identity_cache();
        Ok(PathTickOutcome::Incremental)
    }

    fn path_tick_fallback(
        &mut self,
        prog: &Program,
        quiet: bool,
        policy: PathTickFallbackPolicy,
        reason: &'static str,
        scope: &'static str,
    ) -> Result<PathTickOutcome> {
        if policy == PathTickFallbackPolicy::Forbid {
            return Ok(PathTickOutcome::ForbiddenFallback { reason, scope });
        }
        self.tick(prog, quiet)?;
        Ok(PathTickOutcome::FullFallback { reason, scope })
    }
}
