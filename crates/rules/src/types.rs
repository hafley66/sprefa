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
/// Selector chain: git context -> file path -> structural position.
/// Think of the entire indexed space as a DOM:
///   root > repo[name][branch] > file[path][ext] > (parsed tree nodes)
/// Each rule is a CSS selector against this DOM.
/// All nodes matching the full chain produce captures, and the action
/// turns captures into refs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Rule {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitSelector>,

    pub file: FileSelector,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<Vec<StructStep>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select_ast: Option<AstSelector>,

    /// Regex applied to a named capture to split/filter it.
    /// Named groups from the regex merge back into the capture map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValuePattern>,

    pub action: Action,
}

/// Git context selector. All fields are glob patterns (pipe-delimited alternatives).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GitSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// File path selector. Single glob or list of globs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FileSelector {
    Single(String),
    Multiple(Vec<String>),
}

/// One step in a structural selector chain.
///
/// Steps are consumed left-to-right as the engine walks the parsed tree
/// depth-first. Each step either narrows the current node set or captures
/// a value for later use by the action.
///
/// The `capture` field on Key/KeyMatch/Leaf/Object steps names the value
/// so the action's `emit` array can reference it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum StructStep {
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
        captures: std::collections::HashMap<String, String>,
    },
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

/// What to emit when a rule matches. One rule can emit multiple refs
/// from a single match via the `emit` array.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Action {
    /// Each entry turns a named capture into a ref.
    pub emit: Vec<EmitRef>,

    /// Template for the target repo name. Can reference captures: `"{repo}"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,

    /// Template for the target file path within the target repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,

    /// Confidence score override (0.0 to 1.0). Default: 0.8 for rule matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// One ref to emit from a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmitRef {
    /// Name of the capture to use as the ref value.
    pub capture: String,
    /// RefKind to assign.
    pub kind: ActionKind,
    /// Name of another capture to use as parent_key (links related refs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// Ref kind for action output. Snake_case string aliases for RefKind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    StringLiteral,
    JsonKey,
    JsonValue,
    YamlKey,
    YamlValue,
    TomlKey,
    TomlValue,
    ImportPath,
    ImportName,
    ExportName,
    DepName,
    DepVersion,
    RsUse,
    RsDeclare,
    RsMod,
}

impl ActionKind {
    pub fn to_ref_kind(self) -> sprefa_schema::RefKind {
        match self {
            Self::StringLiteral => sprefa_schema::RefKind::StringLiteral,
            Self::JsonKey => sprefa_schema::RefKind::JsonKey,
            Self::JsonValue => sprefa_schema::RefKind::JsonValue,
            Self::YamlKey => sprefa_schema::RefKind::YamlKey,
            Self::YamlValue => sprefa_schema::RefKind::YamlValue,
            Self::TomlKey => sprefa_schema::RefKind::TomlKey,
            Self::TomlValue => sprefa_schema::RefKind::TomlValue,
            Self::ImportPath => sprefa_schema::RefKind::ImportPath,
            Self::ImportName => sprefa_schema::RefKind::ImportName,
            Self::ExportName => sprefa_schema::RefKind::ExportName,
            Self::DepName => sprefa_schema::RefKind::DepName,
            Self::DepVersion => sprefa_schema::RefKind::DepVersion,
            Self::RsUse => sprefa_schema::RefKind::RsUse,
            Self::RsDeclare => sprefa_schema::RefKind::RsDeclare,
            Self::RsMod => sprefa_schema::RefKind::RsMod,
        }
    }
}
