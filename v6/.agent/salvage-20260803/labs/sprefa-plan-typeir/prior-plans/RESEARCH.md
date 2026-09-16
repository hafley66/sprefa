# Research: common sqlite schema + generated rs/ts types

Lane: `research/schema`. Base verified `92756b54dc0cb633e9636234f5358f3324be1ebf`.
Sections A–C findings salvaged from the prior partial run
(`/private/tmp/claude-501/.../schema-lane-salvage.log`, 1265 lines) and spot-verified;
D–E done fresh under the bounded rules. Every row carries a path. `#n` = line.

## 1. Timeline

| date | artifact | what was said / decided |
|---|---|---|
| 2025-04-04 | `~/projects/hafley-tsp/monomorphization-engine.md:16` | "Manual type synchronization (JSON ↔ Rust structs ↔ TypeScript interfaces)" flagged as problem 1.1; TypeSpec single source of truth, emits to N targets (exec summary :10, §4 scenarios). External prior art for the exact idea. |
| 2026-06-18 | `plans/2026-06-18-north-star-types-modules-parsers.md:13` | "Types are rels": author types in dl, surface via LSP/UI. Authoring axis, not schema codegen. Companion `chat_log/20260618.1.*.md`. |
| 2026-06-28 | `plans/2026-06-28-openapi-speclang-flows.md` | openapi speaklang flows (grep hit for openapi). |
| 2026-07-19 | `v6/plans/2026-07-19-v6-schema.sql`, `v6-plans/2026-07-19-v6-table-design.md`, `v6-plans/2026-07-19-v6-storage-crate.md` | v6 spine schema authored as `.sql` spec + table design + storage crate plan. |
| 2026-07-21 | `v6/plans/2026-07-21-spine-orm.md` | spine ported to sea-orm entity derives (`spine.rs`). |
| 2026-07-23 | `v6/DECISIONS.md:69` | "TS engine + rxjs lowering" DECIDED; Rust cascade "ported 1:1 to TS at `v6/sprefa-store/js/`". |
| 2026-07-23 | `v6/plans/2026-07-23-v6-rxjs-lowering-and-ts-port.md:6,12,19-21,144` | Port LANDED (`js/src/{engine,lib,algo,spine,measure,oracle,tasks,index}.ts`, 2976 lines, golden 11/11). Port map table in §3. |
| 2026-07-23 PM | `v6/DECISIONS.md:86` | "TS SQLite bindings are FROZEN (owner ruling). The TS side is the prototype lab… Agents: do not 'fix' this." Objects to a generated TS seam. |
| 2026-07-29 | `plans/2026-07-29-finish-the-job-epic.md`, `plans/2026-07-30-extract-t2-verdict.md` | grep hits for codegen/schema-gen. |
| 2026-07-30 | `v6/prolog/ARCH.pl:800` + `plans/2026-07-30-openapi-codegen-lab.md:1-4` | openapi_codegen_spine LANDED: one spec generated from prolog facts; openapi-typescript for TS types; progenitor for rust. Closest in-repo precedent. |
| 2026-07-30 | `v6/prolog/ARCH.pl:778` | json_interop_lab done: JSON/json1 is transient wire adaptation; queryable objects normalize to rel rows + int ref edges; 9.38x size win relational vs inline. |
| unbuilt | `v6/prolog/ARCH.pl:751` | `task(schema_import_epic, unbuilt, [])` — USER: "TypeSpec / JSON-Schema / OpenAPI v3 (json + yaml) import as its own epic. Prior art is ours already (prior_art hafley_tsp: TypeSpec app-gen, config/env/CLI sources, @secret redaction). Build-vs-buy law applies." Direct in-repo instance of the idea. |
| unbuilt | `v6/prolog/ARCH.pl:202-203` | `prior_art(hafley_tsp, '~/projects/hafley-tsp', bind_vocabulary, _)`. |
| 2026-08-01 | `plans/2026-08-01-refusal-inventory.md` | grep hit for codegen/typespec (unread). |

chat_log grep hits (dated by filename, unread):
`chat_log/20260720.7.typespec-prolog-soup-lsp-recon.md`,
`chat_log/20260727.1.v6-lang-lab-waves-ruling-queue.md`,
`chat_log/20260724.1.v6-dl-mvp-orchestration-m9-storage-codex.md`,
`chat_log/20260723.2.v6-pivot-ts-on-actual-rxjs-trinity-not-locked.md`,
`chat_log/20260622.3.instant-plugin-system.md`,
`chat_log/20260506.1.macro-rpc-registry-and-host-syntax-tightening.md`,
`chat_log/20260730.1.fable-opus-storm-lab-assimilation.pl`,
`chat_log/20260531.6.v5-type-graph-rev-explorer-d2-thrash-spine-audit.md`.

## 2. Schema-site inventory

