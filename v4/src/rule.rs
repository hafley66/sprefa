//! `Rule` — a callable, parametric pipeline whose output sinks to a fact.
//!
//! Sprefa's `rule` is a named `Pipe<Cursor>` paired with a sink fact
//! table. Calling the rule = building `body > FactWrite(sink)` and
//! expanding it over a caller-supplied seed.
//!
//! Relation reads target the rule's sink table. `rule_name?(...)`
//! lowers through SQL. `rule_name(...)` applies/sends into the rule.
//!
//! Layer-2 input-set memoization (the `Op::probe` path) is deferred;
//! arrives once Component grows a `probe()` shim.

use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, Diag, ExpandOpts, ExpandStats, MemQueue, Next, Node, Pipe, PipeInstance,
    QueueBackend, RenderCtx,
};

use crate::fact::{FactRead, FactWrite};
use crate::runtime_graph::{ArgKey, OwnerNode, RuntimeGraph};
use crate::sql::SqlQueryComponent;
use crate::Cursor;
use effect_runtime::v2::FactStore;
#[cfg(test)]
use effect_runtime::v2::MemFactStore;
// Note: FactStore is now a trait. Rule constructors take
// `Arc<dyn FactStore<Cursor>>` so callers can pick MemFactStore or
// SqliteFactStore at the seam.

/// A callable rule. `body` is the pipe that produces rows; `sink_table`
/// is where they land. Cloning is cheap (Arc-shaped fields).
#[derive(Clone)]
pub struct Rule {
    pub name: Arc<str>,
    pub store: Arc<dyn FactStore<Cursor>>,
    pub sink_table: Arc<str>,
    pub sink_cols: Arc<Vec<Arc<str>>>,
    body_steps: Arc<Vec<Arc<dyn Component<Next = Cursor>>>>,
    /// Closure env. Terms snapshotted from the ENCLOSING rule's
    /// invocation at the moment this (nested) rule became a value
    /// (`outer(A:1).inner` captures `A=1`). Overlaid into the invoke
    /// seed ABOVE the ambient cursor but BELOW the call's explicit
    /// assignments, so a closed-over `A` survives even when the
    /// closure escapes outer's dynamic extent, while an explicit arg
    /// at the call site still wins. Empty for ordinary rules; cheap
    /// `Arc` clone, immutable after capture.
    pub captured: Arc<Vec<(Arc<str>, Arc<str>)>>,
}

impl Rule {
    pub fn new(
        name: impl Into<Arc<str>>,
        store: Arc<dyn FactStore<Cursor>>,
        sink_table: impl Into<Arc<str>>,
        sink_cols: &[&str],
        body: Pipe<Cursor>,
    ) -> Self {
        let sink_table: Arc<str> = sink_table.into();
        store.declare(&sink_table, sink_cols);
        let cols: Vec<Arc<str>> = sink_cols.iter().map(|s| Arc::<str>::from(*s)).collect();
        Self {
            name: name.into(),
            store,
            sink_table,
            sink_cols: Arc::new(cols),
            body_steps: Arc::new(body.steps),
            captured: Arc::new(Vec::new()),
        }
    }

    /// Clone this rule into a closure over `env` (term name → value),
    /// snapshotted from the enclosing invocation. Used by the
    /// `outer.inner` dot projection (Task 5) to make a nested rule a
    /// first-class escaping closure value.
    pub fn with_captured(mut self, env: Vec<(Arc<str>, Arc<str>)>) -> Self {
        self.captured = Arc::new(env);
        self
    }

    /// `rule = fact` shape: empty body, declaration-only. The first
    /// FactWrite still runs its insert side; `run_with` over an empty
    /// seed is a declaration no-op.
    pub fn passthrough(
        name: impl Into<Arc<str>>,
        store: Arc<dyn FactStore<Cursor>>,
        sink_table: impl Into<Arc<str>>,
        sink_cols: &[&str],
    ) -> Self {
        Self::new(name, store, sink_table, sink_cols, Pipe::new())
    }

    pub fn is_passthrough(&self) -> bool {
        self.body_steps.is_empty()
    }

