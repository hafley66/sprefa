# PLAN2: type-info IR breakdown (value + arrow universe, SCIP id column)

Merge check: `git merge --ff-only 2eceb836` returned exit 0, already up to date (base
`2eceb836` was HEAD). Proceed.

Grounding: prior-plans/PLAN.md + RESEARCH.md + plan-notes/SEED-INVENTORY.md (the 2026-08-02
convergent schemagen plans); scip.proto (962 lines, fetched today); v5 src/+plans for the
SCIP surface; the alloy lab in the read-only sprefa-lab-alloy worktree. Every claim cites
`path:line`. Two brief claims failed falsification and are flagged in section 2.

## 1. Step ladder

Universe (ruled, not re-litigated): VALUES (structural, jsonschema-native) + ARROWS
(Input->Output; host signature requestCols->responseCols; openapi is the arrow printer).
Shapes are prolog facts; SCIP symbols are the fact id column, never the shape.

| # | step | target | files (new unless noted) | LOC | exact gate | breaks |
|---|---|---|---|---|---|---|
| a | facts seed + ts row-interface emitter, MVP | (a) | `v6/prolog/compile/3a_spine_schema_facts.pl`, `3_emit_spine_schema.pl`, edit `v6/sprefa-store/js/src/engine/spine.ts` + `types.ts` (marker zones) | ~190 | `swipl -q -l 3_emit_spine_schema.pl -g "emit_spine_schema:rows_ts_text(T),format('~s',[T])" -g halt` output == marker-zone bodies; eyeball for hand edits | none (purely additive) |
| b | staleness gate | MVP gate, proof | `v6/tsv2/tests/spineSchema.test.ts` (new, pattern `bopCommandInventory.test.ts:52-59`) | ~50 | `just test` (tsv2 suite) incl. asserted-equal gate | a: a hand edit that drifts facts vs ts now fails CI |
| c | scip_symbol id column | (b) | `3a_spine_schema_facts.pl` (column/6 ScipSymbol), emitter reads it | ~15 | `swipl -l 3a -g "forall(scip_symbol(_,_,S),valid_scip_symbol(S))"` loads (grammar checker) | a/b: fact lines churn; gate still asserts ts, symbol is inert to ts output |
| d | rust emitter parity | (c) | `4_emit_spine_rust.pl` (new) | ~70 | emitted rust struct == `spine.rs` `#[derive]` Model fields via diff/cargo test | c: adding rust target must not change ts output (target-neutral facts) |
| e | arrows -> openapi | (d) | `arrow/4` facts in `3a`, extend `v6/prolog/labs/openapi_codegen/emit_openapi.pl` seam (`schema_object/2`) | ~40 + facts | `swipl -l emit_openapi.pl -g emit_openapi -g halt` renders one host signature; Redocly-validate the json | d: arrow facts are ignored by value emitters (no schema migration, additive) |
| f | generics list(T) monomorphize | (e) | `3a` (parameterized column terms), `3_emit_spine_schema.pl` + `4_emit_spine_rust.pl` (sql/ts/rust) | ~35 | monomorphized sql/ts/rust emit matches hand sample; jsonschema via `emit_openapi.pl:196` `list(Item)` | e: a parameterized column must monomorphize, not emit a bare `list(...)` |

MVP complete at step b (facts + ts emitter + marker section + staleness test, ~190 LOC).
c-f are the ruled extension ladder on the same fact base.

## 2. Falsification probes (ran, read-only)

