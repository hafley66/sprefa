//! The reactive tick orchestrator: `tick` (full) and `tick_paths` (incremental),
//! lifted out of `engine/mod.rs` to shrink the file an AI re-reads each session
//! (engine breakdown Stage 6). Pure relocation — the bodies are unchanged. As a
//! child module of `engine`, this file reaches `Engine`'s private fields,
//! helpers (reconcile_sources / rebuild_derived / refresh_* / ...), and types
//! directly; both methods stay `pub` (the daemon + CLI drive them), so no
//! visibility change is needed for the lift.

use super::*;

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
}

impl TickReport {
    /// Settled = this tick produced no NON-timer motion and nothing is pending.
    /// `every`/`clock`/`@stream` are steady-state and excluded, so a
    /// timer-driven program still reports settled at a quiet point instead of
    /// spinning forever.
    pub fn is_settled(&self) -> bool {
        !self.derived_moved
            && self.changed_rels.iter().all(|r| is_timer_rel(r))
            && !self.staged_next
            && self.inflight_effects == 0
    }
}

/// The rels whose motion is a recurring timer, not real progress.
pub fn is_timer_rel(rel: &str) -> bool {
    rel == "every" || rel == "clock"
}

impl Engine {
    /// One reactive tick, discarding the settle report (the common path). See
    /// `tick_report` for the settle/quiescence driver.
    #[tracing::instrument(skip_all, level = "info")]
    pub fn tick(&mut self, prog: &Program, quiet: bool) -> Result<()> {
        self.tick_report(prog, quiet).map(|_| ())
    }

