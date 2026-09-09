import "./0_spans.tsp";
import "./0_wire_types.tsp";

// The existing FlatFact wire vocabulary, with export coordinates. These
// coordinates identify emitted rows, not graph nodes, repositories or revisions.
namespace ExtractSql;

scalar Json extends string;
model ExportRow {
  @Entity.pk _row: int64;
  _input_path: string | null;
  _content_id: string | null;
}
model LocatedFact { fact?: uint32; family: Extract.FamilyTag; }

model Protocol { ...ExportRow; record: "protocol"; version: uint32; }
model Run { ...ExportRow; record: "run"; run: uint32; mode: Extract.Mode; tool: string; version: string; scope: string[]; }
model Fact { ...ExportRow; record: "fact"; fact: uint32; relation: string; args: Extract.TsiArg[]; }
model Witness { ...ExportRow; record: "witness"; fact: uint32; run: uint32; method: Extract.Method; }
model Coverage { ...ExportRow; record: "coverage"; run: uint32; relation: string; coverage: Extract.CoverageFlag; }
model Diagnostic { ...ExportRow; record: "diagnostic"; run: uint32; relation: string; detail: string; }

model Node {
  ...ExportRow; record: "node"; ...LocatedFact;
  span: Extract.SpanOut; kind: string; name: string | null;
}
model Edge {
  ...ExportRow; record: "edge"; ...LocatedFact;
  kind: string; from: Extract.SpanOut; from_kind?: string;
  to: Extract.SpanOut; to_kind?: string;
}
model Param { ...ExportRow; record: "param"; ...LocatedFact; span: Extract.SpanOut; pos: uint32; }
model Arg { ...ExportRow; record: "arg"; ...LocatedFact; call: Extract.SpanOut; pos: int64; arg: Extract.SpanOut; }
model DfField { ...ExportRow; record: "df_field"; ...LocatedFact; owner: Extract.SpanOut; name: string; value: Extract.SpanOut; }
model DfLit { ...ExportRow; record: "df_lit"; ...LocatedFact; node: Extract.SpanOut; kind: string; text: string; }
model DfLoop { ...ExportRow; record: "df_loop"; ...LocatedFact; span: Extract.SpanOut; var: string | null; collection: string | null; }
model DfNest { ...ExportRow; record: "df_nest"; ...LocatedFact; call: Extract.SpanOut; loop: Extract.SpanOut; depth: uint32; collection: string | null; }
model DfAllocates { ...ExportRow; record: "df_allocates"; ...LocatedFact; owner: Extract.SpanOut; }
model Sig {
  ...ExportRow; record: "sig"; ...LocatedFact;
  owner: Extract.SpanOut; owner_start: uint32; owner_end: uint32;
  slot: string; pos: uint32; ty: string;
}
model Site { ...ExportRow; record: "site"; ...LocatedFact; span: Extract.SpanOut; callee: string; callee_path: string | null; }
model Const { ...ExportRow; record: "const"; ...LocatedFact; owner: Extract.SpanOut; field: string | null; text: string; kind: string; }
model Doc { ...ExportRow; record: "doc"; ...LocatedFact; owner: Extract.SpanOut; parent: string | null; text: string; }
model DocTag { ...ExportRow; record: "doc_tag"; ...LocatedFact; owner: Extract.SpanOut; tag: string; arg: string | null; text: string; }
model DocNode {
  ...ExportRow; record: "doc_node"; ...LocatedFact;
  span: Extract.SpanOut; kind: string; name: string; parent: string | null;
  target?: string; title?: string; body?: Extract.SpanOut;
}
model DataDoc { ...ExportRow; record: "data_doc"; ...LocatedFact; ordinal: uint32; span: Extract.SpanOut; format: string; doc: Json; }
model DataValue { ...ExportRow; record: "data_value"; ...LocatedFact; ordinal: uint32; path: string; kind: string; text: string | null; span: Extract.SpanOut; }
model Specifier { ...ExportRow; record: "specifier"; ...LocatedFact; span: Extract.SpanOut; name: string; kind: string; module: string | null; imported: string | null; }
model MethodOwner { ...ExportRow; record: "method_owner"; ...LocatedFact; owner: Extract.SpanOut; self_type: string | null; trait: string | null; }
model CfgScope { ...ExportRow; record: "cfg_scope"; ...LocatedFact; span: Extract.SpanOut; cfg: string; }
model TestOnlyCall { ...ExportRow; record: "test_only_call"; ...LocatedFact; callee: string; cfg: string; }
model MacroSite { ...ExportRow; record: "macro_site"; family: Extract.FamilyTag; span: Extract.SpanOut; macro_name: string; source: string; }
model Reference { ...ExportRow; record: "reference"; ...LocatedFact; span: Extract.SpanOut; functor: string; position: string; }
model Unresolved { ...ExportRow; record: "unresolved"; family: Extract.FamilyTag; path?: string; span: Extract.SpanOut; reason: string; detail: string; }
model Projectedge { ...ExportRow; record: "projectedge"; family: Extract.FamilyTag; kind: string; from: Extract.SpanOut; to_blob: string; to: Extract.SpanOut; }
model FlowEdge { ...ExportRow; record: "flow_edge"; family: Extract.FamilyTag; kind: string; from_blob: string; from: Extract.SpanOut; to_blob: string; to: Extract.SpanOut; }
model ResolvedEdge {
  ...ExportRow; record: "resolved_edge"; fact?: uint32;
  caller_path: string; caller_name: string | null;
  callee_path: string; callee_name: string | null;
  caller_site_start: uint32; caller_site_end: uint32; kind: string; resolution_origin: string;
}
model ResolvedTypeEdge {
  ...ExportRow; record: "resolved_type_edge"; fact?: uint32;
  owner_path: string; owner_name: string | null; owner_start: uint32; owner_end: uint32;
  target_path: string; target_name: string | null; kind: string; resolution_origin: string;
}
model ResolvedImport { ...ExportRow; record: "resolved_import"; src_path: string; name: string; local: string; target_path: string; target_name: string | null; kind: string; hops: uint32; }
model FileEdge { ...ExportRow; record: "file_edge"; src_path: string; dst_path: string; kind: string; symbols: uint32; }
model FileUnresolved { ...ExportRow; record: "file_unresolved"; src_path: string; module: string; reason: string; }
model PackageEdge { ...ExportRow; record: "package_edge"; src_manifest: string; dst_manifest: string; kind: string; }
model File { ...ExportRow; record: "file"; path: string; digest: string; bytes: uint32; lines: uint32; }
model SizeSkip { ...ExportRow; record: "size_skip"; path: string; bytes: uint64; limit: uint64; reason: string; }
model Capture { ...ExportRow; record: "capture"; query: string; capture: string; text: string; start: uint32; end: uint32; match_start: uint32; match_end: uint32; }

