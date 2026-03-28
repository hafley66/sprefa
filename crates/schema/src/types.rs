use serde::{Deserialize, Serialize};

/// Controls which branch tier search_refs returns results from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BranchScope {
    /// Only committed branches (is_working_tree = 0)
    Committed,
    /// Only working-tree branches (is_working_tree = 1)
    Local,
    /// Both committed and working-tree (default, no filter)
    All,
}

/// Every interesting string extracted from source files gets classified by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum RefKind {
    StringLiteral = 0,
    JsonKey = 1,
    JsonValue = 2,
    YamlKey = 3,
    YamlValue = 4,
    TomlKey = 5,
    TomlValue = 6,
    ImportPath = 10,
    ImportName = 11,
    ExportName = 12,
    ImportAlias = 13,
    ExportLocalBinding = 14,
    DepName = 20,
    DepVersion = 21,
    RsUse = 30,
    RsDeclare = 31,
    RsMod = 32,
}

impl RefKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::StringLiteral),
            1 => Some(Self::JsonKey),
            2 => Some(Self::JsonValue),
            3 => Some(Self::YamlKey),
            4 => Some(Self::YamlValue),
            5 => Some(Self::TomlKey),
            6 => Some(Self::TomlValue),
            10 => Some(Self::ImportPath),
            11 => Some(Self::ImportName),
            12 => Some(Self::ExportName),
            13 => Some(Self::ImportAlias),
            14 => Some(Self::ExportLocalBinding),
            20 => Some(Self::DepName),
            21 => Some(Self::DepVersion),
            30 => Some(Self::RsUse),
            31 => Some(Self::RsDeclare),
            32 => Some(Self::RsMod),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Convert to the canonical kind string used in matches_v2.
    pub fn to_kind_str(self) -> &'static str {
        match self {
            Self::StringLiteral => "string_literal",
            Self::JsonKey => "json_key",
            Self::JsonValue => "json_value",
            Self::YamlKey => "yaml_key",
            Self::YamlValue => "yaml_value",
            Self::TomlKey => "toml_key",
            Self::TomlValue => "toml_value",
            Self::ImportPath => "import_path",
            Self::ImportName => "import_name",
            Self::ExportName => "export_name",
            Self::ImportAlias => "import_alias",
            Self::ExportLocalBinding => "export_local_binding",
            Self::DepName => "dep_name",
            Self::DepVersion => "dep_version",
            Self::RsUse => "rs_use",
            Self::RsDeclare => "rs_declare",
            Self::RsMod => "rs_mod",
        }
    }

    /// Parse a kind string back to RefKind (for known kinds only).
    pub fn from_kind_str(s: &str) -> Option<Self> {
        match s {
            "string_literal" => Some(Self::StringLiteral),
            "json_key" => Some(Self::JsonKey),
            "json_value" => Some(Self::JsonValue),
            "yaml_key" => Some(Self::YamlKey),
            "yaml_value" => Some(Self::YamlValue),
            "toml_key" => Some(Self::TomlKey),
            "toml_value" => Some(Self::TomlValue),
            "import_path" => Some(Self::ImportPath),
            "import_name" => Some(Self::ImportName),
            "export_name" => Some(Self::ExportName),
            "import_alias" => Some(Self::ImportAlias),
            "export_local_binding" => Some(Self::ExportLocalBinding),
            "dep_name" => Some(Self::DepName),
            "dep_version" => Some(Self::DepVersion),
            "rs_use" => Some(Self::RsUse),
            "rs_declare" => Some(Self::RsDeclare),
            "rs_mod" => Some(Self::RsMod),
            _ => None,
        }
    }
}

/// Row from the repos table.
#[derive(Debug, Clone, Serialize)]
pub struct Repo {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub org: Option<String>,
    pub git_hash: Option<String>,
    pub last_fetched_at: Option<String>,
    pub last_synced_at: Option<String>,
    pub last_remote_commit_at: Option<String>,
    pub scanned_at: Option<String>,
}

/// Row from the files table.
#[derive(Debug, Clone, Serialize)]
pub struct File {
    pub id: i64,
    pub repo_id: i64,
    pub path: String,
    pub content_hash: String,
    pub stem: Option<String>,
    pub ext: Option<String>,
    pub scanned_at: Option<String>,
}

/// Row from the strings table.
#[derive(Debug, Clone, Serialize)]
pub struct StringRow {
    pub id: i64,
    pub value: String,
    pub norm: String,
    pub norm2: Option<String>,
}

/// Row from the refs table.
#[derive(Debug, Clone, Serialize)]
pub struct Ref {
    pub id: i64,
    pub string_id: i64,
    pub file_id: i64,
    pub span_start: i64,
    pub span_end: i64,
    pub is_path: bool,
    pub confidence: Option<f64>,
    pub target_file_id: Option<i64>,
    pub ref_kind: u8,
    pub parent_key_string_id: Option<i64>,
    /// "/"-joined structural path through the parsed tree. Used for anti-unification.
    pub node_path: Option<String>,
}

/// Row from the branch_files junction table.
#[derive(Debug, Clone, Serialize)]
pub struct BranchFile {
    pub repo_id: i64,
    pub branch: String,
    pub file_id: i64,
}

/// Row from the repo_branches table.
#[derive(Debug, Clone, Serialize)]
pub struct RepoBranch {
    pub repo_id: i64,
    pub branch: String,
    pub git_hash: Option<String>,
    pub is_working_tree: bool,
}

/// Row from the git_tags table.
#[derive(Debug, Clone, Serialize)]
pub struct GitTag {
    pub id: i64,
    pub repo_id: i64,
    pub tag_name: String,
    pub commit_hash: Option<String>,
    pub is_semver: bool,
    pub created_at: Option<String>,
}

/// A single ref occurrence returned by a query: where in the codebase a string appears.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefLocation {
    pub repo: String,
    pub file_path: String,
    pub kind: String,
    pub rule_name: String,
    pub span_start: i64,
    pub span_end: i64,
}

/// A matched string plus all the places it appears as a ref.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHit {
    pub string_id: i64,
    pub value: String,
    pub norm: String,
    pub refs: Vec<RefLocation>,
}

/// Row from the repo_packages table.
#[derive(Debug, Clone, Serialize)]
pub struct RepoPackage {
    pub id: i64,
    pub repo_id: i64,
    pub package_name: String,
    pub ecosystem: String,
    pub manifest_path: String,
}
