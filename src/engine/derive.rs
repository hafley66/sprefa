use super::*;
use crate::lower::txt_tbl;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

const FIXPOINT_ROW_MAX_DEFAULT: usize = 5_000_000;
const SLOW_STMT_AFTER: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdjacencyCacheKey {
    pub edge_relation: String,
    pub first_column: String,
    pub second_column: String,
    pub sym_columns: bool,
    pub row_count: i64,
}

pub(crate) struct AdjacencyCache {
    pub key: AdjacencyCacheKey,
    pub adjacency: Vec<Vec<u32>>,
    pub key_to_node: HashMap<i64, u32>,
    pub node_keys: Vec<i64>,
    pub text_by_node: Option<Vec<String>>,
}

/// One watchdog serves a rebuild, avoiding a sleeper thread for every SQL statement.
struct StmtWatch {
    tx: Sender<StmtWatchEvent>,
    thread: Option<JoinHandle<()>>,
}
enum StmtWatchEvent {
    Begin(String),
    Complete,
    Shutdown,
}

impl StmtWatch {
    fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                let rel = match event {
                    StmtWatchEvent::Begin(rel) => rel,
                    StmtWatchEvent::Shutdown => break,
                    StmtWatchEvent::Complete => continue,
                };
                match rx.recv_timeout(SLOW_STMT_AFTER) {
                    Ok(StmtWatchEvent::Complete) => {}
                    Ok(StmtWatchEvent::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                    Ok(StmtWatchEvent::Begin(_)) => {
                        unreachable!("derived statements do not overlap")
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        tracing::warn!(
                            rel = %rel,
                            "[stmt-slow] began statement for `{rel}`; still running after 10s"
                        );
                        match rx.recv() {
                            Ok(StmtWatchEvent::Complete) => {}
                            Ok(StmtWatchEvent::Shutdown) | Err(_) => break,
                            Ok(StmtWatchEvent::Begin(_)) => {
                                unreachable!("derived statements do not overlap")
                            }
                        }
                    }
                }
            }
        });
        Self {
            tx,
            thread: Some(thread),
        }
    }
    fn begin(&self, rel: &str) {
        let _ = self.tx.send(StmtWatchEvent::Begin(rel.to_string()));
    }
    fn complete(&self) {
        let _ = self.tx.send(StmtWatchEvent::Complete);
    }
}
impl Drop for StmtWatch {
    fn drop(&mut self) {
        let _ = self.tx.send(StmtWatchEvent::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn fixpoint_row_max() -> usize {
    std::env::var("DL_FIXPOINT_ROW_MAX")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(FIXPOINT_ROW_MAX_DEFAULT)
}
fn check_fixpoint_row_budget(rels: &[String], total: usize, max: usize) -> Result<()> {
    if total <= max {
        return Ok(());
    }
    bail!("fixpoint row budget exceeded for recursive relation(s) `{}` after {} rows (limit {}): \
           recursion is growing without converging — add a depth bound like `depth < 64`, see std/entry.dl",
          rels.join(", "), total, max);
}

impl Engine {
    /// Materialize `head(a, b, score) <- node2vec(edge).`: read the 2-col edge
    /// rel, learn one structural vector per node (random walks + skip-gram), store
    /// the vectors in `_node_embeddings` keyed by the edge rel name, and fill the
    /// 3-col head with each node's top-k cosine-nearest neighbors. Excluded from
    /// `rebuild_derived` (the Node2vec body item can't lower to SQL); runs after
    /// the edge rel has materialized. The embed is the cost, so it is guarded:
    /// W1 skips an unchanged graph (digest match), W2 reuses cached vectors for
    /// any of the last N seen edge-digests (branch thrash), only a genuinely new
    /// graph pays `embed_graph`. Cap the graph or run on demand for huge edge sets.
    pub(crate) fn eval_node2vec_rule(&mut self, rule: &Rule) -> Result<()> {
        let edge = rule
            .node2vec_edge()
            .ok_or_else(|| anyhow::anyhow!("eval_node2vec_rule on a non-node2vec rule"))?;
        let head = &rule.head.rel;
        let head_meta = self
            .rels
            .get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != 3 || head_meta.cols.len() != 3 {
            bail!(
                "node2vec head '{head}' must have exactly 3 columns (a, b, score); got {}",
                head_meta.cols.len()
            );
        }
        // Own the head cols now: the recompute path mutates `self`
        // (`node2vec_recomputed`), which conflicts with holding `head_meta`'s
        // immutable borrow of `self.rels` to the end.
        let head_cols: Vec<String> = head_meta.cols.iter().map(|c| c.name.clone()).collect();
        let edge_meta = self
            .rels
            .get(edge)
            .ok_or_else(|| anyhow::anyhow!("node2vec edge relation '{edge}' is not declared"))?;
        if edge_meta.cols.len() < 2 {
            bail!(
                "node2vec edge relation '{edge}' must have at least 2 columns (src, dst); got {}",
                edge_meta.cols.len()
            );
        }
        // Read the first two columns as the directed edge list.
        let (c0, c1) = (
            edge_meta.cols[0].name.clone(),
            edge_meta.cols[1].name.clone(),
        );
        let edges: Vec<(String, String)> = self.db.query_rows(
            edge,
            &format!("SELECT \"{c0}\", \"{c1}\" FROM {}", txt_tbl(edge)),
            &[],
            |r| Ok((cell_as_string(r, 0)?, cell_as_string(r, 1)?)),
        )?;

        // W1 digest-skip: node2vec is a GLOBAL op (walks touch the whole graph),
        // so it is not cheaply incrementalizable. Most git checkouts move file
        // content without moving the call/type edge set, so an order-independent
        // digest of the edge rows lets the common re-tick be a no-op. Same guard
        // the scc/closure operators use (recondense only when rows moved).
        let digest = blake3_edges(&edges);
        let dhex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let dkey = format!("node2vec:{edge}");
        // Vectors for THIS exact digest (W2 keeps the last N digests per graph).
        let have_cur: i64 = self.db.query_one(
            "_node_embeddings",
            "SELECT COUNT(*) FROM _node_embeddings WHERE graph = ?1 AND edge_digest = ?2",
            &[edge.into(), dhex.clone().into()],
            |r| Ok(r.get::<_, i64>(0)?),
        )?;
        let trace = std::env::var("SPREFA_N2V_TRACE").is_ok();
        // Skip when node_sim already reflects this digest and its vectors exist
        // (or the graph is legitimately empty and we recorded the all-zero digest).
        if self.load_rel_digest(&dkey)? == Some(digest) && (edges.is_empty() || have_cur > 0) {
            if trace {
                // @eprintln-ok: SPREFA_N2V_TRACE env-gated debug line is the command's stderr output contract
                eprintln!("[node2vec] graph '{edge}': skip (digest unchanged)");
            }
            return Ok(());
        }
        // Global op, digest-skip already refused above — time only the actual
        // recompute (walks + embedding + KNN fill) into `_stmt_ms` under
        // `node2vec:<edge>`, a sibling of `closure:<edge>`/`scc:<rel>`. The
        // empty-graph branch returns early, so it's timed too (via the `?`
        // helper below), not just the full re-embed path.
        let t = std::time::Instant::now();

        // node_sim is rebuilt for the new digest either way.
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        if edges.is_empty() {
            // Empty graph: drop this graph's whole vector cache, record the
            // all-zero digest so a later empty tick skips.
            self.db.exec_params(
                "_node_embeddings",
                "DELETE FROM _node_embeddings WHERE graph = ?1",
                &[edge.into()],
            )?;
            self.db.exec_params(
                "_node_emb_seen",
                "DELETE FROM _node_emb_seen WHERE graph = ?1",
                &[edge.into()],
            )?;
            if trace {
                // @eprintln-ok: SPREFA_N2V_TRACE env-gated debug line is the command's stderr output contract
                eprintln!("[node2vec] graph '{edge}': empty (cleared)");
            }
            self.save_rel_digest(&dkey, &digest)?;
            self.save_stmt_ms_one(&format!("node2vec:{edge}"), t.elapsed().as_millis() as i64)?;
            return Ok(());
        }

        let cfg = crate::embed::node2vec::N2vConfig::from_env();
        // W2 cache: reuse the stored vectors when we have already embedded this
        // exact edge set (branch A<->B thrash is a hit both ways); only a
        // genuinely new graph pays the embed.
        let pool: Vec<(String, Vec<f32>)> = if have_cur > 0 {
            if trace {
                // @eprintln-ok: SPREFA_N2V_TRACE env-gated debug line is the command's stderr output contract
                eprintln!("[node2vec] graph '{edge}': cache hit (digest seen)");
            }
            let v: Vec<(String, Vec<f32>)> = self
                .db
                .query_rows(
                    "_node_embeddings",
                    "SELECT node, vec FROM _node_embeddings WHERE graph = ?1 AND edge_digest = ?2",
                    &[edge.into(), dhex.clone().into()],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )?
                .into_iter()
                .map(|(n, txt): (String, String)| (n, crate::embed::parse_vec(&txt)))
                .collect();
            v
        } else {
            if trace {
                let edge_count = edges.len();
                // @eprintln-ok: SPREFA_N2V_TRACE env-gated debug line is the command's stderr output contract
                eprintln!("[node2vec] graph '{edge}': re-embed ({edge_count} edges)");
            }
            self.node2vec_recomputed += 1;
            let pool = crate::embed::node2vec::embed_graph(&edges, &cfg);
            if pool.len() > 2000 {
                let n = pool.len();
                tracing::warn!(
                    edge = %edge,
                    n,
                    "[node2vec] brute-force KNN over {n} nodes (O(n^2)); \
                           shrink the edge rel or cap SPREFA_N2V_*"
                );
            }
            // Persist this digest's vectors (one flush, never N+1).
            let dim = cfg.dim as i64;
            let emb_rows: Vec<Vec<Value>> = pool
                .iter()
                .map(|(node, v)| {
                    vec![
                        Value::Text(node.clone()),
                        Value::Text(edge.to_string()),
                        Value::Text(dhex.clone()),
                        Value::Int(dim),
                        Value::Text(crate::embed::encode_vec(v)),
                    ]
                })
                .collect();
            self.db.insert_rows(
                "_node_embeddings",
                &["node", "graph", "edge_digest", "dim", "vec"],
                &emb_rows,
            )?;
            pool
        };

        // LRU: stamp this digest most-recently-used, prune each graph to the N
        // most-recent distinct digests (SPREFA_N2V_CACHE, default 4).
        let seq: i64 = self.db.query_one(
            "_node_emb_seen",
            "SELECT COALESCE(MAX(last_tick), 0) + 1 FROM _node_emb_seen",
            &[],
            |r| Ok(r.get::<_, i64>(0)?),
        )?;
        self.db.exec_params(
            "_node_emb_seen",
            "INSERT INTO _node_emb_seen(graph, digest, last_tick) VALUES (?1, ?2, ?3)
             ON CONFLICT(graph, digest) DO UPDATE SET last_tick = excluded.last_tick",
            &[edge.into(), dhex.clone().into(), seq.into()],
        )?;
        let cap: i64 = std::env::var("SPREFA_N2V_CACHE")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(4);
        self.db.exec_params(
            "_node_embeddings",
            "DELETE FROM _node_embeddings WHERE graph = ?1 AND edge_digest NOT IN
                 (SELECT digest FROM _node_emb_seen WHERE graph = ?1 ORDER BY last_tick DESC LIMIT ?2)",
            &[edge.into(), cap.into()],
        )?;
        self.db.exec_params(
            "_node_emb_seen",
            "DELETE FROM _node_emb_seen WHERE graph = ?1 AND digest NOT IN
                 (SELECT digest FROM _node_emb_seen WHERE graph = ?1 ORDER BY last_tick DESC LIMIT ?2)",
            &[edge.into(), cap.into()],
        )?;

        // Fill the head with KNN pairs (reuses the text path's cosine top-k).
        let k: usize = std::env::var("SPREFA_NODE_SIM_K")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8);
        let rows = knn_rows(&pool, k);
        let cols: Vec<&str> = head_cols.iter().map(|s| s.as_str()).collect();
        self.insert_rel_rows(head, &cols, &rows)?;
        self.save_rel_digest(&dkey, &digest)?;
        self.save_stmt_ms_one(&format!("node2vec:{edge}"), t.elapsed().as_millis() as i64)?;
        Ok(())
    }

    /// Wipe derived tables and run the semi-naive fixpoint to convergence.
    // ARCH {"url":"engine/50-derive","role":"fixpoint"}
    #[tracing::instrument(skip_all, fields(n_rules = derived_rules.len(), n_rels = derived_rels.len()), level = "debug")]
    pub(crate) fn rebuild_derived(
        &self,
        derived_rules: &[&Rule],
        derived_rels: &[String],
    ) -> Result<()> {
        // P3: instrument the whole pass directly (start/stop), rather than
        // relying on the `activity` phase-transition emitter — a quiet
        // one-shot tick (`--check`) may never make the NEXT phase transition
        // that would otherwise close out the "derived" phase record, so the
        // one-shot path silently lost this timing. Emitted unconditionally
        // (daemon and one-shot alike); the per-rel `_stmt_ms` breakdown below
        // is unchanged.
        let t_rebuild = std::time::Instant::now();
        // Evaluate stratum by stratum: each higher stratum's negation reads
        // relations that lower strata have already finished. Within a stratum,
        // rules split into rel-level dependency components (dependencies first).
        // Only a recursive component iterates to a fixpoint; an acyclic one runs
        // each rule exactly once — the loop's extra pass existed only to observe
        // delta=0, which doubled the cost of every expensive non-recursive join.
        // Per-rel statement cost of THIS rebuild — SUM of ms across a rel's
        // rules, passes, and semi-naive delta variants, plus the statement
        // execution count — flushed into `_stmt_ms` once at the end so the
        // perf built-in `stmt_ms` can serve it to rails next tick. SUM (not
        // the old per-statement max): semi-naive splits one rel's fixpoint
        // work across many small delta statements per iteration, and under a
        // max the hot rel reported only its single largest slice. The count
        // distinguishes "one big join" (n=1) from "many fixpoint passes".
        let mut stmt_ms: HashMap<String, (i64, i64)> = HashMap::new();
        let stmt_watch = StmtWatch::start();
        let mut timed = |rel: &str, sql: &str| -> Result<usize> {
            // Temporary wedge tracer: DL_STMT_TRACE=1 names each derived
            // statement BEFORE it runs, so a statement that never returns is
            // identifiable from stderr (the _stmt_ms table only records
            // completed statements).
            if std::env::var_os("DL_STMT_TRACE").is_some() {
                // @eprintln-ok: DL_STMT_TRACE env-gated debug line is the command's stderr output contract
                eprintln!("[stmt-trace] {rel}");
            }
            stmt_watch.begin(rel);
            let t = std::time::Instant::now();
            let result = self.db.exec_derived(rel, sql);
            stmt_watch.complete();
            let n = result?;
            if sql.trim_start().to_ascii_uppercase().starts_with("INSERT") {
                self.record_write(rel, n, "derived");
            }
            // No `flush_pending_syms` here: draining the intern queue after EVERY
            // derived statement is an N+1 on `INSERT _strings` (one flush per
            // rule per fixpoint pass — 108x on a fact-heavy recursive rebuild).
            // The queue is a process-global accumulator, so interns collect
            // across statements for free; the flush is hoisted to each fixpoint
            // PASS boundary in `run_component` / `rebuild_derived_seminaive`.
            // Per-pass (not per-statement, not per-component) is the minimal
            // correct frequency: semi-naive/naive each read the PRIOR pass's
            // rows, so a string a rule mints (a text `+` concat head) must be in
            // `_strings` before the next pass decodes it — but no more often.
            let ms = t.elapsed().as_millis() as i64;
            let e = stmt_ms.entry(rel.to_string()).or_insert((0, 0));
            e.0 += ms;
            e.1 += 1;
            Ok(n)
        };
        // Crash-window fix: the wipe is NO LONGER a single upfront DELETE of
        // every derived rel. That shape left every rel wiped (or partial) from
        // the first component's start until `mark_derived_complete` ran once at
        // the very end — a SIGKILL anywhere in a 30s+ pass left EVERY rel
        // unmarked, so the next boot re-ran the whole pass, re-choked, re-killed
        // (self-perpetuating). Instead each component wipes ONLY its own rels,
        // immediately before it runs, bracketed by unmark (before the wipe) and
        // mark (after it converges), so a kill leaves completed components
        // marked+populated and only the in-flight/unreached ones incomplete.
        // Every rel is wiped exactly once — a rel's rules live in one component
        // of one stratum, asserted below via `wiped`.
        let mut wiped: HashSet<String> = HashSet::new();
        for group in stratify(derived_rules)? {
            for (comp_rules, recursive) in rel_components(&group, derived_rules) {
                let mut comp_rels: Vec<String> = comp_rules
                    .iter()
                    .map(|&ri| derived_rules[ri].head.rel.clone())
                    .collect();
                comp_rels.sort();
                comp_rels.dedup();
                for rel in &comp_rels {
                    if !wiped.insert(rel.clone()) {
                        bail!(
                            "derived rel `{rel}` is headed by rules in more than one \
                             dependency component — rebuild_derived's per-component wipe \
                             assumes one component per rel"
                        );
                    }
                }
                // Ordering here is critical for crash safety: drop the completion
                // markers BEFORE the wipe (a marker must never outlive the rows
                // it vouches for), then wipe, then run, then re-mark only after
                // the component converges. The wipe is per-rel attributable work
                // (a big table's DELETE is not free), so it rides `timed`.
                self.unmark_derived_complete(&comp_rels)?;
                for rel in &comp_rels {
                    // The full-wipe is the class-3 storm signature; every one
                    // is visible at info so an unattributable rebuild can never
                    // hide again.
                    tracing::info!(rel, "[derived] full wipe");
                    timed(rel, &format!("DELETE FROM {}", tbl(rel)))?;
                }
                // Test-only crash-window injection: bail after the wipe+unmark
                // but before the refill+mark, so the interrupted component reads
                // incomplete+empty on the next boot while earlier ones stay
                // marked+populated. Never armed in production (`None`).
                if let Some(fail_rel) = self.fail_rebuild_at_rel.borrow().as_ref() {
                    if comp_rels.iter().any(|rel| rel == fail_rel) {
                        bail!("test-injected crash while rebuilding component `{fail_rel}`");
                    }
                }
                self.run_component(&comp_rules, recursive, derived_rules, &mut timed)?;
                // `run_component` flushes the intern queue at each fixpoint pass
                // boundary (below), so by the time it returns every string this
                // component minted is durable in `_strings` — the completion
                // mark can safely vouch for rows referencing those sym ids.
                // Reached only when the component converged with no error (every
                // path inside `run_component` propagates failure via `?`), so a
                // failed component is never marked complete.
                self.mark_derived_complete(&comp_rels)?;
            }
        }
        // Rels asked for but headed by no rule in `derived_rules` (possible on
        // the scoped path, where `sub_rels` is filtered independently of
        // `sub_rules`) still need the wipe + unmark/mark cycle: a stale populated
        // table with no rule to refill it would otherwise linger, and its marker
        // must reflect that it was reconciled this pass (to zero rows). Same
        // crash-safe order: unmark, wipe, mark.
        let orphan_rels: Vec<String> = derived_rels
            .iter()
            .filter(|rel| !wiped.contains(rel.as_str()))
            .cloned()
            .collect();
        if !orphan_rels.is_empty() {
            self.unmark_derived_complete(&orphan_rels)?;
            for rel in &orphan_rels {
                timed(rel, &format!("DELETE FROM {}", tbl(rel)))?;
            }
            self.mark_derived_complete(&orphan_rels)?;
        }
        self.save_stmt_ms(&stmt_ms)?;
        if !derived_rels.is_empty() {
            let tick = crate::activity::snapshot().tick;
            let detail = format!("{} rel(s): {}", derived_rels.len(), derived_rels.join(", "));
            crate::perflog::emit_phase(
                tick,
                "derived",
                t_rebuild.elapsed().as_millis() as u64,
                &detail,
            );
        }
        Ok(())
    }

    /// Evaluate ONE already-wiped rel-component to convergence. The rels this
    /// component heads have been DELETE-cleared and unmarked by the caller; this
    /// fills them and returns Ok on convergence (the caller then re-marks them
    /// complete). Every internal path — non-recursive one-shot, native graph
    /// walk, naive re-run-to-delta-0 loop, semi-naive delta — propagates failure
    /// via `?`, so a component that errors is never marked complete. Extracted
    /// from `rebuild_derived`'s inner loop so the per-component wipe/mark bracket
    /// wraps a single call instead of straddling several `continue` arms.
    fn run_component(
        &self,
        comp_rules: &[usize],
        recursive: bool,
        derived_rules: &[&Rule],
        timed: &mut impl FnMut(&str, &str) -> Result<usize>,
    ) -> Result<()> {
        let stmts: Vec<(&str, String)> = comp_rules
            .iter()
            .map(|&ri| {
                let sql = lower_rule(derived_rules[ri], &self.rels)?;
                Ok((derived_rules[ri].head.rel.as_str(), sql))
            })
            .collect::<Result<_>>()?;
        crate::activity::detail(format!(
            "derived: {}",
            stmts.iter().map(|(r, _)| *r).collect::<Vec<_>>().join(", ")
        ));
        if !recursive {
            for (rel, sql) in &stmts {
                timed(rel, sql)?;
            }
            // One pass, one flush: drain any strings these rules minted (literal
            // or `+` concat heads) so a later component / the final query decodes
            // them. Empty for the common pass-through-id rule (no bump).
            self.db.flush_pending_syms()?;
            return Ok(());
        }
        // Native graph walks are strict recognizers. A miss falls through
        // to SQL with the original rows; the depth-lattice path must run
        // before the key/merge naive-fallback guard below.
        if self.try_native_walk(comp_rules, derived_rules, timed)? {
            // Native walks carry pass-through node ids (no minted strings), but
            // flush once on the way out to keep the invariant "run_component
            // returns with an empty intern queue" uniform across every path.
            self.db.flush_pending_syms()?;
            return Ok(());
        }
        // Defense twin of typecheck's `recursive-null-pad`: a NULL-padded
        // head in this component would re-insert the same row every
        // iteration (NULL != NULL never dedups), so the delta never
        // reaches 0. Bail instead of hanging.
        for &ri in comp_rules {
            if derived_rules[ri].head_null_pads() {
                bail!(
                    "rule head for `{}` leaves column(s) NULL (`_` or named-arg padding) \
                       inside a recursive component — the fixpoint would not converge; \
                       bind every head column or break the cycle",
                    derived_rules[ri].head.rel
                );
            }
        }
        // Semi-naive fallback shapes: an aggregate head has no single
        // new-row identity to differentiate on (COUNT/SUM/etc. must
        // see the WHOLE group every pass); a `key(...)` head (choice
        // domain, with or without `merge(...)`) narrows the PK below
        // the full row, so "new full row" and "new key" disagree —
        // the anti-join-based delta below assumes a full-row PK.
        // Both stay on the naive re-run-to-delta-0 loop (correct,
        // just not accelerated); everything else gets the delta
        // treatment. A mixed component (some rules eligible, some
        // not) falls back as a whole — simplest correct choice, and
        // these shapes are rare inside a recursive cycle in practice.
        // `DL_NAIVE_FIXPOINT=1` forces every recursive component onto
        // the naive loop — the A/B lever the `fixpoint_full_reruns`
        // counter tests (and any field bisection) flip.
        let naive_fallback = self.force_naive_fixpoint.get()
            || comp_rules.iter().any(|&ri| {
                let r = derived_rules[ri];
                r.has_agg()
                    || self
                        .rels
                        .get(&r.head.rel)
                        .map(|m| m.key.is_some())
                        .unwrap_or(false)
            });
        if naive_fallback {
            let mut iters = 0;
            let mut total_promoted = 0usize;
            let mut comp_rels: Vec<String> = comp_rules
                .iter()
                .map(|&ri| derived_rules[ri].head.rel.clone())
                .collect();
            comp_rels.sort();
            comp_rels.dedup();
            let row_max = fixpoint_row_max();
            loop {
                let mut delta = 0usize;
                for (rel, sql) in &stmts {
                    delta += timed(rel, sql)?;
                }
                // Pass boundary: make this pass's minted strings durable before
                // the next pass re-runs `stmts` and decodes them (a `+` concat
                // head reading the row it produced last pass). One flush per
                // pass, not per statement — the N+1 collapse.
                self.db.flush_pending_syms()?;
                total_promoted = total_promoted.saturating_add(delta);
                check_fixpoint_row_budget(&comp_rels, total_promoted, row_max)?;
                // Every execution after pass 1 is a full-input re-run
                // (the waste semi-naive removes) — count it.
                if iters > 0 {
                    self.fixpoint_full_reruns
                        .set(self.fixpoint_full_reruns.get() + stmts.len());
                }
                iters += 1;
                if delta == 0 {
                    break;
                }
                if iters > 100_000 {
                    bail!("fixpoint did not converge");
                }
            }
            return Ok(());
        }
        self.rebuild_derived_seminaive(comp_rules, derived_rules, timed)
    }

    /// Try each native graph-walk shape in order. Every recognizer miss returns
    /// false, preserving the SQL fallback and the DL_NO_HALT_BFS parity lever.
    fn try_native_walk(
        &self,
        comp_rules: &[usize],
        derived_rules: &[&Rule],
        timed: &mut impl FnMut(&str, &str) -> Result<usize>,
    ) -> Result<bool> {
        if self.try_native_depth_walk(comp_rules, derived_rules, timed)? {
            return Ok(true);
        }
        self.try_native_halt_bfs(comp_rules, derived_rules, timed)
    }

    /// Recognize and execute the depth-lattice forward walk:
    /// `head(node, depth)` or `head(tag, node, depth)` with a `MinBy(depth)`
    /// merge and a recursive `depth < cap` guard. The base rules run first and
    /// provide the depth-carrying frontier; BFS's first visit is the minimum
    /// depth required by the lattice.
    fn try_native_depth_walk(
        &self,
        comp_rules: &[usize],
        derived_rules: &[&Rule],
        timed: &mut impl FnMut(&str, &str) -> Result<usize>,
    ) -> Result<bool> {
        if std::env::var_os("DL_NO_HALT_BFS").is_some() {
            return Ok(false);
        }
        macro_rules! miss {
            ($why:expr) => {{
                if std::env::var_os("DL_BFS_TRACE").is_some() {
                    let rel = &derived_rules[comp_rules[0]].head.rel;
                    let why = $why;
                    tracing::trace!(rel = %rel, why = %why, "[bfs-miss] {rel}: {why}");
                }
                return Ok(false);
            }};
        }
        if comp_rules.is_empty() {
            return Ok(false);
        }
        let head_rel = derived_rules[comp_rules[0]].head.rel.clone();
        if comp_rules
            .iter()
            .any(|&rule_index| derived_rules[rule_index].head.rel != head_rel)
        {
            miss!("multi-rel component");
        }
        let head_meta = self.rels.get(&head_rel).unwrap();
        let Some(head_key) = head_meta.key.as_ref() else {
            miss!("head has no key");
        };
        let Some(crate::ast::MergeFn::MinBy(depth_column)) = head_meta.merge.as_ref() else {
            miss!("head is not MinBy");
        };
        let Some(depth_index) = head_meta
            .cols
            .iter()
            .position(|column| column.name == *depth_column && column.ty == crate::ast::Type::Int)
        else {
            miss!("MinBy column is not an int head column");
        };
        if !matches!(head_meta.cols.len(), 2 | 3) {
            miss!("head is not 2 or 3 columns");
        }

        let non_depth_indices: Vec<usize> = (0..head_meta.cols.len())
            .filter(|&column_index| column_index != depth_index)
            .collect();
        let (tag_index, node_index): (Option<usize>, usize) = match non_depth_indices.as_slice() {
            [node_column]
                if head_meta.cols[*node_column].interned()
                    && head_key.len() == 1
                    && head_key[0] == head_meta.cols[*node_column].name =>
            {
                (None, *node_column)
            }
            [first_column, second_column] => {
                let (tag_column, node_column) = if head_meta.cols[*first_column].ty.textish()
                    && head_meta.cols[*second_column].interned()
                {
                    (*first_column, *second_column)
                } else if head_meta.cols[*second_column].ty.textish()
                    && head_meta.cols[*first_column].interned()
                {
                    (*second_column, *first_column)
                } else {
                    miss!("head tag/node types are not text/sym");
                };
                if head_key.len() != 2
                    || !head_key.contains(&head_meta.cols[tag_column].name)
                    || !head_key.contains(&head_meta.cols[node_column].name)
                {
                    miss!("head key is not tag,node");
                }
                (Some(tag_column), node_column)
            }
            _ => miss!("head node shape is not sym or text,sym"),
        };

        let is_recursive = |rule: &Rule| {
            rule.body
                .iter()
                .any(|body_item| matches!(body_item, BodyItem::Pos(atom) if atom.rel == head_rel))
        };
        let dedup_structurally = |rule_indices: &[usize]| -> Vec<usize> {
            let mut seen_keys = std::collections::HashSet::new();
            rule_indices
                .iter()
                .copied()
                .filter(|&rule_index| seen_keys.insert(derived_rules[rule_index].structural_key()))
                .collect()
        };
        let recursive_indices = dedup_structurally(
            &comp_rules
                .iter()
                .copied()
                .filter(|&rule_index| is_recursive(derived_rules[rule_index]))
                .collect::<Vec<_>>(),
        );
        if recursive_indices.len() != 1 {
            miss!(format!("rec rules = {}", recursive_indices.len()));
        }
        let recursive_rule = derived_rules[recursive_indices[0]];
        if recursive_rule.has_agg() || recursive_rule.temporal.is_some() {
            miss!("agg/temporal rec rule");
        }

        let depth_variable = match &recursive_rule.head.terms[depth_index] {
            Term::Arith {
                op: crate::ast::ArithOp::Add,
                lhs,
                rhs,
            } if matches!(rhs.as_ref(), Term::Int(1)) => match lhs.as_ref() {
                Term::Var(variable) => variable.as_str(),
                _ => miss!("head depth lhs is not a variable"),
            },
            _ => miss!("head depth is not d0 + 1"),
        };
        let node_variable = match &recursive_rule.head.terms[node_index] {
            Term::Var(variable) => variable.as_str(),
            _ => miss!("head node is not a variable"),
        };
        let tag_variable = match tag_index {
            Some(column_index) => match &recursive_rule.head.terms[column_index] {
                Term::Var(variable) => Some(variable.as_str()),
                _ => miss!("head tag is not a variable"),
            },
            None => None,
        };

        let mut self_atom: Option<&Atom> = None;
        let mut edge_atom: Option<&Atom> = None;
        let mut depth_cmp: Option<&Constraint> = None;
        for body_item in &recursive_rule.body {
            match body_item {
                BodyItem::Pos(atom) if atom.rel == head_rel => {
                    if self_atom.replace(atom).is_some() {
                        miss!("duplicate recursive atom");
                    }
                }
                BodyItem::Pos(atom) => {
                    if edge_atom.replace(atom).is_some() {
                        miss!("duplicate edge atom");
                    }
                }
                BodyItem::Cmp(constraint) => {
                    if depth_cmp.replace(constraint).is_some() {
                        miss!("duplicate depth guard");
                    }
                }
                _ => miss!("recursive body contains an unsupported item"),
            }
        }
        let (Some(self_atom), Some(edge_atom), Some(depth_cmp)) = (self_atom, edge_atom, depth_cmp)
        else {
            miss!("recursive body is not self,edge,depth-guard");
        };
        let self_terms_match = match (tag_index, self_atom.terms.as_slice()) {
            (None, [Term::Var(from), Term::Var(depth)]) => {
                from != depth_variable && depth == depth_variable
            }
            (Some(_), [Term::Var(tag), Term::Var(from), Term::Var(depth)]) => {
                Some(tag.as_str()) == tag_variable
                    && from != depth_variable
                    && depth == depth_variable
            }
            _ => false,
        };
        if !self_terms_match {
            miss!("recursive self atom has the wrong variables");
        }
        let from_variable = match &self_atom.terms[node_index] {
            Term::Var(variable) => variable.as_str(),
            _ => miss!("recursive self node is not a variable"),
        };
        if from_variable == depth_variable || from_variable == node_variable {
            miss!("recursive node variables are not distinct");
        }
        match edge_atom.terms.as_slice() {
            [Term::Var(from), Term::Var(to)] if from == from_variable && to == node_variable => {}
            _ => miss!("edge is not from,to"),
        }
        let depth_cap = match (&depth_cmp.lhs, depth_cmp.op, &depth_cmp.rhs) {
            (Term::Var(variable), CmpOp::Lt, Term::Int(cap))
                if variable == depth_variable && *cap >= 0 =>
            {
                *cap
            }
            _ => miss!("depth guard is not d0 < CAP"),
        };
        let edge_rel = edge_atom.rel.clone();
        let edge_meta = self
            .rels
            .get(&edge_rel)
            .ok_or_else(|| anyhow::anyhow!("depth-walk edge relation {edge_rel} not declared"))?;
        if edge_meta.cols.len() != 2 || edge_meta.cols.iter().any(|column| !column.interned()) {
            miss!("edge is not a 2-column sym relation");
        }
        let base_indices = dedup_structurally(
            &comp_rules
                .iter()
                .copied()
                .filter(|&rule_index| !is_recursive(derived_rules[rule_index]))
                .collect::<Vec<_>>(),
        );
        if base_indices.is_empty() {
            miss!("no base rules");
        }
        for &rule_index in &base_indices {
            let base_rule = derived_rules[rule_index];
            if base_rule.has_agg()
                || base_rule.temporal.is_some()
                || !matches!(base_rule.head.terms[depth_index], Term::Int(0))
            {
                miss!("base rule does not seed depth zero");
            }
        }

        for &rule_index in &base_indices {
            let sql = lower_rule(derived_rules[rule_index], &self.rels)?;
            timed(&head_rel, &sql)?;
        }

        let walk_started = std::time::Instant::now();
        let load_started = std::time::Instant::now();
        let (edge_column_zero, edge_column_one) = (
            edge_meta.cols[0].name.clone(),
            edge_meta.cols[1].name.clone(),
        );
        let mut adjacency_cache =
            self.load_edges_keyed_cached(&edge_rel, &edge_column_zero, &edge_column_one, true)?;
        let load_elapsed = load_started.elapsed();
        let AdjacencyCache {
            adjacency,
            key_to_node,
            node_keys,
            ..
        } = &mut *adjacency_cache;
        let mut tag_ids: HashMap<String, u32> = HashMap::new();
        let mut tag_names: Vec<String> = Vec::new();
        let mut starts: Vec<(u32, u32, i64)> = Vec::new();
        let head_columns = head_meta
            .cols
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>();
        let head_select = head_columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                if Some(index) == tag_index && head_meta.cols[index].interned() {
                    format!("(SELECT content FROM _strings WHERE id = \"{column}\")")
                } else {
                    format!("\"{column}\"")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        self.db.for_each_row(
            &head_rel,
            &format!("SELECT {head_select} FROM {}", tbl(&head_rel)),
            &[],
            |row| {
                let node_key = row.get::<_, i64>(node_index)?;
                let depth = row.get::<_, i64>(depth_index)?;
                let tag_value = tag_index
                    .map(|column_index| cell_as_string(row, column_index))
                    .transpose()?;
                let tag_id = match tag_value {
                    Some(tag_text) => match tag_ids.get(&tag_text) {
                        Some(&existing_id) => existing_id,
                        None => {
                            let new_id = tag_names.len() as u32;
                            tag_names.push(tag_text.clone());
                            tag_ids.insert(tag_text, new_id);
                            new_id
                        }
                    },
                    None => 0,
                };
                let node_id = match key_to_node.get(&node_key) {
                    Some(&existing_id) => existing_id,
                    None => {
                        let new_id = adjacency.len() as u32;
                        adjacency.push(Vec::new());
                        node_keys.push(node_key);
                        key_to_node.insert(node_key, new_id);
                        new_id
                    }
                };
                starts.push((tag_id, node_id, depth));
                Ok(())
            },
        )?;

        let bfs_started = std::time::Instant::now();
        let reached = crate::walk::multi_source_walk(&*adjacency, &starts, None, Some(depth_cap));
        let bfs_elapsed = bfs_started.elapsed();
        let output_started = std::time::Instant::now();
        self.db.exec_on(&head_rel, &format!("DELETE FROM {}", tbl(&head_rel)))?;
        let secondary_indexes: Vec<(String, String)> = self.db.query_rows(
            "sqlite_master",
            "SELECT name, sql FROM sqlite_master WHERE tbl_name = ?1 AND type = 'index' AND sql IS NOT NULL",
            &[tbl(&head_rel).into()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        for (index_name, _) in &secondary_indexes {
            self.db.exec(&format!("DROP INDEX \"{index_name}\""))?;
        }
        let output_columns: Vec<&str> = head_meta
            .cols
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        const OUTPUT_CHUNK: usize = 16000;
        let mut output_rows: Vec<Vec<Value>> =
            Vec::with_capacity(OUTPUT_CHUNK.min(reached.len().max(1)));
        let mut insert_result: Result<()> = Ok(());
        for &(tag_id, node_id, depth) in &reached {
            let mut row_values = vec![Value::Null; head_meta.cols.len()];
            row_values[node_index] = Value::Int(node_keys[node_id as usize]);
            row_values[depth_index] = Value::Int(depth);
            if let Some(tag_column) = tag_index {
                row_values[tag_column] = Value::Text(tag_names[tag_id as usize].clone());
            }
            output_rows.push(row_values);
            if output_rows.len() >= OUTPUT_CHUNK {
                if let Err(error) = self.insert_rel_rows(&head_rel, &output_columns, &output_rows) {
                    insert_result = Err(error);
                    break;
                }
                output_rows.clear();
            }
        }
        if insert_result.is_ok() && !output_rows.is_empty() {
            insert_result = self
                .insert_rel_rows(&head_rel, &output_columns, &output_rows)
                .map(|_| ());
        }
        for (_, index_sql) in &secondary_indexes {
            self.db.exec(index_sql)?;
        }
        insert_result?;
        if std::env::var_os("DL_BFS_TRACE").is_some() {
            let head_rel = head_rel.as_str();
            let load_ms = load_elapsed.as_millis();
            let bfs_ms = bfs_elapsed.as_millis();
            let out_ms = output_started.elapsed().as_millis();
            let rows = reached.len();
            let nodes = adjacency.len();
            // @eprintln-ok: DL_BFS_TRACE env-gated debug line is the command's stderr output contract
            eprintln!("[bfs] {head_rel}: load={load_ms}ms bfs={bfs_ms}ms out+insert={out_ms}ms rows={rows} nodes={nodes}");
        }
        self.save_stmt_ms_one(
            &format!("walk:{head_rel}"),
            walk_started.elapsed().as_millis() as i64,
        )?;
        Ok(true)
    }

    /// Recognize and natively execute the tagged, halt-gated forward-BFS shape —
    /// `port_reach`'s graph-contraction walk — bypassing the SQLite semi-naive
    /// fixpoint. Returns `Ok(true)` iff the component matched AND was executed
    /// (caller then skips the SQL path); `Ok(false)` is a clean recognizer miss.
    ///
    /// The shape (2-col head `head(tag, node)`), any number of base/seed rules
    /// plus EXACTLY ONE recursive rule of the form
    ///   `head(tag, node) <- head(tag, mid), !halt(mid, _), edge(mid, node).`
    /// where `edge` is a 2-col relation. The base rules run first as ordinary
    /// SQL (they already DELETE-cleared with the rest of the component); their
    /// output is the BFS start frontier, and the walk (src/walk.rs) replaces the
    /// head with the full contraction closure — the same rows the fixpoint would
    /// reach, computed once in memory. Halt nodes are recorded when reached but
    /// never expanded, exactly as `!halt(mid, _)` gates the recursive hop.
    fn try_native_halt_bfs(
        &self,
        comp_rules: &[usize],
        derived_rules: &[&Rule],
        timed: &mut impl FnMut(&str, &str) -> Result<usize>,
    ) -> Result<bool> {
        // A/B lever: force every component onto the SQL fixpoint (parity check).
        if std::env::var_os("DL_NO_HALT_BFS").is_some() {
            return Ok(false);
        }
        // port_reach has two independent seed rules. Its edge relation can
        // still be growing when the native component is visited, while the
        // SQL scheduler will revisit the component through the dependency.
        // Keep this relation on the complete fixpoint path.
        if derived_rules[comp_rules[0]].head.rel == "port_reach" {
            return Ok(false);
        }
        macro_rules! miss {
            ($why:expr) => {{
                if std::env::var_os("DL_BFS_TRACE").is_some() {
                    let rel = &derived_rules[comp_rules[0]].head.rel;
                    let why = $why;
                    tracing::trace!(rel = %rel, why = %why, "[bfs-miss] {rel}: {why}");
                }
                return Ok(false);
            }};
        }
        // All component rules must head the same single 2-col, non-key rel.
        let head_rel = derived_rules[comp_rules[0]].head.rel.clone();
        if comp_rules
            .iter()
            .any(|&ri| derived_rules[ri].head.rel != head_rel)
        {
            miss!("multi-rel component");
        }
        let head_meta = self.rels.get(&head_rel).unwrap();
        if head_meta.cols.len() != 2 || head_meta.key.is_some() {
            miss!("head not 2-col non-key");
        }

        // Partition into recursive (body references the head rel) vs base. The
        // `.dl` discovery merge can load a file's rules in duplicate, so dedup
        // structurally identical rules first — otherwise the doubled recursive
        // rule reads as two and misses. Set semantics make duplicates a no-op.
        let is_rec = |r: &Rule| {
            r.body
                .iter()
                .any(|b| matches!(b, BodyItem::Pos(a) if a.rel == head_rel))
        };
        let dedup_structurally = |ris: &[usize]| -> Vec<usize> {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            ris.iter()
                .copied()
                .filter(|&ri| seen.insert(derived_rules[ri].structural_key()))
                .collect()
        };
        let rec_ris = dedup_structurally(
            &comp_rules
                .iter()
                .copied()
                .filter(|&ri| is_rec(derived_rules[ri]))
                .collect::<Vec<_>>(),
        );
        if rec_ris.len() != 1 {
            miss!(format!("rec rules = {}", rec_ris.len()));
        }
        let rec = derived_rules[rec_ris[0]];
        if rec.has_agg() || rec.temporal.is_some() {
            miss!("agg/temporal rec rule");
        }

        // Recursive head must be exactly (tag, node), both plain vars.
        let (tag_v, node_v) = match rec.head.terms.as_slice() {
            [Term::Var(tag), Term::Var(node)] => (tag.as_str(), node.as_str()),
            _ => miss!("rec head not (Var,Var)"),
        };
        // Body must be exactly {self head(tag,mid), !halt(mid,_), edge(mid,node)}.
        let mut self_atom = None;
        let mut halt_atom = None;
        let mut edge_atom = None;
        for b in &rec.body {
            match b {
                BodyItem::Pos(a) if a.rel == head_rel => {
                    if self_atom.replace(a).is_some() {
                        return Ok(false);
                    }
                }
                BodyItem::Neg(a) => {
                    if halt_atom.replace(a).is_some() {
                        return Ok(false);
                    }
                }
                BodyItem::Pos(a) => {
                    if edge_atom.replace(a).is_some() {
                        return Ok(false);
                    }
                }
                other => miss!(format!("body item {:?}", std::mem::discriminant(other))),
            }
        }
        let (Some(self_atom), Some(halt_atom), Some(edge_atom)) = (self_atom, halt_atom, edge_atom)
        else {
            miss!("body != {self,halt,edge}");
        };

        // self head(tag, mid): tag matches the head's tag var, mid is the walk cursor.
        let mid_v = match self_atom.terms.as_slice() {
            [Term::Var(tag), Term::Var(mid)] if tag == tag_v => mid.as_str(),
            _ => miss!("self atom not (tag,mid)"),
        };
        // edge(mid, node): 2-col, first == mid, second == node.
        match edge_atom.terms.as_slice() {
            [Term::Var(from), Term::Var(to)] if from == mid_v && to == node_v => {}
            _ => miss!("edge atom not (mid,node)"),
        }
        let edge_rel = edge_atom.rel.clone();
        // halt(mid, _...): first term == mid, every other term a wildcard.
        match halt_atom.terms.first() {
            Some(Term::Var(from)) if from == mid_v => {}
            _ => miss!("halt atom first != mid"),
        }
        if halt_atom.terms[1..]
            .iter()
            .any(|t| !matches!(t, Term::Wild))
        {
            miss!("halt extra cols bound");
        }
        let halt_rel = halt_atom.rel.clone();

        let edge_meta = self
            .rels
            .get(&edge_rel)
            .ok_or_else(|| anyhow::anyhow!("halt-bfs edge relation {edge_rel} not declared"))?;
        if edge_meta.cols.len() != 2 {
            miss!("edge not 2-col");
        }
        let halt_meta = self
            .rels
            .get(&halt_rel)
            .ok_or_else(|| anyhow::anyhow!("halt-bfs halt relation {halt_rel} not declared"))?;

        // Node identity must live in ONE space. The head (tag, node) and halt
        // node column are readable `text`; the edge columns are commonly `sym`
        // (i64 hashes with the text in `_strings`). Render sym edges back to text
        // on load so the frontier, halt set, and adjacency all key on the same
        // readable string. A head/halt column that is itself `sym` would need its
        // own decode — out of this recognizer's scope, so bail to the SQL path.
        use crate::ast::Type;
        if head_meta.cols[0].ty == Type::Int || head_meta.cols[1].ty == Type::Int {
            miss!("head column is int");
        }
        if halt_meta.cols[0].ty == Type::Int {
            miss!("halt column is int");
        }
        let edge_sym = [edge_meta.cols[0].interned(), edge_meta.cols[1].interned()];
        if edge_sym[0] != edge_sym[1] {
            miss!("mixed edge column types");
        }
        let sym_edge = edge_sym[0];

        // Need at least one base rule to seed the frontier; a pure recursion with
        // no base case produces nothing and is better left to the SQL path.
        let base_ris = dedup_structurally(
            &comp_rules
                .iter()
                .copied()
                .filter(|&ri| !is_rec(derived_rules[ri]))
                .collect::<Vec<_>>(),
        );
        if base_ris.is_empty() {
            miss!("no base rules");
        }

        // === recognized: execute ===
        // 1. Base rules populate the head (already DELETE-cleared) = start frontier.
        for &ri in &base_ris {
            let sql = lower_rule(derived_rules[ri], &self.rels)?;
            timed(&head_rel, &sql)?;
        }

        let t = std::time::Instant::now();
        use crate::spine::StringId;
        // 2. Edge adjacency in integer (string-id) space — a sym graph keeps NO
        //    per-node String. Base nodes with no incident edge are interned on the
        //    fly (recorded, never expanded — no out-edges).
        let (ec0, ec1) = (
            edge_meta.cols[0].name.clone(),
            edge_meta.cols[1].name.clone(),
        );
        let mut adjacency_cache = self.load_edges_keyed_cached(&edge_rel, &ec0, &ec1, sym_edge)?;
        let AdjacencyCache {
            adjacency: adj,
            key_to_node: interner,
            node_keys: id2key,
            text_by_node: text_by_id,
            ..
        } = &mut *adjacency_cache;
        // For a sym graph, keep the base nodes' text (known from the head read) and
        // fetch the rest from `_strings` at output time; text edges carry it all.
        let mut sym_text: HashMap<u32, String> = HashMap::new();
        let intern_key = |key: i64,
                          text: &str,
                          adj: &mut Vec<Vec<u32>>,
                          id2key: &mut Vec<i64>,
                          interner: &mut HashMap<i64, u32>,
                          text_by_id: &mut Option<Vec<String>>|
         -> u32 {
            if let Some(&i) = interner.get(&key) {
                return i;
            }
            let i = adj.len() as u32;
            adj.push(Vec::new());
            id2key.push(key);
            if let Some(tb) = text_by_id.as_mut() {
                tb.push(text.to_string());
            }
            interner.insert(key, i);
            i
        };

        // 3. Start frontier from the base head rows (tag, node). Tags get their own
        //    small id space (one string per distinct port).
        let mut tag_id: HashMap<String, u32> = HashMap::new();
        let mut tag_names: Vec<String> = Vec::new();
        let mut starts: Vec<(u32, u32)> = Vec::new();
        {
            let base_sql = format!(
                "SELECT \"{}\", \"{}\" FROM {}",
                head_meta.cols[0].name,
                head_meta.cols[1].name,
                txt_tbl(&head_rel)
            );
            self.db.for_each_row(
                &head_rel,
                &base_sql,
                &[],
                |r| {
                    let tag_s = r.get::<_, String>(0)?;
                    let node_s = r.get::<_, String>(1)?;
                    let tid = match tag_id.get(&tag_s) {
                        Some(&i) => i,
                        None => {
                            let i = tag_names.len() as u32;
                            tag_names.push(tag_s.clone());
                            tag_id.insert(tag_s, i);
                            i
                        }
                    };
                    let key = StringId::of(&node_s).sqlite();
                    let nid = intern_key(key, &node_s, adj, id2key, interner, text_by_id);
                    if text_by_id.is_none() {
                        sym_text.entry(nid).or_insert_with(|| node_s.clone());
                    }
                    starts.push((tid, nid));
                    Ok(())
                },
            )?;
        }

        // 4. Halt mask over the (possibly extended) node id space.
        let mut halt_mask = vec![false; adj.len()];
        {
            let hc0 = halt_meta.cols[0].name.clone();
            let sql = format!("SELECT DISTINCT \"{hc0}\" FROM {}", txt_tbl(&halt_rel));
            self.db.for_each_row(
                &halt_rel,
                &sql,
                &[],
                |r| {
                    let name = r.get::<_, String>(0)?;
                    if let Some(&id) = interner.get(&StringId::of(&name).sqlite()) {
                        halt_mask[id as usize] = true;
                    }
                    Ok(())
                },
            )?;
        }

        // 5. Native BFS — pure u32 pairs, no strings touched.
        let t_load = t.elapsed();
        let t_bfs0 = std::time::Instant::now();
        let pairs = crate::walk::multi_source_halt_bfs(&adj, &starts, &halt_mask);
        let t_bfs = t_bfs0.elapsed();

        // 6. Resolve output text per reached node. Text edges already carry it; a
        //    sym graph fetches the DISTINCT reached nodes' text from `_strings`
        //    (base nodes already seeded), so the render set is the output, never
        //    the whole graph.
        let t_out0 = std::time::Instant::now();
        if text_by_id.is_none() {
            let mut need: Vec<u32> = pairs
                .iter()
                .map(|&(_, nid)| nid)
                .filter(|nid| !sym_text.contains_key(nid))
                .collect();
            need.sort_unstable();
            need.dedup();
            // Render in a FEW bulk `IN (...)` reads, not one query per node — the
            // statement count stays O(need / 30k), never O(need). Batched under
            // SQLite's variable ceiling; a key->text map keeps only the output
            // strings (which are being written out regardless).
            let keys: Vec<i64> = need.iter().map(|&nid| id2key[nid as usize]).collect();
            let mut keymap: HashMap<i64, String> = HashMap::with_capacity(keys.len());
            let key_params: Vec<crate::db::SqlVal> = keys.iter().map(|&k| k.into()).collect();
            let rows = self.db.query_in_chunks(
                "_strings",
                |n| format!(
                    "SELECT id, content FROM _strings WHERE id IN ({})",
                    crate::db::holes(n)
                ),
                &[],
                &key_params,
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )?;
            for (id, content) in rows {
                keymap.insert(id, content);
            }
            for &nid in &need {
                sym_text.insert(
                    nid,
                    keymap
                        .get(&id2key[nid as usize])
                        .cloned()
                        .unwrap_or_default(),
                );
            }
        }
        let node_text = |nid: u32| -> String {
            match text_by_id.as_ref() {
                Some(tb) => tb[nid as usize].clone(),
                None => sym_text.get(&nid).cloned().unwrap_or_default(),
            }
        };

        // 7. Replace the head, STREAMING the insert in bounded chunks so peak RSS
        //    is one chunk — never the whole result materialized. Secondary indexes
        //    are deferred and rebuilt after (per-row upkeep dominates a bulk load);
        //    the unique autoindex stays and the BFS output is already deduped.
        self.db.exec_on(&head_rel, &format!("DELETE FROM {}", tbl(&head_rel)))?;
        let sidx: Vec<(String, String)> = self.db.query_rows(
            "sqlite_master",
            "SELECT name, sql FROM sqlite_master \
             WHERE tbl_name = ?1 AND type = 'index' AND sql IS NOT NULL",
            &[tbl(&head_rel).into()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        for (name, _) in &sidx {
            self.db.exec_on(&head_rel, &format!("DROP INDEX \"{name}\""))?;
        }
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        const CHUNK: usize = 16000;
        let mut buf: Vec<Vec<Value>> = Vec::with_capacity(CHUNK.min(pairs.len().max(1)));
        let mut insert_res: Result<()> = Ok(());
        for &(tid, nid) in &pairs {
            buf.push(vec![
                Value::Text(tag_names[tid as usize].clone()),
                Value::Text(node_text(nid)),
            ]);
            if buf.len() >= CHUNK {
                if let Err(e) = self.insert_rel_rows(&head_rel, &cols, &buf) {
                    insert_res = Err(e);
                    break;
                }
                buf.clear();
            }
        }
        if insert_res.is_ok() && !buf.is_empty() {
            insert_res = self.insert_rel_rows(&head_rel, &cols, &buf).map(|_| ());
        }
        // Rebuild dropped indexes BEFORE propagating any insert error, so a failure
        // never leaves the head table missing its secondary indexes.
        for (_, sql) in &sidx {
            self.db.exec(sql)?;
        }
        insert_res?;
        if std::env::var_os("DL_BFS_TRACE").is_some() {
            let head_rel = head_rel.as_str();
            let load_ms = t_load.as_millis();
            let bfs_ms = t_bfs.as_millis();
            let out_ms = t_out0.elapsed().as_millis();
            let rows = pairs.len();
            let nodes = adj.len();
            // @eprintln-ok: DL_BFS_TRACE env-gated debug line is the command's stderr output contract
            eprintln!("[bfs] {head_rel}: load={load_ms}ms bfs={bfs_ms}ms out+insert={out_ms}ms rows={rows} nodes={nodes}");
        }
        self.save_stmt_ms_one(
            &format!("halt_bfs:{head_rel}"),
            t.elapsed().as_millis() as i64,
        )?;
        Ok(true)
    }

    /// Semi-naive evaluation of one recursive rel-component. Standard
    /// datalog delta evaluation: instead of every iteration re-running each
    /// rule's FULL join (re-deriving every row ever produced, including all
    /// prior iterations', and discarding the re-derivations on PK conflict),
    /// maintain a `_delta_<rel>` snapshot per targeted rel holding only the
    /// rows born in the PREVIOUS iteration. A rule with a recursive body atom
    /// reruns once per occurrence of that atom (`lower::recursive_occurrences`
    /// / `body_sql_ex`'s `overrides` seam): that ONE occurrence reads the
    /// delta, every other occurrence (recursive or not) reads the full
    /// accumulated relation. This computes exactly the rows NEW this
    /// iteration; naive re-runs of every prior iteration's now-redundant work
    /// are never issued.
    ///
    /// Preconditions enforced by the caller (`rebuild_derived`): every rule in
    /// `comp_rules` heads a rel with a full-row PRIMARY KEY (no `key(...)`,
    /// which would make "new full row" and "new key" disagree) and no
    /// aggregate head (an aggregate must see the whole group, not a delta
    /// slice). Rows/results are otherwise byte-identical to the naive loop:
    /// same PK, same INSERT OR IGNORE dedup, just fewer re-derivations.
    pub(crate) fn rebuild_derived_seminaive(
        &self,
        comp_rules: &[usize],
        derived_rules: &[&Rule],
        timed: &mut dyn FnMut(&str, &str) -> Result<usize>,
    ) -> Result<()> {
        let comp_rels: HashSet<String> = comp_rules
            .iter()
            .map(|&ri| derived_rules[ri].head.rel.clone())
            .collect();
        let mut comp_rel_names: Vec<String> = comp_rels.iter().cloned().collect();
        comp_rel_names.sort();
        let row_max = fixpoint_row_max();
        let mut total_promoted = 0usize;

        // Partition: a rule with zero recursive-atom occurrences is a base
        // case (seed) — its output never changes across iterations, so it
        // runs exactly once, into the real rel table. A rule with >=1
        // recursive occurrence is differentiated per occurrence each pass.
        let mut seed_ris: Vec<usize> = Vec::new();
        let mut rec_ris: Vec<(usize, Vec<(usize, String)>)> = Vec::new();
        for &ri in comp_rules {
            let occs = crate::lower::recursive_occurrences(derived_rules[ri], &comp_rels);
            if occs.is_empty() {
                seed_ris.push(ri);
            } else {
                rec_ris.push((ri, occs));
            }
        }

        for &ri in &seed_ris {
            let rule = derived_rules[ri];
            let sql = lower_rule(rule, &self.rels)?;
            total_promoted = total_promoted.saturating_add(timed(&rule.head.rel, &sql)?);
        }
        check_fixpoint_row_budget(&comp_rel_names, total_promoted, row_max)?;
        // Drain the seed rules' minted strings before the first recursive pass
        // (or the pure-seed return) decodes them. Pass boundary, one flush.
        self.db.flush_pending_syms()?;

        if rec_ris.is_empty() {
            return Ok(());
        } // pure base case, no cycle to iterate

        // One `_delta_<rel>`/`_delta_new_<rel>` TEMP table pair per targeted
        // rel, shaped exactly like the rel (same cols, same full-row PK) so
        // `INSERT OR IGNORE` dedups variant output the same way the real
        // table would. Dropped and recreated every call (a TEMP table
        // persists for the connection's lifetime, and a program edit can
        // change a rel's column set between ticks).
        for rel in &comp_rels {
            let meta = self
                .rels
                .get(rel)
                .ok_or_else(|| anyhow::anyhow!("unknown relation {rel}"))?;
            let col_defs: Vec<String> = meta
                .cols
                .iter()
                .map(|c| format!("\"{}\" {}", c.name, c.sql()))
                .collect();
            let col_names: Vec<String> = meta
                .cols
                .iter()
                .map(|c| format!("\"{}\"", c.name))
                .collect();
            for prefix in ["_delta_", "_delta_new_"] {
                self.db
                    .exec_on(rel, &format!("DROP TABLE IF EXISTS {prefix}{rel}"))?;
                self.db.exec_on(
                    rel,
                    &format!(
                        "CREATE TEMP TABLE {prefix}{rel} ({}, PRIMARY KEY ({}))",
                        col_defs.join(", "),
                        col_names.join(", ")
                    ),
                )?;
            }
        }
        // Seed delta_0 = every row the seed rules just produced. `rel_<rel>`
        // was emptied at the top of `rebuild_derived`, so everything in it
        // right now IS new as of this component's first pass.
        for rel in &comp_rels {
            let col_names: Vec<String> = self
                .rels
                .get(rel)
                .unwrap()
                .cols
                .iter()
                .map(|c| format!("\"{}\"", c.name))
                .collect();
            let cols = col_names.join(", ");
            timed(
                rel,
                &format!(
                    "INSERT INTO _delta_{rel} ({cols}) SELECT {cols} FROM {}",
                    tbl(rel)
                ),
            )?;
        }

        let mut iters = 0usize;
        loop {
            for rel in &comp_rels {
                timed(rel, &format!("DELETE FROM _delta_new_{rel}"))?;
            }
            // Each recursive-body-atom occurrence gets its own variant
            // statement (the standard semi-naive differentiation): that
            // occurrence reads `_delta_<its rel>`, every other atom (incl.
            // other recursive atoms in the same rule) reads the full table.
            // Variants for the same head rel land in the same `_delta_new_`
            // table, deduped by its PK.
            for (ri, occs) in &rec_ris {
                let rule = derived_rules[*ri];
                let target = format!("_delta_new_{}", rule.head.rel);
                for (k, rel_name) in occs {
                    let mut overrides: HashMap<usize, String> = HashMap::new();
                    overrides.insert(*k, format!("_delta_{rel_name}"));
                    let sql =
                        crate::lower::lower_rule_to_ex(rule, &self.rels, &target, &[], &overrides)?;
                    timed(&rule.head.rel, &sql)?;
                }
            }
            // Subtract rows already present in the real table — a variant
            // reading OTHER atoms at full strength can regenerate a
            // combination whose result was already derived in an earlier
            // iteration. What remains is truly new.
            let mut total_new = 0usize;
            for rel in &comp_rels {
                let col_names: Vec<String> = self
                    .rels
                    .get(rel)
                    .unwrap()
                    .cols
                    .iter()
                    .map(|c| format!("\"{}\"", c.name))
                    .collect();
                let cols = col_names.join(", ");
                timed(
                    rel,
                    &format!(
                        "DELETE FROM _delta_new_{rel} WHERE ({cols}) IN (SELECT {cols} FROM {})",
                        tbl(rel)
                    ),
                )?;
                let n: i64 = self.db.query_one(
                    rel,
                    &format!("SELECT COUNT(*) FROM _delta_new_{rel}"),
                    &[],
                    |r| Ok(r.get::<_, i64>(0)?),
                )?;
                total_new += n as usize;
            }
            iters += 1;
            if total_new == 0 {
                break;
            }
            total_promoted = total_promoted.saturating_add(total_new);
            check_fixpoint_row_budget(&comp_rel_names, total_promoted, row_max)?;
            if iters > 100_000 {
                bail!("fixpoint did not converge");
            }
            // Promote this iteration's new rows into the real relation, and
            // hand them to the NEXT iteration as its delta.
            for rel in &comp_rels {
                let col_names: Vec<String> = self
                    .rels
                    .get(rel)
                    .unwrap()
                    .cols
                    .iter()
                    .map(|c| format!("\"{}\"", c.name))
                    .collect();
                let cols = col_names.join(", ");
                timed(
                    rel,
                    &format!(
                        "INSERT OR IGNORE INTO {} ({cols}) SELECT {cols} FROM _delta_new_{rel}",
                        tbl(rel)
                    ),
                )?;
                timed(rel, &format!("DELETE FROM _delta_{rel}"))?;
                timed(
                    rel,
                    &format!(
                        "INSERT INTO _delta_{rel} ({cols}) SELECT {cols} FROM _delta_new_{rel}"
                    ),
                )?;
            }
            // Pass boundary: this iteration's promoted rows (now in `_delta_`)
            // become the next iteration's differentiated input, so their minted
            // strings must be in `_strings` before the next pass's variant
            // statements decode them. One flush per pass — the N+1 collapse.
            self.db.flush_pending_syms()?;
        }
        // The breaking iteration (total_new == 0) leaves only duplicate interns
        // queued (strings already in `_strings` from when their row first
        // derived); drain them so the queue never leaks across components.
        self.db.flush_pending_syms()?;
        Ok(())
    }

    /// Flush one rebuild's per-rel statement timings into `_stmt_ms` (replace
    /// the rebuilt rels' rows, leave the rest — the last known cost of a rel
    /// that did not rebuild this tick stays visible to rails). Each value is
    /// `(sum_ms, statement_count)` for that key's most recent pass.
    pub(crate) fn save_stmt_ms(&self, stmt_ms: &HashMap<String, (i64, i64)>) -> Result<()> {
        if stmt_ms.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = stmt_ms
            .keys()
            .map(|r| format!("'{}'", r.replace('\'', "''")))
            .collect();
        self.db.exec_on(
            "_stmt_ms",
            &format!("DELETE FROM _stmt_ms WHERE rel IN ({})", names.join(","))
        )?;
        let rows: Vec<Vec<Value>> = stmt_ms
            .iter()
            .map(|(rel, (ms, n))| vec![Value::Text(rel.clone()), Value::Int(*ms), Value::Int(*n)])
            .collect();
        self.db
            .insert_rows("_stmt_ms", &["rel", "ms", "n"], &rows)?;
        Ok(())
    }

    /// Single-key convenience wrapper over `save_stmt_ms`, for the derived-phase
    /// sub-passes that run once per tick rather than once per rel (closure
    /// condensation, the operator evals, the term-extract pass) — see the
    /// `closure:<edge>` / `cond_cache:<edge>` / `extract:<rel>` / `closure_seed:<rel>`
    /// / `scc:<rel>` / `node2vec:<edge>` keys their callers use. These are
    /// sibling buckets in the SAME `_stmt_ms` table as the per-rel rebuild costs,
    /// not a new mechanism — `stmt_ms`/`rel_count` rails read them identically.
    pub(crate) fn save_stmt_ms_one(&self, key: &str, ms: i64) -> Result<()> {
        let mut m = HashMap::new();
        m.insert(key.to_string(), (ms, 1));
        self.save_stmt_ms(&m)
    }

    /// First closure edge (if any) whose SCC node table is empty — unlike
    /// `derived_incomplete_rels`, this is intentionally still a plain row-count
    /// probe (P1 scoped to derived rels only; closures are a separate, usually
    /// small set). Returning the offending edge name (not just a bool) lets
    /// the tick record a `full_reason` of `closure-missing:<edge>`.
    pub(crate) fn first_empty_closure_edge(&self, edges: &[&str]) -> Result<Option<String>> {
        for edge in edges {
            let n: i64 = self.db.query_one(
                edge,
                &format!("SELECT COUNT(*) FROM {}", scc_node_tbl(edge)),
                &[],
                |r| Ok(r.get::<_, i64>(0)?),
            )?;
            if n == 0 {
                return Ok(Some(edge.to_string()));
            }
        }
        Ok(None)
    }

    /// True if any named rel's table is empty or absent — the cold-populate
    /// guard for corpus-gated families (spine): skip the family only when file
    /// content is unchanged AND its rels are already populated. A not-yet-
    /// created table counts as empty (cold start must run).
    pub(crate) fn any_rel_empty(&self, rels: &[&str]) -> Result<bool> {
        for rel in rels {
            let n: i64 = self
                .db
                .query_opt(
                    rel,
                    &format!("SELECT COUNT(*) FROM {}", tbl(rel)),
                    &[],
                    |r| Ok(r.get::<_, i64>(0)?),
                )?
                .unwrap_or(0);
            if n == 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Load a 2-col edge rel into u32-interned adjacency keyed by each endpoint's
    /// string-id (i64), so a large `sym` graph stays in integer space with NO
    /// per-node `String` retained. Returns `(adj, key->id, id->key, text_by_id)`.
    /// `sym`: a `sym` column's cell IS the string-id — read the i64 directly, the
    /// text stays in `_strings` and is fetched lazily only for output nodes; a
    /// `text` column's cell is the string — hash it for the key AND keep it in
    /// `text_by_id` for rendering (text edges are the small, non-blowup case).
    /// Both endpoint columns share a type (the caller guarantees it).
    pub(crate) fn load_edges_keyed(
        &self,
        edge: &str,
        c0: &str,
        c1: &str,
        sym: bool,
    ) -> Result<(
        Vec<Vec<u32>>,
        HashMap<i64, u32>,
        Vec<i64>,
        Option<Vec<String>>,
    )> {
        use crate::spine::StringId;
        // sym mode keys on the raw interned StringId (i64), so read the RAW table;
        // the head-node side is read raw too (`tbl(head_rel)`), so both agree in
        // id-space. Text mode hashes the decoded value, so it reads the `_txt`
        // view. Reading the decoded view under sym would feed text to `get::<i64>`
        // and yield an empty adjacency (the walk then never leaves its seeds).
        let source = if sym { tbl(edge) } else { txt_tbl(edge) };
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {source}");
        let mut interner: HashMap<i64, u32> = HashMap::new();
        let mut id2key: Vec<i64> = Vec::new();
        let mut text_by_id: Option<Vec<String>> = if sym { None } else { Some(Vec::new()) };
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        // Read each endpoint as its (key, optional text): text -> (i64 cell, None);
        // text -> (hash(cell), Some(cell)).
        let rows = self.db.query_rows(
            edge,
            &sql,
            &[],
            |r| {
                let a = if sym {
                    (r.get::<_, i64>(0)?, None)
                } else {
                    let s = cell_as_string(r, 0)?;
                    (StringId::of(&s).sqlite(), Some(s))
                };
                let b = if sym {
                    (r.get::<_, i64>(1)?, None)
                } else {
                    let s = cell_as_string(r, 1)?;
                    (StringId::of(&s).sqlite(), Some(s))
                };
                Ok((a, b))
            },
        )?;
        for row in rows {
            let ((ka, ta), (kb, tb)) = row;
            let mut intern = |key: i64, text: Option<String>| -> u32 {
                if let Some(&i) = interner.get(&key) {
                    return i;
                }
                let i = id2key.len() as u32;
                id2key.push(key);
                if let Some(tb) = text_by_id.as_mut() {
                    tb.push(text.unwrap_or_default());
                }
                interner.insert(key, i);
                i
            };
            let a = intern(ka, ta);
            let b = intern(kb, tb);
            pairs.push((a, b));
        }
        let mut adj = vec![Vec::new(); id2key.len()];
        for (a, b) in pairs {
            adj[a as usize].push(b);
        }
        Ok((adj, interner, id2key, text_by_id))
    }

    pub(crate) fn load_edges_keyed_cached(
        &self,
        edge: &str,
        c0: &str,
        c1: &str,
        sym: bool,
    ) -> Result<std::cell::RefMut<'_, AdjacencyCache>> {
        let row_count: i64 = self.db.query_one(
            edge,
            &format!("SELECT COUNT(*) FROM {}", tbl(edge)),
            &[],
            |row| Ok(row.get::<_, i64>(0)?),
        )?;
        let cache_key = AdjacencyCacheKey {
            edge_relation: edge.to_string(),
            first_column: c0.to_string(),
            second_column: c1.to_string(),
            sym_columns: sym,
            row_count,
        };
        {
            let cache_slot = self.adjacency_cache.borrow();
            if cache_slot
                .as_ref()
                .is_some_and(|cache| cache.key == cache_key)
            {
                if std::env::var_os("DL_BFS_TRACE").is_some() {
                    // @eprintln-ok: DL_BFS_TRACE env-gated debug line is the command's stderr output contract
                    eprintln!("[bfs-cache] reuse edge={edge} columns={c0},{c1} sym={sym}");
                }
                drop(cache_slot);
                return Ok(std::cell::RefMut::map(
                    self.adjacency_cache.borrow_mut(),
                    |cache| cache.as_mut().unwrap(),
                ));
            }
        }
        let (adjacency, key_to_node, node_keys, text_by_node) =
            self.load_edges_keyed(edge, c0, c1, sym)?;
        if std::env::var_os("DL_BFS_TRACE").is_some() {
            // @eprintln-ok: DL_BFS_TRACE env-gated debug line is the command's stderr output contract
            eprintln!("[bfs-cache] load edge={edge} columns={c0},{c1} sym={sym}");
        }
        let mut cache_slot = self.adjacency_cache.borrow_mut();
        *cache_slot = Some(AdjacencyCache {
            key: cache_key,
            adjacency,
            key_to_node,
            node_keys,
            text_by_node,
        });
        Ok(std::cell::RefMut::map(cache_slot, |cache| {
            cache.as_mut().unwrap()
        }))
    }

    pub(crate) fn load_edges(
        &self,
        edge: &str,
        c0: &str,
        c1: &str,
    ) -> Result<(Vec<Vec<u32>>, Vec<String>)> {
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {}", txt_tbl(edge));
        let mut intern: HashMap<String, u32> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let rows = self.db.query_rows(
            edge,
            &sql,
            &[],
            |r| Ok((cell_as_string(r, 0)?, cell_as_string(r, 1)?)),
        )?;
        for row in rows {
            let mut id = |s: String| -> u32 {
                if let Some(&i) = intern.get(&s) {
                    return i;
                }
                let i = names.len() as u32;
                intern.insert(s.clone(), i);
                names.push(s);
                i
            };
            let a = id(row.0);
            let b = id(row.1);
            pairs.push((a, b));
        }
        let mut adj = vec![Vec::new(); names.len()];
        for (a, b) in pairs {
            adj[a as usize].push(b);
        }
        Ok((adj, names))
    }

    /// For each edge relation: condense, then replace its scc_node/scc_edge tables.
    /// The closure VIEW reads these; the Theta(V^2) pair table is never built.
    pub(crate) fn rebuild_closures(&self, edges: &[&str]) -> Result<()> {
        // Per-edge cost lands in `_stmt_ms` under `closure:<edge>` (max across
        // this call's edges is not needed here — one entry per edge, one call
        // site per tick pass — so a plain overwrite is correct). This is the
        // condensation (Tarjan) + SCC node/edge table rebuild that a `closure`
        // rel pays every time its edge rebuilds; previously untimed, it was
        // part of the ~32s cold-tick derived-phase gap between the tick's
        // "derived" phase wall time and `SUM(ms) FROM _stmt_ms`.
        let mut stmt_ms: HashMap<String, (i64, i64)> = HashMap::new();
        for edge in edges {
            let t = std::time::Instant::now();
            let meta = self
                .rels
                .get(*edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() < 2 {
                bail!("closure edge {edge} must have at least 2 columns");
            }
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            let (nt, et) = (scc_node_tbl(edge), scc_edge_tbl(edge));
            let mut node_rows: Vec<Vec<Value>> = Vec::with_capacity(names.len());
            for (id, name) in names.iter().enumerate() {
                let comp = cond.comp[id] as i64;
                let cyc = cond.cyclic[cond.comp[id] as usize] as i64;
                node_rows.push(vec![
                    Value::Text(name.clone()),
                    Value::Int(comp),
                    Value::Int(cyc),
                ]);
            }
            let mut edge_rows: Vec<Vec<Value>> = Vec::new();
            for (cu, succ) in cond.cadj.iter().enumerate() {
                for &cw in succ {
                    edge_rows.push(vec![Value::Int(cu as i64), Value::Int(cw as i64)]);
                }
            }
            self.db.exec_on(edge, &format!("DELETE FROM {nt}"))?;
            self.db.exec_on(edge, &format!("DELETE FROM {et}"))?;
            self.db
                .insert_rows(&nt, &["name", "comp", "cyclic"], &node_rows)?;
            self.db
                .insert_rows(&et, &["comp_src", "comp_dst"], &edge_rows)?;
            stmt_ms.insert(
                format!("closure:{edge}"),
                (t.elapsed().as_millis() as i64, 1),
            );
        }
        self.save_stmt_ms(&stmt_ms)?;
        Ok(())
    }
    /// Order-independent content digest of a closure edge relation's `(c0,c1)`
    /// rows. Same edge set ⇒ same digest, regardless of row order; the edge
    /// table is a set (PK), so XOR cannot cancel a duplicate. Lets a tick that
    /// touched the edge's source file skip the recondense when the actual edges
    /// did not move (e.g. a comment edit that leaves call sites unchanged).

    pub(crate) fn edge_content_digest(&self, edge: &str, c0: &str, c1: &str) -> Result<[u8; 32]> {
        let mut acc = [0u8; 32];
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {}", txt_tbl(edge));
        self.db.for_each_row(
            edge,
            &sql,
            &[],
            |row| {
                let a = cell_as_string(row, 0)?;
                let b = cell_as_string(row, 1)?;
                let mut h = blake3::Hasher::new();
                h.update(a.as_bytes());
                h.update(&[0]);
                h.update(b.as_bytes());
                for (x, y) in acc.iter_mut().zip(h.finalize().as_bytes().iter()) {
                    *x ^= *y;
                }
                Ok(())
            },
        )?;
        Ok(acc)
    }

    /// Refresh `self.closure_cache` for the query phase, recondensing an edge
    /// ONLY when its rows actually changed. `dirty` is the set of edges whose
    /// source/derived relation was rebuilt this tick. An edge not in `dirty` is
    /// reused with zero work (no scan, no Tarjan). A dirty edge is digest-checked
    /// first; the Tarjan rebuild runs only if the digest moved. This replaces the
    /// old unconditional per-tick rebuild of every edge's condensation.
    pub(crate) fn refresh_cond_cache(
        &mut self,
        edges: &[&str],
        dirty: &HashSet<&str>,
    ) -> Result<()> {
        self.closure_cache
            .retain(|k, _| edges.iter().any(|e| *e == k.as_str()));
        // Timed only for the edges that actually pay the digest read + Tarjan
        // rebuild below — a reused/unchanged edge costs ~nothing and doesn't
        // need its own row. Key `cond_cache:<edge>`, sibling to `closure:<edge>`
        // (rebuild_closures' SCC-table write) in `_stmt_ms`.
        let mut stmt_ms: HashMap<String, (i64, i64)> = HashMap::new();
        for &edge in edges {
            let meta = self
                .rels
                .get(edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() < 2 {
                continue;
            }
            // Unaffected edge already cached → reuse, no scan.
            if !dirty.contains(edge) && self.closure_cache.contains_key(edge) {
                continue;
            }
            let t = std::time::Instant::now();
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let digest = self.edge_content_digest(edge, &c0, &c1)?;
            // Dirty but rows unchanged (e.g. comment edit) → reuse, skip Tarjan.
            if self.closure_cache.get(edge).map(|c| c.digest) == Some(digest) {
                stmt_ms.insert(
                    format!("cond_cache:{edge}"),
                    (t.elapsed().as_millis() as i64, 1),
                );
                continue;
            }
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            self.recondensed += 1;
            let id = names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i as u32))
                .collect();
            self.closure_cache.insert(
                edge.to_string(),
                ClosureCache {
                    cond,
                    names,
                    id,
                    digest,
                },
            );
            stmt_ms.insert(
                format!("cond_cache:{edge}"),
                (t.elapsed().as_millis() as i64, 1),
            );
        }
        self.save_stmt_ms(&stmt_ms)?;
        Ok(())
    }

    /// Answer `reaches(src=SEED, dst=?)` as a seeded BFS over the condensation.
    /// Same row set as the view's src-pinned slice, computed in microseconds.
    /// Seeded closure point query. `forward` = src pinned (walk out, callees);
    /// otherwise dst pinned (walk in, callers). Emits the same rows as the view's
    /// pinned slice, in microseconds.
    pub(crate) fn run_reaches_point(
        &self,
        q: &Query,
        cc: &ClosureCache,
        seed: &str,
        forward: bool,
    ) -> Result<()> {
        let meta = self.rels.get(&q.head.rel).unwrap();
        let header = |pos: usize| match &q.head.terms[pos] {
            Term::Var(v) => v.clone(),
            _ => meta.col_name(pos).to_string(),
        };
        let mut hits: Vec<&str> = Vec::new();
        if let Some(&sid) = cc.id.get(seed) {
            let walk = if forward {
                scc::reaches_from(&cc.cond, sid)
            } else {
                scc::reached_by(&cc.cond, sid)
            };
            hits = walk
                .iter()
                .map(|&i| cc.names[i as usize].as_str())
                .collect();
            hits.sort_unstable();
        }
        let row = |h: &str| {
            if forward {
                vec![
                    serde_json::Value::String(seed.to_string()),
                    serde_json::Value::String(h.to_string()),
                ]
            } else {
                vec![
                    serde_json::Value::String(h.to_string()),
                    serde_json::Value::String(seed.to_string()),
                ]
            }
        };
        match self.query_format {
            QueryOutputFormat::Ndjson => {
                let rows: Vec<Vec<serde_json::Value>> = hits.iter().map(|h| row(h)).collect();
                emit_query_json(&q.head.rel, &[header(0), header(1)], &rows);
            }
            QueryOutputFormat::JsonRows => {
                let rows: Vec<Vec<serde_json::Value>> = hits.iter().map(|h| row(h)).collect();
                emit_query_json_rows(&[header(0), header(1)], &rows);
            }
            QueryOutputFormat::Text => {
                println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
                for h in &hits {
                    if forward {
                        println!("{seed}\t{h}");
                    } else {
                        println!("{h}\t{seed}");
                    }
                }
                println!("  ({} rows)\n", hits.len());
            }
        }
        Ok(())
    }

    /// Both endpoints pinned: an existence probe answered by the seeded walk —
    /// same row semantics as the view's doubly-pinned slice (one row when src
    /// reaches dst, zero otherwise), without evaluating the view.
    pub(crate) fn run_reaches_pair(
        &self,
        q: &Query,
        cc: &ClosureCache,
        src: &str,
        dst: &str,
    ) -> Result<()> {
        let meta = self.rels.get(&q.head.rel).unwrap();
        let header = |pos: usize| match &q.head.terms[pos] {
            Term::Var(v) => v.clone(),
            _ => meta.col_name(pos).to_string(),
        };
        let hit = match (cc.id.get(src), cc.id.get(dst)) {
            (Some(&sid), Some(&did)) => scc::reaches_from(&cc.cond, sid).contains(&did),
            _ => false,
        };
        let rows: Vec<Vec<serde_json::Value>> = if hit {
            vec![vec![
                serde_json::Value::String(src.to_string()),
                serde_json::Value::String(dst.to_string()),
            ]]
        } else {
            Vec::new()
        };
        match self.query_format {
            QueryOutputFormat::Ndjson => emit_query_json(&q.head.rel, &[header(0), header(1)], &rows),
            QueryOutputFormat::JsonRows => emit_query_json_rows(&[header(0), header(1)], &rows),
            QueryOutputFormat::Text => {
                println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
                if hit {
                    println!("{src}\t{dst}");
                }
                println!("  ({} rows)\n", rows.len());
            }
        }
        Ok(())
    }

    /// Evaluate a closure-seedable derived rule (one closure endpoint pinned to a
    /// literal) by a seeded BFS over the cross-tick condensation, writing its head
    /// table. This is the rule-body twin of `run_reaches_point`: same walk, but
    /// the reached set is projected through the head and inserted, not printed.
    /// Runs in the query phase (after `refresh_cond_cache`), so the condensation
    /// is ready; it reads `self.closure_cache` and never recondenses.
    pub(crate) fn eval_closure_seed_rule(&self, rule: &Rule, cs: &ClosureSeed) -> Result<()> {
        let t = std::time::Instant::now();
        let r = self.eval_closure_seed_rule_inner(rule, cs);
        self.save_stmt_ms_one(
            &format!("closure_seed:{}", rule.head.rel),
            t.elapsed().as_millis() as i64,
        )?;
        r
    }

    /// Body of `eval_closure_seed_rule`, split out so the wrapper can time the
    /// whole seeded-BFS-plus-flush pass into `_stmt_ms` (`closure_seed:<rel>`)
    /// without an early `?` return skipping the timing write.
    pub(crate) fn eval_closure_seed_rule_inner(&self, rule: &Rule, cs: &ClosureSeed) -> Result<()> {
        let head = &rule.head.rel;
        let head_meta = self
            .rels
            .get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != head_meta.cols.len() {
            bail!(
                "head {} expects {} cols, got {}",
                head,
                head_meta.cols.len(),
                rule.head.terms.len()
            );
        }
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        // The closure atom binds `free_var`; the pinned endpoint var (if any)
        // binds to the seed. The rule head may project these plus literals.
        let pinned_var: Option<&str> = rule.body.iter().find_map(|it| match it {
            BodyItem::Pos(a) if a.rel != *head && a.terms.len() == 2 => {
                // the closure atom: the non-free endpoint's var, if it is a var
                let other = a
                    .terms
                    .iter()
                    .find(|t| !matches!(t, Term::Var(v) if *v == cs.free_var));
                match other {
                    Some(Term::Var(v)) => Some(v.as_str()),
                    _ => None,
                }
            }
            _ => None,
        });
        let cells: Result<Vec<Value>> = rule
            .head
            .terms
            .iter()
            .map(|t| match t {
                Term::Str(s) => Ok(Value::Text(s.clone())),
                Term::Int(n) => Ok(Value::Int(*n)),
                Term::Var(v) if *v == cs.free_var => Ok(Value::Text(String::new())), // filled per-row
                Term::Var(v) if Some(v.as_str()) == pinned_var => Ok(Value::Text(cs.seed.clone())),
                Term::Var(v) => bail!(
                    "seeded closure rule head var '{v}' is neither the \
                                   free reached endpoint nor the pinned seed; only those \
                                   two (plus literals) can appear in the head"
                ),
                Term::Wild => bail!("'_' not allowed in a seeded closure rule head"),
                Term::Interp(_) => {
                    bail!("interpolation not supported in a seeded closure rule head")
                }
                Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                Term::Arith { .. } => {
                    bail!("arithmetic not supported in a seeded closure rule head")
                }
                Term::Call { .. } => {
                    bail!("function call not supported in a seeded closure rule head")
                }
            })
            .collect();
        let template = cells?;
        let free_positions: Vec<usize> = rule
            .head
            .terms
            .iter()
            .enumerate()
            .filter_map(|(i, t)| matches!(t, Term::Var(v) if *v == cs.free_var).then_some(i))
            .collect();

        let Some(cc) = self.closure_cache.get(&cs.edge) else {
            return Ok(());
        };
        let mut rows: Vec<Vec<Value>> = Vec::new();
        if let Some(&sid) = cc.id.get(&cs.seed) {
            let walk = if cs.forward {
                scc::reaches_from(&cc.cond, sid)
            } else {
                scc::reached_by(&cc.cond, sid)
            };
            for &i in &walk {
                let name = &cc.names[i as usize];
                let mut row = template.clone();
                for &p in &free_positions {
                    row[p] = Value::Text(name.clone());
                }
                rows.push(row);
            }
        }
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        self.insert_rel_rows(head, &cols, &rows)?; // one flush, never N+1
        Ok(())
    }

    /// Evaluate `head(rep, member) <- scc(edge).` — materialize SCC membership
    /// from `edge`'s already-computed condensation. One row per node: (component
    /// representative, node), where the representative is the lexicographically-min
    /// member name in the component (a stable, readable cluster id). Shares
    /// `closure(edge)`'s condensation cache — no second Tarjan run. Binds
    /// positionally: head col 0 = rep, col 1 = member. Runs in the query phase
    /// after `refresh_cond_cache`, so the cond is ready; the head is otherwise
    /// excluded from `rebuild_derived` (the Scc body item can't lower to SQL).
    pub(crate) fn eval_scc_rule(&self, rule: &Rule) -> Result<()> {
        let t = std::time::Instant::now();
        let r = self.eval_scc_rule_inner(rule);
        self.save_stmt_ms_one(
            &format!("scc:{}", rule.head.rel),
            t.elapsed().as_millis() as i64,
        )?;
        r
    }

    /// Body of `eval_scc_rule`, split out so the wrapper can time the whole
    /// membership-materialization pass into `_stmt_ms` (`scc:<rel>`) past any
    /// early `?` return.
    pub(crate) fn eval_scc_rule_inner(&self, rule: &Rule) -> Result<()> {
        let edge = rule
            .scc_edge()
            .ok_or_else(|| anyhow::anyhow!("eval_scc_rule on a non-scc rule"))?;
        let head = &rule.head.rel;
        let head_meta = self
            .rels
            .get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != 2 || head_meta.cols.len() != 2 {
            bail!(
                "scc head '{head}' must have exactly 2 columns (rep, member); got {}",
                head_meta.cols.len()
            );
        }
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        let Some(cc) = self.closure_cache.get(edge) else {
            return Ok(()); // edge not condensed this tick (e.g. empty) -> head empty
        };
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for c in 0..cc.cond.ncomp {
            let members = &cc.cond.members[c];
            if members.is_empty() {
                continue;
            }
            let rep = members
                .iter()
                .map(|&i| cc.names[i as usize].as_str())
                .min()
                .unwrap_or("");
            for &m in members {
                rows.push(vec![
                    Value::Text(rep.to_string()),
                    Value::Text(cc.names[m as usize].clone()),
                ]);
            }
        }
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        self.insert_rel_rows(head, &cols, &rows)?; // one flush, never N+1
        Ok(())
    }
}

#[cfg(test)]
mod crash_window_tests {
    //! Force-kill crash window: `rebuild_derived` wipes + marks completion
    //! PER COMPONENT (not one upfront DELETE + one final mark), and the tick
    //! DEFERS its source-digest saves until after the rebuild lands. Both
    //! properties are exercised through the `fail_rebuild_at_rel` hook, which
    //! bails between one component's wipe and its refill — the on-disk shape a
    //! SIGKILL would leave.

    use crate::{db, engine::Engine, lex, parse};
    use std::fs;
    use std::path::{Path, PathBuf};

    // Two INDEPENDENT derived rels over one source rel: `reach_a`/`reach_b`
    // land in separate dependency components, so a failure injected at
    // `reach_b` leaves `reach_a` fully derived.
    const PROG: &str = r#"
rel src_a(p: file, w: text).
src_a(p, w) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /alpha_(?<w>\w+)/, line).
rel reach_a(w: text).
reach_a(w) <- src_a(_, w).
rel reach_b(w: text).
reach_b(w) <- src_a(_, w).
"#;

    fn sandbox(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("crash_window_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    fn fresh_engine(dir: &Path) -> (Engine, crate::ast::Program) {
        let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
        let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
        (Engine::new(conn, dir.to_path_buf()), prog)
    }

    fn complete_set(eng: &Engine) -> std::collections::HashSet<String> {
        eng.query_sql("SELECT rel FROM _derived_complete", &[])
            .unwrap()
            .into_iter()
            .map(|row| row[0].as_str().unwrap().to_string())
            .collect()
    }

    fn rel_len(eng: &Engine, rel: &str) -> usize {
        eng.query_sql(&format!("SELECT w FROM rel_{rel}_txt"), &[])
            .unwrap()
            .len()
    }

    /// A rebuild interrupted between one component's wipe and its refill leaves
    /// the completed component marked+populated and the interrupted one
    /// unmarked+empty; `derived_incomplete_rels` names exactly the unfinished
    /// rel; and a follow-up rebuild scoped to just that rel repairs it without
    /// touching the finished one.
    #[test]
    fn interrupted_component_is_incomplete_finished_one_survives() {
        let dir = sandbox("interrupt");
        fs::write(dir.join("src/a.rs"), "// alpha_one\n").unwrap();
        let (mut eng, prog) = fresh_engine(&dir);

        // Clean tick: both rels complete + populated.
        eng.tick(&prog, true).unwrap();
        assert_eq!(rel_len(&eng, "reach_a"), 1);
        assert_eq!(rel_len(&eng, "reach_b"), 1);
        let done = complete_set(&eng);
        assert!(done.contains("reach_a") && done.contains("reach_b"));

        // Edit the source so both derived rels are affected, and arm the crash
        // hook at `reach_b`. The scoped rebuild wipes each component, then bails
        // when it reaches `reach_b` — after that component's wipe, before refill.
        fs::write(dir.join("src/a.rs"), "// alpha_seventeen\n").unwrap();
        *eng.fail_rebuild_at_rel.borrow_mut() = Some("reach_b".into());
        let err = eng.tick(&prog, true).unwrap_err();
        assert!(
            err.to_string().contains("test-injected crash"),
            "expected the injected crash, got: {err}"
        );

        // Completed component survived; interrupted one is wiped + unmarked.
        let done = complete_set(&eng);
        assert!(
            done.contains("reach_a"),
            "the finished component must stay marked complete, got {done:?}"
        );
        assert!(
            !done.contains("reach_b"),
            "the interrupted component's marker must have been cleared before its wipe, got {done:?}"
        );
        assert!(rel_len(&eng, "reach_a") >= 1, "reach_a must stay populated");
        assert_eq!(rel_len(&eng, "reach_b"), 0, "reach_b was wiped and never refilled");

        // `derived_incomplete_rels` names exactly the unfinished rel.
        let derived_rels = vec!["reach_a".to_string(), "reach_b".to_string()];
        let incomplete = eng.derived_incomplete_rels(&derived_rels).unwrap();
        assert_eq!(incomplete, vec!["reach_b".to_string()]);

        // Disarm, then a rebuild SCOPED to just `reach_b` repairs it and leaves
        // `reach_a` untouched (still complete + populated).
        *eng.fail_rebuild_at_rel.borrow_mut() = None;
        let reach_b_rules: Vec<&crate::ast::Rule> = prog
            .items
            .iter()
            .filter_map(|item| match item {
                crate::ast::Item::Rule(rule) if rule.head.rel == "reach_b" => Some(rule),
                _ => None,
            })
            .collect();
        eng.rebuild_derived(&reach_b_rules, &["reach_b".to_string()]).unwrap();

        let done = complete_set(&eng);
        assert!(done.contains("reach_a") && done.contains("reach_b"));
        assert!(rel_len(&eng, "reach_a") >= 1);
        assert_eq!(rel_len(&eng, "reach_b"), 1, "the follow-up repaired reach_b");
    }

    /// A tick that fails during the derived rebuild must NOT have persisted the
    /// source digests it detected: the next tick re-detects the same source
    /// change (proving the change was not silently swallowed by a premature
    /// digest save).
    #[test]
    fn failed_tick_does_not_persist_source_digests() {
        let dir = sandbox("digest_defer");
        fs::write(dir.join("src/a.rs"), "// alpha_one\n").unwrap();
        let (mut eng, prog) = fresh_engine(&dir);

        // Clean tick baselines `src_a`'s digest.
        eng.tick(&prog, true).unwrap();

        // Edit the source, arm the crash hook, and run a tick that detects the
        // `src_a` change but dies in the derived rebuild before the deferred
        // digest flush.
        fs::write(dir.join("src/a.rs"), "// alpha_seventeen\n").unwrap();
        *eng.fail_rebuild_at_rel.borrow_mut() = Some("reach_b".into());
        assert!(eng.tick(&prog, true).is_err());

        // Rerun cleanly: because the failed tick never flushed `src_a`'s new
        // digest, the change is re-detected here. Had the save raced ahead of
        // the rebuild, `src_a` would read unchanged and drop out of the report.
        *eng.fail_rebuild_at_rel.borrow_mut() = None;
        let report = eng.tick_report(&prog, true).unwrap();
        assert!(
            report.changed_rels.contains(&"src_a".to_string()),
            "the rerun must re-detect the src_a change the killed tick left unpersisted, got {:?}",
            report.changed_rels
        );
        assert_eq!(rel_len(&eng, "reach_b"), 1, "reach_b is repaired on the rerun");
    }
}
