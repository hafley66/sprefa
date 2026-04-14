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

## Tier 4 — cross-ref runtime (2026-04-14, commit f6b8008)

- [x] Layer 0: `OpInvocation.crossrefs` populated at host-parse time
- [x] Layer 0: `$VAR` ≡ `${VAR}` synonymy (host parser + walker)
- [x] Layer 1: `RuleHandle.depends_on` collected in `RuleFactory::parse`
- [x] Layer 1: `_11_dag.rs` Kahn topo + DFS cycle recovery
- [x] Layer 1: `xref/cycle` diag wired to `DocSession::load`
- [x] Layer 2: `LoweredOp { op, xrefs }` wraps `Pipeline::Op`
- [x] Layer 2: `_12_result_store.rs` per-rule row store, Pending/Complete gating
- [x] Layer 2: `OpCtx.result_store` + `xref_seen` per-run dedup
- [x] Layer 2: `expand_xrefs` adapter spliced into `Pipeline::run_with_step`
- [x] Layer 2: `xref/empty-join` Hint, `xref/cartesian-limit` Warn
- [x] Layer 2: `RuntimeConfig.xref_cartesian_limit` (default 10_000)
- [x] Layer 2.5: walker `Leaf { capture: Some(_) }` constrains-when-prebound
- [x] Layer 2.5: `${rule.$VAR}` lowers to `Leaf { capture: Some(VAR) }`

## Tier 5 — runner write path + provenance (pending)

- [ ] Layer 4: `$$repo` / `$$rev` via same adapter pattern in `_5_op.rs`
- [ ] Layer 5: Runner appends emitted cursor captures to `ResultStore`
- [ ] Layer 5: Runner calls `mark_complete(rule)` on stream end
- [ ] Layer 5: Level-barrier scheduling via `RuleDag::Ordered.levels` (tokio multi-thread, intra-level parallel, inter-level barrier)
- [ ] Layer 5: End-to-end integration test (rule-A produces, rule-B xref-pulls)
- [ ] Layer 5: Streaming cartesian-limit counter (avoid second walk on overflow)
- [ ] Layer 5: ResultStore lifecycle policy across LSP re-runs (wipe vs diff vs retain)
- [ ] Layer 5: Cycle diag UX — per-rule sites vs single-site with cycle path

## Future small items

- [ ] Walker `SelectStep::CrossRef` as a real variant (today loses `rule` component at lower time)
- [ ] Cartesian-limit Warn could include per-rule row counts in render
- [ ] ResultStore `Mutex<HashMap>` may want `DashMap` upgrade once Layer 5 parallel runner lands (no new dep without approval)

