// Generated typed SQLite rows and writers. Do not edit.

#[derive(Debug)]

pub enum InsertError { Sql(rusqlite::Error), Json(serde_json::Error), OrdinalOverflow }

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

pub fn insert_all(conn: &rusqlite::Connection, source: &Source<'_>, rows: &[Fact]) -> Result<usize, InsertError> {

    if rows.is_empty() { return Ok(0); }

    source.row.checked_add(i64::try_from(rows.len() - 1).map_err(|_| InsertError::OrdinalOverflow)?)

        .ok_or(InsertError::OrdinalOverflow)?;

    let mut inserted = 0;

    for (index, row) in rows.iter().enumerate() {

        let row_source = Source { row: source.row + index as i64, input_path: source.input_path, content_id: source.content_id };

        inserted += row.insert(conn, &row_source)?;

    }

    Ok(inserted)

}

impl models::Protocol {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"protocol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\") VALUES (?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "protocol", self.version])?)
    }
}

impl models::Run {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let scope_json = serde_json::to_string(&self.scope)?;
        Ok(conn.prepare_cached("INSERT INTO \"run\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"mode\", \"tool\", \"version\", \"scope\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "run", self.run, self.mode.as_str(), self.tool.as_str(), self.version.as_str(), &scope_json])?)
    }
}

impl models::Fact {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let args_json = serde_json::to_string(&self.args)?;
        Ok(conn.prepare_cached("INSERT INTO \"fact\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"relation\", \"args\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "fact", self.fact, self.relation.as_str(), &args_json])?)
    }
}

impl models::Witness {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"witness\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"run\", \"method\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "witness", self.fact, self.run, self.method.as_str()])?)
    }
}

impl models::Coverage {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"coverage\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"coverage\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "coverage", self.run, self.relation.as_str(), self.coverage.as_str()])?)
    }
}

impl models::Diagnostic {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"run\", \"relation\", \"detail\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "diagnostic", self.run, self.relation.as_str(), self.detail.as_str()])?)
    }
}

impl models::Node {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "node", self.fact, self.family.as_str(), self.span.start, self.span.end, self.kind.as_str(), self.name.as_deref()])?)
    }
}

impl models::Edge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"kind\", \"from__start\", \"from__end\", \"from_kind\", \"to__start\", \"to__end\", \"to_kind\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "edge", self.fact, self.family.as_str(), self.kind.as_str(), self.from.start, self.from.end, self.from_kind.as_deref(), self.to.start, self.to.end, self.to_kind.as_deref()])?)
    }
}

impl models::Param {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"param\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"pos\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "param", self.fact, self.family.as_str(), self.span.start, self.span.end, self.pos])?)
    }
}

impl models::Arg {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"arg\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"pos\", \"arg__start\", \"arg__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "arg", self.fact, self.family.as_str(), self.call.start, self.call.end, self.pos, self.arg.start, self.arg.end])?)
    }
}

impl models::DfField {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"df_field\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"name\", \"value__start\", \"value__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "df_field", self.fact, self.family.as_str(), self.owner.start, self.owner.end, self.name.as_str(), self.value.start, self.value.end])?)
    }
}

impl models::DfLit {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"df_lit\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"node__start\", \"node__end\", \"kind\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "df_lit", self.fact, self.family.as_str(), self.node.start, self.node.end, self.kind.as_str(), self.text.as_str()])?)
    }
}

impl models::DfLoop {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"df_loop\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"var\", \"collection\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "df_loop", self.fact, self.family.as_str(), self.span.start, self.span.end, self.var.as_deref(), self.collection.as_deref()])?)
    }
}

impl models::DfNest {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"df_nest\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"call__start\", \"call__end\", \"loop__start\", \"loop__end\", \"depth\", \"collection\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "df_nest", self.fact, self.family.as_str(), self.call.start, self.call.end, self.r#loop.start, self.r#loop.end, self.depth, self.collection.as_deref()])?)
    }
}

impl models::DfAllocates {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"df_allocates\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "df_allocates", self.fact, self.family.as_str(), self.owner.start, self.owner.end])?)
    }
}

