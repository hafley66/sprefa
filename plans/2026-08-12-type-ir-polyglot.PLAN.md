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

### Candidate matrix

| Candidate | Direction and covered targets | Type coverage and loss against this IR | License | Can Prolog drive it? | “Tree-sitter of type systems” test |
|---|---|---|---|---|---|
| [TypeSpec](https://typespec.io/docs/) | TypeSpec source to OpenAPI, JSON Schema, protobuf and target-language emitters; import is centered on OpenAPI rather than arbitrary language syntax | Models, scalar widths, arrays, nullable unions, named unions, enums and decorators cover the portable rows. Relational list identity, intern dictionaries, owner/refcount and explicit links have no counterpart unless modeled as visible helper records. Missing versus null remains separate. | [MIT](https://github.com/microsoft/typespec/blob/main/LICENSE) | Yes. A Prolog emitter can write `.tsp`, then Node-based `tsp compile` runs chosen emitters. Making TypeSpec authoritative adds npm/TypeSpec compiler inputs. | Fixed TypeSpec compiler model plus handwritten emitters. Alloy supplies reusable output components, but each target language still has an emitter implementation ([emitter framework](https://typespec.io/docs/extending-typespec/emitter-framework/)). |
| [JSON Schema 2020-12 + quicktype](https://github.com/glideapps/quicktype) | JSON, JSON Schema, TypeScript and GraphQL inputs to more than 20 language renderers, including Rust, TypeScript, Go and Python | Records, arrays, primitive unions, null and JSON fit. i64 intent depends on bounds/renderer choices. Payload-enum tags must be encoded in schema. Relational collection semantics have no schema counterpart except exposed helper objects. Generated types follow JSON representation rather than storage identity. | [Apache-2.0](https://github.com/glideapps/quicktype/blob/master/LICENSE) | Yes. The existing Prolog JSON Schema emitter is the direct input, followed by the Node CLI/library. Current emitter must first retain enum rows, i64 bounds and collection policy. | Fixed quicktype type graph plus handwritten language renderers. Inputs and outputs are plugins in code, not declarative type-system descriptions. |
| [Protocol Buffers](https://protobuf.dev/programming-guides/editions/) | `.proto` source to official C++, C#, Java, Kotlin, Objective-C, PHP, Python, Ruby and Rust code; other plugins add targets | Exact integer widths, floats, strings, bools, messages, enums, repeated fields, maps, `oneof`, explicit presence. Arbitrary JSON needs `Value`/`Any`; null is not a normal scalar; repeated fields cannot appear directly in `oneof`; relational list identities require helper messages. Field numbers and wire evolution become new required state. | [BSD-3-Clause](https://github.com/protocolbuffers/protobuf/blob/main/LICENSE) | Yes. Prolog can emit `.proto`, then `protoc` and target plugins run. It introduces protobuf wire/runtime types even if only declarations are wanted. | Fixed descriptor model plus code plugins. A target plugin interprets descriptors in code; source languages do not supply declarative type-system grammars. |
| [Cap'n Proto](https://capnproto.org/language.html) | `.capnp` schema to C++ and compiler-plugin targets such as Rust, Go, Java and others | Sized integers, floats, text/data, structs, enums, unions, lists and generics fit portable data. Null/absence uses default/pointer semantics rather than the plane's single `none` atom. Arbitrary JSON and all four relational collection contracts need helper structs or lose meaning. Ordinals and schema IDs become required state. | [MIT](https://github.com/capnproto/capnproto/blob/master/LICENSE) | Yes. Prolog emits schema text and invokes `capnp compile`; the compiler is a C++ binary and target support comes from plugins. | Fixed schema node model plus compiler plugins ([tool documentation](https://capnproto.org/capnp-tool.html)). Each backend is code. |
| [Smithy 2.0](https://smithy.io/2.0/spec/) | Smithy models to clients, servers, docs and language-specific SDKs; OpenAPI conversion exists | Structures, unions, enums, lists, maps, scalar widths, traits and services cover portable declarations. Smithy `document` covers arbitrary JSON. Nullable versus missing and relational collection identities need traits/helper shapes and backend conventions. | [Apache-2.0](https://github.com/smithy-lang/smithy/blob/main/LICENSE) | Yes. Prolog can emit Smithy IDL/JSON AST and invoke smithy-build. Custom generators are normally JVM code and use `SymbolProvider`/writers ([generator guide](https://smithy.io/2.0/guides/building-codegen/implementing-the-generator.html)). | Fixed Smithy semantic model plus handwritten generators. Traits extend metadata, not the host and target type-system grammar declaratively. |
| [ts-rs](https://github.com/Aleph-Alpha/ts-rs) | Annotated Rust structs/enums to TypeScript declarations | Rust structs, enums, generics, options and serde naming/tag attributes map to TS. Rust is authoritative. It cannot ingest `.dl6`, TS, JSON Schema or OpenAPI; it cannot preserve runtime collection identity unless those artifacts are first represented as Rust types. i64 still lands in TS number unless overridden. | [MIT](https://github.com/Aleph-Alpha/ts-rs/blob/main/LICENSE) | Indirect only. Prolog would first emit and compile Rust types with derives, then run export code/tests. | One Rust input frontend and one TS backend implemented by proc macros. No per-type-system declaration format. |
| [Specta](https://docs.rs/specta/latest/specta/) | Rust type introspection to exporters; documented stable output is TypeScript | Structs, payload enums, generics, option and serde representation metadata fit Rust-authored values. The documented crate says only TypeScript is currently supported. No `.dl6`/schema input; storage-specific list meanings need Rust helper types and custom exporter treatment. | [MIT](https://github.com/specta-rs/specta/blob/main/LICENSE) | Indirect only. Prolog must emit compilable Rust with `Type` derives, then execute a Rust exporter. | Fixed Rust type model plus coded exporters. No declarative target-language description. |
| [Schemars](https://docs.rs/schemars/latest/schemars/) | Rust types deriving `JsonSchema` to JSON Schema | Serde-shaped records, enums, options, arrays/maps and validation metadata fit. Rust is the only source. Schema describes serialized shape, so nominal identity and relational collection machinery disappear unless helper structs are public. Output schema shape may change without a semver break. | [MIT](https://github.com/GREsau/schemars/blob/master/LICENSE) | Indirect only. Prolog must generate Rust and compile derives, which duplicates the direct Prolog schema emitter. | One Rust frontend and one JSON Schema backend encoded in traits/macros. |
| [Typeshare](https://github.com/1Password/typeshare) | Annotated Rust source parsed to Kotlin, Swift, Scala, TypeScript and Go declarations | Structs, enums, options and common containers fit its Rust/serde-oriented subset. It does not ingest schemas or `.dl6`. Language-specific unsupported Rust shapes, i64 target differences, arbitrary JSON and relational list contracts require annotations or helper types. | [MIT](https://github.com/1Password/typeshare/blob/main/LICENSE) | Indirect only. Prolog would emit annotated Rust source, then invoke the Rust CLI. Rust syntax becomes an intermediate IDL. | Fixed Rust-source parser and handwritten backends. |
| [serde-reflection](https://github.com/zefchain/serde-reflection) | Traces Rust Serde formats into a registry; `serde-generate` consumes that registry for C++, Dart, Go, Java, Python, Rust, TypeScript and others | Structs, enums, tuples, options, sequences and maps within Serde's data model fit. Tracing may require samples for formats whose shape depends on values. Arbitrary self-describing JSON, custom serializers, skipped/defaulted behavior and relational collection identity are outside or require adapters/helper structs. | [Apache-2.0](https://github.com/zefchain/serde-reflection/blob/master/LICENSE) | Indirect only. Prolog would generate Rust Serde types or generate the registry format itself, then use the Rust toolchain. Registry compatibility becomes an integration contract. | Fixed Serde data model plus handwritten generators. It separates registry from backends, but language type systems are not declarative inputs. |
| [OpenAPI Generator](https://openapi-generator.tech/docs/generators/) | OpenAPI documents to client/server/model generators for a large language list | Reuses OpenAPI schema coverage: records, arrays, primitives, nullable unions and discriminated schemas. Generator-specific mappings vary. It cannot recover enum/storage meaning already erased by the current emitter; relational lists need explicit component models. | [Apache-2.0](https://github.com/OpenAPITools/openapi-generator/blob/master/LICENSE) | Yes. Existing Prolog OpenAPI output can feed the JVM CLI after schema fidelity is widened. | Fixed OpenAPI model plus handwritten templates/generator code for each target. Template configuration changes spelling, not the target type system's semantic mapping. |
| [Typify](https://github.com/oxidecomputer/typify) | JSON Schema to Rust types, builders and conversion code | Directly covers the current schema emitter's portable output. It provides one Rust target only. Schema cannot carry relational identity unless helpers are exposed; null/absence and enum tagging are constrained by input schema fidelity. | [Apache-2.0](https://github.com/oxidecomputer/typify/blob/main/LICENSE) | Yes. Prolog emits JSON Schema; Rust build script, macro, or CLI generates Rust. | Fixed JSON Schema input and one coded Rust backend. |
| [json-schema-to-typescript](https://github.com/bcherny/json-schema-to-typescript) | JSON Schema to TypeScript declarations | Records, arrays, unions, enums, refs and optional properties fit JSON-shaped types. Integer width becomes `number`; runtime validation and nominal/relational identity are absent. | [MIT](https://github.com/bcherny/json-schema-to-typescript/blob/master/LICENSE) | Yes. Existing schema output can feed its Node API/CLI once missing IR distinctions are encoded. | Fixed JSON Schema input and one coded TypeScript backend. |

### Candidate architecture comparison

| Architecture | Candidates | New source-language cost | New target-language cost |
|---|---|---|---|
| Neutral IDL as authority | TypeSpec, protobuf, Cap'n Proto, Smithy | Write/import into the fixed IDL model | Implement or configure a backend/plugin |
| Existing JSON shape as interchange | JSON Schema + quicktype, OpenAPI Generator, Typify, json-schema-to-typescript | Emit JSON Schema/OpenAPI, already partly built | Select an existing generator or implement its backend |
| Rust as authority | ts-rs, Specta, Schemars, Typeshare, serde-reflection | Express every source in Rust first | Use each tool's supported coded exporters |

No candidate implements `type-system-description(Language) -> parser + IR mapping + printer` as data. The nearest structural match is a fixed neutral model with plugins: TypeSpec emitters, Smithy generators, protobuf/Cap'n Proto compiler plugins, quicktype renderers, or serde-reflection plus serde-generate. Their growth curve is N input adapters plus M output backends around one fixed model. The adapters and backends remain programs.

### Directly usable pieces

| Existing seam | Candidate path it enables | Required fidelity work before a credible trial |
|---|---|---|
| `jsonschema_text/3` | quicktype, Typify, json-schema-to-typescript | i64 bounds, payload-enum representation, null-versus-missing ruling, explicit policy for four relational list constructors |
| `openapi_document/3` | OpenAPI Generator, TypeSpec OpenAPI import | Same schema work; emitted operations currently carry no response bodies (`v6/prolog/compile/5_emit_openapi.pl:59-74`) |
| Prolog text emitter pattern | TypeSpec, protobuf, Cap'n Proto, Smithy | One new emitter plus golden fixtures; each IDL introduces identifiers, reserved words and evolution metadata |
| Rust handwritten surfaces | ts-rs, Specta, Schemars, Typeshare, serde-reflection | Establish Rust as authority or accept generated intermediate Rust before any tool can run |

## Two populations

Program-derived types deliver value first. Their authority and traversal input already exist in `Plan`, `Types`, `RelPlans`, and catalog rows (`v6/prolog/sweep.pl:103-121`). The Rust-SQLite emitter can consume generated row structs without first migrating library types.

| Population | Authority today | Measured current surface | First useful output | Implementation price | Migration price | Forced later work |
|---|---|---:|---|---|---|---|
| Program-derived relation types | `.dl6` declarations, inferred relation plan, shared type table and lowered plan | Type plane 920 lines; JSON Schema emitter 176; OpenAPI emitter 103; these are compiler inputs/precedent rather than duplicated target types | Rust structs/enums for one compiled `.dl6`, beside `4_emit_jsonschema.pl` and `5_emit_openapi.pl` | 1 emitter file, about 250-450 Prolog lines; 1 test/golden file, about 200-350 lines; 10-25 focused fixtures. Add 50-120 lines if invoking a bought schema-to-Rust tool instead of rendering Rust in Prolog. | 0 existing handwritten type lines must move for the first output. The consuming Rust-SQLite lane changes its generated artifact boundary. | Reserved-word maps, stable naming, enum tag convention, null/absence ruling, i64 policy, and explicit generated representations for relational list flavors |
| Handwritten library types | No single authority; Rust and TypeScript declarations are maintained in their respective packages | 4,249 lines across 5 measured files | One selected library model emitted compatibly into both Rust and TypeScript | IDL and generator integration: 2-4 config/schema entry files plus about 300-700 lines of adapters, annotations, or generator configuration. Authoritative schema size is proportional to the migrated declarations and is expected to replace, not add to, the 4,249 maintained lines. | Review and migrate 5 type files plus every import, constructor, narrowing site, serde/JSON boundary, and test snapshot that depends on their exact shapes. Budget as a separate multi-commit arc; line count cannot be priced from declaration files alone because call-site count has not been inventoried in this recon. | Generated-file ownership, package publishing/order, compatibility shims during migration, custom-code escape hatches, and one authority for serde tags/defaults and TS runtime validation |

### Program-derived sequence

| Step | Read | Write | Uniqueness condition |
|---|---|---|---|
| 1. Collect declarations | `Plan = plan(..., Types, RelPlans, ...)` and catalog rows, matching `sweep_one/5` at `v6/prolog/sweep.pl:103-121` | Ordered language-neutral declarations in memory | One declaration per fully qualified relation path; catalog row ids are lookup details, not emitted names |
| 2. Classify constructors | `kind_schema/7` precedent and the unexpanded enum/list metadata | Target-neutral scalar, option, record, enum and collection cases | Every constructor must select one case; no default-to-JSON for target-language emission |
| 3. Allocate names | Module path, local relation name, field/variant names | Target-safe type and member identifiers | Collision table includes case folding, reserved words and generated helper suffixes |
| 4. Render or pass to generator | Classified declarations | One output module/file set | Deterministic topological order; named references resolve once |
| 5. Verify | Golden fixture matrix | Snapshots and target compiler checks | Same IR produces byte-stable output and target compiler accepts it |

### Handwritten-library sequence

| Step | Read | Write | Uniqueness condition |
|---|---|---|---|
| 1. Inventory declarations and consumers | Five measured files plus imports and construction/narrowing sites | Migration manifest | Every exported type has one owner and every duplicate pair has an explicit correspondence |
| 2. Select authority | External IDL, Rust, or expanded `.dl6` type plane, by human ruling | Canonical declarations | One authority per migrated type; generated outputs are never edited |
| 3. Prove representation parity | Serde attributes, TS discriminants, optional fields, branded ids, recursive types | Golden JSON examples and compile fixtures | Each wire shape has one canonical encoding and decoding rule |
| 4. Generate side by side | Canonical declarations | New Rust and TS artifacts | Generated names cannot collide with retained handwritten types |
| 5. Migrate consumers in slices | Old and generated surfaces | Imports/call sites | One package boundary at a time; compatibility adapters have deletion receipts |
| 6. Remove duplicates | Migration manifest | Reduced handwritten surfaces | Removal occurs only after reference count reaches zero and snapshots agree |

The 4,249-line count is declaration-surface size, not migration size. A full price requires reference counts and serialization-boundary counts, which belong after the authority fork is ruled.

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
| External candidate facts | Linked primary documentation and project license files | Complete |
| Population line counts | `wc -l` over the five named handwritten files | 4,249 |
| Plan index | `dl examples/gen-plans-index.dl --check` | Run after final todo set |

## Staffing

| Item | Value |
|---|---|
| Work | Recon and two documentation artifacts only |
| Agent | Codex, single lane; subagents prohibited by brief |
| Worktree | `plans/type-ir-polyglot-recon` |
| Base SHA | `9d5cd3d3` |
| Suite budget | Documentation checks, link/receipt checks, plan-index rail; zero production tests required |
