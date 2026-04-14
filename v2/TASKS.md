# v2 Scaffold Tasks

Dependency-ordered. Traits + pure static types + pure utils only. No impls, no ops, no storage backends in this pass.

## Tier 0 — root static types (`_0_types.rs`)
- [x] RunId, OpId, RowId, FileId, StringId, RefId newtypes
- [x] Severity enum
- [x] Span { start: u32, end: u32 }
- [x] FilePath newtype over Arc<Path>
- [x] ParseSite + ParseSeg enum
- [x] PathSeg enum + SprfPath(Arc<[PathSeg]>)
- [x] Capture
- [x] Cursor
- [x] RunCtx
- [x] RunEvent enum
- [x] SkipReason, RunStatus, RewriteKind enums

## Tier 1 — framework traits + structural types
- [x] Diagnostic trait (object-safe)
- [x] Renderer trait
- [x] Reader trait (signatures only)
- [x] Writer trait (signatures only)
- [x] Writer param structs: RefEntry, ExtractionRow, ProvenanceRow, RunVisit, ViolationRow, EffectLogRow, FileEdit, ShellCall, ShellReply
- [x] RuleTableSpec, ColumnSpec
- [x] Reader return shapes: ScanKind, ScanCombo, CrossRefHit, ViolationEntry, ParsedTree, ParserKind
- [x] OpCtx
- [x] DiagSink, EventSink newtypes
- [x] Pipeline enum
- [x] OpInvocation
- [x] BraceMode, GrammarRef placeholder types
- [x] ProgramCtx
- [x] Op trait
- [x] Operator trait
- [x] Extractor trait
- [x] Runner trait
- [x] Config + ConfigDiff + content_hash sig

## Tier 2 — pure utils
- [x] parse_capture
- [x] parse_provenance
- [x] parse_cross_ref
- [x] classify_token
- [x] scan_balanced
- [x] host_parse (outer splitter)
- [x] glob_match (keep)
- [x] levenshtein (keep)
- [x] cursor_hash
- [x] path_hash_static
- [x] content_hash wrapper

## Tier 3 — registry shell
- [x] OperatorRegistry struct + register + resolve (alias-aware)
- [x] lower_rules two-pass shell (pre_register then parse)

After this: compiles empty, all surfaces fixed, step 1 impls slot in.
