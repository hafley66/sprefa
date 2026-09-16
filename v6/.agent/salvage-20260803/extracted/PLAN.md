# Plan: schema facts in prolog -> generated rs/ts/sql

Base `92756b54`. Scope: MVP slice only (ts row-interface emission + staleness gate);
rust-struct and DDL emission and the N-language emitters are staged, not built.
Ground truth: `RESEARCH.md`; source-of-truth files cited inline as `path:line`.

## 1. Fact schema — type signatures first

Facts live in one consult file, target-neutral. The type/seeding/emit split mirrors
`emit_openapi.pl:13-23` ("Owns: the dialect; Does not own: any route/parameter/schema"):
facts hold structure only, every emitter owns its language dialect.

### Predicates (arities)

```
% One closed spine table.
table(Name,         % atom, snake_case, == spine.rs table_name (Iden)
      WithoutRowid).% bool; true for composite-key junctions

% One column. Pk is the column's position in the PRIMARY KEY (1..n) or `none`.
column(Table,  Name,   BaseType,   Nullable,  Pk)
       atom    atom     base       bool       none | pos(1..n)

% Secondary index / unique constraint not expressible as single-col UNIQUE.
index(Name,   Table,			   Cols,       Opts)
      atom    atom                 list(atom)  list( unique | partial(WhereAtom) )

% FK edges (owner law; also what a rust `belongs_to` / a jsonschema ref would need).
fk(FromTable, FromCol,  ToTable,  ToCol)
   atom        atom    atom       atom
```

`BaseType` is the closed set of physical types (see mapping table). No length/scale,
no dialect flag, no rust/ts idiom inside facts — those are emitter concerns.

### Mapping table (sqlite <-> rust <-> ts)

Sea-orm derive is the rust source; the map is derived from `spine.rs` field types +
`#[sea_orm]` attrs.

| BaseType  | sqlite         | rust (sea-orm Model)                         | ts (row interface)      | nullable? null spelling                   |
|-----------|----------------|----------------------------------------------|-------------------------|-------------------------------------------|
| integer   | `INTEGER`      | `i64`                                        | `number`                | `Option<T>` / `T \| null`                |
| int32     | `INTEGER`      | `i32`                                        | `number`                | `Option<T>` / `T \| null`                |
| text      | `TEXT`         | `String`                                     | `string`                | `Option<T>` / `T \| null`                |
| blob      | `BLOB`         | `Vec<u8>`                                    | `Uint8Array`            | `Option<T>` / `T \| null`                |
| pk        | `PRIMARY KEY`  | `#[sea_orm(primary_key)]` (+`auto_increment`)| (sentinel on member)    | pks never nullable                         |

### Emitters — pseudo-code as comments

% ---- emit_ts_rows/2 : row interfaces into spine.ts marker section -----------
```
% layout:
%   table('node'), column('node','node_id',integer,false,pos(1)), column('node','name_id',integer,true,none), ...
% output per table:
%   export interface NodeRow { node_id: number; name_id: number | null; }
ts_rows_text(Text) :-
    findall(Row, (table(T,_), table_row_interface(T,Row)), Rows),
    atomic_list_concat(Rows, '\n\n', Text).

table_row_interface(T, Row) :-
    findall(Field, (column(T,N,B,Null,_), ts_field(N,B,Null,Field)), Fields),
    pascal(T, PT),                                  % snake -> Pascal ("node" -> "Node")
    atomic_list_concat(Fields, ';\n  ', Body),
    format(atom(Row), 'export interface ~wRow {\n  ~w;\n}', [PT, Body]).

ts_field(N,B,false, Fmt) :- ts_type(B,false,Ty),  format(atom(Fmt),'~w: ~w',[N,Ty]).
ts_field(N,B,true,  Fmt) :- ts_type(B,true, Ty),  format(atom(Fmt),'~w: ~w',[N,Ty]).

% name authority: PascalCase export is the ONE derived identifier (hafley rule:
% no renaming across langs — column names pass through untouched; only the
% interface *name* is PascalCased, and it is NOT referenced by any manual code,
% so deriving it is safe).
```

