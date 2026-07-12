use super::*;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

const FIXPOINT_ROW_MAX_DEFAULT: usize = 5_000_000;
const SLOW_STMT_AFTER: Duration = Duration::from_secs(10);

/// One watchdog serves a rebuild, avoiding a sleeper thread for every SQL statement.
struct StmtWatch { tx: Sender<StmtWatchEvent>, thread: Option<JoinHandle<()>> }
enum StmtWatchEvent { Begin(String), Complete, Shutdown }

impl StmtWatch {
    fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || while let Ok(event) = rx.recv() {
            let rel = match event {
                StmtWatchEvent::Begin(rel) => rel,
                StmtWatchEvent::Shutdown => break,
                StmtWatchEvent::Complete => continue,
            };
            match rx.recv_timeout(SLOW_STMT_AFTER) {
                Ok(StmtWatchEvent::Complete) => {}
                Ok(StmtWatchEvent::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(StmtWatchEvent::Begin(_)) => unreachable!("derived statements do not overlap"),
                Err(RecvTimeoutError::Timeout) => {
                    eprintln!("[stmt-slow] began statement for `{rel}`; still running after 10s");
                    match rx.recv() {
                        Ok(StmtWatchEvent::Complete) => {}
                        Ok(StmtWatchEvent::Shutdown) | Err(_) => break,
                        Ok(StmtWatchEvent::Begin(_)) => unreachable!("derived statements do not overlap"),
                    }
                }
            }
        });
        Self { tx, thread: Some(thread) }
    }
    fn begin(&self, rel: &str) { let _ = self.tx.send(StmtWatchEvent::Begin(rel.to_string())); }
    fn complete(&self) { let _ = self.tx.send(StmtWatchEvent::Complete); }
}
impl Drop for StmtWatch {
    fn drop(&mut self) {
        let _ = self.tx.send(StmtWatchEvent::Shutdown);
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

fn fixpoint_row_max() -> usize {
    std::env::var("DL_FIXPOINT_ROW_MAX").ok().and_then(|value| value.parse().ok())
        .unwrap_or(FIXPOINT_ROW_MAX_DEFAULT)
}
fn check_fixpoint_row_budget(rels: &[String], total: usize, max: usize) -> Result<()> {
    if total <= max { return Ok(()); }
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
        let edge = rule.node2vec_edge()
            .ok_or_else(|| anyhow::anyhow!("eval_node2vec_rule on a non-node2vec rule"))?;
        let head = &rule.head.rel;
        let head_meta = self.rels.get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != 3 || head_meta.cols.len() != 3 {
            bail!("node2vec head '{head}' must have exactly 3 columns (a, b, score); got {}",
                  head_meta.cols.len());
        }
        // Own the head cols now: the recompute path mutates `self`
        // (`node2vec_recomputed`), which conflicts with holding `head_meta`'s
        // immutable borrow of `self.rels` to the end.
        let head_cols: Vec<String> = head_meta.cols.iter().map(|c| c.name.clone()).collect();
        let edge_meta = self.rels.get(edge)
            .ok_or_else(|| anyhow::anyhow!("node2vec edge relation '{edge}' is not declared"))?;
        if edge_meta.cols.len() < 2 {
            bail!("node2vec edge relation '{edge}' must have at least 2 columns (src, dst); got {}",
                  edge_meta.cols.len());
        }
        // Read the first two columns as the directed edge list.
        let (c0, c1) = (edge_meta.cols[0].name.clone(), edge_meta.cols[1].name.clone());
        let edges: Vec<(String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!("SELECT {c0}, {c1} FROM {}", tbl(edge)))?;
            let v: Vec<(String, String)> = s.query_map([], |r|
                Ok((cell_as_string(r, 0)?, cell_as_string(r, 1)?)))?
                .filter_map(|x| x.ok()).collect();
            v
        };

        // W1 digest-skip: node2vec is a GLOBAL op (walks touch the whole graph),
        // so it is not cheaply incrementalizable. Most git checkouts move file
        // content without moving the call/type edge set, so an order-independent
        // digest of the edge rows lets the common re-tick be a no-op. Same guard
        // the scc/closure operators use (recondense only when rows moved).
        let digest = blake3_edges(&edges);
        let dhex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let dkey = format!("node2vec:{edge}");
        // Vectors for THIS exact digest (W2 keeps the last N digests per graph).
        let have_cur: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM _node_embeddings WHERE graph = ?1 AND edge_digest = ?2",
            rusqlite::params![edge, dhex], |r| r.get(0))?;
        let trace = std::env::var("SPREFA_N2V_TRACE").is_ok();
        // Skip when node_sim already reflects this digest and its vectors exist
        // (or the graph is legitimately empty and we recorded the all-zero digest).
        if self.load_rel_digest(&dkey)? == Some(digest)
            && (edges.is_empty() || have_cur > 0) {
            if trace { eprintln!("[node2vec] graph '{edge}': skip (digest unchanged)"); }
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
            self.db.conn().execute("DELETE FROM _node_embeddings WHERE graph = ?1", [edge])?;
            self.db.conn().execute("DELETE FROM _node_emb_seen WHERE graph = ?1", [edge])?;
            if trace { eprintln!("[node2vec] graph '{edge}': empty (cleared)"); }
            self.save_rel_digest(&dkey, &digest)?;
            self.save_stmt_ms_one(&format!("node2vec:{edge}"), t.elapsed().as_millis() as i64)?;
            return Ok(());
        }