    /// Build the executable pipe (`body > FactWrite(sink)`).
    pub fn into_pipe(self) -> Pipe<Cursor> {
        let mut steps: Vec<Arc<dyn Component<Next = Cursor>>> = (*self.body_steps).clone();
        if let Some(last) = steps.last_mut() {
            if let Some(sql) = last
                .describe()
                .and_then(|desc| desc.downcast_ref::<SqlQueryComponent>())
            {
                *last = Arc::new(
                    sql.with_terminal_fact_write(self.store.clone(), self.sink_table.clone()),
                );
                return Pipe::from_steps(steps);
            }
        }
        steps.push(Arc::new(FactWrite::new(self.store, self.sink_table)));
        Pipe::from_steps(steps)
    }

    /// Run this rule over caller-supplied seed cursors. Builds the
    /// pipe instance and calls `expand`. Bypass for the input-set
    /// memoization path; seed is the caller's, not the rule's.
    pub fn run_with(
        self,
        queue: Arc<dyn QueueBackend<Cursor>>,
        seed: Vec<Arc<Cursor>>,
        opts: ExpandOpts,
    ) -> ExpandStats {
        let inst = self.into_pipe().into_instance();
        expand(&inst, queue, seed, opts)
    }

    /// `rule?` — produce a `FactRead` Component aimed at this rule's
    /// sink table. The Component slots into any pipe.
    pub fn query(
        store: Arc<dyn FactStore<Cursor>>,
        sink_table: impl Into<Arc<str>>,
        key_term: impl Into<Arc<str>>,
        project: &[&str],
    ) -> FactRead {
        FactRead::new(store, sink_table, key_term, project)
    }
}

#[derive(Clone)]
pub enum RuleInvokeValue {
    Term(Arc<str>),
    Value,
    Literal(Arc<str>),
}

#[derive(Clone)]
pub struct RuleInvokeAssign {
    pub col: Arc<str>,
    pub value: RuleInvokeValue,
}

pub struct RuleInvokeComponent {
    rule: Rule,
    assignments: Arc<Vec<RuleInvokeAssign>>,
    force: bool,
    // Phase 3: the unbounded in-RAM `cache` / `fired_at_gen` BTreeMaps
    // are gone. The body-result memo is now the disk-backed
    // `crate::memo::Memo` owned by the RuntimeGraph (hot StripedLru +
    // cold MEMO table). Probe → replay vs run lives in `render`.
}

impl RuleInvokeComponent {
    pub fn new(rule: Rule, assignments: Vec<RuleInvokeAssign>, force: bool) -> Self {
        Self {
            rule,
            assignments: Arc::new(assignments),
            force,
        }
    }