% ---- emit_ddl/1 : CREATE TABLE / index strings (step 2, not MVP) -------------
```
% sqlite column def: base -> INTEGER/TEXT/BLOB + `NOT NULL` unless nullable,
% plus `PRIMARY KEY`/`UNIQUE` from column attr; WITHOUT ROWID from table/2.
% index/4 -> CREATE [UNIQUE] INDEX; partial(Where) -> raw WHERE clause.
```

## 2. MVP slice — pick, and prove non-drift

**Pick (a): emit the 9 ts row interfaces into a marker section of `spine.ts`.**
Justification: it is the smallest add-only change (no rust rewrite, no new seams)
that retires the highest-density hand-kept twin — the 9 `*Row` interfaces at
`v6/sprefa-store/js/src/engine/spine.ts:63-102`, duplicated from the sea-orm
`Model` structs in `src/spine.rs`. (c) retires nothing: rust models are already the
source via `DeriveEntityModel`, so emitted rust would invert the authority. (b) is
deferred to step 2 because rust DDL is itself derived (sea-orm), so "both sides
consume" forces a rust-side unwind of the derive or an `include_str` seam — a
refactor, not an MVP.

Target correction vs the brief: the row interfaces live in `spine.ts` (the ported
entity twin), not `types.ts` (which holds engine-side `NodeRow`/`EdgeRow`/`SpanRow`
plus a `SpanRow` with no rust twin, `types.ts:80-102`). Emit into `spine.ts`'s
`// entity row types` block only. `span_row` is out of scope (no spine table).

### The MVP gate (staleness test, runnable in CI)