impl models::Sig {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"sig\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"owner_start\", \"owner_end\", \"slot\", \"pos\", \"ty\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "sig", self.fact, self.family.as_str(), self.owner.start, self.owner.end, self.owner_start, self.owner_end, self.slot.as_str(), self.pos, self.ty.as_str()])?)
    }
}

impl models::Site {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"callee\", \"callee_path\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "site", self.fact, self.family.as_str(), self.span.start, self.span.end, self.callee.as_str(), self.callee_path.as_deref()])?)
    }
}

impl models::Const {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"const\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"field\", \"text\", \"kind\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "const", self.fact, self.family.as_str(), self.owner.start, self.owner.end, self.field.as_deref(), self.text.as_str(), self.kind.as_str()])?)
    }
}

impl models::Doc {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"parent\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "doc", self.fact, self.family.as_str(), self.owner.start, self.owner.end, self.parent.as_deref(), self.text.as_str()])?)
    }
}

impl models::DocTag {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"doc_tag\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"tag\", \"arg\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "doc_tag", self.fact, self.family.as_str(), self.owner.start, self.owner.end, self.tag.as_str(), self.arg.as_deref(), self.text.as_str()])?)
    }
}

impl models::DocNode {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"doc_node\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"kind\", \"name\", \"parent\", \"target\", \"title\", \"body__start\", \"body__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "doc_node", self.fact, self.family.as_str(), self.span.start, self.span.end, self.kind.as_str(), self.name.as_str(), self.parent.as_deref(), self.target.as_deref(), self.title.as_deref(), self.body.as_ref().map(|value| value.start), self.body.as_ref().map(|value| value.end)])?)
    }
}

impl models::DataDoc {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let doc_json = serde_json::to_string(&self.doc)?;
        Ok(conn.prepare_cached("INSERT INTO \"data_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"span__start\", \"span__end\", \"format\", \"doc\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "data_doc", self.fact, self.family.as_str(), self.ordinal, self.span.start, self.span.end, self.format.as_str(), &doc_json])?)
    }
}

impl models::DataValue {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"data_value\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"ordinal\", \"path\", \"kind\", \"text\", \"span__start\", \"span__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "data_value", self.fact, self.family.as_str(), self.ordinal, self.path.as_str(), self.kind.as_str(), self.text.as_deref(), self.span.start, self.span.end])?)
    }
}

impl models::Specifier {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"specifier\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"name\", \"kind\", \"module\", \"imported\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "specifier", self.fact, self.family.as_str(), self.span.start, self.span.end, self.name.as_str(), self.kind.as_str(), self.module.as_deref(), self.imported.as_deref()])?)
    }
}

impl models::MethodOwner {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"method_owner\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"owner__start\", \"owner__end\", \"self_type\", \"trait\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "method_owner", self.fact, self.family.as_str(), self.owner.start, self.owner.end, self.self_type.as_deref(), self.r#trait.as_deref()])?)
    }
}

impl models::CfgScope {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"cfg_scope\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"cfg\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "cfg_scope", self.fact, self.family.as_str(), self.span.start, self.span.end, self.cfg.as_str()])?)
    }
}

impl models::TestOnlyCall {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"test_only_call\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"callee\", \"cfg\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "test_only_call", self.fact, self.family.as_str(), self.callee.as_str(), self.cfg.as_str()])?)
    }
}

impl models::MacroSite {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"macro_site\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"span__start\", \"span__end\", \"macro_name\", \"source\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "macro_site", self.family.as_str(), self.span.start, self.span.end, self.macro_name.as_str(), self.source.as_str()])?)
    }
}

impl models::Reference {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"reference\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"family\", \"span__start\", \"span__end\", \"functor\", \"position\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "reference", self.fact, self.family.as_str(), self.span.start, self.span.end, self.functor.as_str(), self.position.as_str()])?)
    }
}

impl models::Unresolved {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"path\", \"span__start\", \"span__end\", \"reason\", \"detail\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "unresolved", self.family.as_str(), self.path.as_deref(), self.span.start, self.span.end, self.reason.as_str(), self.detail.as_str()])?)
    }
}

impl models::Projectedge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"projectedge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "projectedge", self.family.as_str(), self.kind.as_str(), self.from.start, self.from.end, self.to_blob.as_str(), self.to.start, self.to.end])?)
    }
}

