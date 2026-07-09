use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type { Text, Int, Path, File, Dir, Repo, Rev }

impl Type {
    pub fn sql(self) -> &'static str {
        match self { Type::Int => "INTEGER", _ => "TEXT" }
    }
    pub fn parse(s: &str) -> Option<Type> {
        Some(match s {
            "text" => Type::Text,
            "int" => Type::Int,
            "path" => Type::Path,
            "file" => Type::File,
            "dir" => Type::Dir,
            // The top two data-model layers, reified as types: a repo coordinate
            // (config slug / path / "." self) and a rev coordinate (git rev).
            "repo" => Type::Repo,
            "rev" => Type::Rev,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub enum Value { Text(String), Int(i64), Null }

impl Value {
    pub fn as_str(&self) -> String {
        match self { Value::Text(s) => s.clone(), Value::Int(n) => n.to_string(), Value::Null => String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Col {
    pub name: String,
    pub ty: Type,
    /// The declared brand name when the column's type is a `type X <: Y` brand
    /// (`None` for a plain base type). Storage stays `ty` (text), but the brand
    /// name drives `check_rule_types` unification. Resolved from the raw type
    /// keyword at load time (a brand keyword that is not a base `Type`).
    pub brand: Option<String>,
}

impl Col {
    pub fn plain(name: String, ty: Type) -> Col { Col { name, ty, brand: None } }
}

/// A lattice merge function on key conflict: among rows sharing a `key(...)`
/// functional dependency, which one wins. `MaxBy(col)` keeps the whole row
/// whose `col` value is greatest (row-selection, the dispatch case: one
/// response per request id, highest-priority rule). Lowers to
/// `ON CONFLICT(key) DO UPDATE SET <non-key> = excluded.<non-key>
///  WHERE excluded.<col> > <col>`. Per-column lattice merges (Sum/Max per
/// column) are deferred; `MaxBy` is the only merge today.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeFn {
    /// Row-selection by a priority column: on conflict on the `key`, replace
    /// the stored row's non-key columns with the incoming row's only when the
    /// incoming row's `col` is strictly greater. The whole winning row stays
    /// intact (no Galois cross-row mixing).
    MaxBy(String),
}

impl MergeFn {
    pub fn parse(name: &str, arg: &str) -> Option<MergeFn> {
        Some(match name {
            "MaxBy" => MergeFn::MaxBy(arg.to_string()),
            _ => return None,
        })
    }
}

/// Which way rows cross the engine boundary for a port rel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDir { In, Out }

/// `@in(class)` / `@out(class)` decl qualifier: the rel is a port — rows enter
/// (`@in`, injected by a serving loop before its tick) or leave (`@out`,
/// drained to a transport after the tick). `class` names the CONTRACT the port
/// abides by, never a transport: `rpc` = Promise-shaped (envelope
/// id/method/params in, id/result out, one reply per id closes the request).
/// `stream`/`duplex` are reserved. Binding a class to a wire is a CLI profile
/// (`--mcp` = rpc ports over stdio x jsonrpc), so the same program serves any
/// transport that speaks the class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub dir: PortDir,
    pub class: String,
}

impl Port {
    /// The column envelope a class requires, by (name, type) — order-free,
    /// mirroring `diag`'s by-name mapping. `None` = unknown class. The rpc `id`
    /// is TEXT holding the raw JSON serialization of the request id (`1`,
    /// `"abc"`), so both integer and string JSON-RPC ids round-trip exactly.
    pub fn envelope(class: &str, dir: PortDir) -> Option<&'static [(&'static str, Type)]> {
        match (class, dir) {
            ("rpc", PortDir::In) => Some(&[("id", Type::Text), ("method", Type::Text), ("params", Type::Text)]),
            ("rpc", PortDir::Out) => Some(&[("id", Type::Text), ("result", Type::Text)]),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RelDecl {
    pub name: String,
    pub cols: Vec<Col>,
    /// `key(c1, c2, ...)` qualifier: a functional dependency / choice-domain
    /// (Soufflé APLAS'21). The conflict target is this column subset, not the
    /// full row. With no `merge`, `INSERT OR IGNORE` keeps the first row per
    /// key (silent-pick choice). With a `merge`, the ON CONFLICT clause drives
    /// the lattice. `None` = the legacy full-row PRIMARY KEY (set semantics).
    pub key: Option<Vec<String>>,
    /// `merge(MaxBy(col))` qualifier: the lattice merge on key conflict.
    /// Requires `key` to be set (the conflict target). `None` = first-wins.
    pub merge: Option<MergeFn>,
    /// `@in(class)` / `@out(class)` qualifier: this rel is a port (see `Port`).
    pub port: Option<Port>,
    /// Doc-table group for a built-in relation ("core", "module", ...); "" for
    /// user-declared rels. Lives on the decl so the schema and the doc cannot
    /// drift — `rel_catalog` and the generated README table read it from here.
    pub group: &'static str,
    /// One-line summary for a built-in relation; "" for user-declared rels.
    /// `undocumented_builtins` (and its test) fail while a built-in decl leaves
    /// this empty. Avoid `|` so it renders inside a markdown table cell.
    pub doc: &'static str,
    /// `rel <name>: <shape>.` records the referenced shape name here (with `cols`
    /// left empty). Resolved to the shape's columns by `typecheck::expand_shapes`
    /// at load; `None` (and expanded) before the engine sees the decl.
    pub shape_ref: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RelMeta {
    pub cols: Vec<Col>,
    pub key: Option<Vec<String>>,
    pub merge: Option<MergeFn>,
    pub port: Option<Port>,
}

impl RelMeta {
    pub fn col_name(&self, i: usize) -> &str { &self.cols[i].name }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp { Eq, Ne, Lt, Le, Gt, Ge, Match, Glob }

impl CmpOp {
    pub fn sql(self) -> &'static str {
        match self {
            CmpOp::Eq => "=", CmpOp::Ne => "<>",
            CmpOp::Lt => "<", CmpOp::Le => "<=",
            CmpOp::Gt => ">", CmpOp::Ge => ">=",
            CmpOp::Match => "REGEXP", CmpOp::Glob => "GLOB",
        }
    }
}

#[derive(Clone, Debug)]
pub enum InterpPart { Lit(String), Var(String) }

/// Integer arithmetic operator for `Term::Arith`. `/` is SQLite/Rust integer
/// division (truncating); `%` is the remainder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithOp { Add, Sub, Mul, Div, Mod }

impl ArithOp {
    pub fn sql(self) -> &'static str {
        match self {
            ArithOp::Add => "+", ArithOp::Sub => "-",
            ArithOp::Mul => "*", ArithOp::Div => "/", ArithOp::Mod => "%",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Term {
    Var(String),
    Str(String),
    Int(i64),
    Wild,
    Interp(Vec<InterpPart>),
    /// A typed path literal `scheme:body` (`fs:src/x`, `glob:src/**/*.rs`). Carries
    /// the raw body and the source span for diagnostics. Resolved to canonical
    /// text/pattern (then rewritten to `Str`) at lower time by the engine.
    PathLit { scheme: String, body: String, span: (u32, u32) },
    /// Int arithmetic `a + 1`, `n * 2 - 1` — allowed in rule heads (derived AND
    /// source) and on either side of a comparison; never a binding position, so
    /// body atoms reject it. Derived rules lower it to SQL arithmetic; source
    /// rules evaluate it on the bound row values.
    Arith { op: ArithOp, lhs: Box<Term>, rhs: Box<Term> },
    /// Pure string function call `split(s, "/", -1)`, `replace(s, "_", "-")`.
    /// Same positioning rules as `Arith`: head columns and comparison sides,
    /// never a binding position. Derived rules lower to SQL (`replace` is
    /// native; `split` is the `sprf_split` UDF); source rules evaluate via
    /// `val_of`. The function set is whitelist-checked at lower/eval time.
    Call { name: String, args: Vec<Term> },
}

#[derive(Clone, Debug)]
pub struct Atom {
    pub rel: String,
    pub terms: Vec<Term>,
    /// Named args `col: term` collected at parse time, unresolved. The frontend's
    /// `resolve_named_args` folds these into positional `terms` slots using the
    /// relation's declared column names (unnamed columns become `Term::Wild`),
    /// then clears this. Empty for a fully-positional atom (the common case) and
    /// always empty after the frontend pass, so every later reader sees only
    /// `terms`. The `col:` production is the reusable colon syntax.
    pub named: Vec<(String, Term)>,
}

/// An aggregation function in a rule head: `count(T)`, `sum(T)`, `min(T)`,
/// `max(T)`. Count/Sum produce an `Int`; Min/Max produce the argument's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggFn { Count, Sum, Min, Max }

impl AggFn {
    pub fn parse(s: &str) -> Option<AggFn> {
        Some(match s {
            "count" => AggFn::Count,
            "sum" => AggFn::Sum,
            "min" => AggFn::Min,
            "max" => AggFn::Max,
            _ => return None,
        })
    }
    pub fn sql(self) -> &'static str {
        match self { AggFn::Count => "COUNT", AggFn::Sum => "SUM", AggFn::Min => "MIN", AggFn::Max => "MAX" }
    }
    /// The output type of the aggregate. Count/Sum are always Int; Min/Max carry
    /// the argument column's type (resolved by the caller from the arg var).
    pub fn fixed_out(self) -> Option<Type> {
        match self { AggFn::Count | AggFn::Sum => Some(Type::Int), AggFn::Min | AggFn::Max => None }
    }
}

#[derive(Clone, Debug)]
pub struct Constraint { pub lhs: Term, pub op: CmpOp, pub rhs: Term }

#[derive(Clone, Debug)]
pub enum BodyItem {
    Pos(Atom),
    Neg(Atom),
    Scan { repo: Term, rev: Term, glob: Term, path: Term, rev_out: Term },
    /// `col`/`end_col` are the whole-match span's 0-based byte columns within
    /// `line`. Optional trailing args after `line`: 1 ⇒ `id`; 2 ⇒ `col, end_col`;
    /// 3 ⇒ `id, col, end_col`. Feeds sub-line diagnostic spans.
    Match { path: Term, rev: Term, regex: String, line: Term, id: Option<Term>,
            col: Option<Term>, end_col: Option<Term> },
    Ast { path: Term, rev: Term, lang: String, query: String, line: Term, end: Option<Term>, id: Option<Term> },
    /// `line`/`col`/`end_line`/`end_col` are the match span (1-based lines,
    /// 0-based byte columns). All four accept the kwarg/`_` form: positional in
    /// order, `name: term` to bind a later slot, or unmentioned to bind nothing.
    /// `Term::Wild` marks an unbound output; the engine arm skips it.
    ///
    /// `rev: Some` = the FILE form `sg(path, rev, :lang, "pat", ...)` (`src` is a
    /// path scanned at `rev`, span-located, id resolvable). `rev: None` = the TERM
    /// form `sg(:lang, src, "pat", ...)` (`src` is a bound `str` content value — an
    /// embedded language body, a column, a captured fence — parsed directly, no
    /// file). The term form runs in the join+extract hybrid pass, like the term
    /// form of `json`/`jsonp`; its `line`/`col` spans are RELATIVE to the bound
    /// string (byte 0 = the start of that value, not the enclosing file), and it
    /// carries no `id` (there is no file to locate against — the caller adds the
    /// enclosing region's own line to lift spans back to file coordinates).
    Sg { src: Term, rev: Option<Term>, lang: String, pattern: String, line: Term,
         col: Term, end_line: Term, end_col: Term, id: Option<Term> },
    /// ast-grep relational YAML rule (RuleCore: inside/has/any/all/not/kind/
    /// regex/pattern). `yaml` is the (usually backtick, multiline) body; the
    /// span + capture binds mirror `Sg`. Superset of `Sg` — a bare `pattern:`
    /// body matches the same surface, but `inside:`/`has:` add the structural
    /// relationships a pattern alone can't express.
    AstYaml { path: Term, rev: Term, lang: String, yaml: String, line: Term,
              col: Term, end_line: Term, end_col: Term },
    /// `jsonp(path, rev, "a.b.*", out, id?)` — dotted-string evaluator over
    /// json/yaml/toml (dispatched by extension; `*` = any key/element). Renamed
    /// from `json` to free the name for the declarative brace pattern. Trailing
    /// optional `id` = the matched value's located span (christmas #9).
    /// `rev: Some` = the FILE form (`src` is a path read at `rev`, span-located);
    /// `rev: None` = the TERM form (`src` is a bound `str` content value — a
    /// response body / column / stdout — parsed directly, no file, no span). The
    /// term form runs in the join+extract hybrid pass, not the file scan.
    JsonP { src: Term, rev: Option<Term>, jpath: String, out: Term, id: Option<Term> },
    /// `json(path, rev, q:{ $k: $v })` — declarative brace pattern over a
    /// json/yaml/toml document (dispatched by extension, like jsonp). Each
    /// match binds N named captures (keys AND values) as rule vars, mirroring
    /// match's named groups. `pat` is the raw `q:` body (validated at parse
    /// time; the engine re-parses it to walk the Step tree). `rev: None` = the
    /// term form (`src` is bound content), like `JsonP`.
    Json { src: Term, rev: Option<Term>, pat: String },
    /// Shell out per matched file: `cmd(p, rev, "tool {file}", line, out)` binds
    /// one row per stdout line. Cached like every source op: rows re-run only
    /// when the file content or the rule text moves (the docker-layer contract).
    Cmd { path: Term, rev: Term, template: String, line: Term, out: Term },
    /// Comment-marker regions: `comment(p, rev, /open/[, /close/], l0, l1, label)`.
    /// One row per region; `l0`/`l1` are 1-based lines (open marker line and the
    /// region's last line), `label` is the open regex's first named group or the
    /// trimmed post-match tail ("" if neither). See comment.rs for the modes.
    Comment { path: Term, rev: Term, open: String, close: Option<String>,
              l0: Term, l1: Term, label: Term },
    Cmp(Constraint),
    /// Transitive closure of an edge relation, e.g. `reaches(a,b) <- closure(calls).`
    Closure { rel: String },
    /// SCC membership of an edge relation, e.g. `g(rep, member) <- scc(calls).`
    /// One row per node: (component representative, node). The representative is
    /// the min member name in the component. Reuses the closure condensation
    /// (Tarjan) — shares `closure(edge)`'s per-edge cache, no second Tarjan run.
    Scc { rel: String },
    /// Structural node embedding of an edge relation, e.g.
    /// `node_sim(a, b, score) <- node2vec(g).` Reads the 2-col edge rel `g`,
    /// learns one vector per node (random walks + skip-gram, src/embed/node2vec.rs),
    /// and emits top-k cosine-nearest node pairs into the 3-col head. Vectors
    /// persist in `_node_embeddings` keyed by the edge rel name. The graph sibling
    /// of the text `similar` rel.
    Node2vec { rel: String },
    /// An effect CALL inside an `@async`/`@stream` rule body: `gh(repo, path) ->
    /// (status, body)`. `name` resolves to a `ShellFn` (the effect kind/template);
    /// `args` fill the template `{hole}`s (param-positional); `outs` are fresh
    /// vars bound to the executor's response columns, in order. The body-effect
    /// form D-3 desugars to; the runtime sees ONE model. Not a lowerable relation
    /// (`body_sql` ignores it); the engine reads it via `Rule::effect()`.
    Effect { name: String, args: Vec<Term>, outs: Vec<Term> },
}

/// A temporal rule modifier, written `@next` / `@async` / `@stream` after the
/// `<-` neck. `Next`: the head is staged into the next tick's seed (carry state)
/// instead of this tick's fixpoint. `Async`: the body fires a one-shot effect
/// whose response lands as a fact at a later tick (non-blocking IO). `Stream`: the
/// body opens a long-lived subscription (`sh*`) whose rows append over many ticks,
/// drained by `Engine::drain_streams` (one head row per output line). `None` on the
/// `Rule` = today's same-tick deductive rule. See
/// docs/research-reactive-effectful-datalog.md §8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Temporal { Next, Async, Stream }

#[derive(Clone, Debug)]
pub struct Rule {
    pub head: Atom,
    pub body: Vec<BodyItem>,
    /// Aggregation marker parallel to `head.terms`: `aggs[i] == Some(f)` means the
    /// i-th head term is the argument of aggregate `f` (`count(T)` etc.); `None`
    /// means a plain group-by term. Empty (the common case) = no aggregation.
    /// Kept off `Atom` so query heads, body atoms, and source rules stay untouched;
    /// only a derived rule head ever carries aggs (see plan T4, head-position only).
    pub aggs: Vec<Option<AggFn>>,
    /// The file this rule was parsed from (canonical). Lets a self-form scan
    /// (`scan("WORK", …)` / `.` / `self`) resolve to the rule's own `.git`
    /// ancestor instead of the engine's `self.root`, so a script loaded into a
    /// rootless daemon scans the repo it lives in. Stamped by the frontend loader.
    pub origin: Option<PathBuf>,
    /// `@next` / `@async` modifier after the neck, if any. `None` = the ordinary
    /// same-tick deductive rule (every rule today). See `Temporal`.
    pub temporal: Option<Temporal>,
}

impl Rule {
    /// Does any head term carry an aggregate?
    pub fn has_agg(&self) -> bool { self.aggs.iter().any(|a| a.is_some()) }

    /// Does any head slot lower to SQL NULL? A `_` head term (explicit, or the
    /// named-arg padding for an unnamed column) projects NULL. A NULL row never
    /// dedups in a fixpoint delta (NULL != NULL under INSERT OR IGNORE), so this
    /// is only legal on a non-recursive (sink) head — typecheck flags it and
    /// `rebuild_derived` bails when the head rel sits in a recursive component.
    /// An aggregated `_` (`count(_)`) is not a projected slot and does not count.
    pub fn head_null_pads(&self) -> bool {
        self.head.terms.iter().enumerate().any(|(i, t)| {
            matches!(t, Term::Wild) && !matches!(self.aggs.get(i), Some(Some(_)))
        })
    }

    /// `@next` carry rule: head staged for the next tick's seed.
    pub fn is_next(&self) -> bool { self.temporal == Some(Temporal::Next) }

    /// `@async` effect rule: body emits a request, response lands later.
    pub fn is_async(&self) -> bool { self.temporal == Some(Temporal::Async) }

    /// `@stream` subscription rule: body opens a long-lived `sh*` source whose
    /// rows append over many ticks, fanned into the head by `drain_streams`.
    pub fn is_stream(&self) -> bool { self.temporal == Some(Temporal::Stream) }

    /// The single `BodyItem::Effect` of an `@async`/`@stream` rule, if present.
    /// After the frontend desugar every effectful rule carries exactly one (the
    /// head-response form synthesizes it). Returns `(kind, args, outs)`.
    pub fn effect(&self) -> Option<(&str, &[Term], &[Term])> {
        self.body.iter().find_map(|b| match b {
            BodyItem::Effect { name, args, outs } => Some((name.as_str(), &args[..], &outs[..])),
            _ => None,
        })
    }

    pub fn is_source(&self) -> bool {
        self.body.iter().any(|b| match b {
            BodyItem::Scan { .. } | BodyItem::Match { .. } | BodyItem::Ast { .. }
            | BodyItem::AstYaml { .. }
            | BodyItem::Cmd { .. } | BodyItem::Comment { .. } => true,
            // A FILE-form json/jsonp/sg (rev=Some) scans a file = a source op. The
            // TERM form (rev=None) reads a bound value, so it is NOT a source —
            // it runs in the join+extract hybrid pass over already-derived rels.
            BodyItem::JsonP { rev, .. } | BodyItem::Json { rev, .. }
            | BodyItem::Sg { rev, .. } => rev.is_some(),
            _ => false,
        })
    }

    /// True iff the body has a TERM-form `json`/`jsonp` (`rev=None`): the source
    /// is a bound `str` value, not a file. Such a rule joins relations (the Pos
    /// atoms bind the content var) and then extracts — the hybrid evaluator.
    pub fn has_term_extract(&self) -> bool {
        self.body.iter().any(|b| matches!(b,
            BodyItem::JsonP { rev: None, .. } | BodyItem::Json { rev: None, .. }
            | BodyItem::Sg { rev: None, .. }))
    }

    /// Some(edge) iff this rule is exactly `head(..) <- closure(edge).`
    pub fn closure_edge(&self) -> Option<&str> {
        match self.body.as_slice() {
            [BodyItem::Closure { rel }] => Some(rel),
            _ => None,
        }
    }

    /// Some(edge) iff this rule is exactly `head(..) <- scc(edge).`
    pub fn scc_edge(&self) -> Option<&str> {
        match self.body.as_slice() {
            [BodyItem::Scc { rel }] => Some(rel),
            _ => None,
        }
    }

    /// Some(edge) iff this rule is exactly `head(..) <- node2vec(edge).`
    pub fn node2vec_edge(&self) -> Option<&str> {
        match self.body.as_slice() {
            [BodyItem::Node2vec { rel }] => Some(rel),
            _ => None,
        }
    }

    /// A `repo`-sink rule: head rel `repo`. Its body is compiled as a SELECT and
    /// drained post-fixpoint to clone + register repos (the dynamic-pull path),
    /// NOT inserted into the `repo` table by `rebuild_derived` (which would wipe
    /// the engine-emitted registered set). Excluded from source/derived
    /// classification so neither reconcile nor rebuild touches the `repo` table.
    pub fn is_repo_sink(&self) -> bool { self.head.rel == "repo" }
    pub fn is_checkout_sink(&self) -> bool { self.head.rel == "checkout" }
}

/// A `? atom.` query. Filtering is done by nesting (a literal head term pins a
/// column; a derived rule with a body constraint filters), so a query carries no
/// constraints of its own — the `where` clause was removed (see
/// plans/2026-06-02-kill-where-seed-closures-by-nesting.md).
#[derive(Clone, Debug)]
pub struct Query { pub head: Atom }

/// `anchor <name> = <fs-literal>.` A named filesystem anchor. `name` is `~` or an
/// ident; `body` is the `fs:` literal's raw body. v1 accepts the declaration but
/// only the default `~` anchor (scan root) is referenced in literal bodies; named
/// anchor refs in bodies are deferred (with `rs:`).
#[derive(Clone, Debug)]
pub struct AnchorDecl { pub name: String, pub body: String, pub span: (u32, u32) }

/// A brand: a named subtype used only at typecheck time (runtime storage stays
/// text). Two surface forms:
///   `type <ident> <: <parent>.`         — nominal brand over a base type/brand.
///   `type <ident> = "a" | "b" | ... .`  — an ENUM brand: a closed set of text
///     literals. `parent` is `"text"` (the base storage type) and `variants` is
///     `Some([...])`. A literal filling an enum-branded column must be in the set
///     (`enum-variant-unknown` otherwise). `variants` is `None` for the `<:` form.
#[derive(Clone, Debug)]
pub struct BrandDecl { pub name: String, pub parent: String, pub variants: Option<Vec<String>> }

/// `type <name>(col: ty, ...).` A named row SHAPE: a reusable column list. A
/// `rel <name>: <shape>.` decl expands to an ordinary `RelDecl` whose columns are
/// the shape's, at load (frontend + typecheck seam), so downstream code only ever
/// sees plain `RelDecl`s. A shape column may reference a brand or an enum brand.
#[derive(Clone, Debug)]
pub struct ShapeDecl { pub name: String, pub cols: Vec<Col> }

/// Where a `gen` rule's rendered rows land.
#[derive(Clone, Debug)]
pub enum GenTarget {
    /// `gen("docs/{f}.md", ...)`: a path template with `{var}` holes, resolved
    /// per row; rows group by rendered path. Relative to the scan root. `append`
    /// is the `gen(:append, "path", ...)` form: multiple gen rules may target the
    /// same file and their rendered rows concatenate in PROGRAM ORDER (like the
    /// Splice form), so a header rule + a rows rule assemble one ordered page
    /// without markers. The default form (`append=false`) keeps the one-rule-per-
    /// file rule (a second rule to the same path bails).
    File { path_tmpl: String, append: bool },
    /// `gen(p, l0, l1, ...)`: splice between two marker lines of a WORK file
    /// (exclusive of both), the `comment` op's paired coordinates. Rows group
    /// by (path, l0, l1).
    Splice { path: Term, l0: Term, l1: Term },
    /// `gen(:mode, p, lo, hi, ...)`: byte-accurate splice into a WORK file at
    /// `[lo, hi)` driven by `mode`. The port of v4's `write_cursor` to v5: lo/hi
    /// come from the ref spine (`ref(id, _, path, lo, hi)`), so spine-captured
    /// sites are editable in place. One read-modify-write per file, regions
    /// applied right-to-left so earlier offsets stay valid across the batch.
    Cursor { mode: SpliceMode, path: Term, lo: Term, hi: Term },
    /// `gen(:zone, p, name, ...)`: replace the content between a NAMED marker
    /// pair in a WORK file. The engine finds `BEGIN: name` and the next `END:`
    /// after it (comment-prefix-tolerant: `//`, `#`, `/*`, `;`, `<!--`) and
    /// rewrites everything strictly between them, keeping the markers. Survives
    /// surrounding-text edits without re-anchoring line numbers (the trap with
    /// `Splice`). Rows group by (path, name). `path_tmpl`/`name_tmpl` are
    /// `{var}`-hole templates (literal when no holes) so a single rule can fan
    /// over many files / many zones via body bindings.
    Zone { path_tmpl: String, name_tmpl: String },
}

/// Mode for `GenTarget::Cursor`, ported from v4's `WriteMode`:
///   `:replace`  splice value into [lo, hi)              (the only line-Splice mode)
///   `:append`   insert value at hi                       (lo ignored at apply time)
///   `:prepend`  insert value at lo                       (hi ignored at apply time)
///   `:wrap`     insert value at lo AND hi                (surrounds [lo, hi))
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpliceMode { Replace, Append, Prepend, Wrap }

impl SpliceMode {
    pub fn tag(self) -> &'static str {
        match self {
            SpliceMode::Replace => "replace",
            SpliceMode::Append => "append",
            SpliceMode::Prepend => "prepend",
            SpliceMode::Wrap => "wrap",
        }
    }
}

/// `gen(<target>, "row template") <- body.` — the codegen sink. The body is an
/// ordinary derived-rule body (Pos/Neg/Cmp); after the fixpoint each result row
/// renders through the template (`{var}` holes) in deterministic order and the
/// joined lines are written to the target. Writes are skipped when the bytes
/// already match, so a converged tick is a no-op.
#[derive(Clone, Debug)]
pub struct GenRule {
    pub target: GenTarget,
    pub row_tmpl: String,
    pub body: Vec<BodyItem>,
}

#[derive(Clone, Debug)]
pub enum Item { Rel(RelDecl), Rule(Rule), Query(Query), Anchor(AnchorDecl), Brand(BrandDecl), Shape(ShapeDecl), Gen(GenRule), Shell(ShellFn) }

/// The read/mutate/stream axis of a `sh` decl: `sh` = `Read` (cached, deduped,
/// at-least-once, retryable), `sh!` = `Mutate` (idempotency-keyed, exactly-once,
/// never cached), `sh*` = `Stream` (long-lived, many rows over time). The bang/
/// star is the only thing that distinguishes the three at parse time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind { Read, Mutate, Stream }

/// `sh name(p1, p2) -> (c1: t1, c2: t2) = `cmd {p1}`.` — a named, typed,
/// shell-callable effect template. `name` IS the effect kind (it matches the
/// `@async` head rel in the head-response form). `params` are the `{hole}`
/// names filled from the request args; `outs` are the typed response columns the
/// executor fills from stdout. `body` is the raw shell (a backtick `Tok::Str`),
/// `{param}` holes replaced per request. The typed sibling of `effect_cmd`; the
/// daemon builds a `ShellEffectExec` from the registry of these.
#[derive(Clone, Debug)]
pub struct ShellFn {
    pub name: String,
    pub params: Vec<String>,
    pub outs: Vec<Col>,
    pub body: String,
    pub kind: ShellKind,
}

/// `use "path".` — module inclusion. The loader resolves `path` against the
/// include roots (program dir, `$SPREFA_STD`, `<exe>/../std`), reads the file,
/// parses its surface, and splices its Core items into the merge. Diamond
/// imports load once (canonical-path dedup).
#[derive(Clone, Debug)]
pub struct Import { pub path: String }

/// `def name(p1, p2) <- body.` — a parameterized rule template. The body is
/// inlined at every call site (`name(args)` as a body atom) with the params
/// substituted by the call's args and every non-param internal var
/// alpha-renamed so two instantiations never capture each other. A `def` is
/// not itself a rule: it emits zero rules on its own, only via call sites.
#[derive(Clone, Debug)]
pub struct RuleTemplate {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<BodyItem>,
}

/// One top-level form on the module surface. The frontend parses this; `expand`
/// is the only place `Use` disappears (recursively loaded and spliced) and the
/// only place `Def` disappears (collected into a template table, then inlined
/// at every call site in the surrounding program).
#[derive(Clone, Debug)]
pub enum SurfaceItem {
    Core(Item),
    Use(Import),
    Def(RuleTemplate),
}

/// Diagnostic severity for a `TypeDiag`. `Error` fails `--check` (non-zero exit);
/// `Warn` prints but does not fail (the coerce grandfather case).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity { Error, Warn }

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self { Severity::Error => "error", Severity::Warn => "warn" }
    }
}

/// A lower-time type diagnostic. `path` is the program file (literals carry no
/// per-file path of their own; the diagnostic points at the `.dl` source). `span`
/// is the (start, end) byte offset of the offending literal/atom in that source.
/// Codes match the spec table: `brand-mismatch`, `path-escapes-root`,
/// `unknown-anchor`, `unknown-scheme`, `coerce-text-path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDiag {
    pub path: String,
    pub span: (u32, u32),
    pub severity: Severity,
    pub code: String,
    pub msg: String,
}

#[derive(Clone, Debug, Default)]
pub struct Program { pub items: Vec<Item> }

pub type Rels = HashMap<String, RelMeta>;