    fn cache_key(&self, c: &Cursor) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(self.rule.name.as_bytes());
        h.update(b"\0");
        h.update(&c.content_hash());
        // Distinct closures (same rule, different captured env) must
        // not share a memo entry.
        for (k, v) in self.rule.captured.iter() {
            h.update(b"\0cap:");
            h.update(k.as_bytes());
            h.update(b"=");
            h.update(v.as_bytes());
        }
        for assignment in self.assignments.iter() {
            h.update(b"\0");
            h.update(assignment.col.as_bytes());
            h.update(b"=");
            match &assignment.value {
                RuleInvokeValue::Term(term) => {
                    h.update(b"term:");
                    h.update(term.as_bytes());
                }
                RuleInvokeValue::Value => {
                    h.update(b"value");
                }
                RuleInvokeValue::Literal(value) => {
                    h.update(b"literal:");
                    h.update(value.as_bytes());
                }
            }
        }
        *h.finalize().as_bytes()
    }

    /// Canonical positional arg keys for `(rule, c, assignments)`.
    /// `Literal` -> interned StringId of the literal payload.
    /// `Term`    -> interned StringId of the cursor-resolved value, or
    ///              Unbound when the cursor has no value for the term.
    /// `Value`   -> interned StringId of the cursor's primary value.
    /// Used for `RuleMemo` keying and graph-owner input_key derivation.
    fn arg_keys(&self, graph: &RuntimeGraph, c: &Cursor) -> Vec<ArgKey> {
        let mut out = Vec::with_capacity(self.assignments.len() + self.rule.captured.len());
        // Closure env participates in owner identity: outer(A:1).inner
        // and outer(A:2).inner are distinct owners.
        for (k, v) in self.rule.captured.iter() {
            out.push(ArgKey::Literal(
                graph.store.intern_string(&format!("{k}={v}")),
            ));
        }
        for assignment in self.assignments.iter() {
            let key = match &assignment.value {
                RuleInvokeValue::Literal(value) => {
                    ArgKey::Literal(graph.store.intern_string(value.as_ref()))
                }
                RuleInvokeValue::Term(term) => match c.get(term) {
                    Some(value) => ArgKey::Literal(graph.store.intern_string(value)),
                    None => ArgKey::Unbound,
                },
                RuleInvokeValue::Value => {
                    ArgKey::Literal(graph.store.intern_string(c.value.as_ref()))
                }
            };
            out.push(key);
        }
        out
    }

    /// Memoize the OwnerNode for this call's arg tuple. Same
    /// `(rule_name, arg_keys)` => same OwnerNode across calls.
    fn memoize_owner(&self, graph: &RuntimeGraph, c: &Cursor) -> OwnerNode {
        let rule_name_id = graph.store.intern_string(self.rule.name.as_ref());
        let args = self.arg_keys(graph, c);
        let rule_name = self.rule.name.clone();
        let args_for_build = args.clone();
        graph.rule_memo_owner(rule_name_id, args, move || {
            // input_key is the canonical fingerprint of the arg tuple;
            // it doubles as the durable identity seed for declare_owner.
            let input_key = arg_keys_input_key(&rule_name, &args_for_build);
            let ast_uri = format!("sprf://ast/rule/{}", rule_name);
            graph.declare_owner(&ast_uri, None, &input_key, "head", "pure", 0)
        })
    }

    fn seed_for(&self, ctx: &RenderCtx, c: &Cursor) -> Option<Cursor> {
        let mut seed = c.clone();
        // Closure env: overlay snapshotted terms ABOVE the ambient
        // cursor (so a closed-over `A` survives when the closure
        // escapes), BELOW the explicit assignments applied next.
        for (k, v) in self.rule.captured.iter() {
            seed.set_arc(k, v.clone());
        }
        for assignment in self.assignments.iter() {
            match &assignment.value {
                RuleInvokeValue::Term(term) => {
                    let Some(value) = c.get(term) else {
                        ctx.diag.emit(Diag::error(
                            "rule/missing-arg",
                            format!(
                                "{}: term `{term}` is required for column `{}`",
                                self.rule.name, assignment.col,
                            ),
                        ));
                        return None;
                    };
                    seed.set(&assignment.col, value);
                }
                RuleInvokeValue::Value => {
                    seed.set_arc(&assignment.col, c.value.clone());
                }
                RuleInvokeValue::Literal(value) => {
                    seed.set_arc(&assignment.col, value.clone());
                }
            }
        }
        Some(seed)
    }

    fn run_rule(&self, ctx: &RenderCtx, seed: Cursor) -> Node<Cursor> {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let mut steps = self.rule.clone().into_pipe().steps;
        steps.push(Arc::new(InvokeCollector { sink: sink.clone() }));
        let inst = PipeInstance::new(steps);
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let mut opts = ExpandOpts::default()
            .with_bus(ctx.bus.clone())
            .with_diag(ctx.diag.clone());
        // Phase 3: carry the RuntimeGraph into the rule body so its
        // inner reads (fs/read/fact) record into MEMO_DEPS under this
        // owner — the memo's staleness check is a gen-compare over
        // exactly those recorded deps.
        if let Some(graph) = ctx.runtime::<RuntimeGraph>() {
            opts = opts.with_runtime(graph);
        }
        expand(&inst, queue, vec![Arc::new(seed)], opts);

        let rows = sink.lock().unwrap();
        match rows.len() {
            0 => Node::Done,
            1 => Node::Emit(Arc::new(rows[0].clone())),
            _ => Node::Many(
                rows.iter()
                    .cloned()
                    .map(|cursor| Node::Emit(Arc::new(cursor)))
                    .collect(),
            ),
        }
    }
}

