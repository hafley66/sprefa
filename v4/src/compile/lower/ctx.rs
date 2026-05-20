use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{ByteRange, FactStore, Pipe, ProbeSink};

use crate::config::SprfConfig;
use crate::rule::Rule;
use crate::store::SprfStore;
use crate::telemetry::PipelineTelemetry;
use crate::vfs::VfsOverlay;
use crate::{Cursor, Interner};

/// Lower-time context. Carries the fact store, optional Interner, root
/// path, and a binding map used to resolve `${X}` interpolations in
/// DSL bodies. Builder-style mutation via `with_*`.
pub struct LowerCtx {
    pub store: Arc<dyn FactStore<Cursor>>,
    pub interner: Option<Arc<Interner>>,
    pub root: PathBuf,
    /// Directory of the `.sprf` script currently being lowered. `fs`'s
    /// branch-3 fallback walks THIS dir when an input cursor carries no
    /// path value and no repo+rev coord. Distinct from `root`: `root`
    /// follows `--root` (the corpus), `sprf_dir` always tracks the
    /// script file's own folder. Defaults to `root` when unset so
    /// non-host callers (tests) keep prior behavior.
    pub sprf_dir: PathBuf,
    /// Capture name → Pipe that grounds to a constant string. Used by
    /// `str` to substitute `${X}` at lower-time.
    pub bindings: HashMap<Arc<str>, Pipe<Cursor>>,
    /// Lowered bodied rules available for body invocation at later call
    /// sites in the same compile pass.
    rules: Arc<Mutex<HashMap<Arc<str>, Rule>>>,
    /// Optional per-emit probe sink. When set, the walker wraps every
    /// lowered Component in a `SpannedComponent` that fires a probe on
    /// each emitted cursor, tagged with the source op's byte range.
    /// Default = `None` = zero overhead beyond a single Option check.
    pub probe: Option<Arc<dyn ProbeSink<Cursor>>>,
    /// Optional content-derived intern store. Set via
    /// `with_sprf_store(...)` from the host (`SprfState::run` or
    /// equivalent). When present, source/pattern emitters stamp the
    /// coord-space side of Cursor (`value_id`, `at`, `terms`).
    pub sprf_store: Option<Arc<SprfStore>>,
    /// Layer 5a — XDG-style repo config used by the bare `repo()` op
    /// to drive a generator over `SprfConfig.repos`. Threaded by the
    /// host (`SprfState::run` / `SprfState::ingest`); call sites that
    /// don't set it leave the bare-form generator emitting zero rows.
    pub config: Option<Arc<SprfConfig>>,
    /// Optional runtime telemetry sinks. When present, lowerers attach
    /// op-specific counters and the host may wrap lowered components
    /// with timing probes.
    pub telemetry: Option<Arc<PipelineTelemetry>>,
    /// Phase 4 — buffer-text overlay handed down from `SprfState.vfs`.
    /// Lowerers pass this to source-reading components (Ast, AstYaml,
    /// Read, Json) so their `SourceReader::with_overlay` reads buffer
    /// text in preference to disk. `None` on CLI/test paths.
    pub vfs: Option<Arc<VfsOverlay>>,
    /// Per-compile counter handed out to force-apply call sites so
    /// content-deduped fact stores still see distinct rows across
    /// repeated `r!.(…)` invocations. The counter is stamped onto the
    /// apply seed as `_call_seq`; cache-respecting bare/apply calls
    /// don't touch it.
    apply_seq: Arc<AtomicU64>,
    /// Per-compile counter handed out to every lexical rule QUERY
    /// (`r?()`) / `sql` site so syntactically-identical queries get
    /// distinct durable mount lineages. Lowering is deterministic and
    /// single-threaded per compile, so the Nth query site always draws
    /// the same ordinal across runs of the same source — replay
    /// reconstructs the same `mount_id` because the tag rides inside
    /// the persisted SQL text. See `sql::with_query_site_tag`.
    query_site: Arc<AtomicU64>,
    /// Warm-skip predicate for the `fs` source. `Some(f)` ⇒ `f(path)`
    /// is `true` for files UNCHANGED since the prior run (recorded
    /// `_memo_deps` identity == current no-read identity); `fs` then
    /// never emits a cursor for them, so they are never parsed,
    /// probed, or re-emitted (their `hits` rows persist in the fact
    /// store, retraction being owner-scoped). `None` / cold run ⇒ no
    /// skipping. Composed with the static `glob` filter in `FsDef`.
    pub warm_skip: Option<Arc<dyn Fn(&str) -> bool + Send + Sync>>,
    /// Warm dirty-slice for the `fs` source. `Some(paths)` ⇒ `fs` emits
    /// exactly these (changed tracked deps ∪ git-dirty) instead of
    /// running its `WalkBuilder` over the whole tree — no 63k-entry
    /// readdir/stat on a warm run. `None` ⇒ cold run ⇒ full walk.
    /// `Some(empty)` ⇒ warm, nothing changed ⇒ `fs` emits nothing.
    pub warm_slice: Option<Arc<Vec<PathBuf>>>,
    /// Span of the op call currently being lowered. Set by the
    /// Registry around each `OperatorDef::lower_call_with_chain`
    /// invocation. Ops that anchor end-of-run diagnostics (no per-row
    /// span available) read this from `lower(...)`. `Cell` is fine
    /// because lower runs single-threaded per compile.
    pub current_call_span: Cell<Option<ByteRange>>,
    /// Whether the pipe currently being walked re-derives its FULL
    /// extent each run (constant literal/stream head, no
    /// fs/read/ast/glob/repo/lsp/http source). `walk_pipe` sets this
    /// from the statement's head ops before walking steps; a rule write
    /// reads it to tag its `FactWrite::full_extent` for owner-scoped
    /// retraction soundness. `Cell` — lower is single-threaded.
    pub pipe_full_extent: Cell<bool>,
    /// Lexical nesting of rule bodies currently being lowered. The
    /// walker pushes the enclosing `rule(:name)`'s name before
    /// recursing into its `{ block }` and pops after. `register_rule`
    /// keys by the dotted join of this path so a nested
    /// `rule(:inner)` lands as `outer.inner`; `get_rule` resolves a
    /// bare name by walking this path outward (the binding graph for
    /// rule names). Single-threaded per compile, hence `RefCell`.
    scope_path: RefCell<Vec<Arc<str>>>,
    /// Declared type tag per `(table, column)`. Written when a type's
    /// column is itself typed (the tree-sitter importer is the main
    /// writer: a node field whose value is another node kind). Read by
    /// `resolve_dot` to re-`.typed()` a projected value so chained
    /// access (`x.a.b.c`) keeps resolving past the first hop. Empty for
    /// scalar columns (the common case). Single-threaded per compile,
    /// hence `RefCell`, mirroring `scope_path`.
    col_types: RefCell<HashMap<Arc<str>, HashMap<Arc<str>, Arc<str>>>>,
}

