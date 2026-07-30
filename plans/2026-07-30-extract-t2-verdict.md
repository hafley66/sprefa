# Extract tier-2 (schema reification) lab — verdict

Contract: [2026-07-30-extract-t2-lab-header.md](2026-07-30-extract-t2-lab-header.md).
Base sha `3de711f3`. Five reifiers, five formats, both doors, byte-diffed.

**Headline.** The algebra holds, and it holds on real documents rather than
worked examples: five reifiers over five vendored schema files derive **951
type facts from 19 world-fed rows**, and every one of those facts is
byte-identical on the reference engine and the served tsv2 engine. The cross-repo join is real — three repos,
one of them serving the actual Swagger Petstore contract, joined into
`calls_shape` plus two lints that catch an undeclared cross-repo dependency and
a contract reference that resolves nowhere.

What the algebra did NOT survive is one shape it has to meet constantly: a
schema slot that holds a string in one field and an object in the next. Avro's
`type`, and every format's equivalent, is heterogeneous, and **there is no
column type under which a `decode/2` hole over heterogeneous values is correct
on both doors** — `text` renders differently on each door, `json` silently drops
every scalar row. That is finding D2, it is the sharpest thing in this lab, and
the workaround (one hole, one kind) is a real discipline the surface does not
enforce or mention.

Nothing here is wired. Design record plus a fixture-promotion list.

---

## 1. Build-vs-buy, before any line was written

The question per format is narrow: **can an existing tool hand us this schema as
a JSON DOCUMENT, which the json plane already eats, or would we have to write a
parser?** A bespoke parser for any of these formats is the outcome to avoid.

| candidate | what it emits | exact invocation | JSON out? | licence | install cost | adoption | verdict |
|---|---|---|---|---|---|---|---|
| **avro `.avsc`** | the schema itself | none — read the file | **already JSON** | Apache-2.0 | zero | avro 3.3k stars | **BOUGHT, at zero cost.** The best case in the survey: the repo's own checked-in file is the document. Confirmed against the Avro spec and by vendoring Apache's own `interop.avsc` and validating it. |
| **openapi / json-schema (JSON form)** | the schema itself | none | **already JSON** | per document | zero | — | **BOUGHT, at zero cost.** Petstore fetched and used unmodified. |
| **openapi (YAML form)** | the schema | `js-yaml` `yaml.load` then `JSON.stringify`, 3 lines | yes | MIT | one pure-JS dep | 279M weekly npm downloads, the highest number in the survey | **BOUGHT for the YAML half.** `yq` (MIT, Go binary) and `@redocly/cli` (MIT, 2.0M/wk) both do it too; redocly and `swagger-cli` are full lint/bundle toolchains, so they are heavier than the one call this needs. Not exercised here — the vendored openapi corpus is already JSON. |
| **protobufjs `pbjs -t json`** | protobufjs's own reflection-JSON tree of a `.proto` | `npx -p protobufjs-cli@1.1.3 pbjs -t json x.proto` | **yes** | protobufjs MIT, `protobufjs-cli` BSD-3-Clause | pure JS, no C++/Go toolchain | protobufjs 75.5M weekly, cli 1.5M weekly | **BOUGHT.** Verified here by generating four descriptors, including one from protoc's own 58,877-byte `descriptor.proto`. The output is a message → `fields` → `type`/`id` tree that `**` descent walks directly. Not the standard `FileDescriptorSet` shape, which costs nothing here and would matter to anyone diffing against `descriptor.proto`'s own field numbers. |
| **protoc `--descriptor_set_out`** | a **binary** `FileDescriptorSet` | `protoc --descriptor_set_out=x.pb x.proto` | **no** | BSD-3-Clause | 17.9 MB brew formula | 71.7k stars | **REJECTED for this job, on a measured fact rather than a preference.** `protoc --help` carries no JSON flag of any kind; `--decode` and `--decode_raw` both emit protobuf TEXT format (bare keys, no commas, unquoted enum names), not JSON. So protoc alone cannot close the gap, and pairing it with a binary decoder is strictly more machinery than pbjs. protoc is still used here — it is where the vendored `.proto` sources came from. |
| **buf** | `FileDescriptorSet`, format chosen by extension | `buf build -o descriptor.json` | documented yes | Apache-2.0 | Go binary | 11.3k stars | **Priced, NOT verified.** Buf's own CLI docs list `binpb\|json\|txtpb\|yaml` for `-o`, plus `--as-file-descriptor-set`. Not installed, not run, so this row is documentation and is marked as such. Worth re-testing if the standard descriptor shape ever matters, since buf produces it natively. |
| **graphql-js** | introspection JSON from SDL | `buildSchema(sdl)` then `introspectionFromSchema(schema)` | **yes** | MIT | one pure-JS dep | 44.6M weekly | **BOUGHT.** Measured here: swapi's real 35,868-byte SDL → 110,822-byte introspection, **3.09x, 15 ms**. That expansion is the whole content of `slot_graphql_entry`. |
| `get-graphql-schema`, `@graphql-inspector/cli` | introspection JSON | HTTP introspection query | yes | MIT | one dep | 109k / 397k weekly | **REJECTED for this job.** Both introspect a **running server**. A parse-only pass over checked-out repos has no server, which is the same reason grpc reflection is out. |
| **quicktype** | JSON Schema, from json / typescript / graphql / postman | `quicktype --src-lang json --lang schema in.json` | yes | Apache-2.0 | pure JS | 317k weekly (`quicktype`), 573k (`quicktype-core`) | **REJECTED as the normalizer, with the reason measured.** Two independent problems. (1) Its IR is not the library surface: `quicktype-core` exports `Type`/`ClassType`/`UnionType`/`TypeBuilder`, but the `TypeGraph` class itself is not re-exported, so "consume the IR" is not on offer — only render targets are. (2) More decisive: for `graphql` and `postman` sources quicktype does **sample-shape inference**, not type extraction. Pointed at a GraphQL schema it produces a schema shaped like one query's `{data, errors}` response envelope, not the type catalogue. For a t2 pass that wants the *declared* surface, inference is the wrong instrument no matter how good it is. The `typescript` source path IS true type extraction and is the one arm worth revisiting if TS-declared contracts ever enter scope. |
| `@apidevtools/json-schema-ref-parser` | `$ref`-resolved schema | `.dereference(path)` / `.bundle(path)` | yes | MIT | one dep | 15.4M weekly | **REJECTED, and this one produced a finding.** `dereference()` on an ordinary self-referential schema (`Comment { replies: Comment[] }` — trees, threads, org charts) returns a genuine JS reference cycle: it throws no error, and then `JSON.stringify` on the result throws `TypeError: Converting circular structure to JSON`. `bundle()` on the same input round-trips fine because it leaves internal pointers alone. So **`$ref` should be reified as a named-reference FACT and never dereferenced** — which is what the reifiers here do, and it is cheaper and safer than the alternative. |
| a bespoke parser for any format | — | — | — | — | — | — | **Refused.** Every format in scope is reachable as JSON through a tool that already exists, four of them at literally zero conversion cost. |

