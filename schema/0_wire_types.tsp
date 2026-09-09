// Wire vocabularies from extract::types and extract::tsi::types.
namespace Extract;

enum FamilyTag {
  Df: "df", Flow: "flow", Call: "call", Type: "type",
  Module: "module", Cst: "cst", Cfg: "cfg", Data: "data",
}
enum Mode { Syntax: "syntax", Semantic: "semantic" }
enum CoverageFlag { Partial: "partial", Complete: "complete" }
enum Method {
  SameFile: "same_file", CorpusUnique: "corpus_unique", ModulePlane: "module_plane",
  Checker: "checker", AliasChain: "alias_chain", Param: "param", Receiver: "receiver",
  SelfType: "self_type", IfaceImpl: "iface_impl", Decorator: "decorator",
  Subscript: "subscript", ReturnCall: "return_call", Scip: "scip",
  Unresolved: "unresolved", Parse: "parse", CheckerWalk: "checker_walk", Foreign: "foreign",
}

model IdArg { id: uint32; }
model SpanArg { span: [string, uint32, uint32]; }
model TextArg { text: string; }
model IntArg { int: int64; }
model AtomArg { atom: string; }
union TsiArg { id: IdArg, span: SpanArg, text: TextArg, int: IntArg, atom: AtomArg }
