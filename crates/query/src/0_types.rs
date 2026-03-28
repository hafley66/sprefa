// ── Result types ─────────────────────────────────────────────────────────────

/// A single match: a string_id with a confidence score and source location.
#[derive(Debug, Clone)]
pub struct Hit {
    pub string_id: i64,
    pub value: String,
    pub confidence: f64,
    pub file_id: i64,
    pub file_path: String,
    pub repo_name: String,
    pub branch: String,
    pub kind: String,
    pub rule_name: String,
    pub span_start: i64,
    pub span_end: i64,
}

/// Composite key for deduplication in set operations.
/// Two hits are "the same ref" when they share (string_id, file_id, span_start).
impl Hit {
    pub fn ref_key(&self) -> (i64, i64, i64) {
        (self.string_id, self.file_id, self.span_start)
    }
}

/// Result set from evaluating an expression. Ordered by confidence descending.
#[derive(Debug, Clone, Default)]
pub struct HitSet {
    pub hits: Vec<Hit>,
}

impl HitSet {
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    pub fn len(&self) -> usize {
        self.hits.len()
    }
}

// ── Expression tree ──────────────────────────────────────────────────────────

/// A query expression over the ref index. Constructed programmatically.
/// Parsing from URTSL text syntax is a separate concern (not in this crate).
#[derive(Debug, Clone)]
pub enum Expr {
    /// Leaf: match strings by pattern.
    Atom(Atom),
    /// Narrow a result set by metadata predicate.
    Filter(Box<Expr>, Filter),
    /// Combine two result sets.
    SetOp(Box<Expr>, SetOp, Box<Expr>),
    /// Run RHS scoped to files where LHS matched. Boosts confidence when
    /// both sides match in the same file.
    Cascade(Box<Expr>, Box<Expr>),
}

// ── Atoms ────────────────────────────────────────────────────────────────────

/// How to match against strings in the index.
#[derive(Debug, Clone)]
pub enum Atom {
    /// FTS5 trigram substring match on norm column.
    Substring(String),
    /// Exact match on the value column (confidence = 1.0).
    Exact(String),
    /// Path-aware match: splits on `/`, matches segment subsequence.
    /// "utils/helpers" matches "src/utils/helpers.ts" and "lib/utils/helpers/index.js".
    Path(String),
    /// Module-path-aware match: splits on `::`, matches segment subsequence.
    /// "utils::helpers" matches "crate::utils::helpers" and "self::utils::helpers::foo".
    Mod(String),
    /// File stem match (no extension, no directory).
    /// "helpers" matches strings where the referenced file's stem is "helpers".
    Stem(String),
    /// Ordered segment match across any separator (/, ::, -, _).
    /// Seg(["utils", "helpers"]) matches "utils/helpers", "utils::helpers",
    /// "utils-helpers", "utils_helpers".
    Seg(Vec<String>),
    /// Edit-distance fuzzy match. Threshold is the minimum similarity (0.0 to 1.0).
    Fuzzy(String, f64),
}

// ── Filters ──────────────────────────────────────────────────────────────────

/// Predicates that narrow a result set without changing confidence.
#[derive(Debug, Clone)]
pub enum Filter {
    /// Restrict to refs in this repo (exact name match).
    InRepo(String),
    /// Restrict to refs in files matching this glob.
    InFile(String),
    /// Restrict to these kind strings (e.g. "dep_name", "import_path").
    OfKind(Vec<String>),
    /// Exclude these kind strings.
    NotKind(Vec<String>),
    /// Restrict to refs visible on branches matching this glob.
    OnBranch(String),
    /// Only refs whose target_file_id is non-null (resolved imports).
    Resolved,
    /// Only refs whose target_file_id is null (unresolved / external).
    Unresolved,
    /// File path depth >= n.
    DepthMin(u32),
    /// File path depth <= n.
    DepthMax(u32),
}

// ── Set operations ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOp {
    /// Intersection: both sides match the same string_id.
    /// Confidence: min(left, right).
    Intersect,
    /// Union: either side matches.
    /// Confidence: max(left, right) for shared string_ids.
    Union,
    /// Difference: left but not right.
    /// Confidence: left's score.
    Diff,
}

// ── Builder helpers ──────────────────────────────────────────────────────────

impl Expr {
    // -- constructors for atoms --

    pub fn substring(s: impl Into<String>) -> Self {
        Expr::Atom(Atom::Substring(s.into()))
    }

    pub fn exact(s: impl Into<String>) -> Self {
        Expr::Atom(Atom::Exact(s.into()))
    }

    pub fn path(s: impl Into<String>) -> Self {
        Expr::Atom(Atom::Path(s.into()))
    }

    pub fn module(s: impl Into<String>) -> Self {
        Expr::Atom(Atom::Mod(s.into()))
    }

    pub fn stem(s: impl Into<String>) -> Self {
        Expr::Atom(Atom::Stem(s.into()))
    }

    pub fn seg(segments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Expr::Atom(Atom::Seg(segments.into_iter().map(Into::into).collect()))
    }

    pub fn fuzzy(s: impl Into<String>, threshold: f64) -> Self {
        Expr::Atom(Atom::Fuzzy(s.into(), threshold))
    }

    // -- filter chaining --

    pub fn in_repo(self, repo: impl Into<String>) -> Self {
        Expr::Filter(Box::new(self), Filter::InRepo(repo.into()))
    }

    pub fn in_file(self, glob: impl Into<String>) -> Self {
        Expr::Filter(Box::new(self), Filter::InFile(glob.into()))
    }

    pub fn of_kind(self, kinds: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Expr::Filter(Box::new(self), Filter::OfKind(kinds.into_iter().map(Into::into).collect()))
    }

    pub fn not_kind(self, kinds: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Expr::Filter(Box::new(self), Filter::NotKind(kinds.into_iter().map(Into::into).collect()))
    }

    pub fn on_branch(self, glob: impl Into<String>) -> Self {
        Expr::Filter(Box::new(self), Filter::OnBranch(glob.into()))
    }

    pub fn resolved(self) -> Self {
        Expr::Filter(Box::new(self), Filter::Resolved)
    }

    pub fn unresolved(self) -> Self {
        Expr::Filter(Box::new(self), Filter::Unresolved)
    }

    pub fn depth_min(self, n: u32) -> Self {
        Expr::Filter(Box::new(self), Filter::DepthMin(n))
    }

    pub fn depth_max(self, n: u32) -> Self {
        Expr::Filter(Box::new(self), Filter::DepthMax(n))
    }

    // -- set operations --

    pub fn and(self, other: Expr) -> Self {
        Expr::SetOp(Box::new(self), SetOp::Intersect, Box::new(other))
    }

    pub fn or(self, other: Expr) -> Self {
        Expr::SetOp(Box::new(self), SetOp::Union, Box::new(other))
    }

    pub fn minus(self, other: Expr) -> Self {
        Expr::SetOp(Box::new(self), SetOp::Diff, Box::new(other))
    }

    // -- cascade --

    /// Run `then` scoped to files where `self` matched.
    pub fn cascade(self, then: Expr) -> Self {
        Expr::Cascade(Box::new(self), Box::new(then))
    }
}