**Per-format verdict, the row the header asked for:**

| format | route | consumed |
|---|---|---|
| avro `.avsc` | none | **natively by the json plane** |
| openapi / json-schema, JSON form | none | **natively by the json plane** |
| openapi, YAML form | `js-yaml`, 3 lines | via one existing tool's JSON output |
| protobuf | `pbjs -t json` | via one existing tool's JSON output |
| graphql | graphql-js introspection | via one existing tool's JSON output |
| avro IDL `.avdl` | `avro-tools idl2schemata` (JVM) | via one existing tool — **priced, not run** (needs a JVM + jar download) |
| any of them | — | **bespoke parser: never needed** |

---

## 2. The corpus

Everything vendored under `v6/prolog/labs/extract_t2/corpus/`, byte-unmodified,
regenerable by `corpus/regen.sh` (the only script that touches the network).

| file | bytes | source |
|---|---|---|
| `openapi-petstore.json` | 17,106 | `https://petstore3.swagger.io/api/v3/openapi.json`, live, OpenAPI 3.0.4 |
| `avro-interop.avsc` | 1,238 | `apache/avro` `share/test/schemas/interop.avsc` |
| `struct.proto` | 4,317 | protoc 35.1's own `google/protobuf/struct.proto` |
| `descriptor.proto` | 58,877 | protoc 35.1's own `google/protobuf/descriptor.proto` |
| `graphql-swapi.graphql` | 35,868 | `graphql/swapi-graphql` `schema.graphql` |
| `xrepo/pet-contracts/openapi.json` | 17,106 | the Petstore again, as the repo that SERVES it |
| `xrepo/{pet-dashboard,pet-billing}/**` | small | authored for the lab, and said so: two consumer repos with real-shaped `package.json` files and OpenAPI documents that reference pet-contracts by external `$ref` |

