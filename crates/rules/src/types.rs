use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How to match an object key in a destructuring pattern.
///
/// Serializes as a plain string:
/// - `"$_"` → Wildcard
/// - `"$NAME"` (screaming after `$`) → Capture
/// - contains `*`, `?`, `[` → Glob
/// - otherwise → Exact
#[derive(Debug, Clone)]
pub enum KeyMatcher {
    /// Match exact key name.
    Exact(String),
    /// Match key name by glob pattern (e.g. `"dep_*"`).
    Glob(String),
    /// Capture the key name into a named variable (e.g. `"$KEY"`).
    Capture(String),
    /// Match any key, don't bind.
    Wildcard,
}

impl KeyMatcher {
    pub fn parse(s: &str) -> Self {
        if s == "$_" {
            KeyMatcher::Wildcard
        } else if s.starts_with('$')
            && s.len() > 1
            && s[1..].starts_with(|c: char| c.is_ascii_uppercase())
            && !s.contains('/')
            && !s.contains(':')
        {
            // Pure $NAME capture (whole key). Mixed patterns like $ORG/$REPO
            // go through Glob → SegmentCapture instead.
            KeyMatcher::Capture(s[1..].to_string())
        } else if s.contains('*') || s.contains('?') || s.contains('[') || s.contains('$') {
            // Glob or segment-capture pattern. compile_pattern detects $
            // and routes to SegmentCapture automatically.
            KeyMatcher::Glob(s.to_string())
        } else {
            KeyMatcher::Exact(s.to_string())
        }
    }

    fn as_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            KeyMatcher::Exact(s) | KeyMatcher::Glob(s) => std::borrow::Cow::Borrowed(s),
            KeyMatcher::Capture(name) => std::borrow::Cow::Owned(format!("${name}")),
            KeyMatcher::Wildcard => std::borrow::Cow::Borrowed("$_"),
        }
    }
}

impl Serialize for KeyMatcher {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for KeyMatcher {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(KeyMatcher::parse(&s))
    }
}

impl JsonSchema for KeyMatcher {
    fn schema_name() -> String {
        "KeyMatcher".to_string()
    }
    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            ..Default::default()
        }
        .into()
    }
}

/// One branch in a destructuring Object pattern.
///
/// The `key` matches against object key names. The `value` is a sub-chain
/// of SelectSteps applied to the matched key's value.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObjectEntry {
    pub key: KeyMatcher,
    pub value: Vec<SelectStep>,
}

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

    /// Line matcher: segment capture (default) or regex (re: prefix).
    /// Runs against a named capture, merges extracted groups back into the capture map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<LineMatcher>,

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

    /// Filter by git rev (branch or tag). pipe-delimited glob.
    Rev {
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

    /// Destructure an object node: match keys and recurse into values.
    ///
    /// Each entry matches a key (exact, glob, capture, or wildcard) and
    /// applies its `value` sub-chain to the matched child. All entries
    /// are conjunctive (all must match). Results are cross-producted
    /// across entries for ancestor carry-forward.
    Object { entries: Vec<ObjectEntry> },

    /// Iterate array elements, applying the sub-chain to each.
    ///
    /// Captures from the surrounding context carry forward into each
    /// element's sub-chain (ancestor carry-forward).
    Array { item: Vec<SelectStep> },

    /// Match a leaf value against a segment-capture pattern.
    ///
    /// Pattern string contains `$VAR` holes compiled to segment captures.
    /// Example: `"$REPO:$TAG"` matches `"nginx:latest"` → {REPO: "nginx", TAG: "latest"}.
    LeafPattern { pattern: String },
}

impl SelectStep {
    /// Returns true for steps that filter by context (git/filesystem) rather
    /// than walking the parsed tree.
    pub fn is_context_step(&self) -> bool {
        matches!(
            self,
            SelectStep::Repo { .. }
                | SelectStep::Rev { .. }
                | SelectStep::Folder { .. }
                | SelectStep::File { .. }
        )
    }
}

