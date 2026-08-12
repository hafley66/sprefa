# Type IR polyglot door: recon and price

## TOC

1. [Context](#context)
2. [Expressive frontier](#expressive-frontier)
3. [Build-versus-buy](#build-versus-buy)
4. [Two populations](#two-populations)
5. [Input side](#input-side)
6. [Language-design forks](#language-design-forks)
7. [Decisions](#decisions)
8. [Verification](#verification)
9. [Staffing](#staffing)

## Context

The current compiler already has one schema input door and two schema output doors. It has no language-type output door.

| Existing piece | Receipt | Current boundary |
|---|---|---|
| Shared type table and column storage classification | `v6/prolog/0_type_plane.pl:64-75`, `:77-151` | Closed scalar set, named relation records, option and collection wrappers |
| OpenAPI and JSON Schema input | `v6/tsv2/scripts/openapi_to_dl6.ts:327-385`, `:400-416` | Emits `.dl6`; unsupported shapes widen to `json` in safe mode |
| JSON Schema output | `v6/prolog/compile/4_emit_jsonschema.pl:109-160` | Records, five primitives, list/json-list, scalar option, relation reference |
| OpenAPI output | `v6/prolog/compile/5_emit_openapi.pl:23-40` | Reuses the JSON Schema component builder under OpenAPI 3.1 |
| Sweep driver | `v6/prolog/sweep.pl:103-130` | Builds catalog rows, restores scalar options, writes JSON Schema |
| Canonical type parser | `v6/prolog/compile/parse_dl_dcg.pl:510-534` | One `type_expr//1` grammar shared by columns; postfix `?` means `option` |
| Handwritten printer | `v6/prolog/print_dl.pl:324-335`; `plans/2026-08-12-cleanroom-dcg-bakeoff.md:24-45` | Separate output implementation; two clean-room reverse-DCG attempts did not terminate |
| Cross-language Rust codegen dependencies | repository-wide `rg -n 'ts-rs\|schemars\|specta\|typeshare' --glob Cargo.toml` | 0 matches |

The maintained handwritten surfaces total 4,249 lines: Rust extraction types 1,834, TSV2 runtime types 1,069, DL types 630, store engine types 546, and lowering types 170 (`v6/sprefa-extract/src/types.rs`, `v6/tsv2/runtime/types.ts`, `v6/dl/src/0_types.ts`, `v6/sprefa-store/js/src/engine/types.ts`, `v6/sprefa-store/js/src/lower/types.ts`).

## Expressive frontier

The cells describe a loss-minimizing spelling. `No counterpart` marks a semantic distinction that the target type language cannot carry without metadata, helper types, or generated runtime behavior.

| Type constructor | Meaning in the current plane | `.dl6` | TypeScript | Rust | JSON Schema 2020-12 | OpenAPI 3.1 | TypeSpec |
|---|---|---|---|---|---|---|---|
| `int` | SQLite-range signed integer; arrival checks reject values outside signed 64-bit | `int` | `number` plus generated safe-integer/range validation; `bigint` changes JSON representation | `i64` | `{ "type": "integer", "minimum": -9223372036854775808, "maximum": 9223372036854775807 }`; current emitter omits bounds | Same schema; integer `format: int64` is advisory | `int64` |
| `float` | Finite IEEE-754 double; integer arrivals widen under REAL affinity | `float` | `number`, with finite-value validation | `f64` | `{ "type": "number" }`; JSON itself excludes NaN and infinities | `{ "type": "number", "format": "double" }` | `float64` |
| `text` | Prolog atom or string at ingress, stored as SQLite TEXT | `text` | `string` | `String` | `{ "type": "string" }` | `{ "type": "string" }` | `string` |
| `bool` | Two boolean literals | `bool` | `boolean` | `bool` | `{ "type": "boolean" }` | `{ "type": "boolean" }` | `boolean` |
| `json` | Arbitrary canonical JSON document, including `none` as JSON null | `json` | `JsonValue` recursive alias or `unknown`; `any` loses checking | `serde_json::Value` | `{}` | `{}` | `unknown` or a recursive JSON alias; no builtin exact JSON-value type |
| named relation record | Closed object with declared column order and named nested-record references; canonical JSON keys are sorted | `rel span(start: int, text: text).`; column `span` | `interface Span { start: number; text: string }`; structural assignability accepts extra compatible objects unless checked at boundary | `struct Span { start: i64, text: String }`; explicit field types required | object, `properties`, `required`, `additionalProperties: false`, `$ref` | component schema plus `$ref` | `model Span { start: int64; text: string; }` |
| payload enum | Closed variants, each with zero or more named payload fields; lowered to variant rels plus an id/tag rel | `rel body(page(view: view) ; redirect(to: text)).` | discriminated union needs a generated tag field; an untagged structural union cannot preserve variant identity for overlapping payloads | `enum Body { Page { view: View }, Redirect { to: String } }` | `oneOf` objects with required discriminator/variant fields; current emitter has no enum row kind | `oneOf` plus `discriminator`; current emitter has no enum row kind | named `union` of variant models; a decorator/convention is needed for wire tags |
| `option(T)` / postfix `?` | `none` atom represents absence and serializes as JSON null; expansion mints companion relations | `option(text)` or `text?` | `T | null`; `?: T` represents absence instead, and `T | null | undefined` combines both | `Option<T>`; serde attributes decide missing versus null behavior | `anyOf: [T, {type: null}]`; property is omitted from `required` by current emitter, so it admits missing and null | Same as JSON Schema 2020-12 | `T | null`; optional property `p?: T` is a separate absence dimension |
| `json_list(T)` | Inline JSON-array carrier with recursively checked scalar/JSON-list elements; relation elements rejected | `json_list(text)` | `T[]` | `Vec<T>` | `{ "type": "array", "items": T }` | Same | `T[]` |
| `list(T)` | Relational indexed sequence: generated list entity and `(list_id, index, value)` member relation | `list(text)` | `T[]` carries values and order but loses entity id and relational membership; exact counterpart requires generated wrapper records | `Vec<T>` has the same loss; exact counterpart requires generated wrapper structs | Array loses entity identity; normalized object graph can preserve it | Same | Array loses entity identity; models can encode generated artifacts |
| `list_entity_dense_sequence(T)` | Relational sequence plus owner and refcount relations | `list_entity_dense_sequence(text)` | No counterpart. `T[]` has no owner/refcount identity | No counterpart. `Vec<T>` has no owner/refcount identity | No counterpart as an array. Generated object models can expose implementation records | Same | No counterpart as an array. Generated models can expose implementation records |
| `list_interned_set(T)` | Relational set whose values live in an intern dictionary and members reference value ids | `list_interned_set(text)` | No counterpart. `Set<T>` has uniqueness but no intern dictionary identity or JSON-native representation | No counterpart. `HashSet<T>` has uniqueness but no intern ids and adds hash/equality bounds | `uniqueItems: true` expresses equality-based uniqueness, not interning or value ids | Same | No counterpart. Array constraints do not carry intern identity |
| `list_entity_linked_sequence(T)` | Relational sequence with member identity and explicit link edges | `list_entity_linked_sequence(text)` | No counterpart. `T[]` loses member ids and link topology | No counterpart. Linked-list libraries still do not expose this relational identity contract | No counterpart as an array. Generated node/link object models can preserve it | Same | No counterpart as an array. Generated node/link models can preserve it |

Constructor receipts: scalar storage is `v6/prolog/0_type_plane.pl:77-131`; `json_list` element closure is `:105-143`; wrapper inventory is `:145-151`; named records and references are `:64-75`, `:171-178`; enum surface and payload restriction are `v6/prolog/compile/parse_dl_dcg.pl:526-534`; enum lowering is `v6/prolog/0_enum_expand.pl:122-196`; collection artifacts are `v6/prolog/0_generic_expand.pl:125-176`.

### Composition and translation cracks

| Case | Current meaning | Translation requirement |
|---|---|---|
| `option(list(T))` | Optional list entity/reference | TS needs `(T[] | null)` plus entity metadata if identity matters; Rust needs `Option<Vec<T>>` plus metadata |
| `list(option(T))` | Present list whose elements may be `none` | TS `Array<T | null>`; Rust `Vec<Option<T>>`; schema `items` contains the null union |
| Null versus absence | `none` serializes as JSON null (`v6/prolog/0_type_plane.pl:709`) | Current JSON Schema emitter also removes option properties from `required` (`v6/prolog/compile/4_emit_jsonschema.pl:120-132`), admitting two wire states for one IR state |
| Integer width | Plane gate is signed 64-bit (`v6/prolog/0_type_plane.pl:513-545`) | TS `number` cannot exactly represent every i64; TypeSpec/Rust/protobuf can name width; JSON Schema needs numeric bounds and consumers that honor them |
| Structural versus nominal identity | A named relation is an identity-bearing type and nested values are interned | TS interfaces and JSON Schema validate shape; neither preserves nominal or intern identity without tags/ids |
| Enum tagging | Variant identity exists in generated tag relations | Rust can retain it directly; TS, JSON Schema, OpenAPI, and TypeSpec require one selected discriminator convention |
| Generic inference | `.dl6` has only unary closed wrappers; `json_list` has a closed element set | Rust output must spell type parameters and concrete generic arguments; TS commonly infers them from use sites, which is unavailable in declaration-only generation |

## Build-versus-buy

Research pending in deliverable 2.

## Two populations

Pricing pending in deliverable 3.

## Input side

Pricing pending in deliverable 4.

## Language-design forks

Forks pending in deliverable 5.

## Decisions

No language-design fork is selected in this recon.

## Verification

| Check | Command or receipt | State |
|---|---|---|
| Base | `git log --oneline -1` | `9d5cd3d3` |
| Owned paths only | `git status --short` | Run before every commit |
| Constructor inventory | Parser, type plane, enum expansion, generic expansion receipts above | Complete |
| External candidate facts | Primary documentation and project license files | Pending deliverable 2 |
| Plan index | `dl examples/gen-plans-index.dl --check` | Run after final todo set |

## Staffing

| Item | Value |
|---|---|
| Work | Recon and two documentation artifacts only |
| Agent | Codex, single lane; subagents prohibited by brief |
| Worktree | `plans/type-ir-polyglot-recon` |
| Base SHA | `9d5cd3d3` |
| Suite budget | Documentation checks, link/receipt checks, plan-index rail; zero production tests required |