Derived, by the bought tools: `proto-struct.json` (2,205 B),
`proto-descriptor.json` (44,996 B), `proto-repo-{contracts,consumer}.json`,
`graphql-swapi-introspection.json` (110,822 B).

The corpus was chosen so that **no single document could carry the verdict**.
Petstore has records, optionality, `$ref`, arrays, inline enums and formats but
no map and no union; `struct.proto` has a map, a oneof, a repeated field and a
named enum; `interop.avsc` has bytes, null, fixed, float beside double, int
beside long, a union, a map, and a record that refers to itself. The claimed
gaps are all present as real data somewhere.

---

## 3. Q1 — does the algebra hold?

**Yes, on the type plane, for all five formats.** Each row of the claimed
algebra was reified from at least one real document and graded.

| algebra row | claimed mapping | verdict | where it was proven |
|---|---|---|---|
| record | struct_as_rows | **HOLDS** | `type_def(kind 'record')` + `field_def` rows, all five formats |
| enum | variant rels | **HOLDS, with a naming decision** | `type_def(kind 'enum')` + `enum_variant`. proto and graphql enums are NAMED; **an OpenAPI enum has no name at all** — it is an anonymous member of one property, so a name has to be MINTED (`Pet.status`). That minting is a decision, not a translation. Finding A2. |
| union | variant rels | **HOLDS** | proto `oneof` → `type_def(kind 'union')` + `union_member`; avro's bare-array union → `field_union_member`. graphql `UNION` reified; swapi declares none, so that arm is covered but unexercised. |
| optional | ABSENCE, no flag column | **HOLDS, and is the format's own reading twice over** | OpenAPI `required: [...]` spreads into `field_required` and optionality is its absence. proto3 singular fields carry no presence marker at all, so absence is already the format's default. graphql inverts it (`NON_NULL` is the marker) and lands in the same rel. |
| repeated | its own row | **HOLDS** | `field_repeated`, all four formats that have it, including graphql's `LIST` wrapper |
| map | "literally a rel" | **HOLDS** | proto `keyType` → `field_map(.., key_prim, value_type)`; avro `{"type":"map"}` → `field_map`. A four-column rel, exactly as claimed. |
| named-ref | `ref(Type)` | **HOLDS, but not the way the header assumed** | see A3 below: it is a JOIN, not a column type, because dl6 cannot take a JSON Pointer apart |
| primitive | `field_prim` | **HOLDS** | every scalar in all five formats, including avro's `bytes` and `null` |

### Construct census, walked from the source rather than from the rules

Every key occurring inside the Petstore's `components.schemas`, classified by
whether any rule reads it:

```
type        33 covered      example       16 HOLE
format      10 covered      xml            9 HOLE
properties   6 covered      name           7 HOLE
enum         2 covered      description    3 HOLE
$ref         2 covered      wrapped        2 HOLE
items        2 covered
required     1 covered
                            covered 56 / 93 key occurrences, 5 constructs unread
```

**All five holes are annotations, not type constructs**: `example`,
`description`, and the `xml`/`name`/`wrapped` serialization triple. There is no
TYPE construct in this document the algebra fails to reach. That is the
strongest form of the Q1 answer available, because it is produced by walking
the document, not by reading the rules.

### Named holes, on top of the census

| hole | what it is | slot |
|---|---|---|
| int width | Petstore carries `format: int64` and `format: int32` on different fields; dl6 has ONE `int`. The distinction is reified as data (`field_format`) and is **unrepresentable as a column type**. | `slot_int_width` |
| bytes | avro `bytes` and avro `fixed` (with a WIDTH: `MD5`, 16) both reify as facts; neither has a column type. | `slot_bytes_spelling` |
| float | avro carries `float` AND `double`; reified as data. `float` exists as a dl6 column type but the golden plan still records `avg()` as the one real hole. | (existing) |
| defaults | proto and avro both carry field defaults. Nothing reads them here. | `slot_defaults_residency` |
| nested anonymous types | an inline `{"type":"object"}` inside a property is a record the algebra has no NAME for. Excluded by rule in `openapi.dl6`; Petstore has none. | new: `slot_anonymous_nested_record` |
| avro union member that is itself an anonymous type | `interop.unionField` has three members, one of which is an anonymous array-of-bytes type. Two are reported; the third is **not**, and is recorded rather than papered. | rides `slot_anonymous_nested_record` |