        let cfg = crate::embed::node2vec::N2vConfig::from_env();
        // W2 cache: reuse the stored vectors when we have already embedded this
        // exact edge set (branch A<->B thrash is a hit both ways); only a
        // genuinely new graph pays the embed.
        let pool: Vec<(String, Vec<f32>)> = if have_cur > 0 {
            if trace { eprintln!("[node2vec] graph '{edge}': cache hit (digest seen)"); }
            let conn = self.db.conn();
            let mut s = conn.prepare(
                "SELECT node, vec FROM _node_embeddings WHERE graph = ?1 AND edge_digest = ?2")?;
            let v: Vec<(String, Vec<f32>)> = s.query_map(rusqlite::params![edge, dhex], |r|
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok())
                .map(|(n, txt): (String, String)| (n, crate::embed::parse_vec(&txt)))
                .collect();
            v
        } else {
            if trace { eprintln!("[node2vec] graph '{edge}': re-embed ({} edges)", edges.len()); }
            self.node2vec_recomputed += 1;
            let pool = crate::embed::node2vec::embed_graph(&edges, &cfg);
            if pool.len() > 2000 {
                eprintln!("[node2vec] brute-force KNN over {} nodes (O(n^2)); \
                           shrink the edge rel or cap SPREFA_N2V_*", pool.len());
            }
            // Persist this digest's vectors (one flush, never N+1).
            let dim = cfg.dim as i64;
            let emb_rows: Vec<Vec<Value>> = pool.iter().map(|(node, v)| vec![
                Value::Text(node.clone()), Value::Text(edge.to_string()), Value::Text(dhex.clone()),
                Value::Int(dim), Value::Text(crate::embed::encode_vec(v))]).collect();
            self.db.insert_rows("_node_embeddings",
                &["node", "graph", "edge_digest", "dim", "vec"], &emb_rows)?;
            pool
        };

        // LRU: stamp this digest most-recently-used, prune each graph to the N
        // most-recent distinct digests (SPREFA_N2V_CACHE, default 4).
        let seq: i64 = self.db.conn().query_row(
            "SELECT COALESCE(MAX(last_tick), 0) + 1 FROM _node_emb_seen", [], |r| r.get(0))?;
        self.db.conn().execute(
            "INSERT INTO _node_emb_seen(graph, digest, last_tick) VALUES (?1, ?2, ?3)
             ON CONFLICT(graph, digest) DO UPDATE SET last_tick = excluded.last_tick",
            rusqlite::params![edge, dhex, seq])?;
        let cap: i64 = std::env::var("SPREFA_N2V_CACHE").ok()
            .and_then(|s| s.parse().ok()).filter(|n| *n >= 1).unwrap_or(4);
        self.db.conn().execute(
            "DELETE FROM _node_embeddings WHERE graph = ?1 AND edge_digest NOT IN
                 (SELECT digest FROM _node_emb_seen WHERE graph = ?1 ORDER BY last_tick DESC LIMIT ?2)",
            rusqlite::params![edge, cap])?;
        self.db.conn().execute(
            "DELETE FROM _node_emb_seen WHERE graph = ?1 AND digest NOT IN
                 (SELECT digest FROM _node_emb_seen WHERE graph = ?1 ORDER BY last_tick DESC LIMIT ?2)",
            rusqlite::params![edge, cap])?;

