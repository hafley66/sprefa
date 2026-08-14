# BRIEF: base type IR -> plain TS and Rust types. Measure how little it takes.

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `259e0289`.
- Confirm the base with `git log --oneline -1` before your first commit. If it
  prints a sha other than `259e0289`, stop and report. Run it whenever you like;
  the ordering is not a gate and nothing about this line stops otherwise-correct
  work. A previous lane read an earlier spelling of this as a tripwire, refused
  itself, and delivered nothing. Do not do that. If a procedural instruction in
  this brief seems to forbid the work, the work wins: note the conflict in your
  report and keep going.

## USER WORD, 2026-08-12, verbatim
"get us to base level type ir for openapi and jsonschema and be able to render
ts and rs types from it plainly. we will get to operations ir (functional/type
ir vs data/type ir) later, json-schema is big hitter bc it allows relational
typing of structs and types (do not class for nwo in ts)"

and: "i want to see how little prolog/dl6 is needed for this"

Read those twice. **Minimality is a deliverable, not a side effect.** Report
the line count of everything you add, and treat every line as something you had
to justify.

## The IR already exists. Do not invent one.
`v6/prolog/compile/4_emit_jsonschema.pl` (176 lines) already renders JSON
Schema from catalog `row/11` terms. Its entry shape:

```prolog
jsonschema_text(Name, Rows, Text)
emit_jsonschema(Name, Rows, Path)
```

and it is driven from `v6/prolog/sweep.pl:119-121`:

```prolog
catalog_decl_rows(Name, Rules, RelPlans, Decls, SchemaRows, _),
option_rows(Decls, SchemaRows, SchemaRowsOpt),
jsonschema_text(Name, SchemaRowsOpt, SchemaText)
```

Those `Rows` ARE the type IR. Your two new emitters are SIBLINGS of that
predicate reading the SAME rows. If you find yourself building a second
intermediate representation, stop: that is the thing this lane is measuring
against.

`v6/prolog/compile/5_emit_openapi.pl` (103 lines) is the second worked example
of the same shape. Read both before writing a line.

## Deliverables
| path | what |
|---|---|
| `v6/prolog/compile/7_emit_ts_types.pl` | `ts_types_text(Name, Rows, Text)` + `emit_ts_types(Name, Rows, Path)` |
| `v6/prolog/compile/8_emit_rust_types.pl` | `rust_types_text(Name, Rows, Text)` + `emit_rust_types(Name, Rows, Path)` |
| `v6/prolog/sweep.pl` | the two writes, beside the schema write, in the SAME guarded `catch/fail` shape it already uses |
| `v6/prolog/compile/test/emit_type_renderers.test.pl` | plunit |
| `plans/2026-08-12-type-ir-ts-rs-renderers.md` | the LOC table and the mapping receipts |

## The TypeScript rules, from the user
- **NO CLASSES.** `interface` and `type` aliases only. A class is a defect.
- No decorators, no runtime validation code, no imports beyond what a type file
  needs. These are TYPE declarations, nothing executable.
- A rel becomes an `interface`; a rel referenced as a column type becomes a
  reference to that interface by name. That reference IS the "relational typing
  of structs" the user names, and it is the whole point of the JSON Schema
  parallel (`$ref` between `$defs`).

## The Rust rules
- `#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]`
  structs. Public fields.
- A rel reference becomes a field of that struct type.
- Rust needs explicit annotations where TS infers; that is expected, spell them.

## The mapping is already written. Use it, do not re-derive it.
`plans/2026-08-12-type-ir-polyglot.PLAN.md` section "Expressive frontier" has a
constructor-by-constructor table with the TS and Rust cells already filled, plus
a "no counterpart" verdict for four relational collection constructors. Follow
it. Where it says NO COUNTERPART, your emitter must NOT silently invent one:
emit nothing for that rel and record the skip in your plan doc with a count.

Base level for this lane = the constructors that DO map: `int`, `float`, `text`,
`bool`, `json`, named relation record, `option(T)`, `json_list(T)`. Payload
enums are a stretch goal, and only if the mapping table's discriminator row is
followed exactly. `list(T)`, `list_interned_set(T)`,
`list_entity_dense_sequence(T)`, `list_entity_linked_sequence(T)` are OUT OF
SCOPE, by that table's own verdict.