model ScipDef { ...ExportRow; record: "scip_def"; symbol: string; file: string; repo: string; }
model ScipName { ...ExportRow; record: "scip_name"; symbol: string; name: string; }
model ScipRef { ...ExportRow; record: "scip_ref"; file: string; symbol: string; def_file: string; repo: string; }
model ScipEdge { ...ExportRow; record: "scip_edge"; src: string; dst: string; repo: string; }
model ScipFnEdge { ...ExportRow; record: "scip_fn_edge"; caller: string; callee: string; }
model ScipCalleeType { ...ExportRow; record: "scip_callee_type"; sym: string; type: string; }
model ScipLocal { ...ExportRow; record: "scip_local"; `fn`: string; name: string; }
model ScipImpl { ...ExportRow; record: "scip_impl"; impl: string; iface: string; }
model ScipIndex {
  ...ExportRow; record: "scip_index"; reused: boolean; tool_name: string; tool_version: string;
  documents: uint32; index_mtime_unix_ms: uint64 | null; staleness: string;
}
model ScipSkip { ...ExportRow; record: "scip_skip"; lang: string; bin: string; reason: string; detail: string; }
model ScipOccurrence {
  ...ExportRow; record: "scip_occurrence"; path: string; symbol: string;
  start: uint32; end: uint32; roles: int32;
  definition: boolean; `import`: boolean; write_access: boolean; read_access: boolean;
  generated: boolean; test: boolean; forward_definition: boolean; syntax_kind: int32;
  enclosing_start: uint32 | null; enclosing_end: uint32 | null; text?: string;
}
model ScipOccurrenceDoc { ...ExportRow; record: "scip_occurrence_doc"; path: string; start: uint32; end: uint32; pos: uint32; text: string; }
model ScipDiagnostic { ...ExportRow; record: "scip_diagnostic"; path: string; start: uint32; end: uint32; severity: int32; code: string; message: string; source: string; tags: int32[]; }
model ScipSymbol { ...ExportRow; record: "scip_symbol"; path: string | null; symbol: string; display_name: string; kind: int32; enclosing_symbol: string; }
model ScipDocumentation { ...ExportRow; record: "scip_documentation"; symbol: string; pos: uint32; text: string; }
model ScipSignature { ...ExportRow; record: "scip_signature"; symbol: string; language: string; text: string; }
model ScipSignatureOccurrence { ...ExportRow; record: "scip_signature_occurrence"; symbol: string; ref_symbol: string; start: uint32; end: uint32; roles: int32; }
model ScipMetadata { ...ExportRow; record: "scip_metadata"; version: int32; tool_name: string; tool_version: string; tool_arguments: string[]; project_root: string; text_document_encoding: int32; }
model ScipDocument { ...ExportRow; record: "scip_document"; path: string; language: string; position_encoding: int32; text: string | null; }
model ScipRelationship { ...ExportRow; record: "scip_relationship"; symbol: string; related_symbol: string; is_reference: boolean; is_implementation: boolean; is_type_definition: boolean; is_definition: boolean; }