---

## 4. Q2 — is the reifier a dl6 program, and does it grade on both doors?

**Yes and yes.** Five programs, zero TypeScript, zero bespoke parsing. Every
fact comes from `decode/2` over a `json` column plus ordinary joins, negation
and `concat`.

| program | document | bytes | deltas, both doors |
|---|---|---|---|
| `openapi.dl6` | Swagger Petstore | 17,106 | **91 IDENTICAL** |
| `proto.dl6` | `google/protobuf/struct.proto` | 2,205 | **39 IDENTICAL** |
| `avro.dl6` | apache/avro `interop.avsc` | 1,238 | **50 IDENTICAL** |
| `xrepo.dl6` | three-repo federation | 21,841 | **64 IDENTICAL** |
| `graphql.dl6` | swapi introspection | 110,822 | **725 IDENTICAL** (see below) |

The graphql row excludes ONE rel and says so on every run: the world-fed
`introspection` rel echoes the source document back, and that echo is the only
place json-flex **card C3** can bite — the reference engine's stand-in for json
null is the atom `none`, so it prints `"description":"none"` where the emitter
prints `"description":null`. **Every derived fact matches.** This is worth
reporting upward: C3 was filed as a priced card with no known real-world
trigger, and GraphQL introspection triggers it on essentially every type in
every schema (`description`, `ofType` and `defaultValue` are null nearly
everywhere). It is no longer hypothetical.

---

## 5. Q3 — the cross-repo join (the headline)

Three repos arrive as seven rows — each repo's OpenAPI contract and each repo's
`package.json`, as ordinary `json` columns, plus one row naming the file
pet-contracts publishes. Nothing else is fed; every fact below is derived.

- **pet-contracts** serves the shapes. Its `openapi.json` IS the real Swagger
  Petstore document, 17,106 bytes, unmodified.
- **pet-dashboard** uses four of them by external `$ref` and DECLARES the
  dependency. One of its four refs names a shape that does not exist.
- **pet-billing** uses two of them and DECLARES NO SUCH DEPENDENCY.

t2 (the type surface) meets t1 (the dependency edge) in `calls_shape`:

```
calls_shape          pet-billing   Invoice        subject        pet-contracts  Pet
calls_shape          pet-billing   Invoice        buyer          pet-contracts  User
calls_shape          pet-dashboard DashboardPanel featured       pet-contracts  Pet
calls_shape          pet-dashboard DashboardPanel owner          pet-contracts  User
calls_shape          pet-dashboard DashboardPanel recentOrders   pet-contracts  Order

undeclared_shape_dep pet-billing   pet-contracts  Pet
undeclared_shape_dep pet-billing   pet-contracts  User

dangling_shape       pet-dashboard DashboardPanel ghost
                     pet-contracts/openapi.json#/components/schemas/Invoice

depends_on           pet-dashboard pet-contracts
depends_on           pet-billing   decimal.js
```

Byte-identical on both doors. Three things this receipt actually establishes:

1. **The join is a join.** `calls_shape` correlates a `$ref` string in one
   repo's document against a type declared in another repo's document. Nothing
   is pre-resolved and nothing is dereferenced.
2. **Both lints are live, not decorative.** `undeclared_shape_dep` catches
   pet-billing using two shapes it never declared a dependency on — the actual
   thing a 800-repo federation wants to know. `dangling_shape` catches a
   reference nothing in the federation can answer, while correctly NOT flagging
   pet-dashboard's internal `#/components/schemas/PanelLayout`.
3. **The lint is not inert.** Sabotage (d): delete pet-dashboard's one declared
   dependency and `undeclared_shape_dep` goes 2 → 5, picking up exactly its
   three cross-repo uses.

**The honest scope line.** The t2 side is a real vendored contract. The t1 side
is three real-shaped `package.json` files, and the two consumer documents were
authored for this lab — the header sanctioned "toy repos" and these are toy
repos. What is NOT claimed: that these use sites were extracted from source
code. Joining t2 facts against tree-sitter-extracted call sites is the next
step, and it needs the phase-2 extraction host, not a new construct.

---

## 6. Q4 — the price

```
document                   bytes   facts  oracle_ms  served_ms  floor_ms    stmts
openapi petstore           17106      91        177         29        28       23
avro interop                1238      50        109         33        28       21
proto struct                2205      39        113         29        26       22
proto descriptor           44996     722        282         73        35       22
graphql swapi             110822     726        535         64        29       20
xrepo federation           21841      64        157         36        31       20
```