`v6/tsv2/tests/spineSchema.test.ts`, pattern-copy of `bopCommandInventory.test.ts`
(the repo's established staleness gate, `tests/bopCommandInventory.test.ts:52-59`):

```
test("generated spine row interfaces are current with canonical prolog facts", () => {
  const emitted = spawnSync("swipl", ["-q","-l", EMITTER_PL,
     "-g","emit_spine_schema:rows_ts_text(T),format('~s',[T])","-g","halt"], {encoding:"utf8"});
  assert.equal(emitted.status, 0, emitted.stderr);
  assert.equal(readFileSync(SPINE_SCHEMA_TS,"utf8"), emitted.stdout);
});
test("prolog column facts and spine.ts hand-DDL name the same column set", () => { /* source-scan */ });
```

`SPINE_SCHEMA_TS` is the extracted marker-section body read off `spine.ts`. Running
in CI requires `swipl` on PATH — already a test dependency for the existing
openapi/cli gates, so no new toolchain. One non-drift proof, symmetric to the bop
gate.

## 3. Seeding — how facts get bootstrapped the first time

**Recommend: hand-transcribe the 9 tables** into `3a_spine_schema_facts.pl` from
`src/spine.rs` (full inventory already transcribed in `plan-notes/SEED-INVENTORY.md`).
Why over sprefa-extract harvest: the set is closed and small (9 tables, 37 columns,
5 indexes, 13 FKs ≈ 60 fact lines), and an AST harvest must read **through** the
`DeriveEntityModel` derive to recover `primary_key`, `auto_increment`, `unique`, and
`Option<T>` nullability — more machinery than the one-time seed warrants. The
staleness gate then holds every future edit in sync, so the one-time transcription
never needs repeating. sprefa-extract harvest is a viable later automation if the
fact set grows (e.g. the open rel-table mint), but not for a closed 9-table model.

## 4. Instance lifetimes / storage layout

| path | role | checked-in | lifetime |
|---|---|---|---|
| `v6/prolog/compile/3a_spine_schema_facts.pl` | the canonical facts (authority) | yes | permanent; edited in place |
| `v6/prolog/compile/3_emit_spine_schema.pl` | `rows_ts_text/1` + marker writer + `emit_spine_schema/0` | yes | permanent |
| `v6/sprefa-store/js/src/engine/spine.ts` | marker section (`// BEGIN/END GENERATED spine ROWS`), manual transforms preserved around it | yes | generated section regenerated; manual code never overwritten (hafley `_auto` rule) |
| `v6/tsv2/tests/spineSchema.test.ts` | CI staleness gate | yes | permanent |

Marker-section (not whole-file) emission, copying `1_emit_registry_docs.pl:24-33`
`replace_generated_section`. Generated output checked in (repo norm,
`RESEARCH.md:101`). No `_auto` new file; the twin being retired is an inline marker
zone, matching the registry-docs precedent rather than the cli-inventory file
precedent.

## 5. N-language stage-setting

Keep the fact schema target-neutral now so future emitters bolt on with no fact
migration: `BaseType` stays a closed 4-member set (`integer`/`int32`/`text`/`blob`);
each emitter maps `(BaseType, Nullable)` to its own atom and owns its dialect
spelling (`emit_openapi.pl:13` holds the openapi dialect, `lower.pl` the SQL text —
same rule). Emitters to stage, all additive, all keyed on the same
`column/5`/`index/4`/`fk/4` sets: openapi (`integer`/`integer(int32)`/`string`/
`string format: byte`), jsonschema (same + `format: base64` for blob), typespec
(`int64`/`int32`/`string`/`bytes`). Never encode a dialect keyword into a fact.

## 6. Buy check

| candidate | would replace | verdict |
|---|---|---|
| `sea-orm-cli` entity gen | the hand-typed rust `Model` derives | **loses for MVP**: reverse-generates rust entities from a running DB only; single-target (rust), cannot emit ts. Already refused in `RESEARCH.md:108` |
| `openapi-typescript` | the ts emitter | **deferred, legit**: consumes spec->ts; would force a 2-hop facts->openapi->ts chain plus a checked-in `openapi.json`. Fine when a real HTTP consumer appears (it already exists for openapi_codegen_lab) |
| `alloy` (typespec emitter) | the whole facts+emitters layer | **loses for MVP**: `.tsp` must itself be authored (same spine.rs seeding problem) and drags a tsc/alloy toolchain into a repo whose compile engine is already swipl; wins only on ecosystem emitter breadth, not on this closed 9-table set |
| prolog emitters | — | **wins for MVP**: the emit mechanism is already in-repo (`emit_openapi.pl`, `2_emit_cli_inventory.pl`), zero new deps, one hop, multi-target by construction |

## 7. Effort — ordered steps, MVP gate first

| # | step | files | LOC | gate |
|---|---|---|---|---|
| 1 | transcribe facts (seed) | `3a_spine_schema_facts.pl` (new) | ~60 | `swipl -l 3a -g "forall(column(_,_,_,_,_),true)"` loads |
| 2 | emit ts row interfaces | `3_emit_spine_schema.pl` (new) | ~80 | eyeball: emitted == current `spine.ts:63-102` |
| 3 | add BEGIN/END markers, check in | `spine.ts` | ~2 | diff clean |
| 4 | **MVP gate** — staleness test | `tests/spineSchema.test.ts` (new) | ~50 | `just test` (tsv2 suite) passes, incl. asserted-equal gate |
| 5 | DDL fingerprint + parity | `3_emit_spine_schema.pl` + `tests/spineSchema.test.ts` | ~40 | gate 2: emitted DDL == `spine.ts:115-123` source-scan == sea-orm output (cargo test) |
| 6 | N-language emitter stubs | `4_emit_spine_openapi.pl` (stub) | ~20 | compiles; no output yet |
| 7 | docs (ARCH.pl, PLANS) | `v6/prolog/ARCH.pl`, `PLANS.md` | ~10 | review |

MVP complete at step 4. Steps 2-3 are the generator+retire; step 4 is the proof.
Total build for MVP slice (1-4): ~190 LOC across 3 new files + ~2 edited lines.
