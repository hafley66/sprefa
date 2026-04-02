/// Parse tree for .sprf files.
///
/// .sprf text -> Vec<Statement> -> lower -> Vec<Rule>

/// A complete .sprf file: a sequence of statements.
pub type Program = Vec<Statement>;

/// One top-level statement.
#[derive(Debug, Clone)]
pub enum Statement {
    Rule(RuleDecl),
    Link(LinkDecl),
    Query(QueryDecl),
}

/// A rule with a head declaration and selector chain body.
///
/// ```sprf
/// rule deploy_config($SVC, repo($REPO), rev($TAG)) >
///     fs(**/services.yaml) > json({ services: { $SVC: { repo: $REPO, tag: $TAG } } });
/// ```
#[derive(Debug, Clone)]
pub struct RuleDecl {
    pub name: String,
    pub captures: Vec<Capture>,
    pub chain: SelectorChain,
}

/// One capture in a rule head: bare `$VAR` or annotated `repo($VAR)`.
#[derive(Debug, Clone)]
pub struct Capture {
    pub var: String,
    pub annotation: Option<CaptureAnnotation>,
}

/// Annotation on a capture variable in a rule head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureAnnotation {
    /// `repo($VAR)` -- value drives IS_REPO demand scanning.
    Repo,
    /// `rev($VAR)` -- value drives IS_REV demand scanning.
    Rev,
    /// `name($VAR)` -- semantic tag, no runtime behavior yet.
    Name,
    /// `file($VAR)` -- path resolution, no runtime behavior yet.
    File,
}

/// A chain of slots separated by `>`, terminated by `;`.
///
/// ```sprf
/// fs(**/Cargo.toml) > json({ package: { name: $NAME } });
/// ```
#[derive(Debug, Clone)]
pub struct SelectorChain {
    pub slots: Vec<Slot>,
}

/// One segment of a selector chain.
#[derive(Debug, Clone)]
pub enum Slot {
    /// A bare glob string (no tag). Could be repo, branch, or fs
    /// depending on position during lowering.
    Bare(String),
    /// A tagged slot: `tag(body)` or `tag[arg](body)`.
    Tagged {
        tag: Tag,
        arg: Option<String>,
        body: String,
    },
}

/// A link declaration: `link(src_kind > tgt_kind, pred, ...) > $kind_name;`
#[derive(Debug, Clone)]
pub struct LinkDecl {
    pub src_kind: String,
    pub tgt_kind: String,
    pub predicates: Vec<String>,
    pub kind_name: Option<String>,
}

/// Known tag names in `tag(body)` notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Json,
    Re,
    Ast,
    Repo,
    Rev,
    Fs,
}

impl Tag {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Tag::Json),
            "re" => Some(Tag::Re),
            "ast" => Some(Tag::Ast),
            "repo" => Some(Tag::Repo),
            "rev" | "branch" | "tag" => Some(Tag::Rev),
            "fs" => Some(Tag::Fs),
            _ => None,
        }
    }
}

/// A query rule: `query head($A, $C) > rel($A, $B)  head($B, $C);`
///
/// Compiles to a SQL CTE (recursive when head appears in body).
#[derive(Debug, Clone)]
pub struct QueryDecl {
    pub head: Atom,
    pub body: Vec<Atom>,
    /// When true, this is a `check` rule: non-empty result = violation.
    pub is_check: bool,
}

/// One atom in a query rule: `relation($ARG1, $ARG2)` or `relation($ARG1, "literal")`.
#[derive(Debug, Clone)]
pub struct Atom {
    pub relation: String,
    pub args: Vec<Term>,
    /// When true, this atom is negated (`not rel(...)`). Compiles to NOT EXISTS.
    pub negated: bool,
}

/// A term in a query atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// `$VAR` -- binds or unifies with a named variable.
    Var(String),
    /// `"literal"` or bare identifier -- matches a specific string value.
    Lit(String),
    /// `$_` -- matches anything, no binding.
    Wild,
}