| path | defines | lang | hand-kept / generated |
|---|---|---|---|
| `v6/sprefa-store/src/spine.rs:314-352` | 9 sqlite tables as sea-orm `Model` structs (`strings, repos, roots, repo_revs, files, revs_files, file_bytes, node, edge`); DDL emitted via `create_table_from_entity` `spine.rs:408-417` | rust | hand-kept (derive) |
| `v6/sprefa-store/src/engine.rs:102-122,1083-1091,1369-1372` | raw `CREATE TABLE` `format!` for cascade/reconcile working relations + fact table (`engine.rs:116`) | rust | hand-kept SQL strings |
| `v6/sprefa-store/src/measure.rs:574,629-636` | raw `CREATE TABLE` for golden harness (`g_node`/`g_edge` `measure.rs:629-636`) | rust | hand-kept SQL strings |
| `v6/sprefa-store/js/src/engine/types.ts:80-97` | `NodeRow`/`EdgeRow`/`SpanRow` row interfaces (header file, "Row shapes (spine entities)" :751) | ts | hand-kept |
| `v6/sprefa-store/js/src/spine.ts` | schema DDL + row types (ported twin of `spine.rs`, per plan `2026-07-23-...-ts-port.md:19`) | ts | hand-kept |
| `v6/sprefa-extract/src/schema.rs:17` | JSONL wire contract `SCHEMA` const (`--schema`), not sqlite | rust const | hand-mirrored from `FlatFact` (`schema.rs:12` "Keep it in sync… mirrors it… without a doc-build step") |
| `v6/sprefa-extract/src/types.rs:622-668` | generic `Node`/`Edge<F>` family structs + `FlatFact` | rust | hand-kept |
| `v6/sprefa-extract/src/rows.rs:3` | re-export of `crate::types::{Edge, FamilyBundle, Node}` | rust | hand-kept re-export |
| `v6/sprefa-extract/src/scip_v5_rels.rs` | scip record shapes | rust | hand-kept |
| `v6/tsv2/gen_emitted/*.ts` | `CREATE TABLE` strings inside generated test artifacts (e.g. `float_arithmetic_is_binary64.ts:134`) | ts | generated by dl test emitter (not source of truth) |
| `v6/plans/2026-07-19-v6-schema.sql` | the v6 spine schema authored as sql | sql | authoring doc |

Key finding: the sqlite schema lives in **sprefa-store** (`spine.rs` + `engine.rs` + `measure.rs`),
not in sprefa-extract. Extract owns a JSONL wire contract (`schema.rs`), not sqlite DDL.

## 3. Port map (rust → ts) in sprefa-store

Source `v6/plans/2026-07-23-v6-rxjs-lowering-and-ts-port.md:12,19-21,144` + module lists.

| rust | ts | judgment (name+signature) |
|---|---|---|
| `src/spine.rs` | `js/src/spine.ts` | faithful-by-declaration "schema DDL + row types"; DDL spelled via sea-orm derive (rust) vs raw (ts); row types hand-duplicated → drifted risk |
| `src/engine.rs` | `js/src/engine/engine.ts` | faithful "ported verbatim 1:1" (plan :12) |
| `src/lib.rs` (Store+ingest) | `js/src/engine/lib.ts` + `engine/ingest.ts` | faithful; ingest split out in ts |
| `src/algo.rs` (Reach) | `js/src/engine/algo.ts` | faithful (delegation; body not verified) |
| `src/measure.rs` | `js/src/engine/measure.ts` | faithful |
| `src/oracle.rs` | `js/src/engine/oracle.ts` | deliberately partial — "dd/salsa NOT ported", oracle math only (plan :20-21) |
| `src/tasks.rs` (parity traits) | `js/src/engine/tasks.ts` | faithful |
| — | `js/src/engine/{counter,sqlRunner,types}.ts`, `js/src/lower/*`, `js/src/gen/reach.gen.ts`, `index.ts` | ts-only additions, no rust twin |

Named row-type pair: rust `node::Model` `spine.rs:314` ↔ ts `NodeRow` `types.ts:80`; rust
`edge::Model` `spine.rs:351` ↔ ts `EdgeRow` `types.ts:90`. Parallel and hand-kept, not generated.

## 4. TypeSpec assets

### `~/projects/hafley-tsp` (bounded read)
- `AGENTS.md:131-175` "Cross-Boundary Type Authority": TypeSpec is the single source of truth for any string/enum/type that crosses a process/system boundary; "one place for the closed set, N emitters to N targets" (:159).
- `AGENTS.md:110` "No identifier renaming… one name, one string, every language" (grep-ability) — the affordance a common-schema generator wants.
- `AGENTS.md:21-39` emitter `_auto` files, manual files never overwritten; `:195-201` preamble (render timestamp, input hash, contributing sources) + diff-to-preserve-mtime.
- `monomorphization-engine.md` (see timeline; tables `tsp_symbols`/`tsp_deps` proposed against sprefa SQLite `:54-79`).
- `docs/design/{delta-and-causal-chains,readme-emitter-and-doc-refs,route-convention,route-namespace-system}.md` — present, unread (bounded).
- `examples/{routes,ghcacher,todo-app.tsp}` route/state modeling (`_types.tsp`).
- Per brief `README.md` **does not exist** at root; `src/` is empty. Recorded, no substitute.