    /// One reactive tick: declare, reconcile sources incrementally, rebuild
    /// derived only if a source fact changed, then run queries. Returns a
    /// `TickReport` of what moved (`dl --settle` drives this to a fixpoint).
    #[tracing::instrument(skip_all, level = "info")]
    pub fn tick_report(&mut self, prog: &Program, quiet: bool) -> Result<TickReport> {
        self.rev_cache.clear();
        self.extraction_drops.clear();
        CMD_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        self.db.tick_begin();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i {
            Item::Rule(r) => Some(r), _ => None,
        }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        // `repo`-sink rules: head rel `repo`. Drained post-fixpoint (clone +
        // register), never inserted by reconcile/rebuild (which would wipe the
        // engine-emitted registered set). Must be derived-style: the drain
        // compiles the body as a SELECT, so a scan/match/... body is rejected.
        let repo_sinks: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_repo_sink()).collect();
        for r in &repo_sinks {
            if r.is_source() {
                bail!("repo-sink rule must be derived-style (no scan/match/ast/...); \
                       its body is compiled as a SELECT over already-derived relations");
            }
        }
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
            .filter(|r| r.has_term_extract() && !r.is_source()).collect();
        let all_derived: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && !r.is_repo_sink() && !r.is_next() && !r.is_async()
                && !r.is_stream() && !r.has_term_extract()
                && r.closure_edge().is_none() && r.scc_edge().is_none()
                && r.node2vec_edge().is_none()).collect();
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
                eprintln!("[profile] {label}: {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);
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
        let recon = self.reconcile_sources(&source_rules, &source_rels, &consumed)?;
        phase("reconcile-sources", t);
        // A carried-in @next rel that moved is an EDB change for this tick's
        // derived rules (e.g. a `poll` rule that reads the carried `etag`).
        let mut changed = recon.changed || carry_changed;
        // Per-rel change attribution for the scoped rebuild below (perf gap B):
        // every source/built-in relation whose rows moved this tick lands here,
        // and only the derived rels dependency-reachable from the set re-derive.
        // Baselining each source relation's content digest doubles as the
        // attribution for extraction rels (the digests were already computed
        // here for tick_paths' bytes-moved-rows-didn't prune).
        let mut changed_source_rels: HashSet<String> =
            self.seed_rel_digests(&source_rels)?.into_iter().collect();
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
        for fam in crate::rels::extract_families_pre_node() {
            if !fam.used(prog) { continue; }
            let t = std::time::Instant::now();
            if fam.refresh(self)? {
                for r in fam.rels() { changed_source_rels.insert(r.to_string()); }
            }
            phase(fam.name(), t);
        }
        if node_rels_used(prog) {
            let t = std::time::Instant::now();
            if self.refresh_node_rels()? {
                changed = true;
                for n in NODE_RELS { changed_source_rels.insert(n.to_string()); }
            }
            phase("node-rels", t);
        }
        for fam in crate::rels::extract_families_post_node() {
            if !fam.used(prog) { continue; }
            let t = std::time::Instant::now();
            if fam.refresh(self)? {
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
            if k.used(prog) && k.refresh(self)? {
                changed = true;
                for r in k.rels() { changed_source_rels.insert(r.to_string()); }
            }
        }
        if daemon_rels_used(prog) {
            self.refresh_daemon_rels()?;
            for r in DAEMON_RELS { changed_source_rels.insert(r.to_string()); }
        }
        if effect_rels_used(prog) {
            self.refresh_effect_rels()?;
            for r in EFFECT_RELS { changed_source_rels.insert(r.to_string()); }
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
                self.save_rel_digest(&key, &d)?;
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
                    self.save_rel_digest(&key, &d)?;
                    changed = true;
                    changed_source_rels.insert(rel.to_string());
                }
            }
        }
        let src_ms = t_src.elapsed().as_secs_f64() * 1000.0;

        let t_der = std::time::Instant::now();
        let der_digest = derived_program_digest(&derived_rules, &seed_rules, &edges);
        let derived_moved = self.load_rel_digest("derived:program")? != Some(der_digest);
        // A blank slate, a program-shape change, or an unattributable EDB change
        // (a carried @next rel has no per-rel entry above) rebuilds everything.
        // Otherwise a changed tick scopes the rebuild to the derived rels
        // dependency-reachable from what actually moved (perf gap B — the full
        // tick's twin of tick_paths' affected_derived scoping).
        let need_full = derived_moved || carry_changed
            || self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)?;
        let mut affected: HashSet<String> = HashSet::new();
        let mut dirty_edges: HashSet<&str> = HashSet::new();
        self.last_derived_rebuilt = Vec::new();
        if need_full {
            self.rebuild_derived(&strata.pre_rules, &strata.pre_rels)?;
            self.rebuild_closures(&edges)?;
            dirty_edges = edges.iter().copied().collect();
            self.last_derived_rebuilt = strata.pre_rels.clone();
        } else if changed {
            affected = affected_derived(&derived_rules, &changed_source_rels);
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
            self.rebuild_derived(&strata.pre_rules, &strata.pre_rels)?;
            self.rebuild_closures(&edges)?;
            dirty_edges = edges.iter().copied().collect();
            self.last_derived_rebuilt = strata.pre_rels.clone();
        }
        // persisted only after the rebuild lands, so a failed tick retries
        if derived_moved { self.save_rel_digest("derived:program", &der_digest)?; }
        let der_ms = t_der.elapsed().as_secs_f64() * 1000.0;

        if !quiet {
            eprintln!("[tick] files {}/{} parsed, +{} -{} source facts, derived {} | source {:.1}ms, derived {:.1}ms",
                recon.parsed, recon.total, recon.extracted, recon.retracted,
                if changed { "rebuilt" } else { "unchanged" }, src_ms, der_ms);
        }
        // Only the edges actually rebuilt this tick are dirty for the cond
        // cache (scoped or full); the digest check inside still skips the
        // Tarjan for edges whose rows didn't move.
        let cond_edges = cond_edges_for(&edges, &scc_rules);
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
            } else if changed {
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
        // The priming tick skips `?` evaluation: it exists only to derive the
        // coordinates a data-driven scan / repo-sink reads on the real tick.
        // A quiet tick (the daemon's reactive path) also skips PRINTING the
        // query tables — the RPC `query` capture is the daemon's read path, so
        // re-rendering every `?` table to daemon.log on each tick is pure noise
        // that grows the log without bound. Foreground `dl prog.dl` / `--watch`
        // pass quiet=false and still print.
        if !self.prime_tick && !quiet {
            for item in &prog.items {
                // Each `?` query is independent: a failed or malformed query
                // (unknown rel, bad point-query shape) reports and the rest still
                // run. Aborting the whole chain on the first failure hid every
                // later answer behind one broken question.
                if let Item::Query(q) = item {
                    if let Err(e) = self.run_query(q, &closures) {
                        eprintln!("[dl] query `{}` failed: {e}", q.head.rel);
                    }
                }
            }
        }
        self.run_gens(prog, quiet)?;
        // Drain `repo`-sinks AFTER the fixpoint + gens so their bodies see this
        // tick's derived rows. A pull clones + registers into self.repos; the
        // new repo is scannable / appears in the `repo` builtin on the NEXT tick
        // (mid-tick registration would shift the repo set under derived rules).
        self.run_repo_pulls(&repo_sinks)?;
        // Drain `checkout`-sinks after the pull: this tick's derived
        // `checkout(repo, branch, pr_heads)` rows keep each named repo's checkout
        // current (clone-if-missing + fetch + fast-forward the default branch).
        if rules.iter().any(|r| r.is_checkout_sink()) {
            self.run_checkout_sweeps()?;
        }
        if self.dropped > 0 {
            eprintln!("[checked-type] dropped {} rows failing file/dir/path checks", self.dropped);
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
            eprintln!("[audit] {} relation(s)", counts.len());
            for (rel, n) in &counts { eprintln!("[audit]   {rel}: {n}"); }
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
        Ok(TickReport {
            changed,
            derived_moved,
            changed_rels: changed_source_rels.into_iter().collect(),
            staged_next,
            inflight_effects: self.inflight_nonstream(prog)?,
        })
    }

    /// Reactive tick driven by a known set of changed paths (from the file
    /// watcher): reconciles only those paths, never walking or statting the
    /// tree. Only WORK source rules participate; route git-rev changes to `tick`.
    #[tracing::instrument(skip_all, fields(n_changed = changed.len()), level = "info")]
    pub fn tick_paths(&mut self, prog: &Program, changed: &[PathBuf], quiet: bool) -> Result<()> {
        let _tick_started = std::time::Instant::now();
        self.rev_cache.clear();
        self.extraction_drops.clear();
        CMD_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        self.db.tick_begin();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        // A `repo`-sink program pulls dynamically; the drain runs only on the
        // full-tick path, so an incremental tick defers to the full tick. (The
        // sink's body depends on derived relations whose churn is otherwise
        // invisible to the path-scoped reconcile.) A data-driven scan (variable
        // repo/rev) reads last tick's coordinate relation at reconcile time too,
        // so it also defers.
        if rules.iter().any(|r| r.is_repo_sink() || r.is_checkout_sink() || scan_has_var_coords(r)) {
            tracing::debug!("full-tick fallback: program has a repo-sink, checkout-sink, or data-driven scan");
            return self.tick(prog, quiet);
        }

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        let all_derived: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && r.closure_edge().is_none() && r.scc_edge().is_none()
                && r.node2vec_edge().is_none()).collect();
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
            return self.tick(prog, quiet);
        }
        // Same for the derived layer: an edited derived rule or ground fact
        // rebuilds everything, which is the full tick's job.
        let der_digest = derived_program_digest(&derived_rules, &seed_rules, &edges);
        if self.load_rel_digest("derived:program")? != Some(der_digest) {
            tracing::debug!("full-tick fallback: derived rule/ground-fact edited");
            return self.tick(prog, quiet);
        }
        // A changed path outside self.root (a config or dynamically-registered
        // repo's source edit) can't be reconciled by the path-scoped loop below,
        // which strips against self.root only. Fall back to the full tick so
        // every folder in view stays reactive, not just the self worktree.
        if changed.iter().any(|p| !p.starts_with(&self.root)) {
            tracing::debug!("full-tick fallback: changed path outside self.root (#6a)");
            return self.tick(prog, quiet);
        }

        // WORK source rules with compiled glob matchers
        // The incremental watcher delta covers the self repo's WORK tree only
        // (changed paths under self.root); non-self repos scan via the full tick.
        let mut work_rules: Vec<(&Rule, globset::GlobMatcher)> = Vec::new();
        for r in &source_rules {
            // Variable-coord scans defer to the full tick (guarded above), so
            // every remaining source rule has a literal scan_spec here.
            let spec = scan_spec_of(r)?;
            let (repo, declared, glob) = (str_of(&spec.repo)?, str_of(&spec.rev)?, str_of(&spec.glob)?);
            let is_self = repo.is_empty() || repo == "." || repo == "self";
            if declared == "WORK" && is_self { work_rules.push((*r, globset::Glob::new(&glob)?.compile_matcher())); }
        }

        let prev = self.load_file_meta()?;
        let mut changed_facts = false;
        let mut changed_source_rels: HashSet<String> = HashSet::new();
        let mut module_delta_paths: HashSet<String> = HashSet::new();
        let mut module_full_work = false;
        // Files whose `_file` row changed this tick (modified or deleted),
        // for the path-scoped CST `node`/`child` refresh. A file that the
        // digest prune skipped (content unchanged) is NOT added.
        let mut node_delta_paths: HashSet<String> = HashSet::new();
        let (mut extracted, mut retracted, mut npaths) = (0usize, 0usize, 0usize);
        // Every repo-relative path this tick saw move; `ScipKind::dirty` reads it
        // to gate the SCIP reload on `index.scip` itself changing.
        let mut seen: HashSet<String> = HashSet::new();
        let wants_module_rels = module_rels_used(prog);
        // The watcher only watches this engine's own `--root`, so every
        // incrementally-changed file belongs to the self repo.
        let slug = self.self_slug();

        for p in changed {
            let rel = match p.strip_prefix(&self.root) { Ok(r) => r.to_string_lossy().replace('\\', "/"), Err(_) => continue };
            if !seen.insert(rel.clone()) { continue; }
            let matching: Vec<&Rule> = work_rules.iter().filter(|(_, m)| m.is_match(&rel)).map(|(r, _)| *r).collect();
            if matching.is_empty() {
                if wants_module_rels && module_manifest_path(&rel) { module_full_work = true; }
                continue;
            }
            npaths += 1;
            let abs = self.root.join(&rel);
            if abs.is_file() {
                let bytes = std::fs::read(&abs).unwrap_or_default();
                let h = blake3::hash(&bytes).to_hex().to_string();
                if prev.get(&(slug.clone(), rel.clone(), "WORK".to_string())).map(|t| &t.0) == Some(&h) { continue; }
                if prev.contains_key(&(slug.clone(), rel.clone(), "WORK".to_string())) {
                    module_delta_paths.insert(rel.clone());
                } else {
                    module_full_work = true;
                }
                node_delta_paths.insert(rel.clone());
                retracted += self.retract_path(&slug, &rel, &source_rels)?;
                // Collect located spans across every matching rule for this file and
                // flush once after the loop (one `bump()`), not per-rule. Per-rule
                // flushing trips the N+1 screamer once enough files change.
                let mut where_rows: Vec<(String, String, spine::WhereBytes, Option<String>)> = Vec::new();
                for rule in &matching {
                    let (rows, where_bytes, dropped) = parse_file(rule, &slug, &rel, "WORK", &h, &self.root, &self.rels, &self.rev_index, &[])?;
                    self.dropped += dropped;
                    if dropped > 0 { self.record_extraction_drop(&rel, &rule.head.rel, dropped); }
                    let meta = self.rels.get(&rule.head.rel)
                        .ok_or_else(|| anyhow::anyhow!("unknown relation {}", rule.head.rel))?.clone();
                    extracted += self.insert_source_rows(&rule.head.rel, &meta, &slug, &rel, &rows)?;
                    where_rows.extend(where_bytes.into_iter().map(|(w, t)| (slug.clone(), rel.clone(), w, Some(t))));
                    changed_source_rels.insert(rule.head.rel.clone());
                }
                self.insert_spine_where_bytes(&where_rows)?;
                let (mt, sz) = std::fs::metadata(&abs).ok().map(|m| (mtime_secs(&m), m.len() as i64)).unwrap_or((0, 0));
                self.db.conn().execute(
                    "INSERT INTO _file(repo, path, rev, hash, mtime, size) VALUES (?1, ?2, 'WORK', ?3, ?4, ?5)
                     ON CONFLICT(repo, path, rev) DO UPDATE SET hash=excluded.hash, mtime=excluded.mtime, size=excluded.size",
                    rusqlite::params![slug, rel, h, mt, sz])?;
                changed_facts = true;
            } else {
                if prev.contains_key(&(slug.clone(), rel.clone(), "WORK".to_string())) { module_full_work = true; }
                node_delta_paths.insert(rel.clone());
                retracted += self.retract_path(&slug, &rel, &source_rels)?;
                self.db.conn().execute("DELETE FROM _file WHERE repo = ?1 AND path = ?2 AND rev = 'WORK'", [&slug, &rel])?;
                for rule in &matching { changed_source_rels.insert(rule.head.rel.clone()); }
                changed_facts = true;
            }
        }

        // A changed file's bytes moved, but did its extracted rows? Prune the
        // source rels whose content digest is unchanged (comment/format edits),
        // so they do not propagate a rebuild. v4's `Replay` at relation grain.
        let files_changed = changed_facts;
        if changed_facts {
            changed_source_rels = self.prune_unchanged_by_digest(changed_source_rels)?;
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
            // The ExtractFamily registry, minus module (dispatched below via
            // `ModuleFamily::refresh_delta` — it must also fire on a
            // manifest-only change, outside this files-changed guard). Each
            // family reports whether its input digest moved (perf gap C): a
            // changed file OUTSIDE a family's corpus (e.g. an edited .dl or
            // .md under a type-graph program) no longer marks the family's
            // rels changed, so their derived dependents stay put. `spine`
            // (post-node) reports true unconditionally — conservative mark,
            // as before.
            for fam in crate::rels::extract_families_paths_pre_node() {
                if fam.used(prog) && fam.refresh(self)? {
                    for r in fam.rels() { changed_source_rels.insert(r.to_string()); }
                }
            }
            // Node rels write into the spine meta tables, so refresh them BEFORE
            // the spine projection (else this tick's `ref`/`string` miss node spans).
            // Path-scoped: re-walk ONLY this tick's changed files; the other
            // files' node/child rows are untouched.
            if node_rels_used(prog) && self.refresh_node_rels_delta(&node_delta_paths)? {
                for n in NODE_RELS { changed_source_rels.insert(n.to_string()); }
                changed_facts = true;
            }
            for fam in crate::rels::extract_families_post_node() {
                if fam.used(prog) && fam.refresh(self)? {
                    for r in fam.rels() { changed_source_rels.insert(r.to_string()); }
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
        // The module family's incremental dispatch: the full-work vs
        // path-delta decision lives in `ModuleFamily::refresh_delta`; the
        // per-file loop above computed the classification (manifest / new /
        // deleted file -> full WORK-rev redo, content edit -> path-scoped).
        // Outside the files-changed guard on purpose: a manifest-only change
        // sets `module_full_work` without any matched source file moving.
        if wants_module_rels
            && crate::rels::ModuleFamily.refresh_delta(self, module_full_work, &module_delta_paths)?
        {
            for m in MODULE_RELS { changed_source_rels.insert(m.to_string()); }
            changed_facts = true;
        }
        // The RelKind families. Most self-diff and re-run every incremental tick
        // (a no-op returns false); `dirty(&seen)` lets a family opt out — scip
        // gates its index reload on `index.scip` being in the changed set so a
        // source edit never forces a full SCIP reload. Every save can move the
        // worktree diff, so the `changed` family re-reads whenever the program
        // joins it; the false-on-no-op result keeps the rebuild scope tight.
        for k in crate::rels::rel_kinds() {
            if k.used(prog) && k.dirty(&seen) && k.refresh(self)? {
                for r in k.rels() { changed_source_rels.insert(r.to_string()); }
                changed_facts = true;
            }
        }
        if daemon_rels_used(prog) { self.refresh_daemon_rels()?; }
        if changed_source_rels.is_empty() { changed_facts = false; }

        // Cold start (or empty derived/closure) needs a full rebuild; otherwise
        // rebuild only the derived rels dependency-reachable from what changed,
        // plus the closures over affected edges. Untouched chains are left intact.
        let need_full = self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)?;
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
            eprintln!("[tick] {npaths} path(s) changed, +{extracted} -{retracted} source facts, rebuilt derived: {what}");
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
                        eprintln!("[dl] query `{}` failed: {e}", q.head.rel);
                    }
                }
            }
        }
        self.run_gens(prog, quiet)?;
        if self.dropped > 0 { eprintln!("[checked-type] dropped {} rows", self.dropped); self.dropped = 0; }
        self.last_n1 = self.db.tick_end();
        // Surface a slow incremental tick in the LSP server log so live
        // dogfooding catches a perf regression (e.g. a CST refresh that
        // silently went full-corpus). Gated on `tick_log_ms()` (env
        // `DL_TICK_LOG_MS`, default 250ms) so a normal fast tick is silent.
        let tick_ms = _tick_started.elapsed().as_secs_f64() * 1000.0;
        if tick_ms >= tick_log_ms() {
            eprintln!("[tick] incremental tick took {tick_ms:.1}ms over {} changed path(s)", changed.len());
        }
        Ok(())
    }
}