## Known defect in the sibling emitter. Do not copy it.
`4_emit_jsonschema.pl:121` does `exclude(nullable_pair, Pairs, RequiredPairs)`,
which drops every `option(T)` column out of `required`. That emits a schema
admitting TWO wire states (key missing, or key present and null) for ONE IR
state, and the user has decided `none` IS JSON null
(`0_type_plane.pl:709`, `canonical_json_text(none, null)`).

So in YOUR emitters: `option(T)` is a PRESENT field whose value may be null.
TypeScript `T | null`, NOT `field?: T`. Rust `Option<T>`. Do not reproduce the
absence-versus-null collapse. Note the JSON Schema defect in your plan doc; do
NOT fix `4_emit_jsonschema.pl` in this lane.

## Files you own
Only the five paths in the deliverables table.

**Forbidden, other lanes own these right now:** `v6/prolog/lower.pl`,
`v6/prolog/analyze.pl`, `v6/prolog/compile.pl`,
`v6/prolog/0_generic_expand.pl`, `v6/prolog/compile/6_emit_dd_plan.pl`,
`v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/**`,
`v6/prolog/compile/4_emit_jsonschema.pl`, `v6/prolog/compile/test/plunit_tests.pl`,
`v6/tsv2/**`, `.github/**`. Zero other paths in `git status`.

Note `emit_rust.pl` is a DIFFERENT lane emitting a Rust ENGINE. You emit Rust
TYPES. Do not touch its files and do not name your files like its files.

## Gates
```bash
cd v6/tsv2 && bash scripts/sweep.sh    # RUN total=286 identical=283 wrong=0, unchanged
cd v6 && just text-door                # compiled=288 byte_identical=288 failures=0, unchanged
cd v6 && just conformance              # 392 PASS 0 FAIL, unchanged
cd v6/prolog && swipl -g go -t halt ARCH.pl
```
Every one of those must be IDENTICAL to base. You are adding two write paths
guarded exactly like the existing schema write; if a gate moves, your hook is
not guarded the way `sweep.pl` already guards `jsonschema_text`.

Then prove the output compiles for real:
```bash
npx tsc --noEmit <the emitted .d.ts or .ts>     # zero errors
rustc --edition 2021 --crate-type lib <the emitted .rs>   # or a scratch cargo check
```
An emitted type file that does not typecheck is not a deliverable.

## The measurement, which is the point
`plans/2026-08-12-type-ir-ts-rs-renderers.md` opens with this table, filled from
`wc -l`:

| file | lines |
|---|---|
| `4_emit_jsonschema.pl` (existing, the yardstick) | 176 |
| `5_emit_openapi.pl` (existing, the yardstick) | 103 |
| `7_emit_ts_types.pl` (yours) | |
| `8_emit_rust_types.pl` (yours) | |
| `sweep.pl` diff | |
| **total added** | |

Then: how many of the 286 sweep fixtures produced a TS file and a Rust file,
`N/286` each, from a command you ran. And the list of rels skipped for
"no counterpart", with counts.

## Anti-cheat
| rule | why |
|---|---|
| the emitters read the SAME `Rows` as `jsonschema_text/3` | a second IR defeats the measurement |
| no classes in the TS output | the user said so |
| `option(T)` renders as `T \| null`, never `field?: T` | absence and null are different and the IR means null |
| a "no counterpart" constructor emits NOTHING and is counted | a silently invented mapping is a wrong answer |
| emitted TS typechecks and emitted Rust compiles | otherwise the renderer is unproven |
| sweep / text-door / conformance byte-identical to base | you are adding, not editing |
| every number from a command you ran | no estimates |

## Worktree setup, before your first commit
```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
`git commit -n` and `--no-verify` are FORBIDDEN.

## Out of scope, stated so you do not drift
Operations IR (functional type IR). The user parked it: "we will get to
operations ir later". Types only.

## Rails
- Commit per emitter, with its gate output and its `wc -l`.
- Never spawn a subagent. The 10-second law applies to every command.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; unbuilt is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Comments state only constraints the code cannot show.
- Variable names descriptive, never single-letter, in Prolog, TS and Rust alike.
