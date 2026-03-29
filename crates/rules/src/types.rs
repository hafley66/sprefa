use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level rules file: an array of rules plus optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuleSet {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub rules: Vec<Rule>,
    /// Link rules create edges in `match_links` between matches produced by
    /// extraction rules. Each link rule supplies a raw SQL WHERE clause that
    /// is injected into a fixed query skeleton.
    ///
    /// ## SQL skeleton
    ///
    /// The engine runs each link rule against this template:
    ///
    /// ```sql
    /// INSERT OR IGNORE INTO match_links (source_match_id, target_match_id, link_kind)
    /// SELECT src_m.id, tgt_m.id, '<link_rule.kind>'
    /// FROM matches src_m
    /// JOIN refs    src_r ON src_m.ref_id     = src_r.id
    /// JOIN strings src_s ON src_r.string_id  = src_s.id
    /// JOIN files   src_f ON src_r.file_id    = src_f.id
    /// JOIN repos   src_rp ON src_f.repo_id   = src_rp.id
    ///
    /// JOIN matches tgt_m ON tgt_m.id != src_m.id
    /// JOIN refs    tgt_r ON tgt_m.ref_id     = tgt_r.id
    /// JOIN strings tgt_s ON tgt_r.string_id  = tgt_s.id
    /// JOIN files   tgt_f ON tgt_r.file_id    = tgt_f.id
    ///
    /// WHERE src_rp.name = :repo_name
    ///   AND NOT EXISTS (
    ///       SELECT 1 FROM match_links ml
    ///       WHERE ml.source_match_id = src_m.id AND ml.link_kind = '<link_rule.kind>'
    ///   )
    ///   AND (<link_rule.sql>)        -- ← your WHERE clause goes here
    /// ```
    ///
    /// ## Available columns in your WHERE clause
    ///
    /// | Alias   | Table   | Useful columns                                     |
    /// |---------|---------|----------------------------------------------------|
    /// | src_m   | matches | id, ref_id, rule_name, kind                        |
    /// | src_r   | refs    | id, string_id, file_id, span_start, span_end,      |
    /// |         |         | target_file_id, parent_key_string_id, node_path     |
    /// | src_s   | strings | id, value, norm, norm2                             |
    /// | src_f   | files   | id, repo_id, path, stem, ext                       |
    /// | src_rp  | repos   | id, name, root_path                                |
    /// | tgt_m   | matches | id, ref_id, rule_name, kind                        |
    /// | tgt_r   | refs    | id, string_id, file_id, span_start, span_end,      |
    /// |         |         | target_file_id, parent_key_string_id, node_path     |
    /// | tgt_s   | strings | id, value, norm, norm2                             |
    /// | tgt_f   | files   | id, repo_id, path, stem, ext                       |
    ///
    /// ## Examples
    ///
    /// Import binding (scoped to resolved target file, exact string match):
    /// ```json
    /// {
    ///   "kind": "import_binding",
    ///   "sql": "src_m.kind = 'import_name' AND tgt_m.kind = 'export_name' AND src_r.target_file_id = tgt_r.file_id AND tgt_r.string_id = src_r.string_id"
    /// }
    /// ```
    ///
    /// Cross-repo dep linking by normalized name:
    /// ```json
    /// {
    ///   "kind": "dependency",
    ///   "sql": "src_m.kind = 'dep_name' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm"
    /// }
    /// ```
    ///
    /// ## WARNING: raw SQL injection
    ///
    /// The `sql` field is interpolated directly into the query with no
    /// sanitization. This is a local developer toolchain, not a web service.
    /// The tradeoff is intentional: full SQL expressiveness for prototyping,
    /// with a documented skeleton so the user knows exactly what they're
    /// plugging into. A DSL may replace this in the future.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_rules: Vec<LinkRule>,
}

/// A link rule creates edges in `match_links` between matches.
///
/// The `sql` field is a raw SQL WHERE fragment injected into the skeleton
/// documented on [`RuleSet::link_rules`]. See that doc comment for the
/// full template and available column aliases.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LinkRule {
    /// Identifier for this link rule. Written to match_links.link_kind
    /// and used in log messages.
    pub kind: String,
    /// Raw SQL WHERE clause. Plugged into the skeleton as `AND (<sql>)`.
    /// Available aliases: src_m, src_r, src_s, src_f, src_rp, tgt_m, tgt_r, tgt_s, tgt_f.
    pub sql: String,
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
