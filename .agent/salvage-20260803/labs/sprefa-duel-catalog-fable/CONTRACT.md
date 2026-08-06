# CONTRACT: spine catalog emitter (type-IR MVP, steps a+b)

Build the type-IR MVP per PLAN2.md at this worktree root (its section 1 rows
a+b ONLY; c-f are out of scope). Both open questions in PLAN2 section 6 are
RULED: (1) TWO marker zones — spine.ts keeps its 7 row interfaces, types.ts
keeps NodeRow/EdgeRow, each wrapped in its own generated-section markers;
span_row out of scope. (2) synthetic scip version pins to `dev` (the
scip_symbol column ships in the facts now but is INERT to ts output — step c
wires the checker later).

## Deliverables

1. NEW `v6/prolog/compile/3a_spine_schema_facts.pl`:
   `table(Name, WithoutRowid)`, `table_symbol(Table, ScipSymbol)`,
   `column(Table, Name, BaseType, Nullable, Pk, ScipSymbol)` (column/6, Pk =
   none | pos(N)). Seed = the 9 tables / 37 columns TRANSCRIBED from the live
   files (spine.ts:62-102 interfaces, types.ts:77-95 NodeRow/EdgeRow/marker,
   v6/sprefa-store/src/spine.rs DDL 314-473 for WithoutRowid + pk truth).
   Verify your transcription against all three sources; record receipts.
2. NEW `v6/prolog/compile/3_emit_spine_schema.pl`: module emit_spine_schema,
   exports `emit_spine_schema/0` (writes both zones) and `rows_ts_text/2`
   (Zone, Text) or equivalent — follow the marker-section precedent
   `1_emit_registry_docs.pl` (`replace_generated_section/5` :244) and the
   module/export shape of `2_emit_cli_inventory.pl`.
3. EDIT `v6/sprefa-store/js/src/engine/spine.ts` + `types.ts`: wrap the
   existing interfaces in begin/end marker comments. THE PROOF: the emitter's
   output must equal the existing hand-written interfaces BYTE-FOR-BYTE
   (whitespace included) — zero drift between facts and reality. Any
   mismatch means your facts are wrong, never that the ts should change.
4. NEW `v6/tsv2/tests/spineSchema.test.ts` staleness gate, pattern copied
   from `bopCommandInventory.test.ts:52-59` (spawnSync swipl, assert emitted
   text equals file content). Find the runner that executes
   bopCommandInventory.test.ts and run yours the same way; record the exact
   command.
5. REPORT.md: gates table with exact commands + outputs, transcription
   receipts, deviations (STOP-and-record, never improvise).

## Gates (all recorded)

| gate | command |
| --- | --- |
| emitter loads | `swipl -q -l v6/prolog/compile/3_emit_spine_schema.pl -g halt` clean |
| byte-equality | emitted zone text == current file zone text (the test proves it) |
| staleness test | the spineSchema.test.ts run, same runner as bopCommandInventory |
| plunit untouched | `swipl -g go -t halt ARCH.pl` from v6/prolog still exits 0 |
| tsc | tsv2/store typescript still compiles (find and run the repo's check) |

## Laws

No commits. Nothing outside this worktree. Never run the full battery
(green-all) — scoped gates only. Comment budget: constraints only. No em
dashes. Never provenance, substrate, load-bearing, regime. Descriptive
names; interfaces carry I prefix where the file's convention says so;
colocated consistency wins over any other style rule. dl variable names
descriptive. No subagents.