/// ast-grep selector for source code files.
///
/// Exactly one of `pattern`, `rule`, or `rule_file` must be set.
///
/// ## Simple pattern
/// ```json
/// { "pattern": "console.log($ARG)", "language": "js", "captures": { "$ARG": "arg" } }
/// ```
///
/// ## Inline ast-grep rule object
/// The `rule` field is the ast-grep `rule:` matcher object (supports `pattern`, `inside`,
/// `has`, `all`, `any`, `not`, `kind`, `regex`). `language` is required.
/// `constraints` mirrors the ast-grep `constraints:` field.
/// ```json
/// {
///   "language": "typescript",
///   "rule": { "all": [{ "pattern": "foo($ARG)" }, { "inside": { "kind": "function_declaration" } }] },
///   "constraints": { "ARG": { "regex": "^bar" } },
///   "captures": { "$ARG": "arg" }
/// }
/// ```
///
/// ## Rule file reference
/// Path to a complete ast-grep `.yml` rule file, relative to the rules JSON/YAML file.
/// Language is read from the rule file. `captures` can still be provided here to map
/// metavariables to capture names.
/// ```json
/// { "rule_file": "ast-rules/my-rule.yml", "captures": { "$NAME": "name" } }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AstSelector {
    /// Simple pattern string (e.g. `"import $NAME from $PATH"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Inline ast-grep rule matcher object. Mirrors the `rule:` field in ast-grep YAML.
    /// Supports `pattern`, `inside`, `has`, `follows`, `precedes`, `all`, `any`, `not`,
    /// `kind`, `regex`, `matches`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<serde_json::Value>,

    /// Inline `constraints:` for metavariables. Only used when `rule` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<serde_json::Value>,

    /// Path to a complete ast-grep `.yml` rule file, relative to the sprefa rules file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_file: Option<String>,

    /// Language override. Required when using `rule`. Inferred from file extension otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Single-capture: the whole matched node's text goes into this capture name.
    /// Ignored when `captures` is set. Default: "$NAME".
    #[serde(default = "default_ast_capture")]
    pub capture: String,

    /// Multi-capture: map of metavar (e.g. "$FUNC") to capture name (e.g. "name").
    /// When set, each listed metavar is extracted as a separate capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captures: Option<std::collections::BTreeMap<String, String>>,

    /// Segment-capture post-processing for synthetic metavars.
    /// Maps metavar name (without `$`) to a segment pattern string.
    /// e.g. `"_SPREFA_0" -> "use${ENTITY}Query"`.
    /// After ast-grep matches, the metavar's text is matched against the segment
    /// pattern to extract named sub-captures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_captures: Option<std::collections::BTreeMap<String, String>>,
}

fn default_ast_capture() -> String {
    "$NAME".to_string()
}

/// Matcher applied to a named capture's text. Extracts sub-captures
/// and merges them back into the capture map.
///
/// Default mode: segment capture (`$NAME:$TAG` style).
/// Prefix the pattern with `re:` for raw regex with `(?P<name>...)` groups.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LineMatcher {
    /// Segment capture: `$ORG/$REPO`, `$NAME:$TAG`, etc.
    Segments {
        source: String,
        pattern: String,
    },
    /// Raw regex with named groups via `(?P<name>...)`.
    Regex {
        source: String,
        pattern: String,
        #[serde(default = "default_true")]
        full_match: bool,
    },
}

fn default_true() -> bool {
    true
}

fn is_false(v: &bool) -> bool {
    !v
}

/// One match to create from a captured value.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MatchDef {
    /// Name of the capture to use as the ref value.
    pub capture: String,
    /// Free-text kind string (e.g. "dep_name", "deploy_value", "operation_id").
    pub kind: String,
    /// Name of another capture to use as parent_key (links related refs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// When set, this match's value drives demand scanning.
    /// "repo" = value is a repository name, "rev" = value is a tag/branch to scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<String>,
}