### `~/projects/claude-research` (rg + top-3 reads)
- 11 typespec skills archived at `skills_archive/typespec-{core,rest,emitters,custom-emitters,tooling,templates,functions,validation,input-output,cross-layer}/SKILL.md` + `alloy-{core,languages}/SKILL.md`; `tsp-arch/{SESSION.md,8_evolution.md}`.
- Top-3 read: `chat_log/20260314_223532_hafley-plugin-setup.md` (hafley plugin org, 11 typespec skill families), `chat_log/20260315.1.hafley-alloy-rust-codegen-layers-0-1.md` (Alloy-based Rust emitter: TypeSpec models → clean Rust, no case rename, binder/refkey import gen). These are TypeSpec/Alloy emitter prior art, not sqlite-schema related.

## 5. Codegen precedents already in-repo

| artifact | generates | mechanism |
|---|---|---|
| `v6/prolog/src/emit_ts.pl:3,8,49` | TS `Program` AST (rels/rules) e.g. `emit(reach,'out.gen.ts')`; header `GENERATED by v6/prolog/src/emit_ts.pl` | prolog facts → ts |
| `v6/prolog/emit_ts.pl:4` | module `emit_ts`, root variant | prolog |
| `v6/prolog/labs/openapi_codegen/emit_openapi.pl:1-40` | OpenAPI 3.1 document `openapi_json_text/1` + `emit_openapi/0` | facts → spec; "same shape as compile/2_emit_cli_inventory.pl" (:6-9) |
| `v6/prolog/compile/2_emit_cli_inventory.pl:1-25` | `v6/tsv2/cli/0_inventory.ts` from `cli_command/3`,`http_route/3`; `cli_inventory_text/1` + `emit_cli_inventory/0` | facts → ts inventory |
| `v6/prolog/compile/1_emit_registry_docs.pl:9-16,1182-1194` | SYNTAX.md `surface/5` table via begin/end markers + `replace_generated_section` | facts → md |
| `plans/2026-07-30-openapi-codegen-lab.md:1-4,108-110,118-1120` | ts types from spec: buy = openapi-typescript 7.13 (consumes, codegen, zero runtime deps); staleness gate `tests/bopCommandInventory.test.ts` against checked-in `0_inventory.ts` | consume-spec consumer + staleness gate |
| `v6/sprefa-store/js/src/gen/reach.gen.ts:1` | `GENERATED by v6/prolog/src/emit_ts.pl from program 'reach'` | emit_ts output checked in |

Pattern: generated-artifact-checked-in + staleness-gated is the repo norm
(`plans/2026-07-30-openapi-codegen-lab.md:1102-1104`).

## 6. Gaps

- No single canonical sqlite schema both rust and ts consume. rust = sea-orm derives (`spine.rs`) + raw `CREATE TABLE` (`engine.rs`,`measure.rs`); ts = `spine.ts` DDL + `types.ts` row interfaces, duplicated by hand.
- No generator emits row-loading types (rust struct / ts interface) from DDL. `emit_ts.pl` emits dl `Program` AST (rels/rules), not sqlite row types. `emit_openapi.pl` + openapi-typescript is spec→types and is the nearest, but is bound to an HTTP spec, not sqlite DDL.
- No sqlite→rust entity generator used (sea-orm has sea-orm-cli entity gen; not adopted).
- Ownership conflict: the proposal says "sprefa-extract exports ONE canonical sqlite schema"; today extract exports a JSONL wire contract (`schema.rs:17-21`), sqlite DDL is store-owned. Ownership must be resolved before any canonical schema lands.
- `schema_import_epic` (`v6/prolog/ARCH.pl:751`) is unbuilt and explicitly "build-vs-buy law applies before any bespoke line" — the buy-research step for the shared-IDL/schema axis is not done.

## 7. Contradictions / evidence against

- `v6/DECISIONS.md:86`: TS SQLite bindings are FROZEN by owner ruling; TS side is "the prototype lab"; "Agents: do not 'fix' this." A generated TS loading seam would touch frozen TS bindings.
- `v6/DECISIONS.md:81`: "Z-set IVM in TS is the resident-RAM trap the unification killed" — TS is lab, rust is production; a shared generator must reconcile two tiers with different mandates.
- `v6/sprefa-extract/src/schema.rs:12`: extract chose a hand-mirrored prose `SCHEMA` const over a doc-build/generated step ("without a doc-build step") — a deliberate non-generation choice for the very contract the proposal wants to generate.
- Scope drift: existing openapi codegen precedent generates from prolog facts for an HTTP spec, not from sqlite DDL; porting that mechanism to the db seam is new surface, not a proven path (`v6/prolog/ARCH.pl:800`).
