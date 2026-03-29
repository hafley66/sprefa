use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level rules file: an array of rules plus optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuleSet {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub rules: Vec<Rule>,
}

/// A single extraction rule.
///
/// The `select` chain is a CSS selector against the full index DOM:
///   root > repo > branch > tag > folder* > file > (parsed tree nodes)
///
/// Context steps (Repo, Branch, Tag, Folder, File) filter by git context
/// and file path before the file is parsed. Structural steps (Key, KeyMatch,
/// Any, etc.) walk the parsed tree to produce captures.
///
/// Ordering constraint enforced at compile time: all context steps must
/// precede all structural steps.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Rule {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    pub select: Vec<SelectStep>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select_ast: Option<AstSelector>,

    /// Regex applied to a named capture to split/filter it.
    /// Named groups from the regex merge back into the capture map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValuePattern>,

    /// Each entry turns a named capture into a match row.
    #[serde(alias = "emit")]
    pub create_matches: Vec<MatchDef>,

    /// Confidence score override (0.0 to 1.0). Default: 0.8 for rule matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// One step in a selector chain.
///
/// Context steps filter before parsing. Structural steps walk the parsed tree.
/// Pattern fields use pipe-delimited glob syntax: `"main|release/*"`.
///
/// For Folder: `*` captures one path segment, `**` captures the remaining path.
/// For File: glob matches against the full relative file path.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum SelectStep {
    // ── Context steps ──────────────────────────────

    /// Filter by repository name (pipe-delimited glob).
    Repo {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    /// Filter by branch name (pipe-delimited glob).
    Branch {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    /// Filter by git tag (pipe-delimited glob).
    Tag {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    /// Filter by directory path (pipe-delimited glob against dir portion).
    Folder {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    /// Filter by file path (pipe-delimited glob against full relative path).
    File {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    // ── Structural steps ───────────────────────────

    /// Enter the child with this exact key name.
    Key {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    /// Enter children whose key matches this glob pattern (pipe-delimited).
    KeyMatch {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    /// Descend through any number of levels (like CSS `**` or jq `..`).
    Any,

    /// Filter: current depth >= n (0 = root).
    DepthMin { n: u32 },

    /// Filter: current depth <= n.
    DepthMax { n: u32 },

    /// Filter: current depth == n.
    DepthEq { n: u32 },

    /// Filter: parent key matches this glob pattern.
    ParentKey { pattern: String },

    /// Descend into array elements.
    ArrayItem,

    /// Filter: node is a leaf (string, number, bool).
    /// Captures the leaf value.
    Leaf {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },

    /// Capture multiple sibling keys from an object node at once.
    /// Map of json_key -> capture_name.
    /// Example: `{ "repository": "repo", "tag": "tag" }`
    Object {
        captures: std::collections::BTreeMap<String, String>,
    },
}

impl SelectStep {
    /// Returns true for steps that filter by context (git/filesystem) rather
    /// than walking the parsed tree.
    pub fn is_context_step(&self) -> bool {
        matches!(
            self,
            SelectStep::Repo { .. }
                | SelectStep::Branch { .. }
                | SelectStep::Tag { .. }
                | SelectStep::Folder { .. }
                | SelectStep::File { .. }
        )
    }
}

/// ast-grep pattern selector for code files.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AstSelector {
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default = "default_ast_capture")]
    pub capture: String,
}

fn default_ast_capture() -> String {
    "$NAME".to_string()
}

/// Regex applied to a named capture. Named groups from the regex
/// merge into the capture map, enabling split/transform of captured values.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValuePattern {
    /// Which capture to run the regex against.
    pub source: String,
    /// Regex pattern with named groups via `(?P<name>...)`.
    pub pattern: String,
    /// Anchor the regex to the full string. Default: true.
    #[serde(default = "default_true")]
    pub full_match: bool,
}

fn default_true() -> bool {
    true
}

/// One match to create from a captured value.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MatchDef {
    /// Name of the capture to use as the ref value.
    pub capture: String,
    /// Free-text kind string (e.g. "dep_name", "helm_value", "operation_id").
    pub kind: String,
    /// Name of another capture to use as parent_key (links related refs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}
