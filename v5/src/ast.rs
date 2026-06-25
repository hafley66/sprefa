use std::collections::HashMap;

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
pub enum Value { Text(String), Int(i64) }

impl Value {
    pub fn as_str(&self) -> String {
        match self { Value::Text(s) => s.clone(), Value::Int(n) => n.to_string() }
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

#[derive(Clone, Debug)]
pub struct RelDecl { pub name: String, pub cols: Vec<Col> }

#[derive(Clone, Debug)]
pub struct RelMeta { pub cols: Vec<Col> }

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
pub struct Atom { pub rel: String, pub terms: Vec<Term> }

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
    Match { path: Term, rev: Term, regex: String, line: Term, id: Option<Term> },
    Ast { path: Term, rev: Term, lang: String, query: String, line: Term, end: Option<Term>, id: Option<Term> },
    /// `line`/`col`/`end_line`/`end_col` are the match span (1-based lines,
    /// 0-based byte columns). All four accept the kwarg/`_` form: positional in
    /// order, `name: term` to bind a later slot, or unmentioned to bind nothing.
    /// `Term::Wild` marks an unbound output; the engine arm skips it.
    Sg { path: Term, rev: Term, lang: String, pattern: String, line: Term,
         col: Term, end_line: Term, end_col: Term },
    /// ast-grep relational YAML rule (RuleCore: inside/has/any/all/not/kind/
    /// regex/pattern). `yaml` is the (usually backtick, multiline) body; the
    /// span + capture binds mirror `Sg`. Superset of `Sg` — a bare `pattern:`
    /// body matches the same surface, but `inside:`/`has:` add the structural
    /// relationships a pattern alone can't express.
    AstYaml { path: Term, rev: Term, lang: String, yaml: String, line: Term,
              col: Term, end_line: Term, end_col: Term },
    Json { path: Term, rev: Term, jpath: String, out: Term },
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
}

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
}

impl Rule {
    /// Does any head term carry an aggregate?
    pub fn has_agg(&self) -> bool { self.aggs.iter().any(|a| a.is_some()) }

    pub fn is_source(&self) -> bool {
        self.body.iter().any(|b| matches!(b,
            BodyItem::Scan { .. } | BodyItem::Match { .. } | BodyItem::Ast { .. }
            | BodyItem::Sg { .. } | BodyItem::AstYaml { .. } | BodyItem::Json { .. }
            | BodyItem::Cmd { .. } | BodyItem::Comment { .. }))
    }

    /// Some(edge) iff this rule is exactly `head(..) <- closure(edge).`
    pub fn closure_edge(&self) -> Option<&str> {
        match self.body.as_slice() {
            [BodyItem::Closure { rel }] => Some(rel),
            _ => None,
        }
    }
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

/// `type <ident> <: <parent>.` A brand: a named subtype of a base type or a prior
/// brand. Stored in the relation schema metadata; runtime storage stays text.
#[derive(Clone, Debug)]
pub struct BrandDecl { pub name: String, pub parent: String }

/// Where a `gen` rule's rendered rows land.
#[derive(Clone, Debug)]
pub enum GenTarget {
    /// `gen("docs/{f}.md", ...)`: a path template with `{var}` holes, resolved
    /// per row; rows group by rendered path. Relative to the scan root.
    File { path_tmpl: String },
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
pub enum Item { Rel(RelDecl), Rule(Rule), Query(Query), Anchor(AnchorDecl), Brand(BrandDecl), Gen(GenRule) }

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