impl models::FlowEdge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"flow_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"family\", \"kind\", \"from_blob\", \"from__start\", \"from__end\", \"to_blob\", \"to__start\", \"to__end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "flow_edge", self.family.as_str(), self.kind.as_str(), self.from_blob.as_str(), self.from.start, self.from.end, self.to_blob.as_str(), self.to.start, self.to.end])?)
    }
}

impl models::ResolvedEdge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"resolved_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"caller_path\", \"caller_name\", \"callee_path\", \"callee_name\", \"caller_site_start\", \"caller_site_end\", \"kind\", \"resolution_origin\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "resolved_edge", self.fact, self.caller_path.as_str(), self.caller_name.as_deref(), self.callee_path.as_str(), self.callee_name.as_deref(), self.caller_site_start, self.caller_site_end, self.kind.as_str(), self.resolution_origin.as_str()])?)
    }
}

impl models::ResolvedTypeEdge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"resolved_type_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fact\", \"owner_path\", \"owner_name\", \"owner_start\", \"owner_end\", \"target_path\", \"target_name\", \"kind\", \"resolution_origin\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "resolved_type_edge", self.fact, self.owner_path.as_str(), self.owner_name.as_deref(), self.owner_start, self.owner_end, self.target_path.as_str(), self.target_name.as_deref(), self.kind.as_str(), self.resolution_origin.as_str()])?)
    }
}

impl models::ResolvedImport {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"resolved_import\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"name\", \"local\", \"target_path\", \"target_name\", \"kind\", \"hops\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "resolved_import", self.src_path.as_str(), self.name.as_str(), self.local.as_str(), self.target_path.as_str(), self.target_name.as_deref(), self.kind.as_str(), self.hops])?)
    }
}

impl models::FileEdge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"file_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"dst_path\", \"kind\", \"symbols\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "file_edge", self.src_path.as_str(), self.dst_path.as_str(), self.kind.as_str(), self.symbols])?)
    }
}

impl models::FileUnresolved {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"file_unresolved\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_path\", \"module\", \"reason\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "file_unresolved", self.src_path.as_str(), self.module.as_str(), self.reason.as_str()])?)
    }
}

impl models::PackageEdge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"package_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src_manifest\", \"dst_manifest\", \"kind\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "package_edge", self.src_manifest.as_str(), self.dst_manifest.as_str(), self.kind.as_str()])?)
    }
}

impl models::File {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"file\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"digest\", \"bytes\", \"lines\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "file", self.path.as_str(), self.digest.as_str(), self.bytes, self.lines])?)
    }
}

impl models::SizeSkip {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"size_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"bytes\", \"limit\", \"reason\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "size_skip", self.path.as_str(), u64_value(self.bytes), u64_value(self.limit), self.reason.as_str()])?)
    }
}

impl models::Capture {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"capture\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"query\", \"capture\", \"text\", \"start\", \"end\", \"match_start\", \"match_end\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "capture", self.query.as_str(), self.capture.as_str(), self.text.as_str(), self.start, self.end, self.match_start, self.match_end])?)
    }
}

impl models::ScipDef {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_def\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"file\", \"repo\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_def", self.symbol.as_str(), self.file.as_str(), self.repo.as_str()])?)
    }
}

impl models::ScipName {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_name\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"name\") VALUES (?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_name", self.symbol.as_str(), self.name.as_str()])?)
    }
}

impl models::ScipRef {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_ref\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"file\", \"symbol\", \"def_file\", \"repo\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_ref", self.file.as_str(), self.symbol.as_str(), self.def_file.as_str(), self.repo.as_str()])?)
    }
}

impl models::ScipEdge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"src\", \"dst\", \"repo\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_edge", self.src.as_str(), self.dst.as_str(), self.repo.as_str()])?)
    }
}

impl models::ScipFnEdge {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_fn_edge\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"caller\", \"callee\") VALUES (?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_fn_edge", self.caller.as_str(), self.callee.as_str()])?)
    }
}

impl models::ScipCalleeType {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_callee_type\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"sym\", \"type\") VALUES (?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_callee_type", self.sym.as_str(), self.r#type.as_str()])?)
    }
}

impl models::ScipLocal {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_local\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"fn\", \"name\") VALUES (?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_local", self.r#fn.as_str(), self.name.as_str()])?)
    }
}

