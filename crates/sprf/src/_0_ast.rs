/// Parse tree for .sprf files.
///
/// .sprf text -> Vec<Statement> -> lower -> Vec<Rule>

/// A complete .sprf file: a sequence of statements.
pub type Program = Vec<Statement>;

/// One top-level statement. Currently only extraction rules.
#[derive(Debug, Clone)]
pub enum Statement {
    Rule(SelectorChain),
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

/// Known tag names in `tag(body)` notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Json,
    Re,
    Ast,
    Repo,
    Branch,
    Fs,
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
            _ => None,
        }
    }
}