        // Fill the head with KNN pairs (reuses the text path's cosine top-k).
        let k: usize = std::env::var("SPREFA_NODE_SIM_K").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(8);
        let rows = knn_rows(&pool, k);
        let cols: Vec<&str> = head_cols.iter().map(|s| s.as_str()).collect();
        self.db.insert_rows(&tbl(head), &cols, &rows)?;
        self.save_rel_digest(&dkey, &digest)?;
        self.save_stmt_ms_one(&format!("node2vec:{edge}"), t.elapsed().as_millis() as i64)?;
        Ok(())
    }

    /// Wipe derived tables and run the semi-naive fixpoint to convergence.
    #[tracing::instrument(skip_all, fields(n_rules = derived_rules.len(), n_rels = derived_rels.len()), level = "debug")]
    pub(crate) fn rebuild_derived(&self, derived_rules: &[&Rule], derived_rels: &[String]) -> Result<()> {
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
            if std::env::var_os("DL_STMT_TRACE").is_some() { eprintln!("[stmt-trace] {rel}"); }
            stmt_watch.begin(rel);
            let t = std::time::Instant::now();
            let result = self.db.conn().execute(sql, []);
            stmt_watch.complete();
            let n = result?;
            let ms = t.elapsed().as_millis() as i64;
            let e = stmt_ms.entry(rel.to_string()).or_insert((0, 0));
            e.0 += ms;
            e.1 += 1;
            Ok(n)
        };
        // The wipe is per-rel attributable work (a big table's DELETE is not
        // free), so it rides `timed` like every other statement.
        for rel in derived_rels { timed(rel, &format!("DELETE FROM {}", tbl(rel)))?; }
        for group in stratify(derived_rules)? {
            for (comp_rules, recursive) in rel_components(&group, derived_rules) {
                let stmts: Vec<(&str, String)> = comp_rules.iter()
                    .map(|&ri| Ok((derived_rules[ri].head.rel.as_str(),
                                   lower_rule(derived_rules[ri], &self.rels)?)))
                    .collect::<Result<_>>()?;
                crate::activity::detail(format!(
                    "derived: {}", stmts.iter().map(|(r, _)| *r).collect::<Vec<_>>().join(", ")));
                if !recursive {
                    for (rel, sql) in &stmts { timed(rel, sql)?; }
                    continue;
                }
                // Native fast path: a tagged, halt-gated forward BFS
                // (`port_reach`'s contraction shape) runs in one in-memory pass
                // over src/walk.rs instead of a SQLite semi-naive fixpoint. The
                // recognizer is strict; a miss falls through to the SQL path
                // below with identical semantics.
                if self.try_native_halt_bfs(&comp_rules, derived_rules, &mut timed)? { continue; }
                // Defense twin of typecheck's `recursive-null-pad`: a NULL-padded
                // head in this component would re-insert the same row every
                // iteration (NULL != NULL never dedups), so the delta never
                // reaches 0. Bail instead of hanging.
                for &ri in &comp_rules {
                    if derived_rules[ri].head_null_pads() {
                        bail!("rule head for `{}` leaves column(s) NULL (`_` or named-arg padding) \
                               inside a recursive component — the fixpoint would not converge; \
                               bind every head column or break the cycle",
                              derived_rules[ri].head.rel);
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
                        r.has_agg() || self.rels.get(&r.head.rel).map(|m| m.key.is_some()).unwrap_or(false)
                    });
                if naive_fallback {
                    let mut iters = 0;
                    let mut total_promoted = 0usize;
                    let mut comp_rels: Vec<String> = comp_rules.iter()
                        .map(|&ri| derived_rules[ri].head.rel.clone()).collect();
                    comp_rels.sort();
                    comp_rels.dedup();
                    let row_max = fixpoint_row_max();
                    loop {
                        let mut delta = 0usize;
                        for (rel, sql) in &stmts { delta += timed(rel, sql)?; }
                        total_promoted = total_promoted.saturating_add(delta);
                        check_fixpoint_row_budget(&comp_rels, total_promoted, row_max)?;
                        // Every execution after pass 1 is a full-input re-run
                        // (the waste semi-naive removes) — count it.
                        if iters > 0 {
                            self.fixpoint_full_reruns.set(
                                self.fixpoint_full_reruns.get() + stmts.len());
                        }
                        iters += 1;
                        if delta == 0 { break; }
                        if iters > 100_000 { bail!("fixpoint did not converge"); }
                    }
                    continue;
                }
                self.rebuild_derived_seminaive(&comp_rules, derived_rules, &mut timed)?;
            }
        }
        self.save_stmt_ms(&stmt_ms)?;
        // P1: every rel this call rebuilt (deleted + re-derived) just completed
        // a pass, whatever row count it ended with — mark it so the NEXT
        // tick's `derived_incomplete_rels` sees it as legitimately derived,
        // not "never derived".
        self.mark_derived_complete(derived_rels)?;
        if !derived_rels.is_empty() {
            let tick = crate::activity::snapshot().tick;
            let detail = format!("{} rel(s): {}", derived_rels.len(), derived_rels.join(", "));
            crate::perflog::emit_phase(tick, "derived", t_rebuild.elapsed().as_millis() as u64, &detail);
        }
        Ok(())
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
        if std::env::var_os("DL_NO_HALT_BFS").is_some() { return Ok(false); }
        macro_rules! miss { ($why:expr) => {{
            if std::env::var_os("DL_BFS_TRACE").is_some() {
                eprintln!("[bfs-miss] {}: {}", derived_rules[comp_rules[0]].head.rel, $why);
            }
            return Ok(false);
        }}; }
        // All component rules must head the same single 2-col, non-key rel.
        let head_rel = derived_rules[comp_rules[0]].head.rel.clone();
        if comp_rules.iter().any(|&ri| derived_rules[ri].head.rel != head_rel) { miss!("multi-rel component"); }
        let head_meta = self.rels.get(&head_rel).unwrap();
        if head_meta.cols.len() != 2 || head_meta.key.is_some() { miss!("head not 2-col non-key"); }

        // Partition into recursive (body references the head rel) vs base. The
        // `.dl` discovery merge can load a file's rules in duplicate, so dedup
        // structurally identical rules first — otherwise the doubled recursive
        // rule reads as two and misses. Set semantics make duplicates a no-op.
        let is_rec = |r: &Rule| r.body.iter()
            .any(|b| matches!(b, BodyItem::Pos(a) if a.rel == head_rel));
        let dedup_by_debug = |ris: &[usize]| -> Vec<usize> {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            ris.iter().copied()
                .filter(|&ri| seen.insert(format!("{:?}", derived_rules[ri])))
                .collect()
        };
        let rec_ris = dedup_by_debug(&comp_rules.iter().copied()
            .filter(|&ri| is_rec(derived_rules[ri])).collect::<Vec<_>>());
        if rec_ris.len() != 1 { miss!(format!("rec rules = {}", rec_ris.len())); }
        let rec = derived_rules[rec_ris[0]];
        if rec.has_agg() || rec.temporal.is_some() { miss!("agg/temporal rec rule"); }

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
                    if self_atom.replace(a).is_some() { return Ok(false); }
                }
                BodyItem::Neg(a) => {
                    if halt_atom.replace(a).is_some() { return Ok(false); }
                }
                BodyItem::Pos(a) => {
                    if edge_atom.replace(a).is_some() { return Ok(false); }
                }
                other => miss!(format!("body item {:?}", std::mem::discriminant(other))),
            }
        }
        let (Some(self_atom), Some(halt_atom), Some(edge_atom)) = (self_atom, halt_atom, edge_atom)
            else { miss!("body != {self,halt,edge}"); };

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
        if halt_atom.terms[1..].iter().any(|t| !matches!(t, Term::Wild)) { miss!("halt extra cols bound"); }
        let halt_rel = halt_atom.rel.clone();

        let edge_meta = self.rels.get(&edge_rel)
            .ok_or_else(|| anyhow::anyhow!("halt-bfs edge relation {edge_rel} not declared"))?;
        if edge_meta.cols.len() != 2 { miss!("edge not 2-col"); }
        let halt_meta = self.rels.get(&halt_rel)
            .ok_or_else(|| anyhow::anyhow!("halt-bfs halt relation {halt_rel} not declared"))?;

        // Node identity must live in ONE space. The head (tag, node) and halt
        // node column are readable `text`; the edge columns are commonly `sym`
        // (i64 hashes with the text in `_strings`). Render sym edges back to text
        // on load so the frontier, halt set, and adjacency all key on the same
        // readable string. A head/halt column that is itself `sym` would need its
        // own decode — out of this recognizer's scope, so bail to the SQL path.
        use crate::ast::Type;
        if head_meta.cols[0].ty == Type::Sym || head_meta.cols[1].ty == Type::Sym {
            miss!("head column is sym (unhandled decode)");
        }
        if halt_meta.cols[0].ty == Type::Sym { miss!("halt column is sym (unhandled decode)"); }
        let edge_sym = [edge_meta.cols[0].ty == Type::Sym, edge_meta.cols[1].ty == Type::Sym];

        // Need at least one base rule to seed the frontier; a pure recursion with
        // no base case produces nothing and is better left to the SQL path.
        let base_ris = dedup_by_debug(&comp_rules.iter().copied()
            .filter(|&ri| !is_rec(derived_rules[ri])).collect::<Vec<_>>());
        if base_ris.is_empty() { miss!("no base rules"); }

        // === recognized: execute ===
        // 1. Base rules populate the head (already DELETE-cleared) = start frontier.
        for &ri in &base_ris {
            let sql = lower_rule(derived_rules[ri], &self.rels)?;
            timed(&head_rel, &sql)?;
        }

        let t = std::time::Instant::now();
        // 2. Edge adjacency, with an owned node interner we can extend for base
        //    nodes that carry no incident edge (they record themselves, no hops).
        let (ec0, ec1) = (edge_meta.cols[0].name.clone(), edge_meta.cols[1].name.clone());
        let (mut adj, mut node_names) = self.load_edges_rendered(&edge_rel, &ec0, &ec1, edge_sym)?;
        let mut node_id: HashMap<String, u32> =
            node_names.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect();
        let mut intern_node = |s: String,
                               adj: &mut Vec<Vec<u32>>,
                               node_names: &mut Vec<String>,
                               node_id: &mut HashMap<String, u32>| -> u32 {
            if let Some(&i) = node_id.get(&s) { return i; }
            let i = adj.len() as u32;
            adj.push(Vec::new());
            node_names.push(s.clone());
            node_id.insert(s, i);
            i
        };

        // 3. Start frontier: the base head rows (tag, node). Tag has its own id space.
        let mut tag_id: HashMap<String, u32> = HashMap::new();
        let mut tag_names: Vec<String> = Vec::new();
        let mut starts: Vec<(u32, u32)> = Vec::new();
        {
            let base_sql = format!("SELECT \"{}\", \"{}\" FROM {}",
                head_meta.cols[0].name, head_meta.cols[1].name, tbl(&head_rel));
            let mut stmt = self.db.conn().prepare(&base_sql)?;
            let rows = stmt.query_map([], |r| Ok((cell_as_string(r, 0)?, cell_as_string(r, 1)?)))?;
            for row in rows.flatten() {
                let (tag_s, node_s) = row;
                let tid = match tag_id.get(&tag_s) {
                    Some(&i) => i,
                    None => {
                        let i = tag_names.len() as u32;
                        tag_names.push(tag_s.clone());
                        tag_id.insert(tag_s, i);
                        i
                    }
                };
                let nid = intern_node(node_s, &mut adj, &mut node_names, &mut node_id);
                starts.push((tid, nid));
            }
        }

        // 4. Halt mask over the (possibly extended) node id space.
        let mut halt_mask = vec![false; adj.len()];
        {
            let hc0 = halt_meta.cols[0].name.clone();
            let sql = format!("SELECT DISTINCT \"{hc0}\" FROM {}", tbl(&halt_rel));
            let mut stmt = self.db.conn().prepare(&sql)?;
            let rows = stmt.query_map([], |r| cell_as_string(r, 0))?;
            for name in rows.flatten() {
                if let Some(&id) = node_id.get(&name) { halt_mask[id as usize] = true; }
            }
        }

        // 5. Native BFS, then replace the head with the full closure (the result
        //    is a superset of the base rows, so a clean DELETE + one insert holds).
        let t_load = t.elapsed();
        let t_bfs0 = std::time::Instant::now();
        let pairs = crate::walk::multi_source_halt_bfs(&adj, &starts, &halt_mask);
        let t_bfs = t_bfs0.elapsed();
        let t_build0 = std::time::Instant::now();
        let mut rows: Vec<Vec<Value>> = Vec::with_capacity(pairs.len());
        for (tid, nid) in pairs {
            rows.push(vec![
                Value::Text(tag_names[tid as usize].clone()),
                Value::Text(node_names[nid as usize].clone()),
            ]);
        }
        let t_build = t_build0.elapsed();
        let t_ins0 = std::time::Instant::now();
        self.db.exec(&format!("DELETE FROM {}", tbl(&head_rel)))?;
        // Bulk-load: with 100k+ rows the per-row maintenance of the head's
        // SECONDARY indexes dominates the insert. Drop them, insert once, then
        // rebuild each in a single pass (cheaper than incremental upkeep). The
        // unique autoindex (NULL `sql` in sqlite_master) stays — set semantics
        // need it, and the BFS output is already deduped so it never conflicts.
        // Rebuilt before return, so later strata still read through them.
        let sidx: Vec<(String, String)> = {
            let mut stmt = self.db.conn().prepare(
                "SELECT name, sql FROM sqlite_master \
                 WHERE tbl_name = ?1 AND type = 'index' AND sql IS NOT NULL")?;
            let rows = stmt.query_map([tbl(&head_rel)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.flatten().collect()
        };
        for (name, _) in &sidx { self.db.exec(&format!("DROP INDEX \"{name}\""))?; }
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        let insert_res = self.db.insert_rows(&tbl(&head_rel), &cols, &rows); // one flush, never N+1
        // Rebuild the dropped indexes BEFORE propagating any insert error, so a
        // failure never leaves the head table missing its secondary indexes.
        for (_, sql) in &sidx { self.db.exec(sql)?; }
        insert_res?;
        if std::env::var_os("DL_BFS_TRACE").is_some() {
            eprintln!("[bfs] {head_rel}: load={}ms bfs={}ms build={}ms insert={}ms rows={}",
                t_load.as_millis(), t_bfs.as_millis(), t_build.as_millis(),
                t_ins0.elapsed().as_millis(), rows.len());
        }
        self.save_stmt_ms_one(&format!("halt_bfs:{head_rel}"), t.elapsed().as_millis() as i64)?;
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
        let comp_rels: HashSet<String> = comp_rules.iter()
            .map(|&ri| derived_rules[ri].head.rel.clone()).collect();
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
            if occs.is_empty() { seed_ris.push(ri); } else { rec_ris.push((ri, occs)); }
        }

        for &ri in &seed_ris {
            let rule = derived_rules[ri];
            let sql = lower_rule(rule, &self.rels)?;
            total_promoted = total_promoted.saturating_add(timed(&rule.head.rel, &sql)?);
        }
        check_fixpoint_row_budget(&comp_rel_names, total_promoted, row_max)?;

        if rec_ris.is_empty() { return Ok(()); } // pure base case, no cycle to iterate

        // One `_delta_<rel>`/`_delta_new_<rel>` TEMP table pair per targeted
        // rel, shaped exactly like the rel (same cols, same full-row PK) so
        // `INSERT OR IGNORE` dedups variant output the same way the real
        // table would. Dropped and recreated every call (a TEMP table
        // persists for the connection's lifetime, and a program edit can
        // change a rel's column set between ticks).
        for rel in &comp_rels {
            let meta = self.rels.get(rel)
                .ok_or_else(|| anyhow::anyhow!("unknown relation {rel}"))?;
            let col_defs: Vec<String> = meta.cols.iter()
                .map(|c| format!("\"{}\" {}", c.name, c.ty.sql())).collect();
            let col_names: Vec<String> = meta.cols.iter()
                .map(|c| format!("\"{}\"", c.name)).collect();
            for prefix in ["_delta_", "_delta_new_"] {
                self.db.conn().execute(&format!("DROP TABLE IF EXISTS {prefix}{rel}"), [])?;
                self.db.conn().execute(&format!(
                    "CREATE TEMP TABLE {prefix}{rel} ({}, PRIMARY KEY ({}))",
                    col_defs.join(", "), col_names.join(", ")), [])?;
            }
        }
        // Seed delta_0 = every row the seed rules just produced. `rel_<rel>`
        // was emptied at the top of `rebuild_derived`, so everything in it
        // right now IS new as of this component's first pass.
        for rel in &comp_rels {
            let col_names: Vec<String> = self.rels.get(rel).unwrap().cols.iter()
                .map(|c| format!("\"{}\"", c.name)).collect();
            let cols = col_names.join(", ");
            timed(rel, &format!(
                "INSERT INTO _delta_{rel} ({cols}) SELECT {cols} FROM {}", tbl(rel)))?;
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
                    let sql = crate::lower::lower_rule_to_ex(rule, &self.rels, &target, &[], &overrides)?;
                    timed(&rule.head.rel, &sql)?;
                }
            }
            // Subtract rows already present in the real table — a variant
            // reading OTHER atoms at full strength can regenerate a
            // combination whose result was already derived in an earlier
            // iteration. What remains is truly new.
            let mut total_new = 0usize;
            for rel in &comp_rels {
                let col_names: Vec<String> = self.rels.get(rel).unwrap().cols.iter()
                    .map(|c| format!("\"{}\"", c.name)).collect();
                let cols = col_names.join(", ");
                timed(rel, &format!(
                    "DELETE FROM _delta_new_{rel} WHERE ({cols}) IN (SELECT {cols} FROM {})",
                    tbl(rel)))?;
                let n: i64 = self.db.conn().query_row(
                    &format!("SELECT COUNT(*) FROM _delta_new_{rel}"), [], |r| r.get(0))?;
                total_new += n as usize;
            }
            iters += 1;
            if total_new == 0 { break; }
            total_promoted = total_promoted.saturating_add(total_new);
            check_fixpoint_row_budget(&comp_rel_names, total_promoted, row_max)?;
            if iters > 100_000 { bail!("fixpoint did not converge"); }
            // Promote this iteration's new rows into the real relation, and
            // hand them to the NEXT iteration as its delta.
            for rel in &comp_rels {
                let col_names: Vec<String> = self.rels.get(rel).unwrap().cols.iter()
                    .map(|c| format!("\"{}\"", c.name)).collect();
                let cols = col_names.join(", ");
                timed(rel, &format!(
                    "INSERT OR IGNORE INTO {} ({cols}) SELECT {cols} FROM _delta_new_{rel}",
                    tbl(rel)))?;
                timed(rel, &format!("DELETE FROM _delta_{rel}"))?;
                timed(rel, &format!(
                    "INSERT INTO _delta_{rel} ({cols}) SELECT {cols} FROM _delta_new_{rel}"))?;
            }
        }
        Ok(())
    }

    /// Flush one rebuild's per-rel statement timings into `_stmt_ms` (replace
    /// the rebuilt rels' rows, leave the rest — the last known cost of a rel
    /// that did not rebuild this tick stays visible to rails). Each value is
    /// `(sum_ms, statement_count)` for that key's most recent pass.
    pub(crate) fn save_stmt_ms(&self, stmt_ms: &HashMap<String, (i64, i64)>) -> Result<()> {
        if stmt_ms.is_empty() { return Ok(()); }
        let names: Vec<String> = stmt_ms.keys().map(|r| format!("'{}'", r.replace('\'', "''"))).collect();
        self.db.exec(&format!("DELETE FROM _stmt_ms WHERE rel IN ({})", names.join(",")))?;
        let rows: Vec<Vec<Value>> = stmt_ms.iter()
            .map(|(rel, (ms, n))| vec![Value::Text(rel.clone()), Value::Int(*ms), Value::Int(*n)]).collect();
        self.db.insert_rows("_stmt_ms", &["rel", "ms", "n"], &rows)?;
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
            let n: i64 = self.db.conn().query_row(
                &format!("SELECT COUNT(*) FROM {}", scc_node_tbl(edge)), [], |r| r.get(0))?;
            if n == 0 { return Ok(Some(edge.to_string())); }
        }
        Ok(None)
    }

    /// True if any named rel's table is empty or absent — the cold-populate
    /// guard for corpus-gated families (spine): skip the family only when file
    /// content is unchanged AND its rels are already populated. A not-yet-
    /// created table counts as empty (cold start must run).
    pub(crate) fn any_rel_empty(&self, rels: &[&str]) -> Result<bool> {
        for rel in rels {
            let n: i64 = self.db.conn().query_row(
                &format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))
                .unwrap_or(0);
            if n == 0 { return Ok(true); }
        }
        Ok(false)
    }

    /// Load a 2-col edge relation, intern node names to dense u32 (transient),
    /// return adjacency + id->name. No persistent interning (see plan).
    /// Like `load_edges`, but decodes `sym`-typed endpoint columns back to their
    /// readable text via `_strings` so the adjacency keys on the same string
    /// space as a text-typed frontier/halt set. `sym[i]` selects rendering for
    /// column i; a false entry reads the column raw (already text). Rows whose
    /// sym endpoint has no `_strings` row (the empty sentinel) are dropped — a
    /// flow node is never the empty string, so this can't cut a real edge.
    pub(crate) fn load_edges_rendered(
        &self, edge: &str, c0: &str, c1: &str, sym: [bool; 2],
    ) -> Result<(Vec<Vec<u32>>, Vec<String>)> {
        if !sym[0] && !sym[1] {
            return self.load_edges(edge, c0, c1);
        }
        let expr = |col: &str, is_sym: bool, alias: &str| {
            if is_sym { format!("{alias}.content") } else { format!("e.\"{col}\"") }
        };
        let mut joins = String::new();
        if sym[0] { joins.push_str(&format!(" JOIN _strings s0 ON s0.id = e.\"{c0}\"")); }
        if sym[1] { joins.push_str(&format!(" JOIN _strings s1 ON s1.id = e.\"{c1}\"")); }
        let sql = format!("SELECT {}, {} FROM {} e{}",
            expr(c0, sym[0], "s0"), expr(c1, sym[1], "s1"), tbl(edge), joins);
        let mut stmt = self.db.conn().prepare(&sql)?;
        let mut intern: HashMap<String, u32> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let rows = stmt.query_map([], |r| Ok((cell_as_string(r, 0)?, cell_as_string(r, 1)?)))?;
        for row in rows.flatten() {
            let mut id = |s: String| -> u32 {
                if let Some(&i) = intern.get(&s) { return i; }
                let i = names.len() as u32; intern.insert(s.clone(), i); names.push(s); i
            };
            let a = id(row.0); let b = id(row.1);
            pairs.push((a, b));
        }
        let mut adj = vec![Vec::new(); names.len()];
        for (a, b) in pairs { adj[a as usize].push(b); }
        Ok((adj, names))
    }

    pub(crate) fn load_edges(&self, edge: &str, c0: &str, c1: &str) -> Result<(Vec<Vec<u32>>, Vec<String>)> {
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {}", tbl(edge));
        let mut stmt = self.db.conn().prepare(&sql)?;
        let mut intern: HashMap<String, u32> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let rows = stmt.query_map([], |r| Ok((cell_as_string(r, 0)?, cell_as_string(r, 1)?)))?;
        for row in rows.flatten() {
            let mut id = |s: String| -> u32 {
                if let Some(&i) = intern.get(&s) { return i; }
                let i = names.len() as u32; intern.insert(s.clone(), i); names.push(s); i
            };
            let a = id(row.0); let b = id(row.1);
            pairs.push((a, b));
        }
        let mut adj = vec![Vec::new(); names.len()];
        for (a, b) in pairs { adj[a as usize].push(b); }
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
            let meta = self.rels.get(*edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() < 2 { bail!("closure edge {edge} must have at least 2 columns"); }
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            let (nt, et) = (scc_node_tbl(edge), scc_edge_tbl(edge));
            let mut node_rows: Vec<Vec<Value>> = Vec::with_capacity(names.len());
            for (id, name) in names.iter().enumerate() {
                let comp = cond.comp[id] as i64;
                let cyc = cond.cyclic[cond.comp[id] as usize] as i64;
                node_rows.push(vec![Value::Text(name.clone()), Value::Int(comp), Value::Int(cyc)]);
            }
            let mut edge_rows: Vec<Vec<Value>> = Vec::new();
            for (cu, succ) in cond.cadj.iter().enumerate() {
                for &cw in succ { edge_rows.push(vec![Value::Int(cu as i64), Value::Int(cw as i64)]); }
            }
            self.db.exec(&format!("DELETE FROM {nt}"))?;
            self.db.exec(&format!("DELETE FROM {et}"))?;
            self.db.insert_rows(&nt, &["name", "comp", "cyclic"], &node_rows)?;
            self.db.insert_rows(&et, &["comp_src", "comp_dst"], &edge_rows)?;
            stmt_ms.insert(format!("closure:{edge}"), (t.elapsed().as_millis() as i64, 1));
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
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {}", tbl(edge));
        let mut stmt = self.db.conn().prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let a = cell_as_string(row, 0)?;
            let b = cell_as_string(row, 1)?;
            let mut h = blake3::Hasher::new();
            h.update(a.as_bytes());
            h.update(&[0]);
            h.update(b.as_bytes());
            for (x, y) in acc.iter_mut().zip(h.finalize().as_bytes().iter()) { *x ^= *y; }
        }
        Ok(acc)
    }

    /// Refresh `self.closure_cache` for the query phase, recondensing an edge
    /// ONLY when its rows actually changed. `dirty` is the set of edges whose
    /// source/derived relation was rebuilt this tick. An edge not in `dirty` is
    /// reused with zero work (no scan, no Tarjan). A dirty edge is digest-checked
    /// first; the Tarjan rebuild runs only if the digest moved. This replaces the
    /// old unconditional per-tick rebuild of every edge's condensation.
    pub(crate) fn refresh_cond_cache(&mut self, edges: &[&str], dirty: &HashSet<&str>) -> Result<()> {
        self.closure_cache.retain(|k, _| edges.iter().any(|e| *e == k.as_str()));
        // Timed only for the edges that actually pay the digest read + Tarjan
        // rebuild below — a reused/unchanged edge costs ~nothing and doesn't
        // need its own row. Key `cond_cache:<edge>`, sibling to `closure:<edge>`
        // (rebuild_closures' SCC-table write) in `_stmt_ms`.
        let mut stmt_ms: HashMap<String, (i64, i64)> = HashMap::new();
        for &edge in edges {
            let meta = self.rels.get(edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() < 2 { continue; }
            // Unaffected edge already cached → reuse, no scan.
            if !dirty.contains(edge) && self.closure_cache.contains_key(edge) { continue; }
            let t = std::time::Instant::now();
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let digest = self.edge_content_digest(edge, &c0, &c1)?;
            // Dirty but rows unchanged (e.g. comment edit) → reuse, skip Tarjan.
            if self.closure_cache.get(edge).map(|c| c.digest) == Some(digest) {
                stmt_ms.insert(format!("cond_cache:{edge}"), (t.elapsed().as_millis() as i64, 1));
                continue;
            }
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            self.recondensed += 1;
            let id = names.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect();
            self.closure_cache.insert(edge.to_string(), ClosureCache { cond, names, id, digest });
            stmt_ms.insert(format!("cond_cache:{edge}"), (t.elapsed().as_millis() as i64, 1));
        }
        self.save_stmt_ms(&stmt_ms)?;
        Ok(())
    }

    /// Answer `reaches(src=SEED, dst=?)` as a seeded BFS over the condensation.
    /// Same row set as the view's src-pinned slice, computed in microseconds.
    /// Seeded closure point query. `forward` = src pinned (walk out, callees);
    /// otherwise dst pinned (walk in, callers). Emits the same rows as the view's
    /// pinned slice, in microseconds.
    pub(crate) fn run_reaches_point(&self, q: &Query, cc: &ClosureCache, seed: &str, forward: bool) -> Result<()> {
        let meta = self.rels.get(&q.head.rel).unwrap();
        let header = |pos: usize| match &q.head.terms[pos] {
            Term::Var(v) => v.clone(),
            _ => meta.col_name(pos).to_string(),
        };
        let mut hits: Vec<&str> = Vec::new();
        if let Some(&sid) = cc.id.get(seed) {
            let walk = if forward { scc::reaches_from(&cc.cond, sid) } else { scc::reached_by(&cc.cond, sid) };
            hits = walk.iter().map(|&i| cc.names[i as usize].as_str()).collect();
            hits.sort_unstable();
        }
        let row = |h: &str| if forward {
            vec![serde_json::Value::String(seed.to_string()), serde_json::Value::String(h.to_string())]
        } else {
            vec![serde_json::Value::String(h.to_string()), serde_json::Value::String(seed.to_string())]
        };
        if self.query_json {
            let rows: Vec<Vec<serde_json::Value>> = hits.iter().map(|h| row(h)).collect();
            emit_query_json(&q.head.rel, &[header(0), header(1)], &rows);
        } else {
            println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
            for h in &hits {
                if forward { println!("  {seed}\t{h}"); } else { println!("  {h}\t{seed}"); }
            }
            println!("  ({} rows)\n", hits.len());
        }
        Ok(())
    }

    /// Both endpoints pinned: an existence probe answered by the seeded walk —
    /// same row semantics as the view's doubly-pinned slice (one row when src
    /// reaches dst, zero otherwise), without evaluating the view.
    pub(crate) fn run_reaches_pair(&self, q: &Query, cc: &ClosureCache, src: &str, dst: &str) -> Result<()> {
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
            vec![vec![serde_json::Value::String(src.to_string()),
                      serde_json::Value::String(dst.to_string())]]
        } else { Vec::new() };
        if self.query_json {
            emit_query_json(&q.head.rel, &[header(0), header(1)], &rows);
        } else {
            println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
            if hit { println!("  {src}\t{dst}"); }
            println!("  ({} rows)\n", rows.len());
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
        self.save_stmt_ms_one(&format!("closure_seed:{}", rule.head.rel), t.elapsed().as_millis() as i64)?;
        r
    }

    /// Body of `eval_closure_seed_rule`, split out so the wrapper can time the
    /// whole seeded-BFS-plus-flush pass into `_stmt_ms` (`closure_seed:<rel>`)
    /// without an early `?` return skipping the timing write.
    pub(crate) fn eval_closure_seed_rule_inner(&self, rule: &Rule, cs: &ClosureSeed) -> Result<()> {
        let head = &rule.head.rel;
        let head_meta = self.rels.get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != head_meta.cols.len() {
            bail!("head {} expects {} cols, got {}", head, head_meta.cols.len(), rule.head.terms.len());
        }
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        // The closure atom binds `free_var`; the pinned endpoint var (if any)
        // binds to the seed. The rule head may project these plus literals.
        let pinned_var: Option<&str> = rule.body.iter().find_map(|it| match it {
            BodyItem::Pos(a) if a.rel != *head && a.terms.len() == 2 => {
                // the closure atom: the non-free endpoint's var, if it is a var
                let other = a.terms.iter().find(|t| !matches!(t, Term::Var(v) if *v == cs.free_var));
                match other { Some(Term::Var(v)) => Some(v.as_str()), _ => None }
            }
            _ => None,
        });
        let cells: Result<Vec<Value>> = rule.head.terms.iter().map(|t| match t {
            Term::Str(s) => Ok(Value::Text(s.clone())),
            Term::Int(n) => Ok(Value::Int(*n)),
            Term::Var(v) if *v == cs.free_var => Ok(Value::Text(String::new())), // filled per-row
            Term::Var(v) if Some(v.as_str()) == pinned_var => Ok(Value::Text(cs.seed.clone())),
            Term::Var(v) => bail!("seeded closure rule head var '{v}' is neither the \
                                   free reached endpoint nor the pinned seed; only those \
                                   two (plus literals) can appear in the head"),
            Term::Wild => bail!("'_' not allowed in a seeded closure rule head"),
            Term::Interp(_) => bail!("interpolation not supported in a seeded closure rule head"),
            Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                Term::Arith { .. } => bail!("arithmetic not supported in a seeded closure rule head"),
                Term::Call { .. } => bail!("function call not supported in a seeded closure rule head"),
        }).collect();
        let template = cells?;
        let free_positions: Vec<usize> = rule.head.terms.iter().enumerate()
            .filter_map(|(i, t)| matches!(t, Term::Var(v) if *v == cs.free_var).then_some(i))
            .collect();

        let Some(cc) = self.closure_cache.get(&cs.edge) else { return Ok(()); };
        let mut rows: Vec<Vec<Value>> = Vec::new();
        if let Some(&sid) = cc.id.get(&cs.seed) {
            let walk = if cs.forward { scc::reaches_from(&cc.cond, sid) } else { scc::reached_by(&cc.cond, sid) };
            for &i in &walk {
                let name = &cc.names[i as usize];
                let mut row = template.clone();
                for &p in &free_positions { row[p] = Value::Text(name.clone()); }
                rows.push(row);
            }
        }
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        self.db.insert_rows(&tbl(head), &cols, &rows)?; // one flush, never N+1
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
        self.save_stmt_ms_one(&format!("scc:{}", rule.head.rel), t.elapsed().as_millis() as i64)?;
        r
    }

    /// Body of `eval_scc_rule`, split out so the wrapper can time the whole
    /// membership-materialization pass into `_stmt_ms` (`scc:<rel>`) past any
    /// early `?` return.
    pub(crate) fn eval_scc_rule_inner(&self, rule: &Rule) -> Result<()> {
        let edge = rule.scc_edge()
            .ok_or_else(|| anyhow::anyhow!("eval_scc_rule on a non-scc rule"))?;
        let head = &rule.head.rel;
        let head_meta = self.rels.get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != 2 || head_meta.cols.len() != 2 {
            bail!("scc head '{head}' must have exactly 2 columns (rep, member); got {}",
                  head_meta.cols.len());
        }
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        let Some(cc) = self.closure_cache.get(edge) else {
            return Ok(()); // edge not condensed this tick (e.g. empty) -> head empty
        };
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for c in 0..cc.cond.ncomp {
            let members = &cc.cond.members[c];
            if members.is_empty() { continue; }
            let rep = members.iter()
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
        self.db.insert_rows(&tbl(head), &cols, &rows)?; // one flush, never N+1
        Ok(())
    }

}