`oracle_ms` includes swipl process start, which the 1,238-byte row prices at
~109 ms; that is the floor, not the work. `served_ms` is the wall of the one
`POST /arrivals` carrying the document, and `floor_ms` is the same POST carrying
an empty batch — so the work is the difference, ~1 ms for the Petstore and
~35-40 ms for the two large documents.

**`stmts` is the count-test-law number**, and it is the important column. It is
the statement count of the COMPILED program, and it does not move with the
document: `proto.dl6` emits 22 statements against a 2,205-byte descriptor and
**the same 22** against a 44,996-byte one, deriving 39 facts and 722 facts
respectively. Reification cost is a property of the rules, not of the input.

**Projection to 800 repos.** At one schema document per repo:

| assumption | wall, single-threaded |
|---|---|
| every repo carries a large document (45-110 KB, ~40 ms) | **~32 s** |
| every repo carries a Petstore-sized document (17 KB) | **a few seconds**, dominated by process/HTTP overhead rather than reification |

Either way the fever dream is affordable: t2 over 800 repos is **seconds to
half a minute** on one machine, with no typechecker anywhere. The genuine cost
question is not reification, it is the 800 file reads and whatever conversion
step each format needs — `pbjs` and graphql-js are subprocess-per-repo today,
and graphql-js measured 15 ms for a 36 KB SDL, so even that is ~12 s at 800.

Not measured, and it is the honest gap: **incremental** cost. Every number here
is a cold single-document tick. What a re-tick costs when one repo's schema
changes out of 800 is the question the engine exists to answer, and this lab
did not ask it.

---

## 7. Q5 — fidelity, and the round trip that cannot be written

**Fidelity: EXACT.** The Petstore's `components.schemas` was rebuilt from the
algebra facts ALONE and diffed against the source restricted to the subset the
algebra claims (type, properties, required, `$ref`, enum, items, format):
**6 schemas, 27 properties, exact.**

**But the rebuild is in python, and that is the finding.** The header pointed at
the registry-driven openapi emitter as the precedent, and it is a good
precedent — for prolog. In dl6 the round trip cannot be written at all, because
**the compiler refuses both spellings that could construct a document**:

- a braces literal in value position →
  `unsupported_construct(json_value_expression({paths:{...}}))`
  (`compile/lower.pl:454`), hit on the very first smoke program of this lab;
- the aggregate `json_object/2` head → already a standing refusal.

The reference engine accepts both (`json_object_builds_document` and
`braces_in_head_position` are shipped fixtures). So document construction is
expressible on ONE door, which for a cross-target contract means it is not
expressible. **The json plane is read-only in compiled programs.**

That is a coherent position — t2 only ever reads schemas — but it should be a
stated one, because "reify a contract, then publish a merged contract" is an
obvious next ask and it is currently blocked, not hard.

---

## 8. Findings

Ordered by how much a decision changes.

### D2 — one hole, one kind. A heterogeneous `decode` hole is unrepresentable.

The sharpest finding, and it is not an edge case: a schema's type slot is
heterogeneous BY DESIGN. Avro's `type` holds `"int"` for a primitive, an object
for an array/map/enum/fixed/nested record, and an array for a union.

`avro-heterogeneous.dl6` binds that slot to one variable. Measured, on a
four-value probe (`hole_text.dl6`, `hole_json.dl6`, `corpus/hole-probe.json`):

| landing column | scalar string | number | object | array |
|---|---|---|---|---|
| `text` | agrees | **oracle `16`, emitter `"16"`** | **oracle `{...}`, emitter `"{...}"`** | **oracle `[...]`, emitter `"[...]"`** |
| `json` | **row VANISHES on the emitter** | agrees | agrees | agrees |

Two distinct mechanisms, both worth naming:

- `text`: the emitted SQL coerces every bound value to the declared column type;
  the reference engine keeps the json value's own type. Three of four rows
  differ. On the full avro program, 8 of 17 rows differ.
- `json`: the emitted table is
  `"value" TEXT NOT NULL CHECK (json_valid("value"))` and the write is
  `INSERT OR IGNORE ... SELECT j0.key, j0.value FROM "doc" b0, json_each(...)`.
  `json_each`'s `value` column hands back the DECODED scalar, so the JSON string
  `"int"` arrives as the bare text `int`, `json_valid('int')` is 0, the CHECK
  fails — **and `INSERT OR IGNORE` turns that into a dropped row with no error,
  no refusal and no diagnostic.** Receipt: the reference engine derives 1 row,
  the served engine derives 0 and the serve log is clean.

