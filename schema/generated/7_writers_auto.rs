// Generated typed SQLite rows and writers. Do not edit.

#[derive(Debug)]

pub enum InsertError { Sql(rusqlite::Error), Json(serde_json::Error), OrdinalOverflow, SQLiteLimit(&'static str) }

impl std::fmt::Display for InsertError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") } }

impl std::error::Error for InsertError {}

impl From<rusqlite::Error> for InsertError { fn from(value: rusqlite::Error) -> Self { Self::Sql(value) } }

impl From<serde_json::Error> for InsertError { fn from(value: serde_json::Error) -> Self { Self::Json(value) } }

fn u64_value(value: u64) -> rusqlite::types::Value {

    match i64::try_from(value) { Ok(value) => rusqlite::types::Value::Integer(value), Err(_) => rusqlite::types::Value::Text(value.to_string()) }

}

fn optional_non_null<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(deserializer: D) -> Result<Option<T>, D::Error> {

    <Option<T> as serde::Deserialize>::deserialize(deserializer)?.map(Some).ok_or_else(|| serde::de::Error::custom("null is not allowed"))

}

fn required_nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(deserializer: D) -> Result<Option<T>, D::Error> { <Option<T> as serde::Deserialize>::deserialize(deserializer) }

#[derive(Clone, Copy)]

pub struct Source<'a> {

    pub row: i64,

    pub input_path: Option<&'a str>,

    pub content_id: Option<&'a str>,

}

pub mod models {

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum Mode {
        #[serde(rename = "syntax")]
        Syntax,
        #[serde(rename = "semantic")]
        Semantic,
    }
    impl Mode {
        pub(super) fn as_str(&self) -> &'static str {
            match self {
                Self::Syntax => "syntax",
                Self::Semantic => "semantic",
            }
        }
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(untagged)]
    pub enum TsiArg {
        id(IdArg),
        span(SpanArg),
        text(TextArg),
        int(IntArg),
        atom(AtomArg),
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct IdArg {
        pub id: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SpanArg {
        pub span: (String, u32, u32),
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TextArg {
        pub text: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct IntArg {
        pub int: i64,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct AtomArg {
        pub atom: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum Method {
        #[serde(rename = "same_file")]
        SameFile,
        #[serde(rename = "corpus_unique")]
        CorpusUnique,
        #[serde(rename = "module_plane")]
        ModulePlane,
        #[serde(rename = "checker")]
        Checker,
        #[serde(rename = "alias_chain")]
        AliasChain,
        #[serde(rename = "param")]
        Param,
        #[serde(rename = "receiver")]
        Receiver,
        #[serde(rename = "self_type")]
        SelfType,
        #[serde(rename = "iface_impl")]
        IfaceImpl,
        #[serde(rename = "decorator")]
        Decorator,
        #[serde(rename = "subscript")]
        Subscript,
        #[serde(rename = "return_call")]
        ReturnCall,
        #[serde(rename = "scip")]
        Scip,
        #[serde(rename = "unresolved")]
        Unresolved,
        #[serde(rename = "parse")]
        Parse,
        #[serde(rename = "checker_walk")]
        CheckerWalk,
        #[serde(rename = "foreign")]
        Foreign,
    }
    impl Method {
        pub(super) fn as_str(&self) -> &'static str {
            match self {
                Self::SameFile => "same_file",
                Self::CorpusUnique => "corpus_unique",
                Self::ModulePlane => "module_plane",
                Self::Checker => "checker",
                Self::AliasChain => "alias_chain",
                Self::Param => "param",
                Self::Receiver => "receiver",
                Self::SelfType => "self_type",
                Self::IfaceImpl => "iface_impl",
                Self::Decorator => "decorator",
                Self::Subscript => "subscript",
                Self::ReturnCall => "return_call",
                Self::Scip => "scip",
                Self::Unresolved => "unresolved",
                Self::Parse => "parse",
                Self::CheckerWalk => "checker_walk",
                Self::Foreign => "foreign",
            }
        }
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum CoverageFlag {
        #[serde(rename = "partial")]
        Partial,
        #[serde(rename = "complete")]
        Complete,
    }
    impl CoverageFlag {
        pub(super) fn as_str(&self) -> &'static str {
            match self {
                Self::Partial => "partial",
                Self::Complete => "complete",
            }
        }
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum FamilyTag {
        #[serde(rename = "df")]
        Df,
        #[serde(rename = "flow")]
        Flow,
        #[serde(rename = "call")]
        Call,
        #[serde(rename = "type")]
        Type,
        #[serde(rename = "module")]
        Module,
        #[serde(rename = "cst")]
        Cst,
        #[serde(rename = "cfg")]
        Cfg,
        #[serde(rename = "data")]
        Data,
    }
    impl FamilyTag {
        pub(super) fn as_str(&self) -> &'static str {
            match self {
                Self::Df => "df",
                Self::Flow => "flow",
                Self::Call => "call",
                Self::Type => "type",
                Self::Module => "module",
                Self::Cst => "cst",
                Self::Cfg => "cfg",
                Self::Data => "data",
            }
        }
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SpanOut {
        pub start: u32,
        pub end: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Protocol {
        pub version: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Run {
        pub run: u32,
        pub mode: Mode,
        pub tool: String,
        pub version: String,
        pub scope: Vec<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Fact {
        pub fact: u32,
        pub relation: String,
        pub args: Vec<TsiArg>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Witness {
        pub fact: u32,
        pub run: u32,
        pub method: Method,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Coverage {
        pub run: u32,
        pub relation: String,
        pub coverage: CoverageFlag,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Diagnostic {
        pub run: u32,
        pub relation: String,
        pub detail: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Node {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        pub kind: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub name: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Edge {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub kind: String,
        pub from: SpanOut,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub from_kind: Option<String>,
        pub to: SpanOut,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub to_kind: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Param {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        pub pos: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Arg {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub call: SpanOut,
        pub pos: i64,
        pub arg: SpanOut,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DfField {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub owner: SpanOut,
        pub name: String,
        pub value: SpanOut,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DfLit {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub node: SpanOut,
        pub kind: String,
        pub text: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DfLoop {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        #[serde(deserialize_with = "super::required_nullable")]
        pub var: Option<String>,
        #[serde(deserialize_with = "super::required_nullable")]
        pub collection: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DfNest {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub call: SpanOut,
        pub r#loop: SpanOut,
        pub depth: u32,
        #[serde(deserialize_with = "super::required_nullable")]
        pub collection: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DfAllocates {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub owner: SpanOut,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Sig {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub owner: SpanOut,
        pub owner_start: u32,
        pub owner_end: u32,
        pub slot: String,
        pub pos: u32,
        pub ty: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Site {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        pub callee: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub callee_path: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Const {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub owner: SpanOut,
        #[serde(deserialize_with = "super::required_nullable")]
        pub field: Option<String>,
        pub text: String,
        pub kind: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Doc {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub owner: SpanOut,
        #[serde(deserialize_with = "super::required_nullable")]
        pub parent: Option<String>,
        pub text: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DocTag {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub owner: SpanOut,
        pub tag: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub arg: Option<String>,
        pub text: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DocNode {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        pub kind: String,
        pub name: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub parent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub body: Option<SpanOut>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DataDoc {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub ordinal: u32,
        pub span: SpanOut,
        pub format: String,
        pub doc: serde_json::Value,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DataValue {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub ordinal: u32,
        pub path: String,
        pub kind: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub text: Option<String>,
        pub span: SpanOut,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Specifier {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        pub name: String,
        pub kind: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub module: Option<String>,
        #[serde(deserialize_with = "super::required_nullable")]
        pub imported: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MethodOwner {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub owner: SpanOut,
        #[serde(deserialize_with = "super::required_nullable")]
        pub self_type: Option<String>,
        #[serde(deserialize_with = "super::required_nullable")]
        pub r#trait: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct CfgScope {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        pub cfg: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TestOnlyCall {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub callee: String,
        pub cfg: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MacroSite {
        pub family: FamilyTag,
        pub span: SpanOut,
        pub macro_name: String,
        pub source: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Reference {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub family: FamilyTag,
        pub span: SpanOut,
        pub functor: String,
        pub position: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Unresolved {
        pub family: FamilyTag,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub path: Option<String>,
        pub span: SpanOut,
        pub reason: String,
        pub detail: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Projectedge {
        pub family: FamilyTag,
        pub kind: String,
        pub from: SpanOut,
        pub to_blob: String,
        pub to: SpanOut,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct FlowEdge {
        pub family: FamilyTag,
        pub kind: String,
        pub from_blob: String,
        pub from: SpanOut,
        pub to_blob: String,
        pub to: SpanOut,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ResolvedEdge {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub caller_path: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub caller_name: Option<String>,
        pub callee_path: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub callee_name: Option<String>,
        pub caller_site_start: u32,
        pub caller_site_end: u32,
        pub kind: String,
        pub resolution_origin: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ResolvedTypeEdge {
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub fact: Option<u32>,
        pub owner_path: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub owner_name: Option<String>,
        pub owner_start: u32,
        pub owner_end: u32,
        pub target_path: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub target_name: Option<String>,
        pub kind: String,
        pub resolution_origin: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ResolvedImport {
        pub src_path: String,
        pub name: String,
        pub local: String,
        pub target_path: String,
        #[serde(deserialize_with = "super::required_nullable")]
        pub target_name: Option<String>,
        pub kind: String,
        pub hops: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct FileEdge {
        pub src_path: String,
        pub dst_path: String,
        pub kind: String,
        pub symbols: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct FileUnresolved {
        pub src_path: String,
        pub module: String,
        pub reason: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PackageEdge {
        pub src_manifest: String,
        pub dst_manifest: String,
        pub kind: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct File {
        pub path: String,
        pub digest: String,
        pub bytes: u32,
        pub lines: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SizeSkip {
        pub path: String,
        pub bytes: u64,
        pub limit: u64,
        pub reason: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Capture {
        pub query: String,
        pub capture: String,
        pub text: String,
        pub start: u32,
        pub end: u32,
        pub match_start: u32,
        pub match_end: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipDef {
        pub symbol: String,
        pub file: String,
        pub repo: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipName {
        pub symbol: String,
        pub name: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipRef {
        pub file: String,
        pub symbol: String,
        pub def_file: String,
        pub repo: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipEdge {
        pub src: String,
        pub dst: String,
        pub repo: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipFnEdge {
        pub caller: String,
        pub callee: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipCalleeType {
        pub sym: String,
        pub r#type: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipLocal {
        pub r#fn: String,
        pub name: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipImpl {
        pub r#impl: String,
        pub iface: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipIndex {
        pub reused: bool,
        pub tool_name: String,
        pub tool_version: String,
        pub documents: u32,
        #[serde(deserialize_with = "super::required_nullable")]
        pub index_mtime_unix_ms: Option<u64>,
        pub staleness: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipSkip {
        pub lang: String,
        pub bin: String,
        pub reason: String,
        pub detail: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipOccurrence {
        pub path: String,
        pub symbol: String,
        pub start: u32,
        pub end: u32,
        pub roles: i32,
        pub definition: bool,
        pub import: bool,
        pub write_access: bool,
        pub read_access: bool,
        pub generated: bool,
        pub test: bool,
        pub forward_definition: bool,
        pub syntax_kind: i32,
        #[serde(deserialize_with = "super::required_nullable")]
        pub enclosing_start: Option<u32>,
        #[serde(deserialize_with = "super::required_nullable")]
        pub enclosing_end: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::optional_non_null")]
        pub text: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipOccurrenceDoc {
        pub path: String,
        pub start: u32,
        pub end: u32,
        pub pos: u32,
        pub text: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipDiagnostic {
        pub path: String,
        pub start: u32,
        pub end: u32,
        pub severity: i32,
        pub code: String,
        pub message: String,
        pub source: String,
        pub tags: Vec<i32>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipSymbol {
        #[serde(deserialize_with = "super::required_nullable")]
        pub path: Option<String>,
        pub symbol: String,
        pub display_name: String,
        pub kind: i32,
        pub enclosing_symbol: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipDocumentation {
        pub symbol: String,
        pub pos: u32,
        pub text: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipSignature {
        pub symbol: String,
        pub language: String,
        pub text: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipSignatureOccurrence {
        pub symbol: String,
        pub ref_symbol: String,
        pub start: u32,
        pub end: u32,
        pub roles: i32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipMetadata {
        pub version: i32,
        pub tool_name: String,
        pub tool_version: String,
        pub tool_arguments: Vec<String>,
        pub project_root: String,
        pub text_document_encoding: i32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipDocument {
        pub path: String,
        pub language: String,
        pub position_encoding: i32,
        #[serde(deserialize_with = "super::required_nullable")]
        pub text: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScipRelationship {
        pub symbol: String,
        pub related_symbol: String,
        pub is_reference: bool,
        pub is_implementation: bool,
        pub is_type_definition: bool,
        pub is_definition: bool,
    }

}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]

#[serde(tag = "record")]

pub enum Fact {

    #[serde(rename = "protocol")]
    Protocol(models::Protocol),

    #[serde(rename = "run")]
    Run(models::Run),

    #[serde(rename = "fact")]
    Fact(models::Fact),

    #[serde(rename = "witness")]
    Witness(models::Witness),

    #[serde(rename = "coverage")]
    Coverage(models::Coverage),

    #[serde(rename = "diagnostic")]
    Diagnostic(models::Diagnostic),

    #[serde(rename = "node")]
    Node(models::Node),

    #[serde(rename = "edge")]
    Edge(models::Edge),

    #[serde(rename = "param")]
    Param(models::Param),

    #[serde(rename = "arg")]
    Arg(models::Arg),

    #[serde(rename = "df_field")]
    DfField(models::DfField),

    #[serde(rename = "df_lit")]
    DfLit(models::DfLit),

    #[serde(rename = "df_loop")]
    DfLoop(models::DfLoop),

    #[serde(rename = "df_nest")]
    DfNest(models::DfNest),

    #[serde(rename = "df_allocates")]
    DfAllocates(models::DfAllocates),

    #[serde(rename = "sig")]
    Sig(models::Sig),

    #[serde(rename = "site")]
    Site(models::Site),

    #[serde(rename = "const")]
    Const(models::Const),

    #[serde(rename = "doc")]
    Doc(models::Doc),

    #[serde(rename = "doc_tag")]
    DocTag(models::DocTag),

    #[serde(rename = "doc_node")]
    DocNode(models::DocNode),

    #[serde(rename = "data_doc")]
    DataDoc(models::DataDoc),

    #[serde(rename = "data_value")]
    DataValue(models::DataValue),

    #[serde(rename = "specifier")]
    Specifier(models::Specifier),

    #[serde(rename = "method_owner")]
    MethodOwner(models::MethodOwner),

    #[serde(rename = "cfg_scope")]
    CfgScope(models::CfgScope),

    #[serde(rename = "test_only_call")]
    TestOnlyCall(models::TestOnlyCall),

    #[serde(rename = "macro_site")]
    MacroSite(models::MacroSite),

    #[serde(rename = "reference")]
    Reference(models::Reference),

    #[serde(rename = "unresolved")]
    Unresolved(models::Unresolved),

    #[serde(rename = "projectedge")]
    Projectedge(models::Projectedge),

    #[serde(rename = "flow_edge")]
    FlowEdge(models::FlowEdge),

    #[serde(rename = "resolved_edge")]
    ResolvedEdge(models::ResolvedEdge),

    #[serde(rename = "resolved_type_edge")]
    ResolvedTypeEdge(models::ResolvedTypeEdge),

    #[serde(rename = "resolved_import")]
    ResolvedImport(models::ResolvedImport),

    #[serde(rename = "file_edge")]
    FileEdge(models::FileEdge),

    #[serde(rename = "file_unresolved")]
    FileUnresolved(models::FileUnresolved),

    #[serde(rename = "package_edge")]
    PackageEdge(models::PackageEdge),

    #[serde(rename = "file")]
    File(models::File),

    #[serde(rename = "size_skip")]
    SizeSkip(models::SizeSkip),

    #[serde(rename = "capture")]
    Capture(models::Capture),

    #[serde(rename = "scip_def")]
    ScipDef(models::ScipDef),

    #[serde(rename = "scip_name")]
    ScipName(models::ScipName),

    #[serde(rename = "scip_ref")]
    ScipRef(models::ScipRef),

    #[serde(rename = "scip_edge")]
    ScipEdge(models::ScipEdge),

    #[serde(rename = "scip_fn_edge")]
    ScipFnEdge(models::ScipFnEdge),

    #[serde(rename = "scip_callee_type")]
    ScipCalleeType(models::ScipCalleeType),

    #[serde(rename = "scip_local")]
    ScipLocal(models::ScipLocal),

    #[serde(rename = "scip_impl")]
    ScipImpl(models::ScipImpl),

    #[serde(rename = "scip_index")]
    ScipIndex(models::ScipIndex),

    #[serde(rename = "scip_skip")]
    ScipSkip(models::ScipSkip),

    #[serde(rename = "scip_occurrence")]
    ScipOccurrence(models::ScipOccurrence),

    #[serde(rename = "scip_occurrence_doc")]
    ScipOccurrenceDoc(models::ScipOccurrenceDoc),

    #[serde(rename = "scip_diagnostic")]
    ScipDiagnostic(models::ScipDiagnostic),

    #[serde(rename = "scip_symbol")]
    ScipSymbol(models::ScipSymbol),

    #[serde(rename = "scip_documentation")]
    ScipDocumentation(models::ScipDocumentation),

    #[serde(rename = "scip_signature")]
    ScipSignature(models::ScipSignature),

    #[serde(rename = "scip_signature_occurrence")]
    ScipSignatureOccurrence(models::ScipSignatureOccurrence),

    #[serde(rename = "scip_metadata")]
    ScipMetadata(models::ScipMetadata),

    #[serde(rename = "scip_document")]
    ScipDocument(models::ScipDocument),

    #[serde(rename = "scip_relationship")]
    ScipRelationship(models::ScipRelationship),

}

impl Fact {

    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {

        match self {

            Self::Protocol(row) => row.insert(conn, source),

            Self::Run(row) => row.insert(conn, source),

            Self::Fact(row) => row.insert(conn, source),

            Self::Witness(row) => row.insert(conn, source),

            Self::Coverage(row) => row.insert(conn, source),

            Self::Diagnostic(row) => row.insert(conn, source),

            Self::Node(row) => row.insert(conn, source),

            Self::Edge(row) => row.insert(conn, source),

            Self::Param(row) => row.insert(conn, source),

            Self::Arg(row) => row.insert(conn, source),

            Self::DfField(row) => row.insert(conn, source),

            Self::DfLit(row) => row.insert(conn, source),

            Self::DfLoop(row) => row.insert(conn, source),

            Self::DfNest(row) => row.insert(conn, source),

            Self::DfAllocates(row) => row.insert(conn, source),

            Self::Sig(row) => row.insert(conn, source),

            Self::Site(row) => row.insert(conn, source),

            Self::Const(row) => row.insert(conn, source),

            Self::Doc(row) => row.insert(conn, source),

            Self::DocTag(row) => row.insert(conn, source),

            Self::DocNode(row) => row.insert(conn, source),

            Self::DataDoc(row) => row.insert(conn, source),

            Self::DataValue(row) => row.insert(conn, source),

            Self::Specifier(row) => row.insert(conn, source),

            Self::MethodOwner(row) => row.insert(conn, source),

            Self::CfgScope(row) => row.insert(conn, source),

            Self::TestOnlyCall(row) => row.insert(conn, source),

            Self::MacroSite(row) => row.insert(conn, source),

            Self::Reference(row) => row.insert(conn, source),

            Self::Unresolved(row) => row.insert(conn, source),

            Self::Projectedge(row) => row.insert(conn, source),

            Self::FlowEdge(row) => row.insert(conn, source),

            Self::ResolvedEdge(row) => row.insert(conn, source),

            Self::ResolvedTypeEdge(row) => row.insert(conn, source),

            Self::ResolvedImport(row) => row.insert(conn, source),

            Self::FileEdge(row) => row.insert(conn, source),

            Self::FileUnresolved(row) => row.insert(conn, source),

            Self::PackageEdge(row) => row.insert(conn, source),

            Self::File(row) => row.insert(conn, source),

            Self::SizeSkip(row) => row.insert(conn, source),

            Self::Capture(row) => row.insert(conn, source),

            Self::ScipDef(row) => row.insert(conn, source),

            Self::ScipName(row) => row.insert(conn, source),

            Self::ScipRef(row) => row.insert(conn, source),

            Self::ScipEdge(row) => row.insert(conn, source),

            Self::ScipFnEdge(row) => row.insert(conn, source),

            Self::ScipCalleeType(row) => row.insert(conn, source),

            Self::ScipLocal(row) => row.insert(conn, source),

            Self::ScipImpl(row) => row.insert(conn, source),

            Self::ScipIndex(row) => row.insert(conn, source),

            Self::ScipSkip(row) => row.insert(conn, source),

            Self::ScipOccurrence(row) => row.insert(conn, source),

            Self::ScipOccurrenceDoc(row) => row.insert(conn, source),

            Self::ScipDiagnostic(row) => row.insert(conn, source),

            Self::ScipSymbol(row) => row.insert(conn, source),

            Self::ScipDocumentation(row) => row.insert(conn, source),

            Self::ScipSignature(row) => row.insert(conn, source),

            Self::ScipSignatureOccurrence(row) => row.insert(conn, source),

            Self::ScipMetadata(row) => row.insert(conn, source),

            Self::ScipDocument(row) => row.insert(conn, source),

            Self::ScipRelationship(row) => row.insert(conn, source),

        }

    }

}

pub const TABLE_COUNT: usize = 61;

fn statement_capacity(conn: &rusqlite::Connection, columns: usize, prefix: &str, tuple: &str) -> Result<usize, InsertError> {

    let variables = usize::try_from(conn.limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)?).map_err(|_| InsertError::SQLiteLimit("variable"))?;

    let sql_length = usize::try_from(conn.limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH)?).map_err(|_| InsertError::SQLiteLimit("sql length"))?;

    let by_variables = variables / columns;

    let available = sql_length.checked_sub(prefix.len()).ok_or(InsertError::SQLiteLimit("sql length"))?;

    let by_sql = available.checked_add(2).ok_or(InsertError::SQLiteLimit("sql length"))? / (tuple.len() + 2);

    let capacity = by_variables.min(by_sql);

    if capacity == 0 { return Err(InsertError::SQLiteLimit("one row does not fit")); }

    Ok(capacity)

}

fn multi_row_sql(prefix: &str, tuple: &str, rows: usize) -> String {

    let mut sql = String::with_capacity(prefix.len() + rows * (tuple.len() + 2));

    sql.push_str(prefix);

    for index in 0..rows { if index > 0 { sql.push_str(", "); } sql.push_str(tuple); }

    sql

}

pub fn max_batch_rows(conn: &rusqlite::Connection) -> Result<usize, InsertError> {

    statement_capacity(conn, 5, "INSERT INTO \"protocol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\") VALUES ", "(?, ?, ?, ?, ?)")

}

pub fn insert_all(conn: &rusqlite::Connection, source: &Source<'_>, rows: &[Fact]) -> Result<usize, InsertError> {

    if rows.is_empty() { return Ok(0); }

    source.row.checked_add(i64::try_from(rows.len() - 1).map_err(|_| InsertError::OrdinalOverflow)?)

        .ok_or(InsertError::OrdinalOverflow)?;

    let mut protocol: Vec<(usize, &models::Protocol)> = Vec::new();

    let mut run: Vec<(usize, &models::Run)> = Vec::new();

    let mut fact: Vec<(usize, &models::Fact)> = Vec::new();

    let mut witness: Vec<(usize, &models::Witness)> = Vec::new();

    let mut coverage: Vec<(usize, &models::Coverage)> = Vec::new();

    let mut diagnostic: Vec<(usize, &models::Diagnostic)> = Vec::new();

    let mut node: Vec<(usize, &models::Node)> = Vec::new();

    let mut edge: Vec<(usize, &models::Edge)> = Vec::new();

    let mut param: Vec<(usize, &models::Param)> = Vec::new();

    let mut arg: Vec<(usize, &models::Arg)> = Vec::new();

    let mut df_field: Vec<(usize, &models::DfField)> = Vec::new();

    let mut df_lit: Vec<(usize, &models::DfLit)> = Vec::new();

    let mut df_loop: Vec<(usize, &models::DfLoop)> = Vec::new();

    let mut df_nest: Vec<(usize, &models::DfNest)> = Vec::new();

    let mut df_allocates: Vec<(usize, &models::DfAllocates)> = Vec::new();

    let mut sig: Vec<(usize, &models::Sig)> = Vec::new();

    let mut site: Vec<(usize, &models::Site)> = Vec::new();

    let mut r#const: Vec<(usize, &models::Const)> = Vec::new();

    let mut doc: Vec<(usize, &models::Doc)> = Vec::new();

    let mut doc_tag: Vec<(usize, &models::DocTag)> = Vec::new();

    let mut doc_node: Vec<(usize, &models::DocNode)> = Vec::new();

    let mut data_doc: Vec<(usize, &models::DataDoc)> = Vec::new();

    let mut data_value: Vec<(usize, &models::DataValue)> = Vec::new();

    let mut specifier: Vec<(usize, &models::Specifier)> = Vec::new();

    let mut method_owner: Vec<(usize, &models::MethodOwner)> = Vec::new();

    let mut cfg_scope: Vec<(usize, &models::CfgScope)> = Vec::new();

    let mut test_only_call: Vec<(usize, &models::TestOnlyCall)> = Vec::new();

    let mut macro_site: Vec<(usize, &models::MacroSite)> = Vec::new();

    let mut reference: Vec<(usize, &models::Reference)> = Vec::new();

    let mut unresolved: Vec<(usize, &models::Unresolved)> = Vec::new();

    let mut projectedge: Vec<(usize, &models::Projectedge)> = Vec::new();

    let mut flow_edge: Vec<(usize, &models::FlowEdge)> = Vec::new();

    let mut resolved_edge: Vec<(usize, &models::ResolvedEdge)> = Vec::new();

    let mut resolved_type_edge: Vec<(usize, &models::ResolvedTypeEdge)> = Vec::new();

    let mut resolved_import: Vec<(usize, &models::ResolvedImport)> = Vec::new();

    let mut file_edge: Vec<(usize, &models::FileEdge)> = Vec::new();

    let mut file_unresolved: Vec<(usize, &models::FileUnresolved)> = Vec::new();

    let mut package_edge: Vec<(usize, &models::PackageEdge)> = Vec::new();

    let mut file: Vec<(usize, &models::File)> = Vec::new();

    let mut size_skip: Vec<(usize, &models::SizeSkip)> = Vec::new();

    let mut capture: Vec<(usize, &models::Capture)> = Vec::new();

    let mut scip_def: Vec<(usize, &models::ScipDef)> = Vec::new();

    let mut scip_name: Vec<(usize, &models::ScipName)> = Vec::new();

    let mut scip_ref: Vec<(usize, &models::ScipRef)> = Vec::new();

    let mut scip_edge: Vec<(usize, &models::ScipEdge)> = Vec::new();

    let mut scip_fn_edge: Vec<(usize, &models::ScipFnEdge)> = Vec::new();

    let mut scip_callee_type: Vec<(usize, &models::ScipCalleeType)> = Vec::new();

    let mut scip_local: Vec<(usize, &models::ScipLocal)> = Vec::new();

    let mut scip_impl: Vec<(usize, &models::ScipImpl)> = Vec::new();

    let mut scip_index: Vec<(usize, &models::ScipIndex)> = Vec::new();

    let mut scip_skip: Vec<(usize, &models::ScipSkip)> = Vec::new();

    let mut scip_occurrence: Vec<(usize, &models::ScipOccurrence)> = Vec::new();

    let mut scip_occurrence_doc: Vec<(usize, &models::ScipOccurrenceDoc)> = Vec::new();

    let mut scip_diagnostic: Vec<(usize, &models::ScipDiagnostic)> = Vec::new();

    let mut scip_symbol: Vec<(usize, &models::ScipSymbol)> = Vec::new();

    let mut scip_documentation: Vec<(usize, &models::ScipDocumentation)> = Vec::new();

    let mut scip_signature: Vec<(usize, &models::ScipSignature)> = Vec::new();

    let mut scip_signature_occurrence: Vec<(usize, &models::ScipSignatureOccurrence)> = Vec::new();

    let mut scip_metadata: Vec<(usize, &models::ScipMetadata)> = Vec::new();

    let mut scip_document: Vec<(usize, &models::ScipDocument)> = Vec::new();

    let mut scip_relationship: Vec<(usize, &models::ScipRelationship)> = Vec::new();

    for (index, row) in rows.iter().enumerate() {

        match row {

            Fact::Protocol(value) => protocol.push((index, value)),

            Fact::Run(value) => run.push((index, value)),

            Fact::Fact(value) => fact.push((index, value)),

            Fact::Witness(value) => witness.push((index, value)),

            Fact::Coverage(value) => coverage.push((index, value)),

            Fact::Diagnostic(value) => diagnostic.push((index, value)),

            Fact::Node(value) => node.push((index, value)),

            Fact::Edge(value) => edge.push((index, value)),

            Fact::Param(value) => param.push((index, value)),

            Fact::Arg(value) => arg.push((index, value)),

            Fact::DfField(value) => df_field.push((index, value)),

            Fact::DfLit(value) => df_lit.push((index, value)),

            Fact::DfLoop(value) => df_loop.push((index, value)),

            Fact::DfNest(value) => df_nest.push((index, value)),

            Fact::DfAllocates(value) => df_allocates.push((index, value)),

            Fact::Sig(value) => sig.push((index, value)),

            Fact::Site(value) => site.push((index, value)),

            Fact::Const(value) => r#const.push((index, value)),

            Fact::Doc(value) => doc.push((index, value)),

            Fact::DocTag(value) => doc_tag.push((index, value)),

            Fact::DocNode(value) => doc_node.push((index, value)),

            Fact::DataDoc(value) => data_doc.push((index, value)),

            Fact::DataValue(value) => data_value.push((index, value)),

            Fact::Specifier(value) => specifier.push((index, value)),

            Fact::MethodOwner(value) => method_owner.push((index, value)),

            Fact::CfgScope(value) => cfg_scope.push((index, value)),

            Fact::TestOnlyCall(value) => test_only_call.push((index, value)),

            Fact::MacroSite(value) => macro_site.push((index, value)),

            Fact::Reference(value) => reference.push((index, value)),

            Fact::Unresolved(value) => unresolved.push((index, value)),

            Fact::Projectedge(value) => projectedge.push((index, value)),

            Fact::FlowEdge(value) => flow_edge.push((index, value)),

            Fact::ResolvedEdge(value) => resolved_edge.push((index, value)),

            Fact::ResolvedTypeEdge(value) => resolved_type_edge.push((index, value)),

            Fact::ResolvedImport(value) => resolved_import.push((index, value)),

            Fact::FileEdge(value) => file_edge.push((index, value)),

            Fact::FileUnresolved(value) => file_unresolved.push((index, value)),

            Fact::PackageEdge(value) => package_edge.push((index, value)),

            Fact::File(value) => file.push((index, value)),

            Fact::SizeSkip(value) => size_skip.push((index, value)),

            Fact::Capture(value) => capture.push((index, value)),

            Fact::ScipDef(value) => scip_def.push((index, value)),

            Fact::ScipName(value) => scip_name.push((index, value)),

            Fact::ScipRef(value) => scip_ref.push((index, value)),

            Fact::ScipEdge(value) => scip_edge.push((index, value)),

            Fact::ScipFnEdge(value) => scip_fn_edge.push((index, value)),

            Fact::ScipCalleeType(value) => scip_callee_type.push((index, value)),

            Fact::ScipLocal(value) => scip_local.push((index, value)),

            Fact::ScipImpl(value) => scip_impl.push((index, value)),

            Fact::ScipIndex(value) => scip_index.push((index, value)),

            Fact::ScipSkip(value) => scip_skip.push((index, value)),

            Fact::ScipOccurrence(value) => scip_occurrence.push((index, value)),

            Fact::ScipOccurrenceDoc(value) => scip_occurrence_doc.push((index, value)),

            Fact::ScipDiagnostic(value) => scip_diagnostic.push((index, value)),

            Fact::ScipSymbol(value) => scip_symbol.push((index, value)),

            Fact::ScipDocumentation(value) => scip_documentation.push((index, value)),

            Fact::ScipSignature(value) => scip_signature.push((index, value)),

            Fact::ScipSignatureOccurrence(value) => scip_signature_occurrence.push((index, value)),

            Fact::ScipMetadata(value) => scip_metadata.push((index, value)),

            Fact::ScipDocument(value) => scip_document.push((index, value)),

            Fact::ScipRelationship(value) => scip_relationship.push((index, value)),

        }

    }

    let protocol_capacity = if protocol.is_empty() { 1 } else { statement_capacity(conn, 5, "INSERT INTO \"protocol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\") VALUES ", "(?, ?, ?, ?, ?)")? };

    let run_capacity = if run.is_empty() { 1 } else { statement_capacity(conn, 9, "INSERT INTO \"run\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"mode\", \"tool\", \"version\", \"scope\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let fact_capacity = if fact.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"fact\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"relation\", \"args\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let witness_capacity = if witness.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"witness\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"run\", \"method\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let coverage_capacity = if coverage.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"coverage\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"coverage\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let diagnostic_capacity = if diagnostic.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"detail\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let node_capacity = if node.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let edge_capacity = if edge.is_empty() { 1 } else { statement_capacity(conn, 13, "INSERT INTO \"edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"kind\", \"from__start\", \"from__end\", \"from_kind\", \"to__start\", \"to__end\", \"to_kind\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let param_capacity = if param.is_empty() { 1 } else { statement_capacity(conn, 9, "INSERT INTO \"param\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"pos\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let arg_capacity = if arg.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"arg\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"pos\", \"arg__start\", \"arg__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let df_field_capacity = if df_field.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"df_field\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"name\", \"value__start\", \"value__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let df_lit_capacity = if df_lit.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"df_lit\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"node__start\", \"node__end\", \"kind\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let df_loop_capacity = if df_loop.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"df_loop\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"var\", \"collection\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let df_nest_capacity = if df_nest.is_empty() { 1 } else { statement_capacity(conn, 12, "INSERT INTO \"df_nest\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"loop__start\", \"loop__end\", \"depth\", \"collection\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let df_allocates_capacity = if df_allocates.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"df_allocates\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let sig_capacity = if sig.is_empty() { 1 } else { statement_capacity(conn, 13, "INSERT INTO \"sig\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"owner_start\", \"owner_end\", \"slot\", \"pos\", \"ty\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let site_capacity = if site.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"callee\", \"callee_path\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let r#const_capacity = if r#const.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"const\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"field\", \"text\", \"kind\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let doc_capacity = if doc.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"parent\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let doc_tag_capacity = if doc_tag.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"doc_tag\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"tag\", \"arg\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let doc_node_capacity = if doc_node.is_empty() { 1 } else { statement_capacity(conn, 15, "INSERT INTO \"doc_node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\", \"parent\", \"target\", \"title\", \"body__start\", \"body__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let data_doc_capacity = if data_doc.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"data_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"span__start\", \"span__end\", \"format\", \"doc\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let data_value_capacity = if data_value.is_empty() { 1 } else { statement_capacity(conn, 12, "INSERT INTO \"data_value\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"path\", \"kind\", \"text\", \"span__start\", \"span__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let specifier_capacity = if specifier.is_empty() { 1 } else { statement_capacity(conn, 12, "INSERT INTO \"specifier\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"name\", \"kind\", \"module\", \"imported\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let method_owner_capacity = if method_owner.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"method_owner\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"self_type\", \"trait\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let cfg_scope_capacity = if cfg_scope.is_empty() { 1 } else { statement_capacity(conn, 9, "INSERT INTO \"cfg_scope\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"cfg\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let test_only_call_capacity = if test_only_call.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"test_only_call\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"callee\", \"cfg\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let macro_site_capacity = if macro_site.is_empty() { 1 } else { statement_capacity(conn, 9, "INSERT INTO \"macro_site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"span__start\", \"span__end\", \"macro_name\", \"source\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let reference_capacity = if reference.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"reference\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"functor\", \"position\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let unresolved_capacity = if unresolved.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"path\", \"span__start\", \"span__end\", \"reason\", \"detail\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let projectedge_capacity = if projectedge.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"projectedge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let flow_edge_capacity = if flow_edge.is_empty() { 1 } else { statement_capacity(conn, 12, "INSERT INTO \"flow_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from_blob\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let resolved_edge_capacity = if resolved_edge.is_empty() { 1 } else { statement_capacity(conn, 13, "INSERT INTO \"resolved_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"caller_path\", \"caller_name\", \"callee_path\", \"callee_name\", \"caller_site_start\", \"caller_site_end\", \"kind\", \"resolution_origin\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let resolved_type_edge_capacity = if resolved_type_edge.is_empty() { 1 } else { statement_capacity(conn, 13, "INSERT INTO \"resolved_type_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"owner_path\", \"owner_name\", \"owner_start\", \"owner_end\", \"target_path\", \"target_name\", \"kind\", \"resolution_origin\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let resolved_import_capacity = if resolved_import.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"resolved_import\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"name\", \"local\", \"target_path\", \"target_name\", \"kind\", \"hops\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let file_edge_capacity = if file_edge.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"file_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"dst_path\", \"kind\", \"symbols\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let file_unresolved_capacity = if file_unresolved.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"file_unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"module\", \"reason\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let package_edge_capacity = if package_edge.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"package_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_manifest\", \"dst_manifest\", \"kind\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let file_capacity = if file.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"file\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"digest\", \"bytes\", \"lines\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let size_skip_capacity = if size_skip.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"size_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"bytes\", \"limit\", \"reason\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let capture_capacity = if capture.is_empty() { 1 } else { statement_capacity(conn, 11, "INSERT INTO \"capture\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"query\", \"capture\", \"text\", \"start\", \"end\", \"match_start\", \"match_end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_def_capacity = if scip_def.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"scip_def\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"file\", \"repo\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let scip_name_capacity = if scip_name.is_empty() { 1 } else { statement_capacity(conn, 6, "INSERT INTO \"scip_name\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"name\") VALUES ", "(?, ?, ?, ?, ?, ?)")? };

    let scip_ref_capacity = if scip_ref.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"scip_ref\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"file\", \"symbol\", \"def_file\", \"repo\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_edge_capacity = if scip_edge.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"scip_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src\", \"dst\", \"repo\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let scip_fn_edge_capacity = if scip_fn_edge.is_empty() { 1 } else { statement_capacity(conn, 6, "INSERT INTO \"scip_fn_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"caller\", \"callee\") VALUES ", "(?, ?, ?, ?, ?, ?)")? };

    let scip_callee_type_capacity = if scip_callee_type.is_empty() { 1 } else { statement_capacity(conn, 6, "INSERT INTO \"scip_callee_type\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"sym\", \"type\") VALUES ", "(?, ?, ?, ?, ?, ?)")? };

    let scip_local_capacity = if scip_local.is_empty() { 1 } else { statement_capacity(conn, 6, "INSERT INTO \"scip_local\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fn\", \"name\") VALUES ", "(?, ?, ?, ?, ?, ?)")? };

    let scip_impl_capacity = if scip_impl.is_empty() { 1 } else { statement_capacity(conn, 6, "INSERT INTO \"scip_impl\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"impl\", \"iface\") VALUES ", "(?, ?, ?, ?, ?, ?)")? };

    let scip_index_capacity = if scip_index.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"scip_index\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"reused\", \"tool_name\", \"tool_version\", \"documents\", \"index_mtime_unix_ms\", \"staleness\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_skip_capacity = if scip_skip.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"scip_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"lang\", \"bin\", \"reason\", \"detail\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_occurrence_capacity = if scip_occurrence.is_empty() { 1 } else { statement_capacity(conn, 20, "INSERT INTO \"scip_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"start\", \"end\", \"roles\", \"definition\", \"import\", \"write_access\", \"read_access\", \"generated\", \"test\", \"forward_definition\", \"syntax_kind\", \"enclosing_start\", \"enclosing_end\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_occurrence_doc_capacity = if scip_occurrence_doc.is_empty() { 1 } else { statement_capacity(conn, 9, "INSERT INTO \"scip_occurrence_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"pos\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_diagnostic_capacity = if scip_diagnostic.is_empty() { 1 } else { statement_capacity(conn, 12, "INSERT INTO \"scip_diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"severity\", \"code\", \"message\", \"source\", \"tags\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_symbol_capacity = if scip_symbol.is_empty() { 1 } else { statement_capacity(conn, 9, "INSERT INTO \"scip_symbol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"display_name\", \"kind\", \"enclosing_symbol\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_documentation_capacity = if scip_documentation.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"scip_documentation\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"pos\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let scip_signature_capacity = if scip_signature.is_empty() { 1 } else { statement_capacity(conn, 7, "INSERT INTO \"scip_signature\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"language\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)")? };

    let scip_signature_occurrence_capacity = if scip_signature_occurrence.is_empty() { 1 } else { statement_capacity(conn, 9, "INSERT INTO \"scip_signature_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"ref_symbol\", \"start\", \"end\", \"roles\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_metadata_capacity = if scip_metadata.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"scip_metadata\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\", \"tool_name\", \"tool_version\", \"tool_arguments\", \"project_root\", \"text_document_encoding\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_document_capacity = if scip_document.is_empty() { 1 } else { statement_capacity(conn, 8, "INSERT INTO \"scip_document\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"language\", \"position_encoding\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)")? };

    let scip_relationship_capacity = if scip_relationship.is_empty() { 1 } else { statement_capacity(conn, 10, "INSERT INTO \"scip_relationship\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"related_symbol\", \"is_reference\", \"is_implementation\", \"is_type_definition\", \"is_definition\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")? };

    let mut inserted = 0;

    for chunk in protocol.chunks(protocol_capacity) {
        let sql = multi_row_sql("INSERT INTO \"protocol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\") VALUES ", "(?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in run.chunks(run_capacity) {
        let sql = multi_row_sql("INSERT INTO \"run\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"mode\", \"tool\", \"version\", \"scope\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in fact.chunks(fact_capacity) {
        let sql = multi_row_sql("INSERT INTO \"fact\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"relation\", \"args\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in witness.chunks(witness_capacity) {
        let sql = multi_row_sql("INSERT INTO \"witness\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"run\", \"method\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in coverage.chunks(coverage_capacity) {
        let sql = multi_row_sql("INSERT INTO \"coverage\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"coverage\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in diagnostic.chunks(diagnostic_capacity) {
        let sql = multi_row_sql("INSERT INTO \"diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"detail\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in node.chunks(node_capacity) {
        let sql = multi_row_sql("INSERT INTO \"node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in edge.chunks(edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"kind\", \"from__start\", \"from__end\", \"from_kind\", \"to__start\", \"to__end\", \"to_kind\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in param.chunks(param_capacity) {
        let sql = multi_row_sql("INSERT INTO \"param\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"pos\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in arg.chunks(arg_capacity) {
        let sql = multi_row_sql("INSERT INTO \"arg\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"pos\", \"arg__start\", \"arg__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in df_field.chunks(df_field_capacity) {
        let sql = multi_row_sql("INSERT INTO \"df_field\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"name\", \"value__start\", \"value__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in df_lit.chunks(df_lit_capacity) {
        let sql = multi_row_sql("INSERT INTO \"df_lit\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"node__start\", \"node__end\", \"kind\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in df_loop.chunks(df_loop_capacity) {
        let sql = multi_row_sql("INSERT INTO \"df_loop\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"var\", \"collection\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in df_nest.chunks(df_nest_capacity) {
        let sql = multi_row_sql("INSERT INTO \"df_nest\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"loop__start\", \"loop__end\", \"depth\", \"collection\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in df_allocates.chunks(df_allocates_capacity) {
        let sql = multi_row_sql("INSERT INTO \"df_allocates\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in sig.chunks(sig_capacity) {
        let sql = multi_row_sql("INSERT INTO \"sig\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"owner_start\", \"owner_end\", \"slot\", \"pos\", \"ty\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in site.chunks(site_capacity) {
        let sql = multi_row_sql("INSERT INTO \"site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"callee\", \"callee_path\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in r#const.chunks(r#const_capacity) {
        let sql = multi_row_sql("INSERT INTO \"const\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"field\", \"text\", \"kind\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in doc.chunks(doc_capacity) {
        let sql = multi_row_sql("INSERT INTO \"doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"parent\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in doc_tag.chunks(doc_tag_capacity) {
        let sql = multi_row_sql("INSERT INTO \"doc_tag\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"tag\", \"arg\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in doc_node.chunks(doc_node_capacity) {
        let sql = multi_row_sql("INSERT INTO \"doc_node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\", \"parent\", \"target\", \"title\", \"body__start\", \"body__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in data_doc.chunks(data_doc_capacity) {
        let sql = multi_row_sql("INSERT INTO \"data_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"span__start\", \"span__end\", \"format\", \"doc\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in data_value.chunks(data_value_capacity) {
        let sql = multi_row_sql("INSERT INTO \"data_value\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"path\", \"kind\", \"text\", \"span__start\", \"span__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in specifier.chunks(specifier_capacity) {
        let sql = multi_row_sql("INSERT INTO \"specifier\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"name\", \"kind\", \"module\", \"imported\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in method_owner.chunks(method_owner_capacity) {
        let sql = multi_row_sql("INSERT INTO \"method_owner\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"self_type\", \"trait\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in cfg_scope.chunks(cfg_scope_capacity) {
        let sql = multi_row_sql("INSERT INTO \"cfg_scope\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"cfg\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in test_only_call.chunks(test_only_call_capacity) {
        let sql = multi_row_sql("INSERT INTO \"test_only_call\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"callee\", \"cfg\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in macro_site.chunks(macro_site_capacity) {
        let sql = multi_row_sql("INSERT INTO \"macro_site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"span__start\", \"span__end\", \"macro_name\", \"source\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in reference.chunks(reference_capacity) {
        let sql = multi_row_sql("INSERT INTO \"reference\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"functor\", \"position\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in unresolved.chunks(unresolved_capacity) {
        let sql = multi_row_sql("INSERT INTO \"unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"path\", \"span__start\", \"span__end\", \"reason\", \"detail\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in projectedge.chunks(projectedge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"projectedge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in flow_edge.chunks(flow_edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"flow_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from_blob\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in resolved_edge.chunks(resolved_edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"resolved_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"caller_path\", \"caller_name\", \"callee_path\", \"callee_name\", \"caller_site_start\", \"caller_site_end\", \"kind\", \"resolution_origin\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in resolved_type_edge.chunks(resolved_type_edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"resolved_type_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"owner_path\", \"owner_name\", \"owner_start\", \"owner_end\", \"target_path\", \"target_name\", \"kind\", \"resolution_origin\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in resolved_import.chunks(resolved_import_capacity) {
        let sql = multi_row_sql("INSERT INTO \"resolved_import\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"name\", \"local\", \"target_path\", \"target_name\", \"kind\", \"hops\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in file_edge.chunks(file_edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"file_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"dst_path\", \"kind\", \"symbols\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in file_unresolved.chunks(file_unresolved_capacity) {
        let sql = multi_row_sql("INSERT INTO \"file_unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"module\", \"reason\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in package_edge.chunks(package_edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"package_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_manifest\", \"dst_manifest\", \"kind\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in file.chunks(file_capacity) {
        let sql = multi_row_sql("INSERT INTO \"file\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"digest\", \"bytes\", \"lines\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in size_skip.chunks(size_skip_capacity) {
        let sql = multi_row_sql("INSERT INTO \"size_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"bytes\", \"limit\", \"reason\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in capture.chunks(capture_capacity) {
        let sql = multi_row_sql("INSERT INTO \"capture\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"query\", \"capture\", \"text\", \"start\", \"end\", \"match_start\", \"match_end\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_def.chunks(scip_def_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_def\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"file\", \"repo\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_name.chunks(scip_name_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_name\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"name\") VALUES ", "(?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_ref.chunks(scip_ref_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_ref\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"file\", \"symbol\", \"def_file\", \"repo\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_edge.chunks(scip_edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src\", \"dst\", \"repo\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_fn_edge.chunks(scip_fn_edge_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_fn_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"caller\", \"callee\") VALUES ", "(?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_callee_type.chunks(scip_callee_type_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_callee_type\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"sym\", \"type\") VALUES ", "(?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_local.chunks(scip_local_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_local\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fn\", \"name\") VALUES ", "(?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_impl.chunks(scip_impl_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_impl\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"impl\", \"iface\") VALUES ", "(?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_index.chunks(scip_index_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_index\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"reused\", \"tool_name\", \"tool_version\", \"documents\", \"index_mtime_unix_ms\", \"staleness\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_skip.chunks(scip_skip_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"lang\", \"bin\", \"reason\", \"detail\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_occurrence.chunks(scip_occurrence_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"start\", \"end\", \"roles\", \"definition\", \"import\", \"write_access\", \"read_access\", \"generated\", \"test\", \"forward_definition\", \"syntax_kind\", \"enclosing_start\", \"enclosing_end\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_occurrence_doc.chunks(scip_occurrence_doc_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_occurrence_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"pos\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_diagnostic.chunks(scip_diagnostic_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"severity\", \"code\", \"message\", \"source\", \"tags\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_symbol.chunks(scip_symbol_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_symbol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"display_name\", \"kind\", \"enclosing_symbol\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_documentation.chunks(scip_documentation_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_documentation\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"pos\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_signature.chunks(scip_signature_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_signature\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"language\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_signature_occurrence.chunks(scip_signature_occurrence_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_signature_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"ref_symbol\", \"start\", \"end\", \"roles\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_metadata.chunks(scip_metadata_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_metadata\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\", \"tool_name\", \"tool_version\", \"tool_arguments\", \"project_root\", \"text_document_encoding\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_document.chunks(scip_document_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_document\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"language\", \"position_encoding\", \"text\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    for chunk in scip_relationship.chunks(scip_relationship_capacity) {
        let sql = multi_row_sql("INSERT INTO \"scip_relationship\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"related_symbol\", \"is_reference\", \"is_implementation\", \"is_type_definition\", \"is_definition\") VALUES ", "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", chunk.len());
        let mut statement = conn.prepare_cached(&sql)?;
        let mut parameter = 1;
        for (index, row) in chunk {
            let row_source = Source { row: source.row + *index as i64, input_path: source.input_path, content_id: source.content_id };
            parameter = row.bind(&mut statement, parameter, &row_source)?;
        }
        inserted += statement.raw_execute()?;
    }

    Ok(inserted)

}

impl models::Protocol {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "protocol")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.version)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"protocol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\") VALUES (?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Run {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        let scope_json = serde_json::to_string(&self.scope)?;
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "run")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.run)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.mode.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.tool.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.version.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, &scope_json)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"run\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"mode\", \"tool\", \"version\", \"scope\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Fact {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        let args_json = serde_json::to_string(&self.args)?;
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "fact")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.relation.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, &args_json)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"fact\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"relation\", \"args\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Witness {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "witness")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.run)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.method.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"witness\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"run\", \"method\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Coverage {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "coverage")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.run)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.relation.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.coverage.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"coverage\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"coverage\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Diagnostic {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "diagnostic")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.run)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.relation.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.detail.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"detail\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Node {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "node")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.name.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Edge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from_kind.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to_kind.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"kind\", \"from__start\", \"from__end\", \"from_kind\", \"to__start\", \"to__end\", \"to_kind\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Param {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "param")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.pos)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"param\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"pos\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Arg {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "arg")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.call.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.call.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.pos)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.arg.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.arg.end)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"arg\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"pos\", \"arg__start\", \"arg__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DfField {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "df_field")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.value.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.value.end)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"df_field\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"name\", \"value__start\", \"value__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DfLit {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "df_lit")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.node.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.node.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"df_lit\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"node__start\", \"node__end\", \"kind\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DfLoop {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "df_loop")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.var.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.collection.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"df_loop\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"var\", \"collection\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DfNest {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "df_nest")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.call.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.call.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.r#loop.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.r#loop.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.depth)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.collection.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"df_nest\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"loop__start\", \"loop__end\", \"depth\", \"collection\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DfAllocates {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "df_allocates")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.end)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"df_allocates\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Sig {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "sig")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner_start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner_end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.slot.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.pos)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.ty.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"sig\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"owner_start\", \"owner_end\", \"slot\", \"pos\", \"ty\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Site {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "site")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.callee.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.callee_path.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"callee\", \"callee_path\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Const {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "const")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.field.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"const\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"field\", \"text\", \"kind\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Doc {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "doc")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.parent.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"parent\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DocTag {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "doc_tag")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.tag.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.arg.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"doc_tag\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"tag\", \"arg\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DocNode {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "doc_node")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.parent.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.target.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.title.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.body.as_ref().map(|value| value.start))?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.body.as_ref().map(|value| value.end))?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"doc_node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\", \"parent\", \"target\", \"title\", \"body__start\", \"body__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DataDoc {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        let doc_json = serde_json::to_string(&self.doc)?;
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "data_doc")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.ordinal)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.format.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, &doc_json)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"data_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"span__start\", \"span__end\", \"format\", \"doc\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::DataValue {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "data_value")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.ordinal)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"data_value\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"path\", \"kind\", \"text\", \"span__start\", \"span__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Specifier {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "specifier")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.module.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.imported.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"specifier\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"name\", \"kind\", \"module\", \"imported\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::MethodOwner {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "method_owner")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.self_type.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.r#trait.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"method_owner\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"self_type\", \"trait\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::CfgScope {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "cfg_scope")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.cfg.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"cfg_scope\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"cfg\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::TestOnlyCall {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "test_only_call")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.callee.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.cfg.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"test_only_call\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"callee\", \"cfg\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::MacroSite {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "macro_site")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.macro_name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.source.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"macro_site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"span__start\", \"span__end\", \"macro_name\", \"source\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Reference {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "reference")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.functor.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.position.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"reference\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"functor\", \"position\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Unresolved {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "unresolved")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.span.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.reason.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.detail.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"path\", \"span__start\", \"span__end\", \"reason\", \"detail\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Projectedge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "projectedge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to_blob.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to.end)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"projectedge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::FlowEdge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "flow_edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.family.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from_blob.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.from.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to_blob.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.to.end)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"flow_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from_blob\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ResolvedEdge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "resolved_edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.caller_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.caller_name.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.callee_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.callee_name.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.caller_site_start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.caller_site_end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.resolution_origin.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"resolved_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"caller_path\", \"caller_name\", \"callee_path\", \"callee_name\", \"caller_site_start\", \"caller_site_end\", \"kind\", \"resolution_origin\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ResolvedTypeEdge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "resolved_type_edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.fact)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner_name.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner_start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.owner_end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.target_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.target_name.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.resolution_origin.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"resolved_type_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"owner_path\", \"owner_name\", \"owner_start\", \"owner_end\", \"target_path\", \"target_name\", \"kind\", \"resolution_origin\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ResolvedImport {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "resolved_import")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.src_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.local.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.target_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.target_name.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.hops)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"resolved_import\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"name\", \"local\", \"target_path\", \"target_name\", \"kind\", \"hops\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::FileEdge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "file_edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.src_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.dst_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbols)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"file_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"dst_path\", \"kind\", \"symbols\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::FileUnresolved {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "file_unresolved")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.src_path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.module.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.reason.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"file_unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"module\", \"reason\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::PackageEdge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "package_edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.src_manifest.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.dst_manifest.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"package_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_manifest\", \"dst_manifest\", \"kind\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::File {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "file")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.digest.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.bytes)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.lines)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"file\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"digest\", \"bytes\", \"lines\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::SizeSkip {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "size_skip")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, u64_value(self.bytes))?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, u64_value(self.limit))?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.reason.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"size_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"bytes\", \"limit\", \"reason\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::Capture {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "capture")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.query.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.capture.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.match_start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.match_end)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"capture\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"query\", \"capture\", \"text\", \"start\", \"end\", \"match_start\", \"match_end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipDef {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_def")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.file.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.repo.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_def\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"file\", \"repo\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipName {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_name")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.name.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_name\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"name\") VALUES (?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipRef {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_ref")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.file.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.def_file.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.repo.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_ref\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"file\", \"symbol\", \"def_file\", \"repo\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipEdge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.src.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.dst.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.repo.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src\", \"dst\", \"repo\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipFnEdge {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_fn_edge")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.caller.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.callee.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_fn_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"caller\", \"callee\") VALUES (?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipCalleeType {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_callee_type")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.sym.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.r#type.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_callee_type\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"sym\", \"type\") VALUES (?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipLocal {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_local")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.r#fn.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.name.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_local\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fn\", \"name\") VALUES (?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipImpl {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_impl")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.r#impl.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.iface.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_impl\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"impl\", \"iface\") VALUES (?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipIndex {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_index")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.reused)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.tool_name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.tool_version.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.documents)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.index_mtime_unix_ms.map(u64_value))?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.staleness.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_index\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"reused\", \"tool_name\", \"tool_version\", \"documents\", \"index_mtime_unix_ms\", \"staleness\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipSkip {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_skip")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.lang.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.bin.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.reason.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.detail.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"lang\", \"bin\", \"reason\", \"detail\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipOccurrence {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_occurrence")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.roles)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.definition)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.import)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.write_access)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.read_access)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.generated)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.test)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.forward_definition)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.syntax_kind)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.enclosing_start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.enclosing_end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"start\", \"end\", \"roles\", \"definition\", \"import\", \"write_access\", \"read_access\", \"generated\", \"test\", \"forward_definition\", \"syntax_kind\", \"enclosing_start\", \"enclosing_end\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipOccurrenceDoc {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_occurrence_doc")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.pos)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_occurrence_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"pos\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipDiagnostic {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        let tags_json = serde_json::to_string(&self.tags)?;
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_diagnostic")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.severity)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.code.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.message.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.source.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, &tags_json)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"severity\", \"code\", \"message\", \"source\", \"tags\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipSymbol {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_symbol")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_deref())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.display_name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.kind)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.enclosing_symbol.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_symbol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"display_name\", \"kind\", \"enclosing_symbol\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipDocumentation {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_documentation")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.pos)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_documentation\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"pos\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipSignature {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_signature")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.language.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_str())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_signature\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"language\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipSignatureOccurrence {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_signature_occurrence")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.ref_symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.start)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.end)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.roles)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_signature_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"ref_symbol\", \"start\", \"end\", \"roles\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipMetadata {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        let tool_arguments_json = serde_json::to_string(&self.tool_arguments)?;
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_metadata")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.version)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.tool_name.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.tool_version.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, &tool_arguments_json)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.project_root.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text_document_encoding)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_metadata\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\", \"tool_name\", \"tool_version\", \"tool_arguments\", \"project_root\", \"text_document_encoding\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipDocument {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_document")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.path.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.language.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.position_encoding)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.text.as_deref())?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_document\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"language\", \"position_encoding\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}

impl models::ScipRelationship {
    fn bind(&self, statement: &mut rusqlite::Statement<'_>, mut parameter: usize, source: &Source<'_>) -> Result<usize, InsertError> {
        statement.raw_bind_parameter(parameter, source.row)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.input_path)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, source.content_id)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, "scip_relationship")?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.related_symbol.as_str())?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.is_reference)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.is_implementation)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.is_type_definition)?;
        parameter += 1;
        statement.raw_bind_parameter(parameter, self.is_definition)?;
        parameter += 1;
        Ok(parameter)
    }
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let mut statement = conn.prepare_cached("INSERT INTO \"scip_relationship\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"related_symbol\", \"is_reference\", \"is_implementation\", \"is_type_definition\", \"is_definition\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
        self.bind(&mut statement, 1, source)?;
        Ok(statement.raw_execute()?)
    }
}
