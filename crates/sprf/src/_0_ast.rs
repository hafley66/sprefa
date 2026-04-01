/// Parse tree for .sprf files.
///
/// .sprf text -> Vec<Statement> -> lower -> Vec<Rule>

/// A complete .sprf file: a sequence of statements.
pub type Program = Vec<Statement>;

/// One top-level statement.
#[derive(Debug, Clone)]
pub enum Statement {
    Rule(SelectorChain),
    Link(LinkDecl),
    Query(QueryDecl),
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
    /// A match slot: `match($CAPTURE, kind)`.
    Match {
        capture: String,
        kind: String,
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
    Branch,
    Fs,
    Rule,
}

impl Tag {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Tag::Json),
            "re" => Some(Tag::Re),
            "ast" => Some(Tag::Ast),
            "repo" => Some(Tag::Repo),
            "branch" => Some(Tag::Branch),
            "fs" => Some(Tag::Fs),
            "rule" => Some(Tag::Rule),
            _ => None,
        }
    }
}

/// A query rule: `query head($A, $C) :- rel($A, $B), head($B, $C);`
///
/// Compiles to a SQL CTE (recursive when head appears in body).
#[derive(Debug, Clone)]
pub struct QueryDecl {
    pub head: Atom,
    pub body: Vec<Atom>,
}

/// One atom in a query rule: `relation($ARG1, $ARG2)` or `relation($ARG1, "literal")`.
#[derive(Debug, Clone)]
pub struct Atom {
    pub relation: String,
    pub args: Vec<Term>,
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