impl LowerCtx {
    pub fn next_apply_seq(&self) -> u64 {
        self.apply_seq.fetch_add(1, Ordering::Relaxed)
    }

    /// Next stable ordinal for a rule QUERY / `sql` site (see
    /// `query_site` field doc).
    pub fn next_query_site(&self) -> u64 {
        self.query_site.fetch_add(1, Ordering::Relaxed)
    }
}

impl LowerCtx {
    pub fn new(store: Arc<dyn FactStore<Cursor>>, root: PathBuf) -> Self {
        Self {
            store,
            interner: None,
            sprf_dir: root.clone(),
            root,
            bindings: HashMap::new(),
            rules: Arc::new(Mutex::new(HashMap::new())),
            probe: None,
            sprf_store: None,
            config: None,
            telemetry: None,
            vfs: None,
            warm_skip: None,
            warm_slice: None,
            apply_seq: Arc::new(AtomicU64::new(0)),
            query_site: Arc::new(AtomicU64::new(0)),
            current_call_span: Cell::new(None),
            pipe_full_extent: Cell::new(true),
            scope_path: RefCell::new(Vec::new()),
            col_types: RefCell::new(HashMap::new()),
        }
    }

    /// Record that column `col` of declared type `table` is itself of
    /// type `ty`. Idempotent; a conflicting re-declaration is the
    /// caller's bug (the importer declares each kind once).
    pub fn set_col_type(
        &self,
        table: impl Into<Arc<str>>,
        col: impl Into<Arc<str>>,
        ty: impl Into<Arc<str>>,
    ) {
        self.col_types
            .borrow_mut()
            .entry(table.into())
            .or_default()
            .insert(col.into(), ty.into());
    }