| claim | receipt | verdict |
|---|---|---|
| spine.ts:63-102 is the emit marker zone | `spine.ts:62-102`: `// entity row types` at :62, interfaces StringsRow:63 ReposRow:67 RootsRow:73 RepoRevsRow:78 FilesRow:86 RevsFilesRow:92 FileBytesRow:97 | CORRECT zone, WRONG count: 7 interfaces, **not 9** |
| 9 row interfaces in spine.ts | grep `export interface` in spine.ts yields only the 7 above; no NodeRow/EdgeRow | **CONTRADICTED**: node/edge have **no** row interface in spine.ts |
| NodeRow/EdgeRow live in types.ts | `types.ts:80` NodeRow, `types.ts:90` EdgeRow, SpanRow:94, marker "Row shapes (spine entities)" :77-79 | CORRECT (`types.ts:80-95`) |
| spine.rs DDL lives in STORE, models at 314-417 | `spine.rs`: node struct :314, edge struct :351, `create_all_tables` + `without_rowid` :386-408, `secondary_indexes` :431-473 (partial `ux_repo_revs_work_root` raw :463-465). SEED-INVENTORY line refs match | CORRECT |
| cli-inventory emitter precedent | `2_emit_cli_inventory.pl`: module `emit_cli_inventory`, exports `emit_cli_inventory/0`, `cli_inventory_text/1` (:7-10), whole-file write :17-21 | CORRECT (arity 1/1) |
| openapi emitter precedent | `emit_openapi.pl`: module `emit_openapi`, exports `emit_openapi/0 openapi_json_text/1 openapi_document/1 spec_operations/1` (:25-30); `schema_object(list(Item),_)->array/items` :196; named-schema `$ref` :198-200 | CORRECT (arity 1 each); :196 is the generics/jsonschema precedent for step f |
| registry-docs marker precedent | `1_emit_registry_docs.pl`: begin/end markers :18-22, `replace_generated_section/5` :244 | CORRECT |
| bop staleness gate | `bopCommandInventory.test.ts:52-59` spawnSync swipl + asserted-equal; EMITTER_PL :20 | CORRECT |
| ARCH task shape | `v6/prolog/ARCH.pl:651` comment "task(Name, Status, Needs)"; rows `task(kernel_sql_lowering, done, [])` :655, `task(schema_import_epic, unbuilt, [])` :751, `task(openapi_codegen_spine, done, [])` :800 | **CONTRADICTED**: shape is **task/3**, not task/5 as brief stated |
| alloy lab check/render split | `.../alloy_semantics/2_check.pl:35-40` check before render, `codegen_refused/1`; `run.pl:14-20` refuse + halt(1) before any text; `3_render.pl:20-24` build term tree then fold; receipts `4_receipts.sh` sabotage + parity | CORRECT; MAPPING.md marks mono-morphization unmapped (relevant to step f) |
| v5 SCIP surface | `src/rels/scip.rs:61-88`: `scip_def(symbol:Text,file:Path,repo:Text)`, `scip_ref/4`, `scip_edge/3`, `scip_occurrence/8`, `scip_name/2`; symbols stored as **Text** (full SCIP symbol string). `plans/2026-07-11-scip-atlas.md:6-14`: scip_def/scip_ref/scip_edge/scip_occurrence confirmed | CORRECT; join target is `scip_def/symbol` Text |

No grounding claim remains UNVERIFIED. Two are CONTRADICTED (row-interface count, task
arity); both are recorded above and accounted for in sections 3-4.

## 3. Fact schema, final

```
table(Name, WithoutRowid).                          % table/2, unchanged
table_symbol(Table, ScipSymbol).                   % value-typedef id (the shape itself)
column(Table, Name, BaseType, Nullable, Pk, ScipSymbol).   % column/6
```

column/6 = the column/5 of prior-plans/PLAN.md:21 plus `ScipSymbol` as the last column.
`BaseType` closed set from PLAN.md:41-48: `integer int32 text blob` (pks never nullable).
`Pk` is `none` or `pos(1..n)`; `WithoutRowid` true for composite-key junctions
(revs_files, file_bytes, edge).

column/5 vs column/8 fork: **column/6** (flash leaner), one sentence: the only thing
column/8 adds is per-emitter mapping (openapi/jsonschema spellings), which belongs in the
emitter's dialect clause per `emit_openapi.pl:13-23` and must never leak into facts.