impl Component for RuleInvokeComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let graph_opt = ctx.runtime::<RuntimeGraph>();

        // No graph in ctx ⇒ no memo available; run the body directly.
        let Some(graph) = graph_opt.as_ref() else {
            return match self.seed_for(ctx, c) {
                Some(seed) => self.run_rule(ctx, seed),
                None => Node::Done,
            };
        };

        // Materialize the OwnerNode (durable identity) and derive the
        // memo coordinates: owner_op_id = owner uri, in_key = the
        // (rule, cursor, assignments) cache key hex.
        let owner = self.memoize_owner(graph.as_ref(), c);
        let owner_id = owner.uri().to_string();
        let in_key = {
            let k = self.cache_key(c);
            let mut s = String::with_capacity(64);
            for b in &k {
                s.push_str(&format!("{b:02x}"));
            }
            s
        };

        // Recorded deps from a prior run (Phase 2). Validity is a
        // gen-compare over exactly these; the body is never re-run to
        // check freshness.
        let recorded = graph.memo_deps_ch_of(&owner_id, &in_key);
        let deps: Vec<(crate::source_clock::SourceId, u64, String, String)> = recorded
            .iter()
            .filter_map(|(hex, gen, content, path)| {
                sid_from_hex(hex).map(|s| (s, *gen, content.clone(), path.clone()))
            })
            .collect();

        if !self.force {
            // Content-hash staleness: replay only if every recorded
            // dep's file bytes are byte-identical now. A coarse rule
            // whose input is a directory (auto_run root) has an
            // unreadable fingerprint ⇒ STALE ⇒ body re-runs, and the
            // INNER `ast`/`read` owners' per-file content-memo carries
            // the replay win. No clock / event bump consulted.
            if let Some((val, stale)) = graph.memo().probe_ch(&owner_id, &in_key, &deps) {
                if !stale {
                    // Pure replay: do NOT run the rule body.
                    return rows_to_node(val.out_rows);
                }
            }
        }

        // Miss / stale / forced → run the body. Inner ops record their
        // reads into MEMO_DEPS under `owner_id` (run_rule threads the
        // graph into the inner expand).
        let result = match self.seed_for(ctx, c) {
            Some(seed) => self.run_rule(ctx, seed),
            None => Node::Done,
        };

        // The rule call's conservative dependency is its input source:
        // the file the input cursor names (FS term or focal value).
        // For a pure rule body this is sound — unchanged input file +
        // unchanged referenced tables ⇒ unchanged output. Inner fact
        // reads also recorded their table sources under `owner_id` via
        // the threaded graph (Phase 2).
        if let Some(path) = rule_input_path(c) {
            use crate::source_clock::{SourceClock, SourceId};
            let sid = SourceId::for_file(&path);
            let gen_seen = graph.clock().current_gen(sid);
            // Fingerprint the input file's current bytes (a directory
            // input hashes to None → empty hex → STALE next run, which
            // is the correct conservative default for a coarse rule).
            let content_hex = std::fs::read(&path)
                .ok()
                .map(|b| blake3::hash(&b).to_hex().to_string())
                .unwrap_or_default();
            graph.record_memo_dep(
                &owner_id,
                &in_key,
                sid,
                gen_seen,
                &content_hex,
                &path,
            );
        }

        // Persist the result under the freshly-recorded dep set so the
        // next run can replay it while deps are unchanged.
        let fresh = graph.memo_deps_of(&owner_id, &in_key);
        let fresh_deps: Vec<(crate::source_clock::SourceId, u64)> = fresh
            .iter()
            .filter_map(|(hex, gen)| sid_from_hex(hex).map(|s| (s, *gen)))
            .collect();
        if let Some(rows) = node_to_rows(&result) {
            let out_keys: Vec<String> = rows.iter().map(|r| key_hash_hex(r)).collect();
            graph
                .memo()
                .put(&owner_id, &in_key, &fresh_deps, rows, out_keys);
        }
        result
    }
}

/// The file path a rule-call input names: the `FS` term if present,
/// else the focal value when it looks like a path. `None` ⇒ no file
/// dependency to track (the rule still memoizes on its in_key + any
/// inner table reads).
fn rule_input_path(c: &Cursor) -> Option<String> {
    if let Some(fs) = c.get("FS") {
        if !fs.is_empty() {
            return Some(fs.to_string());
        }
    }
    let v = c.value();
    if !v.is_empty() {
        return Some(v.to_string());
    }
    None
}

/// Parse a 64-char lowercase-hex `SourceId`.
fn sid_from_hex(hex: &str) -> Option<crate::source_clock::SourceId> {
    if hex.len() != 64 {
        return None;
    }
    let mut d = [0u8; 32];
    for (i, byte) in d.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(crate::source_clock::SourceId(d))
}

