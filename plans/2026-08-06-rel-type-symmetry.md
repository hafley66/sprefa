# Rel/type symmetry, four hashes, and hot reload

Diagram: `plans/2026-08-06-rel-type-symmetry.d2` (render: same name, `.png`).
Stylesheet: `plans/_house.d2`.

## Contents

1. [The claim](#1-the-claim)
2. [Receipts: what already exists](#2-receipts-what-already-exists)
3. [The four hashes](#3-the-four-hashes)
4. [Type signatures](#4-type-signatures)
5. [Instance lifetimes](#5-instance-lifetimes)
6. [Storage layout, then reads and writes, then uniqueness](#6-storage-layout-then-reads-and-writes-then-uniqueness)
7. [Prior art, cited](#7-prior-art-cited)
8. [Live defects this plan closes](#8-live-defects-this-plan-closes)
9. [Build order h1..h6](#9-build-order-h1h6)
10. [Refusals this plan adds](#10-refusals-this-plan-adds)
11. [Open questions for the user](#11-open-questions-for-the-user)

## 1. The claim

A type interns VALUES keyed by a column tuple. A rel interns NAMES keyed by a
`(parent, local_name, arity)` tuple. Both use SQLite's `UNIQUE` as the
hash-cons and SQLite's `INTEGER PRIMARY KEY` rowid as the interned id. The rel
system needs exactly ONE thing the type system never needed: a per-file
disambiguator, because two files can carry the same rel name where two enum
variants in one file cannot.

| the question | type system (values, RHS), shipped | rel system (names, LHS), building |
|---|---|---|
| container | a declared type, `rel file(repo: text, at: fpath)` | a module, `rel orchard { ... }` |
| member | a COLUMN | a CHILD REL |
| containment | positional in `CREATE TABLE`, column order = ordinal | `__rel.parent_id -> __rel.rel_id` |
| instance | a row in the dictionary table, `__id INTEGER PRIMARY KEY` | a row in `__rel`, `rel_id INTEGER PRIMARY KEY` |
| identity | `UNIQUE (cols)` | `UNIQUE (parent_id, local_name, arity)` |
| liveness | `__refcount`, hits 0 and the row is deleted | `refcount`, hits 0 and the table is dropped |
| discriminant | `body_tag(Id, page)` | the `kind` column |
| flatten at the seam | `body` + `page` -> `body_page` | `orchard` + `tree` -> `orchard__tree` |
| collision | `throw enum_variant_rel_collision` | `throw rel_name_collision`, UNBUILT |

## 2. Receipts: what already exists

| mechanism | file:line | state |
|---|---|---|
| variant name join, one separator | `v6/prolog/0_enum_expand.pl:176` `variant_rel_name/3` | shipped |
| discriminant rel per enum | `v6/prolog/0_enum_expand.pl:107` `tag_rel_name/2` | shipped |
| collision refusal, generated vs plain names | `v6/prolog/0_enum_expand.pl:89-98` `validate_generated_names/2` | shipped |
| metadata kept because expansion erases its input | `v6/prolog/0_enum_expand.pl:12-16` `enum_context/2` | shipped |
| dictionary table = interned instances | `v6/prolog/lower.pl:851` `CREATE TABLE ... ("__id" INTEGER PRIMARY KEY, ..., UNIQUE (...))` | shipped |
| refcount column, level-headed refs only | `v6/prolog/lower.pl:843-846` | shipped |
| refcount decrement, zero-collect, delete | `v6/prolog/lower.pl:2470-2476` | shipped |
| `__ref_<Type>` TEMP view | `v6/prolog/lower.pl:857`, `:906-910` | shipped |
| catalog contract, 6 columns | `v6/prolog/lower.pl:634-637` `catalog_ddl_contract/2` | shipped (g1) |
| child-walk covering index | `v6/prolog/lower.pl:641-643` | shipped (g1) |
| one `INSERT OR IGNORE` for every catalog row | `v6/prolog/lower.pl:647-658` `catalog_row_ddl/3` | shipped (g1) |
| decl injection so the ordinary `rel_ddl/6` path builds the table | `v6/prolog/compile.pl:131` `materialize_catalog_rel/2` | shipped (g1) |
| catalog subtracted from ArrivalTargets | `v6/prolog/compile.pl:175` | shipped (g1) |
| arity gate on the catalog rel | `v6/prolog/analyze.pl:190` `program_uses_catalog` | shipped (g1) |
| swap re-runs DDL and swallows collisions | `v6/tsv2/serve/3_engine.ts:224-246` `isAlreadyExists`, `bootServedProgram` | shipped, and the reshape hole |
| one connection for the server's life | `v6/tsv2/serve/4_http.ts:156` | shipped |
| `table_name/2` DROPS arity | `v6/prolog/lower.pl:162` | shipped, latent defect |
| catalog ids assigned BY POSITION | `v6/prolog/lower.pl:645-646` comment | shipped, collides across programs |

## 3. The four hashes

Each hash answers one question and is allowed to break exactly one thing.

| hash | input | output | what a change authorizes |
|---|---|---|---|
| `h_id` | module id, local name, arity | the stable identity | nothing. It IS the name key across recompiles |
| `h_schema` | columns, column types, key positions | the DDL fingerprint | `DROP TABLE` + `CREATE TABLE`; rows are lost on purpose |
| `h_rule` | every rule body whose head is this ref, canonical order | the derivation fingerprint | keep the table, `DELETE FROM`, recompute rows |
| `h_rows` | the rows themselves | the data fingerprint | wake the consumers; equal means red stops travelling |

Written out:

```
h_id     = H(module_id, local_name, arity)
h_schema = H(columns, column_types, key_positions)
h_rule   = H(sorted canonical rule bodies for this head)
h_rows   = H(the rows)
```

`h_schema` must NOT appear in the table name. A content hash in the name makes
every column edit a rename, and a rename in SQLite is a new empty table, so the
rows are orphaned. Rust hashes the crate and never the item body for exactly
this reason (see section 7). The hash lives in a COLUMN of `__rel`.

## 4. Type signatures

Compiler side, Prolog:

```prolog
%! module_id(+CanonicalPath, +CompileOptions, -ModuleId) is det.
%   ModuleId is xxh3(CanonicalPath ++ CompileOptions). The StableCrateId
%   analogue: hash the FILE, once. Two files stop colliding; an edit inside a
%   file does not move the id.

%! rel_table_name(+ModuleChain, +LocalName, +Arity, -TableName) is det.
%   atomic_list_concat(ModuleChain ++ [LocalName], '__', Flat),
%   ( program_already_has(Flat)
%   -> throw(unsupported_construct(rel_name_collision(Flat, ModuleChain)))
%   ;  TableName = Flat ).
%   No hash in the name. The refusal is the enum_variant_rel_collision move,
%   applied one level up.

%! rel_hashes(+RelPlan, +Rules, +ModuleId, -Hashes) is det.
%   Hashes = hashes(HId, HSchema, HRule).
%   RelPlan = relplan(Name/Arity, _Kind, Columns, Key, ColumnTypes),
%   HId     = xxh3(ModuleId, Name, Arity),
%   HSchema = xxh3(canonical_term(Columns, ColumnTypes, Key)),
%   HRule   = xxh3(canonical_term(sorted bodies of Rules with head Name/Arity)).

%! catalog_row_ddl(+Decls, +RelPlans, +ModuleId, -Statements) is det.
%   Unchanged shape, four more columns per row, one INSERT OR IGNORE.
```

Runtime side, TypeScript:

```ts
/** One row of __rel as the runtime reads it. */
interface IRelRow {
  readonly relId: number;
  readonly parentId: number;
  readonly ordinal: number;
  readonly localName: string;
  readonly arity: number;
  readonly kind: "primitive" | "rel" | "column" | "module" | "instance";
  readonly typeId: number;
  readonly moduleId: number;
  readonly hId: string;
  readonly hSchema: string;
  readonly hRule: string;
  readonly hRows: string;
  readonly refcount: number;
}

type RelVerdict = "new" | "reshaped" | "rebodied" | "green" | "gone";

interface IReloadPlan {
  readonly verdict: ReadonlyMap<string, RelVerdict>;   // keyed by hId
  readonly ddl: readonly string[];                     // DROP/CREATE, in order
  readonly recompute: readonly string[];               // table names to re-derive
  readonly red: readonly string[];                     // consumers to wake
}

interface IReloadPlanner {
  /**
   * before = rows currently in __rel; after = rows the new compile produced.
   *   for each after-row: look up by hId
   *     miss                         -> "new",      ddl += CREATE
   *     hSchema differs              -> "reshaped", ddl += DROP + CREATE
   *     hRule differs                -> "rebodied", recompute += table
   *     all equal                    -> "green",    nothing
   *   for each before-row absent from after -> "gone", ddl += DROP
   *   red = consumers of every non-green table, closed over the level graph
   */
  plan(before: readonly IRelRow[], after: readonly IRelRow[]): IReloadPlan;
}

interface IRedPropagator {
  /**
   * Walk the level graph forward from `red`. At each hop, recompute, then
   * compare the fresh h_rows against the stored one:
   *   differs -> the consumer joins `red`
   *   equal   -> STOP, this branch is green from here down
   * A red rel on a CYCLE of the level graph marks the whole cycle red at once;
   * half an SCC cannot be green.
   */
  advance(seam: ISqlSeam, red: readonly string[]): Observable<readonly string[]>;
}
```

## 5. Instance lifetimes

| type | created | destroyed | survives restart |
|---|---|---|---|
| `__rel` row | at DDL replay, one `INSERT OR IGNORE` | when `refcount` reaches 0 | yes, it is an ordinary table |
| `h_id`, `h_schema`, `h_rule` | at compile, written with the row | with the row | yes |
| `h_rows` | at the end of each tick that writes the rel | overwritten next such tick | yes |
| `ModuleId` | once per file per compile | never rewritten while the file is unchanged | yes |
| `IReloadPlan` | at swap | at the end of the swap | no |
| `__ref_<Type>` TEMP view | at boot | on connection close | no, TEMP by design |

## 6. Storage layout, then reads and writes, then uniqueness

`__rel`, renamed from `__catalog_rel`:

| column | type | role | new? |
|---|---|---|---|
| `rel_id` | INTEGER PRIMARY KEY | fast local id, the rowid | no |
| `parent_id` | INT | who contains me, 0 at the root | no |
| `ordinal` | INT | 0 on a rel, 1-based argument position on a column | no |
| `local_name` | TEXT | my last segment only | no |
| `kind` | TEXT | primitive / rel / column / module / instance | no |
| `type_id` | INT | what I am | no |
| `arity` | INT | closes the `table_name/2` arity drop | YES |
| `module_id` | INT | which file I came from | YES |
| `h_id` | BLOB | stable identity across recompiles | YES |
| `h_schema` | BLOB | drives `DROP` | YES |
| `h_rule` | BLOB | drives recompute | YES |
| `h_rows` | BLOB | drives propagation | YES |
| `refcount` | INT | how many rules name me | YES |

Sequence of reads and writes on a swap:

1. read `__rel` (all rows) once, into `before`
2. compile the changed file alone, producing `after` with three hashes per rel
3. `plan(before, after)` classifies every row into one of the five verdicts
4. execute `plan.ddl` in order: every `DROP` before every `CREATE`
5. `INSERT OR REPLACE` the new `__rel` rows
6. recompute `plan.recompute`, then write each fresh `h_rows`
7. `advance` walks red forward, stopping wherever `h_rows` came out equal

Uniqueness conditions:

| condition | enforced by |
|---|---|
| one rel per `(parent_id, local_name, arity)` | `UNIQUE (parent_id, local_name, arity)` on `__rel` |
| one rel per `h_id` | `UNIQUE (h_id)` on `__rel` |
| one table per flattened name in a program | `throw rel_name_collision` at compile |
| one row per column tuple in a dictionary table | `UNIQUE (cols)`, already shipped |

## 7. Prior art, cited

| system | unit | id inside the compiler | name at the emit seam | collision policy | load timing |
|---|---|---|---|---|---|
| Rust | crate | `DefId = (CrateNum, DefIndex)` | v0 mangle, `_RNvCs7qp2U7fqm6G_7mycrate7example` | 64-bit `StableCrateId` = H(crate name, all `-C metadata`); symbols incorporate it | demand-driven during name resolution (`CStore`, `CrateLocator`); rmeta decoded lazily from offsets |
| Go | package | `Sym`, "an object name in a segmented (pkg, name) namespace" (`$GOROOT/src/cmd/compile/internal/types/sym.go:14`) | import path + `.` + name, unsafe bytes `%`-escaped (`$GOROOT/src/cmd/internal/objabi/path.go:18-42`) | compile error, never a hash | export data per package, read on import |
| Python | file/dir | `sys.modules["a.b.c"]`, child set as an attribute on the parent (`importlib/_bootstrap.py:1350`) | the dotted string | parent imported first, per-name lock (`_bootstrap.py:1368`) | runtime, lazy, first `import` wins |
| SQLite | attached database | `(schema, table)` | `other.t` | none needed; `main.t` and `other.t` coexist (verified locally, 3.43.2, `MAX_ATTACHED=10`) | `ATTACH` at runtime |
| dl v5 | none | none | the bare rel name | last writer wins, silently | textual include, canonical-path dedup (`src/frontend.rs:349`, `:680-696`) |

Where Rust hashes and where it does not:

| level | hashed | what |
|---|---|---|
| crate root | yes | `StableCrateId`, 64-bit, from crate name + `-C metadata` flags |
| dependency check | yes | SVH, 64-bit; the loader skips a crate with the wrong SVH |
| module path segments | no | length-prefixed identifiers, `7mycrate7example` |
| item body / columns | no | never |
| anonymous items | counter | `DisambiguatedDefPathData` numbers the 2nd closure, the 3rd impl |

Nesting in v0 mangling is `N <namespace> <parent-path> <identifier>`, applied
recursively. That is `parent_id` plus `local_name`, in a string.

Sources: rustc-dev-guide `backend/libs-and-metadata.html` and `hir.html`;
the rustc book `symbol-mangling/v0.html`; `rustc_hir::definitions::DefPathData`;
`rustc_metadata::rmeta::decoder::CrateMetadata`.

## 8. Live defects this plan closes

| defect | evidence | closed by |
|---|---|---|
| arity dropped from the table name; `edge/2` + `edge/3` in one program emit two `CREATE TABLE "edge"` | `v6/prolog/lower.pl:162`. Corpus scan: `fixtures=302 refs=1074 same_name_two_arities=0`, so it has never fired | h2 |
| catalog ids are positional per compile, so two programs booted into one database collide (rel_id 6 demonstrated twice) | `v6/prolog/lower.pl:645-646`; one connection at `v6/tsv2/serve/4_http.ts:156` | h4 |
| a reshaped rel is INVISIBLE under a running server: the swap re-runs DDL and swallows "already exists", so the old table shape survives | `v6/tsv2/serve/3_engine.ts:224-246` | h5 |
| every reload recomputes the whole downstream graph | no `h_rows` comparison exists | h6 |

## 9. Build order h1..h6

| step | what lands | why it is next | receipt that it worked |
|---|---|---|---|
| h1 | rename `__catalog_rel` -> `__rel`, and its index | it is the self-describing rel table, not a side car | conformance byte-identical except the name |
| h2 | `arity` on `__rel` and in `table_name/2` | the arity drop is a live latent defect | a fixture with `edge/2` AND `edge/3` in one program compiles and runs |
| h3 | a dotted name through the parser, `a.b(X) <- c(X).` | the mangler has NO INPUT until this exists | text door parses it; the term door stops minting a rel called `dot_get` |
| h4 | `module_id` + `h_id`, one hash per FILE | two files stop colliding; this is `StableCrateId` and nothing more | two programs in one database, same rel name, both queryable |
| h5 | `h_schema` + `h_rule` and the five-verdict swap | today a reshape is invisible | reshape a rel under a running server and watch the table shape change |
| h6 | `h_rows` + red/green propagation over the level graph | without the stop rule every reload recomputes everything | COUNT test: a no-op edit runs 0 downstream statements |

h1 and h2 are independent of the dotted-head fork and can land today. h3 is the
only input the mangler lacks. h4..h6 are the hot-reload spine.

## 10. Refusals this plan adds

| refusal term | fires when |
|---|---|
| `rel_name_collision(Flat, ModuleChain)` | two module chains flatten to the same table name in one program |
| `rel_arity_collision(Name, A1, A2)` | until h2 lands, two arities of one name in one program (today: silent double `CREATE TABLE`) |
| `module_id_missing(Path)` | a rel plan reaches lowering with no owning file |

## 11. Open questions for the user

1. Hash function: xxh3 (the typescript-go choice) or SHA-256 truncated? SQLite
   has neither as a UDF, so the compiler computes it and the runtime only
   compares BLOBs.
2. Separator: `__` (two underscores) for the module join, so `orchard.tree`
   becomes `orchard__tree` and cannot be confused with the enum join
   `body_page`. Confirm, or pick another.
3. Does a RESHAPED rel drop rows silently, or does the swap refuse and demand
   an explicit `--reshape` flag? Losing rows under a running server is the kind
   of thing that should probably be asked for out loud.
4. Do monomorphized generic instantiations get `kind='instance'` with
   `parent_id` = the generic, or a plain mangled `kind='rel'` row with no link
   back? (carried over from 2026-08-05)
5. Primitives: reserved ids 1..5 as shipped, or looked up by name at seed time?
   (carried over)
6. Does the HTTP door learn `__rel`, or do users curl the flattened name?
   (carried over)