`scip_symbol` value-keyed on the v5 join target `scip_def(symbol:Text, ...)` (`src/rels/scip.rs:61`).
Seed content = the 9 tables / 37 columns transcribed in SEED-INVENTORY.md.

## 4. ARCH task rows (statuses all unbuilt)

Shape is **task/3** in v6/prolog/ARCH.pl (section 2 receipt), mirroring rows :655/751/800.
`Needs` = prior step in the ladder.

```
task(type_ir_facts,          unbuilt, []).                              % table/2 + column/6 + table_symbol (step a base)
task(type_ir_ts_emitter,     unbuilt, [type_ir_facts]).                 % ts row interfaces into spine.ts + types.ts marker zones
task(type_ir_staleness_gate, unbuilt, [type_ir_ts_emitter]).            % spineSchema.test.ts MVP gate (step b)
task(type_ir_scip_id,        unbuilt, [type_ir_facts]).                 % scip_symbol id column, format per scip.proto grammar (step c)
task(type_ir_rust_emitter,   unbuilt, [type_ir_ts_emitter]).            % rust struct parity (step d)
task(type_ir_arrows_openapi, unbuilt, [type_ir_facts]).                 % arrow/4 fact -> emit_openapi.pl seam (step e)
task(type_ir_generics_mono,  unbuilt, [type_ir_arrows_openapi]).        % list(T) monomorphize for sql/ts/rust + jsonschema (step f)
```

## 5. SCIP symbol-string format (step c design, per grammar)

Grammar source `scip.proto:154-177`: `<symbol> ::= <scheme> ' ' <package> ' ' (<descriptor>)+`,
`<package> ::= <manager> ' ' <package-name> ' ' <version>`; descriptors Type `<name>#` :170,
Term `<name>.` :171, Method `<name>(<disambiguator>).` :173.

```
value-typedef : "typeir" " " "." " " <pkg> " " "dev" " " <shape>#
value-member  : "typeir" " " "." " " <pkg> " " "dev" " " <shape>#<member>.
arrow-signature: "typeir" " " "." " " <pkg> " " "dev" " " <host>#<sig>(<role>).
```

| part | value | why |
|---|---|---|
| scheme `typeir` | synthetic domain | grammar: non-empty, must not start with `local` (:160). No compiler indexer emits this scheme (real ones are language names), so synthetic ids never collide with an indexed index |
| manager `.` | placeholder | grammar's empty-manager value (:162). No package manager owns these shapes |
| package-name `<pkg>` | catalog (e.g. `spine`, `hosts`) | separates catalogs so a spine column never collides with a host signature id; mirrors SCIP's namespacing intent |
| version `dev` | constant | grammar requires a non-empty third component (:157). Constant keeps the id string stable across regenerations, so the staleness gate and scip joins are stable |
| Descriptor kind | `#` Type, `.` Term, `(...)` Method | matches SCIP role semantics: value types are Type :170, their columns nest as Term :171, host signatures are Method :173 with the req/res role as the disambiguator |

## 6. Open questions

Blocking:
- MVP emit target split (breaks step a/b gate): spine.ts holds 7 row interfaces, node/edge
  hold none (section 2). Emit all 9 into a single spine.ts marker (migrate NodeRow/EdgeRow
  out of types.ts, update consumers) or add a second marker zone in types.ts? The
  asserted-equal gate depends on this. Default proposed: two marker zones, `span_row` out
  of scope (no spine table).
- Synthetic symbol package-name/version policy (breaks step c): pin `version = dev` and
  document the catalog list so the grammar checker and scip joins stay stable.

Blocking nothing:
- arrows/4 deferred to step e; column/6 + table_symbol added now are backward compatible
  additive, no fact migration for the value steps.
- v5 has no `scip_typedef` (scip-atlas gap 4); joins use `scip_def/symbol` only, which
  exists. Not our gap.
- rust parity gate mechanism (diff vs cargo test) unsettled; step d internal.
- marker-section (registry precedent) vs whole-file (cli-inventory precedent): latched to
  marker-section per prior PLAN.md:138-145; no decision needed.