    /// The declared type of `table.col`, if it was tagged via
    /// `set_col_type`. `None` ⇒ scalar column (terminal for dotting).
    pub fn col_type(&self, table: &str, col: &str) -> Option<Arc<str>> {
        self.col_types
            .borrow()
            .get(table)
            .and_then(|m| m.get(col))
            .cloned()
    }

    /// Push the enclosing rule name before lowering its `{ block }`.
    pub fn enter_scope(&self, name: impl Into<Arc<str>>) {
        self.scope_path.borrow_mut().push(name.into());
    }
    /// Pop after the block is lowered.
    pub fn exit_scope(&self) {
        self.scope_path.borrow_mut().pop();
    }
    /// Dotted prefix of the current scope (`""` at top level).
    fn scope_prefix(&self) -> String {
        let p = self.scope_path.borrow();
        if p.is_empty() {
            String::new()
        } else {
            let mut s = p.join(".");
            s.push('.');
            s
        }
    }
    /// The fully-qualified registry key for a rule defined right now.
    pub fn scoped_rule_key(&self, name: &str) -> Arc<str> {
        let pre = self.scope_prefix();
        if pre.is_empty() {
            Arc::from(name)
        } else {
            Arc::from(format!("{pre}{name}").as_str())
        }
    }
    /// Install the warm-skip predicate (see field docs). Threaded by
    /// the host before `walk_program`; tests/other callers leave it
    /// `None` and keep prior full-walk behavior.
    pub fn with_warm_skip(
        mut self,
        f: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    ) -> Self {
        self.warm_skip = Some(f);
        self
    }
    /// Install the warm dirty-slice (see field docs). `None` keeps the
    /// full-walk behavior for cold runs / non-host callers.
    pub fn with_warm_slice(mut self, paths: Option<Arc<Vec<PathBuf>>>) -> Self {
        self.warm_slice = paths;
        self
    }
    /// Set the `.sprf` script directory used by `fs`'s branch-3
    /// fallback. The host (`SprfState::run`) threads `req.path`'s parent
    /// here; it stays the script folder even when `--root` retargets
    /// `root` at a corpus elsewhere.
    pub fn with_sprf_dir(mut self, dir: PathBuf) -> Self {
        self.sprf_dir = dir;
        self
    }
    pub fn with_interner(mut self, i: Arc<Interner>) -> Self {
        self.interner = Some(i);
        self
    }
    pub fn with_probe(mut self, p: Arc<dyn ProbeSink<Cursor>>) -> Self {
        self.probe = Some(p);
        self
    }
    pub fn with_binding(mut self, name: impl Into<Arc<str>>, pipe: Pipe<Cursor>) -> Self {
        self.bindings.insert(name.into(), pipe);
        self
    }
    pub fn register_rule(&self, rule: Rule) {
        let key = self.scoped_rule_key(&rule.name);
        self.rules.lock().unwrap().insert(key, rule);
    }
    /// Resolve a (possibly bare) rule name against the current scope by
    /// walking the path outward: `a.b.name` → `a.name` → `name`. Top
    /// level (empty scope) degenerates to a plain bare lookup, so
    /// existing single-namespace behavior is unchanged.
    pub fn get_rule(&self, name: &str) -> Option<Rule> {
        let map = self.rules.lock().unwrap();
        // An already-qualified name (caller passed `outer.inner`) or a
        // bare name both work: try progressively shorter scope prefixes.
        let mut path: Vec<Arc<str>> = self.scope_path.borrow().clone();
        loop {
            let key = if path.is_empty() {
                name.to_string()
            } else {
                format!("{}.{}", path.join("."), name)
            };
            if let Some(r) = map.get(key.as_str()) {
                return Some(r.clone());
            }
            if path.pop().is_none() {
                return None;
            }
        }
    }
    /// Resolve `v.key` (one dot segment). Three steps, in order:
    ///   1. instance dot — the value's own metaclass override;
    ///   2. type projection — `v.dots.ty` names a rule whose columns are
    ///      its fields; `key` must be one of them. The projected value
    ///      is a column term-read, re-typeable for chained access;
    ///   3. neither — `DotMiss` (a loud compile error, never empty rows).
    pub fn resolve_dot(
        &self,
        v: &crate::compile::lower::value::Value,
        key: &str,
    ) -> Result<crate::compile::lower::value::Value, LowerError> {
        use crate::compile::lower::value::{CallableKind, CallableRef, Value, ValueKind};
        // 1. instance dot
        if let Some(hit) = v.dots.map.get(key) {
            return Ok(hit.clone());
        }
        // H4: a `Callable(Rule "T")` has `dots.ty == None`, so step 2's
        // projection would never fire. Treat the rule name as the type
        // name so `Callable(Rule T).col` projects exactly like a
        // `.typed(T)` value. Op callables intentionally have no ty.
        let ty_from_kind: Option<Arc<str>> = match v.kind() {
            ValueKind::Callable(CallableRef {
                kind: CallableKind::Rule,
                name,
            }) => Some(name.clone()),
            _ => None,
        };
        // 2. type column projection. A TYPE is a rule whose columns are
        // its fields; the canonical imported-type shape is a decl-only
        // `rule(:Ty, f?...)`, which registers ONLY in the fact-store
        // schema (no body => no `register_rule`). So resolve columns via
        // the declared schema, which covers decl-only types AND bodied
        // rules (the bodied form declares its table too).
        if let Some(ty) = v.dots.ty.clone().or(ty_from_kind) {
            let has_col = self
                .store
                .declared_cols(&ty)
                .map(|cols| cols.iter().any(|c| c.as_str() == key))
                .unwrap_or(false);
            if has_col {
                let term = crate::term::Term::read(Arc::<str>::from(key));
                let proj = Pipe::new().step(Arc::new(term));
                let mut val = Value::pipe(proj);
                // Carry the column's own declared type forward so the
                // NEXT segment in `x.a.b.c` has a type to project
                // against. Scalar column ⇒ no tag ⇒ terminal.
                if let Some(col_ty) = self.col_type(&ty, key) {
                    val = val.typed(col_ty);
                }
                return Ok(val);
            }
            return Err(LowerError::DotMiss {
                ty: Some(ty),
                key: Arc::from(key),
            });
        }
        // 3. loud miss
        Err(LowerError::DotMiss {
            ty: None,
            key: Arc::from(key),
        })
    }

