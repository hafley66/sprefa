# Evaluate capitalized relation names

**Issue** @capitalized-relation-names (042)
**Lane** relation-naming · **Status** evaluated, read-only
**Date** 2026-08-17

Evaluate adopting capitalized authored relation names (`Person`, `Commit`) as the DL6 convention. The parser already accepts them. Every semantic and generated boundary traced below before any recommendation. No naming change was implemented.

## TOC

- [Current behavior table](#current-behavior-table)
- [Concrete syntax examples](#concrete-syntax-examples)
- [Naming flow, authored relation to generated target](#naming-flow)
- [Collision and filesystem portability results](#collision-and-filesystem-portability)
- [Migration surface and golden families](#migration-surface-and-golden-families)
- [Recommendation](#recommendation)

---

## Current behavior table

Probe program used throughout:

```dl6
rel Person(id: int).
rel Commit(author: Person).
```

Facts (1) and (2) from the prior interrupted attempt are re-derived below by direct compile runs.

| # | Target | Emitter / site | Emitted spelling | Case preserved | Case-only `Person`+`person` distinct? |
|---|--------|----------------|------------------|:---:|---|
| 1 | Parser / CST | `parse_dl_dcg.pl:ident/3` (330), `head_atom/3` (877), `relatom_item/3` (1169), `compound_or_var/3` (1244) | `Person`, `Commit` verbatim | yes | yes (both accepted) |
| 2 | Printer / roundtrip | `print_dl.pl` | `rel Person(id: int). rel Commit(author: Person).` | yes | yes |
| 3 | SQLite tables | `lower.pl:rel_ddl/6` (2640) | `CREATE TABLE "Person"`, `CREATE TABLE "Commit"`, `__delta_Person`, `__ref_Person` | yes | strings distinct, **runtime collision** (see §collision) |
| 4 | Struct types | `lower.pl:struct_type_plans/6` (2916) → `emit_ts.ts STRUCT_TYPES` | `{ name: "Person", columns: ["id"] }` | yes | distinct |
| 5 | Ref columns | `STRUCT_REF_COLUMNS` | `"Commit": ["Person"]` | yes | distinct |
| 6 | rel_catalog | `lower.pl:catalog_decl_rows/6` (1495), `emit_ts.pl:program_catalog_rows/8` (855), `rel_catalog_lines/2` (860) | `local_name: "Commit"` (rel_id 8), `local_name: "Person"` (rel_id 10) | yes | distinct surrogate rel_ids |
| 7 | ProgramJson (Rust) | `emit_rust.pl:emit_program/5` | `"rel":"Person"`, `"table_name":"Person"`, `struct_types[].name`, all SQL | yes | distinct JSON strings |
| 8 | Rust types | `compile/8_emit_rust_types.pl` (`type_name/2`) | `pub struct Person`, `pub struct Commit` | **PascalCased** | **same-module: duplicate struct** |
| 9 | TypeScript types | `compile/7_emit_ts_types.pl:type_name/2` (216) | `export interface Person`, `Commit` | **PascalCased** | **same-module: duplicate interface** |
| 10 | JSON Schema | `compile/4_emit_jsonschema.pl` | `$defs: { Person, Commit }` | yes | distinct keys |
| 11 | OpenAPI | `compile/5_emit_openapi.pl` (shared `kind_schema/6`) | `components.schemas: { Person, Commit }` | yes | distinct keys |

Rows 3–7 and 10–11 preserve the authored spelling verbatim. Rows 8–9 (the TS/Rust **type** layer) do not: `type_name/2` (`7_emit_ts_types.pl:216`) splits on `_` and upcases each part, so `person` and `Person` both render `Person`.

## Concrete syntax examples

All accepted by the parser, case preserved through roundtrip:

```dl6
rel Person(id: int).                            # capitalized rel, typed col
rel Commit(author: Person).                     # capitalized rel, capitalized ref type
rel Box(T)(value: T).                           # generic template, capitalized name
rel Shelf(items: Box(int)).                     # generic application, capitalized
rel grade(ripe(sugar: int) ; green(days: int)). # enum, variant rels minted grade_ripe/grade_tag
rel Person(id: int, nick: text?).               # option column -> __opt_text_some/none/tag
rel Log(level: int).                            # capitalized shadow of keyword `log`
```

Rule with capitalized rel call and capitalized variable (keyword pun `Id` → `id`):

```dl6
Commit(Author, Msg) <- Person(id: Author, name: Msg).
```

Parser result (`parse_dl_dcg.pl`), identical for `Person` and `person`:

```prolog
prog([type_decl('Person',[col(id,int),col(name,text)]),
      col_type('Person'/2,id,int), col_type('Person'/2,name,text), ...],
     [<-('Commit'(_A,_M),'Person'(_A,_M))])
```

The `_A`/`_M` are SWI's display of the shared join variables; `Author`/`Msg` bind correctly.

## Naming flow

```mermaid
flowchart LR
    A["authored rel name<br/>Person / Commit"] --> P["parse_dl_dcg.pl<br/>ident/3 case-agnostic<br/>call = name(...), var = bare ident"]
    P --> R["print_dl.pl<br/>verbatim"]
    P --> L["lower.pl<br/>rel_ddl/6, catalog_decl_rows/6, struct_type_plans/6"]
    L --> S["SQLite tables<br/>\"Person\" \"Commit\"<br/>+ __delta/__frontier/__ref"]
    L --> C["rel_catalog<br/>surrogate rel_id, local_name verbatim"]
    L --> E["emit_ts.pl program_catalog_rows<br/>rel_catalog const"]
    L --> PG["emit_rust.pl ProgramJson<br/>verbatim strings"]
    L --> D["catalog_decl_rows + option_rows<br/>type rows"]
    D --> TT["7_emit_ts_types.ts type_name/2<br/>PascalCase + module prefix"]
    D --> RT["8_emit_rust_types.rs type_name/2<br/>PascalCase + module prefix"]
    D --> JS["4_emit_jsonschema<br/>verbatim $defs"]
    D --> OA["5_emit_openapi<br/>verbatim components.schemas"]
```

Key boundaries:

- **Parser**: `ident/3` (`parse_dl_dcg.pl:330`) is case-agnostic (`alpha | '_'` start, `alnum | '_'` continue). Relation heads/calls are distinguished from variables **by position and trailing parens**, never by case: `head_atom/3` (877) and `relatom_item/3` (1169) require `name(...)`; a bare identifier in expression position is a variable via `compound_or_var/3` (1244). So `Person` alone (no parens) is a variable; `Person(...)` is a call. No context is ambiguous because calls always carry parens.
- **Keyword puns**: `capitalized_keyword_pun/2` (`parse_dl_dcg.pl:914`) explicitly lowercases a Capitalized argument's first letter to match a column, e.g. `Id` puns `id`. This mechanism exists precisely for capitalized naming and works (probe `Commit(Author, Msg) <- Person(id: Author, name: Msg).`).
- **Keywords**: `kw/1` (`parse_dl_dcg.pl:320`) matches word terminals case-sensitively. Capitalized `Log`, `Match`, `Set`, `Box` are distinct idents, so capitalized names dodge keyword collisions.
- **Minted names** (generic/enum/option): `canonical_type_name/2` and `readable_stem/2` (`0_generic_expand.pl`) preserve the authored constructor case in the stem: `Box(int)` mints `__gen__Box_int_<digest>`; enum `grade(ripe;green)` mints `grade_ripe`, `grade_tag`; option `text?` mints `__opt_text_some/none/tag`.
- **Module-qualified types**: `use_resolve.pl:expand_uses/6` + `0_dot_expand` splice imports; mounted rels keep their own module identity (`rel_module_decls/3`), and cross-module rels are referenced by bare local name. The TS/Rust type layer prefixes a module name only when `type_name` collides (`emitted_type_name/5`, `7_emit_ts_types.pl:182`).

## Collision and filesystem portability

Probe: `rel Person(id: int). rel person(id: int).` in one module.

| Layer | Result |
|---|---|
| Compiler data / rel_catalog | Distinct: surrogate rel_ids 8 and 10, `local_name` `Person` / `person`. No compiler check flags them (only `rel_arity_collision` at `compile.pl:331` and generic minted-name checks exist; **no case-insensitive rel-name uniqueness check**). |
| SQLite DDL (emit_ts / ProgramJson) | Emits `CREATE TABLE "Person"` and `CREATE TABLE "person"` as distinct strings. **Runtime collision**: SQLite identifiers are case-insensitive, so the second `CREATE TABLE` fails `table "person" already exists` (verified with `sqlite3`). Storage is not injective for case-only-distinct names. |
| TS types | `type_name/2` maps both to `Person`; same-module collision is **not resolved** by the module prefix (both get the same `Case` prefix) → two identical `export interface CasePerson`. Invalid TS. |
| Rust types | Same: two `pub struct CasePerson`. Does not compile. |
| TS/Rust types, **different** modules (`a:Person`, `b:person`) | Collision detected → module prefix resolves: `APerson` / `BPerson`. Distinct, valid. |
| JSON Schema / OpenAPI | `Person` and `person` are distinct verbatim keys. No collision. |
| Filesystem portability | On case-insensitive filesystems (macOS APFS default, NTFS) the authored `.dl6` files `Person` vs `person` collide at the filename level, and any target that turns a rel name into a case-preserved identifier (SQLite, JSON keys, ProgramJson) inherits the same collision. Today nothing prevents a program from silently producing broken SQLite/TS/Rust output. |

Lowercase compatibility: unchanged. Existing lowercase programs (`person`, `ticket`, etc.) parse, roundtrip, and emit identically; the parser accepts both cases and no lowercase path was touched. Mixed-case `Person`+`person` is the only hazardous combination, and it is currently unrejected.

## Migration surface and golden families

No rename is implemented. Adopting capitalized as the convention touches no code; the only required change to make it safe is a **case-insensitive rel-name uniqueness check** (a collision rail), which is a guard, not a rename:

```dl6
rel Person(id: int).   # allowed
rel person(id: int).   # must be refused: case-insensitive collision with Person
```

Golden families that would need to be re-pinned if the convention were adopted and goldens regenerated (each currently keys off the authored spelling; a capitalization change alters every one):

| Family | Path pattern |
|---|---|
| rel_catalog / emit_ts | `v6/prolog/compile/out/*.ts` (`rel_catalog`, `STRUCT_TYPES`, DDL) |
| ProgramJson (Rust) | `v6/prolog/compile/out/*.rs` + `v6/sprefa-engine-rs/tests/*` |
| Typegen goldens (TS/Rust/JSON) | `v6/prolog/compile/test/typegen_golden/*.types.{ts,rs}` |
| Roundtrip renderings | `v6/prolog/compile/dl_view/*.dl6` (G1 regeneration) |
| Schema/OpenAPI fixtures | `v6/prolog/compile/test/emit/schema/*`, `emit/openapi/*` |

A capitalization flip (e.g. `person` → `Person`) is a **renaming of the rel identity**, not a cosmetic change: it changes SQLite table names, rel_catalog `local_name`, ProgramJson strings, and TS/Rust type names (`type_name/2` output can also change when underscores are involved). Any flip must regenerate all five families and re-run the reconciliation gate (`sweep.sh`, `typegen_golden.sh`, `roundtrip.sh`).

## Recommendation

**Allowed**, with one required guard before it becomes the convention.

Evidence for allowed:

- The parser, printer, and roundtrip handle capitalized relation heads, calls, and types exactly, and case is preserved (`basic`/`pun`/`rt_t` probes PASS).
- Every emitted target propagates `Person`/`Commit` verbatim (SQLite DDL, struct types, ref columns, `rel_catalog`, ProgramJson, JSON Schema, OpenAPI).
- Lowercase programs remain fully compatible; capitalized names dodge lowercase keyword collisions; the keyword-pun mechanism already assumes capitalized argument spelling.

Not preferred today, and the reason:

- There is **no case-insensitive uniqueness check** on rel names. `Person` + `person` in one module compiles cleanly yet emits SQLite DDL that fails at runtime (`table "person" already exists`) and duplicate TS/Rust type definitions (`CasePerson` twice). Cross-module case-only pairs are rescued only by the TS/Rust type layer's module prefix, not by storage.

Not blocked: nothing breaks for case-consistent programs; the canonical `Person`/`Commit` example compiles, roundtrips, and emits to every target correctly.

Action to reach preferred: add a case-insensitive rel-name collision rail in the compiler (refuse `person` when `Person` is declared in the same program, at the same point `rel_arity_collision` is thrown, `compile.pl:331`), and pin the five golden families above. That rail is the entire migration surface; no authored-program rename is required.