**There is no column type that is correct.** The working discipline is *one
hole, one kind*: make every hole homogeneous by putting a JOIN in the same rule
as the decode, so a structured value is filtered before it is ever written. In
`avro.dl6` the discriminator is `avro_primitive(Token)` — avro's primitive names
are a closed normative set, fed as ordinary EDB rows — and the type table for
named refs. With that discipline avro grades 50 deltas IDENTICAL.

`openapi.dl6` and `proto.dl6` never hit this only because their type slots are
homogeneous by the format. Nothing warned; the surface has no way to say "this
hole binds text".

- A: refuse a `decode` hole whose head column is `text` when the bound value is
  structured, naming rel and column. Decidable at runtime, cheap, loud.
- B: make the reference engine apply the declared column type (canonicalize the
  bound structure to text for a `text` column), which makes `text` agree and
  leaves `json`'s vanishing row.
- C: fix the `json` arm — `json_quote` the decoded scalar so any json value can
  land in a json column — which makes `json` total and leaves `text` diverging.
- D: B and C together, which is the only pair that makes both column types
  correct.
- *Closes on*: whether a heterogeneous hole should work at all, or be refused.
  Either answer is fine; silence is not.

### D1 — `dl6_oracle.pl`'s schedule mapping is not type-directed, so a json document reaches it as an opaque atom.

There are two schedule writers and they disagree.
`compile/sweep.pl:arrival_value_json/4` is type-directed and writes a `json`
column as a JSON string carrying the canonical document.
`compile/scripts/dl6_oracle.pl:schedule_value/2` is not type-directed at all and
maps every JSON string to an atom.

Consequence: a document that reaches the served engine correctly reaches the
reference engine as an **opaque atom**, and since that engine has no JSON text
parser (json-flex §5), every `decode/2` against it derives nothing — silently,
with a clean empty tick log and exit 0. **A door that answers "no rows" for a
well-formed program over a well-formed document is worse than one that
refuses.** This lab could not grade anything until it was routed around.

`labs/extract_t2/t2_oracle.pl` is `dl6_oracle.pl` with the schedule mapping made
type-directed: it reads the parsed program's own `col_type/3` declarations in
declared order and parses a `json` column's schedule string into the braces term
the engine destructures. Every other column keeps the existing mapping,
including the stated reason for atoms over strings. **The proposed fix to
`dl6_oracle.pl` is that mapping, verbatim** — it is ~20 lines and it makes the
text door able to grade any json program. Not covered by it: `ref(Type)`
columns, whose schedule entry sweep.pl writes as a nested JSON object; no
program here declares one.

### A1 — `**` descent has no anchor, and on a real document that invented facts.

The first draft of `proto.dl6` spelled the message rule
`decode(Body, {'**': {$Name: {fields: {}}}})`. It was **wrong, silently**:
protobufjs writes a field literally named `fields` (`Struct.fields`) and one
named `values` (`ListValue.values`), so the unanchored descent matched a FIELD
MAP as a type declaration and minted `enum_variant(wkt, fields, id)`,
`(.., rule)`, `(.., type)` — three variants of an enum that does not exist.

`**` is depth-agnostic by design, and **there is no path, parent, depth or
`fullkey` predicate to say "only at type-declaration position"**, even though
json1's `json_tree` (which `**` already rides) exposes `fullkey`. The anchor had
to come from the document instead: protobufjs puts every declared type inside a
`nested` map. `proto-unanchored.dl6` is kept as the sabotage receipt; anchored
mints 1 enum variant, unanchored mints 4.

This is not an argument against `**` — it is the production that made a
package-nested descriptor tractable at all. It is an argument that descent needs
either a depth/path binding or a documented warning, because the failure mode is
invented facts rather than missing ones. New slot: `slot_descent_anchor`.

### A3 — no substring, so a JSON Pointer is resolved forward, never taken apart.

`$ref: "#/components/schemas/Category"` has to become the type name `Category`.
dl6 has `concat` but **no split, slice, substring or regex**, so the prefix
cannot be stripped. The whole corpus is therefore written the other way round:
build the pointer each type WOULD export with `concat`, and join on the entire
string.