    /// Attach the content-derived intern store. Emitters lowered after
    /// this call stamp coord-space terms.
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.sprf_store = Some(s);
        self
    }
    /// Attach the XDG repo config so the bare `repo()` generator can
    /// emit one cursor per configured repo at lower time.
    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c);
        self
    }
    pub fn with_telemetry(mut self, t: Arc<PipelineTelemetry>) -> Self {
        self.telemetry = Some(t);
        self
    }
    /// Phase 4 — attach the daemon's buffer overlay so lowered
    /// source-reading components read IDE buffer text in preference to
    /// disk.
    pub fn with_vfs(mut self, vfs: Arc<VfsOverlay>) -> Self {
        self.vfs = Some(vfs);
        self
    }
}

#[derive(Debug, Clone)]
pub enum LowerError {
    /// Validate emitted diags; lower refuses to run.
    Validate(Vec<effect_runtime::v2::Diag>),
    /// Lower-time only: a `${X}` reference has no binding.
    UnboundCapture(Arc<str>),
    /// Dot-access (`x.key`) resolved to nothing: not an instance dot,
    /// and either no TYPE on the value or the TYPE's rule has no such
    /// column. A loud compile error — never silently empty rows.
    DotMiss {
        ty: Option<Arc<str>>,
        key: Arc<str>,
    },
    /// Anything else.
    Unknown(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::Validate(ds) => write!(f, "validate: {} diag(s)", ds.len()),
            LowerError::UnboundCapture(n) => write!(f, "unbound capture: ${{{n}}}"),
            LowerError::DotMiss { ty, key } => match ty {
                Some(t) => write!(f, "no field `.{key}` on type `{t}`"),
                None => write!(f, "no field `.{key}` (value has no type and no instance dot)"),
            },
            LowerError::Unknown(s) => write!(f, "{s}"),
        }
    }
}
impl std::error::Error for LowerError {}