/// Whole-cursor key hash hex (Phase 0 default: empty key_terms).
fn key_hash_hex(c: &Cursor) -> String {
    let h = c.key_hash(&[]);
    let mut s = String::with_capacity(64);
    for b in &h {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Flatten a `Node<Cursor>` into its emitted rows for memo storage.
/// Returns `None` when the result contains a `Yield` (a paused effect
/// carries a `Wake` that cannot round-trip through a row blob — such a
/// result must not be memoized/replayed).
fn node_to_rows(n: &Node<Cursor>) -> Option<Vec<Cursor>> {
    match n {
        Node::Done => Some(Vec::new()),
        Node::Emit(c) => Some(vec![c.as_ref().clone()]),
        Node::Many(children) => {
            let mut out = Vec::new();
            for ch in children {
                out.extend(node_to_rows(ch)?);
            }
            Some(out)
        }
        Node::Yield { .. } => None,
    }
}

/// Reconstruct the replay `Node<Cursor>` from stored rows.
fn rows_to_node(rows: Vec<Cursor>) -> Node<Cursor> {
    match rows.len() {
        0 => Node::Done,
        1 => Node::Emit(Arc::new(rows.into_iter().next().unwrap())),
        _ => Node::Many(
            rows.into_iter()
                .map(|c| Node::Emit(Arc::new(c)))
                .collect(),
        ),
    }
}

/// Deterministic input_key for `declare_owner`. Hashes `(rule_name,
/// arg_keys)` into hex so durable identity matches in-memory memo.
fn arg_keys_input_key(rule: &str, args: &[ArgKey]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(rule.as_bytes());
    h.update(b"\0");
    for arg in args {
        match arg {
            ArgKey::Literal(id) => {
                h.update(b"L:");
                h.update(&id.0.to_le_bytes());
            }
            ArgKey::Unbound => {
                h.update(b"U");
            }
        }
        h.update(b"\0");
    }
    let hash = h.finalize();
    let bytes = hash.as_bytes();
    let mut s = String::with_capacity(32);
    for b in &bytes[..16] {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

struct InvokeCollector {
    sink: Arc<Mutex<Vec<Cursor>>>,
}

impl Component for InvokeCollector {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.sink.lock().unwrap().push(c.clone());
        Node::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effect_runtime::v2::{MemQueue, Node, PipeInstance, QueueBackend, RenderCtx};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct Collector {
        sink: Arc<Mutex<Vec<Cursor>>>,
    }
    impl Component for Collector {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.sink.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn cursor(value: &str, kvs: &[(&str, &str)]) -> Arc<Cursor> {
        let mut c = Cursor::default();
        c.set_value(value);
        for (k, v) in kvs {
            c.set(k, *v);
        }
        Arc::new(c)
    }

    #[test]
    fn passthrough_rule_writes_seed_into_sink() {
        let store = Arc::new(MemFactStore::<Cursor>::new());
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let rule = Rule::passthrough("noop", store.clone(), "out", &["FILE"]);

        rule.run_with(
            queue,
            vec![
                cursor("a", &[("FILE", "x.rs")]),
                cursor("b", &[("FILE", "y.rs")]),
            ],
            ExpandOpts::default(),
        );

        assert_eq!(store.len("out"), 2);
    }

    #[test]
    fn rule_with_body_transforms_then_writes() {
        // body: a tiny Component that uppercases cursor.value.
        struct Upcase;
        impl Component for Upcase {
            type Next = Cursor;
            fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
                let mut next = c.clone();
                next.value = Arc::from(c.value.to_uppercase().as_str());
                Node::Emit(Arc::new(next))
            }
        }

        let store = Arc::new(MemFactStore::<Cursor>::new());
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let body = Pipe::new().step(Arc::new(Upcase));
        let rule = Rule::new("upcase", store.clone(), "loud", &[], body);

        rule.run_with(queue, vec![cursor("hi", &[])], ExpandOpts::default());

        let rows = store.rows_of("loud");
        assert_eq!(rows.len(), 1);
        assert_eq!(&*rows[0].value, "HI");
    }

    #[test]
    fn rule_ending_in_sql_fuses_terminal_fact_write() {
        let store = Arc::new(MemFactStore::<Cursor>::new());
        let sql = SqlQueryComponent::new(
            store.clone(),
            "SELECT input.__cursor_idx, input.value AS value, input.NAME AS NAME FROM input",
        );
        let body = Pipe::new().step(Arc::new(sql));
        let rule = Rule::new("names", store.clone(), "names", &["NAME"], body);

        let pipe = rule.clone().into_pipe();
        assert_eq!(pipe.len(), 1);
        let fused = pipe.steps[0]
            .describe()
            .and_then(|desc| desc.downcast_ref::<SqlQueryComponent>())
            .expect("terminal sql component");
        assert!(fused.has_terminal_fact_write());

        let queue = Arc::new(MemQueue::new());
        rule.run_with(
            queue.clone(),
            vec![
                cursor("one", &[("NAME", "alpha")]),
                cursor("two", &[("NAME", "beta")]),
            ],
            ExpandOpts::default(),
        );

        assert_eq!(queue.depth(), 0);
        let rows = store.rows_of("names");
        assert_eq!(rows.len(), 2);
        assert_eq!(&*rows[0].value, "one");
        assert_eq!(rows[0].get("NAME"), Some("alpha"));
        assert_eq!(&*rows[1].value, "two");
        assert_eq!(rows[1].get("NAME"), Some("beta"));
    }

    #[test]
    fn rule_query_reads_from_sink_table() {
        // Step 1: populate via run_with.
        let store = Arc::new(MemFactStore::<Cursor>::new());
        let q1: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let rule = Rule::passthrough("strings", store.clone(), "strings", &["FILE", "HIT"]);
        rule.run_with(
            q1,
            vec![
                cursor("hi", &[("FILE", "a.rs"), ("HIT", "hi")]),
                cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]),
                cursor("z", &[("FILE", "b.rs"), ("HIT", "z")]),
            ],
            ExpandOpts::default(),
        );
        assert_eq!(store.len("strings"), 3);

        // Step 2: query via Rule::query. Pipe: [Reader, Collector].
        let q2: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(Rule::query(store.clone(), "strings", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe,
            q2,
            vec![cursor("seed", &[("FILE", "a.rs")])],
            ExpandOpts::default(),
        );

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 2);
    }

    struct CountBody {
        count: Arc<AtomicUsize>,
    }

    impl Component for CountBody {
        type Next = Cursor;

        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
            let mut next = c.clone();
            next.set("COUNT", n.to_string());
            Node::Emit(Arc::new(next))
        }
    }

    #[test]
    fn rule_invoke_without_graph_runs_each_time() {
        // Phase 3 contract: the body-result memo is the disk-backed
        // Memo owned by the RuntimeGraph. With no graph in ctx there
        // is no memo to consult, so the body runs every render (the
        // documented fallthrough). The replay path is exercised by
        // the integration test
        // `rule_memo_replays_unchanged_source_zero_dispatch` in
        // tests/runtime_graph_smoke.rs.
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let count = Arc::new(AtomicUsize::new(0));
        let body = Pipe::new().step(Arc::new(CountBody {
            count: count.clone(),
        }));
        let rule = Rule::new("counted", store, "counted", &["COUNT"], body);
        let input = cursor("seed", &[]);
        let ctx = RenderCtx::new(0, 0, 0);

        let cached = RuleInvokeComponent::new(rule.clone(), Vec::new(), false);
        let first = cached.render(&ctx, &input);
        let second = cached.render(&ctx, &input);
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(node_count_value(&first), Some("1".to_string()));
        assert_eq!(node_count_value(&second), Some("2".to_string()));

        let forced = RuleInvokeComponent::new(rule, Vec::new(), true);
        let third = forced.render(&ctx, &input);
        let fourth = forced.render(&ctx, &input);
        assert_eq!(count.load(Ordering::SeqCst), 4);
        assert_eq!(node_count_value(&third), Some("3".to_string()));
        assert_eq!(node_count_value(&fourth), Some("4".to_string()));
    }

    fn node_count_value(node: &Node<Cursor>) -> Option<String> {
        match node {
            Node::Emit(cursor) => cursor.get("COUNT").map(|s| s.to_string()),
            Node::Many(nodes) => nodes.first().and_then(node_count_value),
            Node::Done | Node::Yield { .. } => None,
        }
    }
}