```
export_pointer(Repo, Pointer, Name) <-
  type_def(Repo, Name, 'record'), contract_file(Repo, File),
  Pointer := concat([File, '#/components/schemas/', Name]).
```

This turned out to be **better than stripping**, not a workaround: it is an
equality join the engine indexes, it works identically for internal and external
pointers, and it made the cross-repo join fall out for free. It is also exactly
what the json-schema research independently recommends — `dereference()` is
unsafe on recursive schemas, `$ref` should stay a named reference.

The cost is real and should be stated: **every pointer dialect must be
enumerated in advance.** `#/components/schemas/` is hard-coded in two rules. A
document using `#/definitions/` or `#/$defs/` needs another rule per dialect,
and one using a pointer shape nobody anticipated cannot be read at all. That is
the price of no substring, and it is payable here because JSON Pointer prefixes
are a small closed set in practice.

### A2 — an OpenAPI enum has no name, so the algebra has to mint one.

proto and graphql name their enums. OpenAPI's is an anonymous member of one
property (`Pet.properties.status.enum`). The algebra's enum row needs a name, so
`openapi.dl6` mints `Pet.status` by `concat`. **That is a decision, not a
translation**, and two documents that mint differently will not join. Worth a
ruling if t2 ever federates enums across formats. New slot:
`slot_anonymous_enum_naming`.

### A4 — a leading underscore in a brace key is a variable, and GraphQL's whole introspection root is `__schema`.

`{__schema: ...}` parses the key as a VARIABLE — a key hole matching every key
at that level — because `_` is prolog's anonymous-variable spelling and the key
plane's literal marker is bareness. Every key in `graphql.dl6` is quoted for
this reason. Silent wrong answer, not a refusal. Cheapest fix: refuse a bare
brace key beginning with `_` by name, since it can never be intended as a
literal.

### A5 — no recursion over json, so wrapper chains cost one rule per depth.

GraphQL puts nullability and list-ness in wrapper types:
`NON_NULL(LIST(NON_NULL(Film)))`. Reading the named type at the bottom takes one
rule per chain depth (three arms in `graphql.dl6` for this schema); a deeper
chain silently yields nothing. `**` descends but cannot bind the depth or
correlate a chain, and dl6 has no recursion over a json value. Named, not
solved. New slot: `slot_json_wrapper_chain`.

### A6 — json-flex card C3 is no longer hypothetical.