impl models::ScipImpl {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_impl\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"impl\", \"iface\") VALUES (?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_impl", self.r#impl.as_str(), self.iface.as_str()])?)
    }
}

impl models::ScipIndex {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_index\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"reused\", \"tool_name\", \"tool_version\", \"documents\", \"index_mtime_unix_ms\", \"staleness\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_index", self.reused, self.tool_name.as_str(), self.tool_version.as_str(), self.documents, self.index_mtime_unix_ms.map(u64_value), self.staleness.as_str()])?)
    }
}

impl models::ScipSkip {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_skip\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"lang\", \"bin\", \"reason\", \"detail\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_skip", self.lang.as_str(), self.bin.as_str(), self.reason.as_str(), self.detail.as_str()])?)
    }
}

impl models::ScipOccurrence {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"start\", \"end\", \"roles\", \"definition\", \"import\", \"write_access\", \"read_access\", \"generated\", \"test\", \"forward_definition\", \"syntax_kind\", \"enclosing_start\", \"enclosing_end\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_occurrence", self.path.as_str(), self.symbol.as_str(), self.start, self.end, self.roles, self.definition, self.import, self.write_access, self.read_access, self.generated, self.test, self.forward_definition, self.syntax_kind, self.enclosing_start, self.enclosing_end, self.text.as_deref()])?)
    }
}

impl models::ScipOccurrenceDoc {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_occurrence_doc\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"pos\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_occurrence_doc", self.path.as_str(), self.start, self.end, self.pos, self.text.as_str()])?)
    }
}

impl models::ScipDiagnostic {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let tags_json = serde_json::to_string(&self.tags)?;
        Ok(conn.prepare_cached("INSERT INTO \"scip_diagnostic\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"start\", \"end\", \"severity\", \"code\", \"message\", \"source\", \"tags\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_diagnostic", self.path.as_str(), self.start, self.end, self.severity, self.code.as_str(), self.message.as_str(), self.source.as_str(), &tags_json])?)
    }
}

impl models::ScipSymbol {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_symbol\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"symbol\", \"display_name\", \"kind\", \"enclosing_symbol\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_symbol", self.path.as_deref(), self.symbol.as_str(), self.display_name.as_str(), self.kind, self.enclosing_symbol.as_str()])?)
    }
}

impl models::ScipDocumentation {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_documentation\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"pos\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_documentation", self.symbol.as_str(), self.pos, self.text.as_str()])?)
    }
}

impl models::ScipSignature {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_signature\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"language\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_signature", self.symbol.as_str(), self.language.as_str(), self.text.as_str()])?)
    }
}

impl models::ScipSignatureOccurrence {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_signature_occurrence\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"ref_symbol\", \"start\", \"end\", \"roles\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_signature_occurrence", self.symbol.as_str(), self.ref_symbol.as_str(), self.start, self.end, self.roles])?)
    }
}

impl models::ScipMetadata {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        let tool_arguments_json = serde_json::to_string(&self.tool_arguments)?;
        Ok(conn.prepare_cached("INSERT INTO \"scip_metadata\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"version\", \"tool_name\", \"tool_version\", \"tool_arguments\", \"project_root\", \"text_document_encoding\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_metadata", self.version, self.tool_name.as_str(), self.tool_version.as_str(), &tool_arguments_json, self.project_root.as_str(), self.text_document_encoding])?)
    }
}

impl models::ScipDocument {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_document\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"path\", \"language\", \"position_encoding\", \"text\") VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_document", self.path.as_str(), self.language.as_str(), self.position_encoding, self.text.as_deref()])?)
    }
}

impl models::ScipRelationship {
    pub fn insert(&self, conn: &rusqlite::Connection, source: &Source<'_>) -> Result<usize, InsertError> {
        Ok(conn.prepare_cached("INSERT INTO \"scip_relationship\" (\"_row\", \"_input_path\", \"_content_id\", \"record\", \"symbol\", \"related_symbol\", \"is_reference\", \"is_implementation\", \"is_type_definition\", \"is_definition\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?.execute(rusqlite::params![source.row, source.input_path, source.content_id, "scip_relationship", self.symbol.as_str(), self.related_symbol.as_str(), self.is_reference, self.is_implementation, self.is_type_definition, self.is_definition])?)
    }
}
