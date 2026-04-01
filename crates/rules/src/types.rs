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
        {
            KeyMatcher::Capture(s[1..].to_string())
        } else if s.contains('*') || s.contains('?') || s.contains('[') {
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

/// A compiled query rule: Datalog-style horn clause over match_links.
///
/// Parsed from `.sprf` syntax:
/// ```sprf
/// query all_deps($A, $C) :- dep_to_package($A, $B), all_deps($B, $C);
/// ```
///
/// Compiled to SQL CTEs by `query::compile_query()`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryDef {
    /// Name of this derived relation.
    pub name: String,
    /// Number of columns (arity of the head atom).
    pub arity: usize,
    /// Head argument names (variable names or "_" for wild).
    pub head_args: Vec<String>,
    /// Body atoms: each is (relation_name, args).
    /// Args are either variable names, literal strings prefixed with `=`,
    /// or `_` for wildcard.
    pub body: Vec<QueryAtom>,
    /// Whether head relation appears in body (requires WITH RECURSIVE).
    pub is_recursive: bool,
}

/// One atom in a query body, as lowered from the parse tree.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryAtom {
    pub relation: String,
    /// Each arg is: variable name (e.g. "A"), literal prefixed with `=` (e.g. "=lodash"),
    /// or "_" for wildcard.
    pub args: Vec<String>,
}

/// Top-level rules file: an array of rules plus optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuleSet {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub rules: Vec<Rule>,
    /// Link rules create edges in `match_links` between matches produced by
    /// extraction rules. Each link rule uses either a `predicate` (structured
    /// DSL compiled to SQL) or a raw `sql` WHERE clause injected into a fixed
    /// query skeleton. Exactly one must be set.
    ///
    /// ## Predicate DSL (preferred)
    ///
    /// ```json
    /// {
    ///   "kind": "import_binding",
    ///   "predicate": {
    ///     "op": "and",
    ///     "all": [
    ///       { "op": "kind_eq", "side": "src", "value": "import_name" },
    ///       { "op": "kind_eq", "side": "tgt", "value": "export_name" },
    ///       { "op": "target_file_eq" },
    ///       { "op": "string_eq" }
    ///     ]
    ///   }
    /// }
    /// ```
    ///
    /// Available predicates: `kind_eq`, `norm_eq`, `norm2_eq`, `target_file_eq`,
    /// `string_eq`, `same_repo`, `and`.
    ///
    /// ## Raw SQL escape hatch
    ///
    /// For predicates the DSL cannot express, use `sql` with a raw WHERE fragment.
    /// The fragment is interpolated directly (no sanitization). Available aliases:
    /// src_m, src_r, src_s, src_f, src_rp, tgt_m, tgt_r, tgt_s, tgt_f.
    ///
    /// ```json
    /// {
    ///   "kind": "dependency",
    ///   "sql": "src_m.kind = 'dep_name' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm"
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_rules: Vec<LinkRule>,
    /// Query rules: Datalog-style derived relations over match_links.
    /// Compiled to SQL CTEs at query time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_rules: Vec<QueryDef>,
}

/// A link rule creates edges in `match_links` between matches.
///
/// Exactly one of `sql` or `predicate` must be set. The predicate DSL
/// covers common patterns (kind checks, norm equality, file scoping).
/// The raw `sql` escape hatch remains for anything the DSL can't express.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LinkRule {
    /// Identifier for this link rule. Written to match_links.link_kind
    /// and used in log messages.
    pub kind: String,
    /// Raw SQL WHERE clause. Plugged into the skeleton as `AND (<sql>)`.
    /// Available aliases: src_m, src_r, src_s, src_f, src_rp, tgt_m, tgt_r, tgt_s, tgt_f.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    /// Structured predicate compiled to SQL at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<LinkPredicate>,
    /// Restrict target matches to these repo names. When set, only matches
    /// in the listed repos can be link targets. When absent, targets are
    /// unconstrained (cross-repo by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_repos: Option<Vec<String>>,
}

/// Which side of a link (source or target) a predicate applies to.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Src,
    Tgt,
}

/// Structured predicate for link rules, compiled to SQL WHERE fragments.
///
/// Each variant maps to a specific SQL condition against the link skeleton's
/// aliases (src_m, src_r, src_s, src_f, tgt_m, tgt_r, tgt_s, tgt_f).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum LinkPredicate {
    /// Match kind on one side: `{side}_m.kind = '{value}'`
    KindEq { side: Side, value: String },
    /// Normalized strings equal: `src_s.norm = tgt_s.norm`
    NormEq,
    /// Secondary normalization equal: `src_s.norm2 = tgt_s.norm2`
    Norm2Eq,
    /// Source's resolved target file matches target's file: `src_r.target_file_id = tgt_r.file_id`
    TargetFileEq,
    /// Same string_id on both sides: `tgt_r.string_id = src_r.string_id`
    StringEq,
    /// Both matches in the same repo: `src_f.repo_id = COALESCE(tgt_f.repo_id, tgt_rr.repo_id)`
    SameRepo,
    /// Both matches in the same file: `src_r.file_id = tgt_r.file_id`
    SameFile,
    /// File stem on {side} matches string norm on other side.
    StemEq { side: Side },
    /// File extension on {side} matches string norm on other side.
    ExtEq { side: Side },
    /// File dir on {side} matches string norm on other side.
    DirEq { side: Side },
    /// All sub-predicates must hold.
    And { all: Vec<LinkPredicate> },
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

    /// Destructure an object node: match keys and recurse into values.
    ///
    /// Each entry matches a key (exact, glob, capture, or wildcard) and
    /// applies its `value` sub-chain to the matched child. All entries
    /// are conjunctive (all must match). Results are cross-producted
    /// across entries for ancestor carry-forward.
    Object {
        entries: Vec<ObjectEntry>,
    },

    /// Iterate array elements, applying the sub-chain to each.
    ///
    /// Captures from the surrounding context carry forward into each
    /// element's sub-chain (ancestor carry-forward).
    Array {
        item: Vec<SelectStep>,
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