C3 (the reference engine's json null is the ambiguous atom `none`) was filed
with the open question of whether any real payload triggers it. GraphQL
introspection triggers it on essentially every type: `description`, `ofType` and
`defaultValue` are null nearly everywhere. It is the only reason the graphql
program needs a rel excluded from its two-door grade. This raises C3's priority;
it does not change its analysis.

---

## 9. Slots

| slot | status |
|---|---|
| `slot_bytes_spelling` | **OPEN, measured.** avro `bytes` and `fixed` both reify as facts; `fixed` carries a WIDTH (`MD5`, 16) no column type can hold. Cheapest honest answer: bytes stays a `text` column carrying an encoding, width stays a fact. |
| `slot_int_width` | **OPEN, measured.** Petstore carries `int32` and `int64` on different fields of the same document. Reified as `field_format` data; unrepresentable as a column type. A t2 pass that means to CHECK width compatibility across repos needs the fact, not the type — which the current shape already gives. |
| `slot_format_null_map` | **PARTIALLY ANSWERED.** Per format: openapi = absence from `required`; proto3 = absence is the default (no presence marker exists); graphql = INVERTED, `NON_NULL` is the marker and absence means nullable; avro = null is a TYPE (`"null"`), usually inside a union, not an absence at all. So three of four formats map onto "optional = absence" cleanly and **avro does not** — avro's nullability is a union member, which is why `field_prim(.., 'null')` is a real row here. Recommend: reify avro's null as a union member and let the absence reading come from the union, not from the field. |
| `slot_defaults_residency` | **OPEN, untouched.** proto and avro both carry defaults; nothing here reads them. The coalesce-at-use-site reading is untested. |
| `slot_graphql_entry` | **ANSWERED, measured.** Introspection JSON, via graphql-js `buildSchema` + `introspectionFromSchema`. Cost: 35,868 B SDL → 110,822 B introspection, **3.09x, 15 ms**. SDL itself needs a parser; the CLI alternatives introspect a running server and are therefore unusable for a checked-out repo. |
| `slot_descent_anchor` | **NEW, open** — finding A1. `**` has no way to state position, and got it wrong on a real descriptor. |
| `slot_anonymous_enum_naming` | **NEW, open** — finding A2. |
| `slot_anonymous_nested_record` | **NEW, open** — an inline object type and an anonymous union member both have no name. |
| `slot_json_wrapper_chain` | **NEW, open** — finding A5. |

---

## 10. Fixture-promotion candidates

Ranked. The first three are the ones that would have caught something.

| candidate | shape | why |
|---|---|---|
| **`json_hole_binds_structured_value_into_text_column`** | `hole_text.dl6` + `corpus/hole-probe.json`, 4 rows | **fail-first, finding D2.** Nothing in the 221-fixture corpus binds a decode hole to a value that is not a scalar. Would go red today on 3 of 4 rows. |
| **`json_hole_scalar_into_json_column_vanishes`** | `hole_json.dl6` + `corpus/hole-scalar.json`, 1 row | **fail-first, finding D2.** Silent row loss under `INSERT OR IGNORE`. The strongest fixture here: a wrong answer with no error anywhere. |
| **`json_descent_matches_a_key_of_the_same_name_at_another_depth`** | the `proto-unanchored.dl6` shape, minimised | **finding A1.** Pins that `**` is unanchored, so the behaviour is a decision rather than an accident. Distinct from `json_descent_into_scalars_is_silent`, which covers non-matching, not wrongly-matching. |
| `json_pointer_resolved_by_forward_concat_join` | the `export_pointer` / `field_ref` pair | Pins the no-substring idiom (A3) as the supported way to resolve a reference, before someone adds a `split`. |
| `json_key_capture_over_openapi_components` | `openapi.dl6` reduced to 3 rules over a 4-schema document | A real-document key-capture + spread + required-set fixture. Complements `json_key_capture_nests_and_fans_out`, which is hand-built. |
| `json_brace_key_leading_underscore_is_a_variable` | one rule, one document | **finding A4.** Two rows, decidable, currently silent. Pairs with whatever refusal is chosen. |

Also worth landing outside the fixture corpus: the `t2_oracle.pl` schedule
mapping, as the fix to `compile/scripts/dl6_oracle.pl` (finding D1).

---

## 11. Receipts

Coordinator-reproducible, in this order. All hermetic:
`SPREFA_CONFIG=/nonexistent/...`, `DL_NO_DAEMON=1`, every served database
`:memory:`, no network (`corpus/regen.sh` is the only networked script and is
NOT part of the receipt run).

```
bash v6/prolog/labs/extract_t2/receipts.sh     19 passed, 0 failed
                                               EXTRACT T2 LAB RECEIPTS HOLD
bash v6/prolog/labs/extract_t2/scale.sh        the Q4 price table
```

`receipts.sh` covers: 5 × `bop check`; 5 × two-door byte grading; the Q1 census
and Q5 round trip; the four Q3 answer counts; and four sabotage probes that must
FAIL — unanchored descent inventing 3 phantom variants, the heterogeneous hole
diverging across doors, the vanishing scalar row, and the cross-repo lint going
2 → 5 when one declared dependency is deleted.

Nothing in `v6/prolog`, `v6/tsv2` or the conformance corpus was modified. The
lab is additive and self-contained; conformance, sweep and the text door were
not touched and were not re-run, because nothing outside `labs/extract_t2/`
changed.

One disclosed exception, not authored: the repo's pre-commit hook regenerated
`v6/INDEX.md` to list the new lab files. It is a generated index and the change
is mechanical.

Two package installs were needed to run anything in a fresh worktree and are
noted so the coordinator is not surprised: `pnpm install` in `v6/tsv2` and in
`v6/sprefa-store/js` (the latter is not obvious — `bop` resolves `rxjs` through
`v6/sprefa-store/js/src/engine/lib.ts`, so a tsv2-only install still fails with
`ERR_MODULE_NOT_FOUND: rxjs`).

## 12. Staffing

- Work type: research-then-grade lab, worktree `agent-a71a2ad9e00f1aac7`
- Base sha: `3de711f3`
- Lab files: `v6/prolog/labs/extract_t2/**` — DELETED on landing per the lab
  protocol. The coordinator records the last-copy commit hash here.
- One networked script, run once to build the corpus: `corpus/regen.sh`
  (`npx protobufjs-cli@1.1.3`, `npx graphql@16.11.0`). Both tools pinned.
