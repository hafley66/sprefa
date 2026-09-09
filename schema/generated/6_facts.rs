// Generated SQLite row types from schema/1_facts.tsp.
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protocol {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub run: u32,
  pub mode: String,
  pub tool: String,
  pub version: String,
  pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: u32,
  pub relation: String,
  pub args: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Witness {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: u32,
  pub run: u32,
  pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub run: u32,
  pub relation: String,
  pub coverage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub run: u32,
  pub relation: String,
  pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub kind: String,
  pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub kind: String,
  pub from__start: u32,
  pub from__end: u32,
  pub from_kind: Option<String>,
  pub to__start: u32,
  pub to__end: u32,
  pub to_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub pos: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arg {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub call__start: u32,
  pub call__end: u32,
  pub pos: i64,
  pub arg__start: u32,
  pub arg__end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfField {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub owner__start: u32,
  pub owner__end: u32,
  pub name: String,
  pub value__start: u32,
  pub value__end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfLit {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub node__start: u32,
  pub node__end: u32,
  pub kind: String,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfLoop {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub var: Option<String>,
  pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfNest {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub call__start: u32,
  pub call__end: u32,
  pub loop__start: u32,
  pub loop__end: u32,
  pub depth: u32,
  pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfAllocates {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub owner__start: u32,
  pub owner__end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sig {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub owner__start: u32,
  pub owner__end: u32,
  pub owner_start: u32,
  pub owner_end: u32,
  pub slot: String,
  pub pos: u32,
  pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub callee: String,
  pub callee_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Const {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub owner__start: u32,
  pub owner__end: u32,
  pub field: Option<String>,
  pub text: String,
  pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub owner__start: u32,
  pub owner__end: u32,
  pub parent: Option<String>,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocTag {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub owner__start: u32,
  pub owner__end: u32,
  pub tag: String,
  pub arg: Option<String>,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocNode {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub kind: String,
  pub name: String,
  pub parent: Option<String>,
  pub target: Option<String>,
  pub title: Option<String>,
  pub body__start: Option<u32>,
  pub body__end: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataDoc {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub ordinal: u32,
  pub span__start: u32,
  pub span__end: u32,
  pub format: String,
  pub doc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataValue {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub ordinal: u32,
  pub path: String,
  pub kind: String,
  pub text: Option<String>,
  pub span__start: u32,
  pub span__end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Specifier {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub name: String,
  pub kind: String,
  pub module: Option<String>,
  pub imported: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodOwner {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub owner__start: u32,
  pub owner__end: u32,
  pub self_type: Option<String>,
  pub r#trait: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgScope {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub cfg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestOnlyCall {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub callee: String,
  pub cfg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroSite {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub macro_name: String,
  pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub family: String,
  pub span__start: u32,
  pub span__end: u32,
  pub functor: String,
  pub position: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unresolved {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub family: String,
  pub path: Option<String>,
  pub span__start: u32,
  pub span__end: u32,
  pub reason: String,
  pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projectedge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub family: String,
  pub kind: String,
  pub from__start: u32,
  pub from__end: u32,
  pub to_blob: String,
  pub to__start: u32,
  pub to__end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEdge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub family: String,
  pub kind: String,
  pub from_blob: String,
  pub from__start: u32,
  pub from__end: u32,
  pub to_blob: String,
  pub to__start: u32,
  pub to__end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedEdge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub caller_path: String,
  pub caller_name: Option<String>,
  pub callee_path: String,
  pub callee_name: Option<String>,
  pub caller_site_start: u32,
  pub caller_site_end: u32,
  pub kind: String,
  pub resolution_origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedTypeEdge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub fact: Option<u32>,
  pub owner_path: String,
  pub owner_name: Option<String>,
  pub owner_start: u32,
  pub owner_end: u32,
  pub target_path: String,
  pub target_name: Option<String>,
  pub kind: String,
  pub resolution_origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedImport {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub src_path: String,
  pub name: String,
  pub local: String,
  pub target_path: String,
  pub target_name: Option<String>,
  pub kind: String,
  pub hops: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEdge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub src_path: String,
  pub dst_path: String,
  pub kind: String,
  pub symbols: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUnresolved {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub src_path: String,
  pub module: String,
  pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageEdge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub src_manifest: String,
  pub dst_manifest: String,
  pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub path: String,
  pub digest: String,
  pub bytes: u32,
  pub lines: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeSkip {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub path: String,
  pub bytes: u64,
  pub limit: u64,
  pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capture {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub query: String,
  pub capture: String,
  pub text: String,
  pub start: u32,
  pub end: u32,
  pub match_start: u32,
  pub match_end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipDef {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub symbol: String,
  pub file: String,
  pub repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipName {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub symbol: String,
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipRef {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub file: String,
  pub symbol: String,
  pub def_file: String,
  pub repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipEdge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub src: String,
  pub dst: String,
  pub repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipFnEdge {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub caller: String,
  pub callee: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipCalleeType {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub sym: String,
  pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipLocal {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub r#fn: String,
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipImpl {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub r#impl: String,
  pub iface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipIndex {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub reused: bool,
  pub tool_name: String,
  pub tool_version: String,
  pub documents: u32,
  pub index_mtime_unix_ms: Option<u64>,
  pub staleness: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipSkip {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub lang: String,
  pub bin: String,
  pub reason: String,
  pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipOccurrence {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
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
  pub enclosing_start: Option<u32>,
  pub enclosing_end: Option<u32>,
  pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipOccurrenceDoc {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub path: String,
  pub start: u32,
  pub end: u32,
  pub pos: u32,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipDiagnostic {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub path: String,
  pub start: u32,
  pub end: u32,
  pub severity: i32,
  pub code: String,
  pub message: String,
  pub source: String,
  pub tags: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipSymbol {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub path: Option<String>,
  pub symbol: String,
  pub display_name: String,
  pub kind: i32,
  pub enclosing_symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipDocumentation {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub symbol: String,
  pub pos: u32,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipSignature {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub symbol: String,
  pub language: String,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipSignatureOccurrence {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub symbol: String,
  pub ref_symbol: String,
  pub start: u32,
  pub end: u32,
  pub roles: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipMetadata {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub version: i32,
  pub tool_name: String,
  pub tool_version: String,
  pub tool_arguments: String,
  pub project_root: String,
  pub text_document_encoding: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipDocument {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub path: String,
  pub language: String,
  pub position_encoding: i32,
  pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScipRelationship {
  pub _row: i64,
  pub _input_path: Option<String>,
  pub _content_id: Option<String>,
  pub record: String,
  pub symbol: String,
  pub related_symbol: String,
  pub is_reference: bool,
  pub is_implementation: bool,
  pub is_type_definition: bool,
  pub is_definition: bool,
}
