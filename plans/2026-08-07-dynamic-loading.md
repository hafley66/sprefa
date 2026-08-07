# Dynamic loading: multi-file programs, schema-aware reload, batched reads

Base sha `e1a9696f` (the brief said `1c596cce`; `git log --oneline -1` disagrees). Prior art in `plans/2026-08-06-rel-type-symmetry.md` sections 3, 7, 9; this doc picks up at its h4/h5b boundary.

## Contents

| § | | § | |
|---|---|---|---|
| 1 | The three doors | 6 | Door C: signatures, lifetimes, storage |
| 2 | Receipts | 7 | Step order, per-step fail-first, gate |
| 3 | Build-vs-buy: module resolution, multi-rel read | 8 | Test plan: the MIN/MAX matrix, and the refusals it names |
| 4 | Door A: signatures, lifetimes, storage | 9 | Open, needs Chris |
| 5 | Door B: signatures, lifetimes, storage | | |

## 1. The three doors

| door | today | after |
|---|---|---|
| A multi-file | `compile(source: string)`, one string, `v6/tsv2/serve/0_compile.ts:98`. A module path parses and is refused by name: manifest buckets the three `7_module_path.pl` fixtures `unsupported`, reasons `module_path_unresolved([orchard,tree])`, `([orchard,fruit])`, `([orchard,north,tree])` | `use "lib.dl6".` splices Core items in file order; `orchard.tree(X)` mangles to `orchard__tree` |
| B reload | a reshaped table is kept: `isAlreadyExists` at `v6/tsv2/serve/3_engine.ts:224` swallows every "already exists" while replaying `program.ddl`, so a changed column keeps the OLD table and the OLD rows | five verdicts read off `__rel` fingerprints, executed before the swap |
| C read side | writes batch across the whole entity graph in one tick; reads are one rel per GET (`v6/tsv2/serve/4_http.ts:333` `handleIdbRead$`) | `POST /arrivals` grows a `read` list, so one call carries writes and reads over the same tick; the route count stays at five |

## 2. Receipts

| claim | evidence |
|---|---|
| v5 already ships this module system | `src/frontend.rs:1-23`: `use "path".` lexes with no new keyword, `expand` splices in file order, four include roots, canonical-path dedup, same-name-same-cols dedups, conflicting cols hard-error naming both paths |
| v6 parses a dotted path in functor position, refuses it, and has no import token | `v6/prolog/compile/parse_dl.pl:1062` `head_atom` (one `dotted_path` walk), `:1538` `relatom_item`; refused at `v6/prolog/0_dot_expand.pl:61` `refuse_rel_path_rule`; `grep -niE "\buse\b\|\bimport\b\|\binclude\b" v6/prolog/compile/parse_dl.pl` hits two prose comment lines (`:3`, `:36`) and zero grammar rules |
| the catalog contract and four hashes exist | `v6/prolog/lower.pl:637` `catalog_ddl_contract('__rel', ...)` 11 columns; `:648` `module_hash/2` (sha256 -> 16 hex), `:653` `rel_h_id/4`, `:659` `schema_hash/4`, `:665` `rule_hash/3`, `:684` `canonical_hash_key/2` |
| nothing in TS reads them, and the rows exist only as SQL text | `grep -rn h_schema v6/tsv2 --include=*.ts` -> only `v6/tsv2/tests/catalogRows.test.ts:57,59,158,164,168,169`; `v6/prolog/lower.pl:693` `catalog_row_ddl/4` `format`s ONE `INSERT OR IGNORE INTO "__rel" ... VALUES ~w`, appended into `Ddl` at `:4088` |
| the swap lane, and the catalog gate | `v6/tsv2/serve/4_http.ts:508` `switchMap`, topology comment `:16-36` (compile runs BEFORE the switch, so a bad program is answered 400 and the running one keeps turning); rows emit only for a program naming the catalog rel, `v6/prolog/analyze.pl:198` `program_uses_catalog/2` |
| writes already batch over any rels and both signs | `v6/tsv2/serve/4_http.ts:265` `checkArrivalBody`, per-entry checks `:279-303`, one `engine.submit` at `:322`; `IArrivalRow` / `IArrivalBatch` at `v6/tsv2/runtime/types.ts:42-53`, ordered and duplicate-preserving per decision q1 |
| graph-shaped ingress already normalizes target-first | `v6/prolog/0_type_plane.pl:339` `normalize_relation_reference_rows/3`; fixtures `relation_reference_target_and_parent_share_tick` (`4_struct_values.pl:454`) and `struct_arrival_key_order_canonicalized` (`:176`), both manifest bucket `compiled` |
| the route set is five, generated, and reads one rel at a time | `v6/prolog/compile/registry.pl:552-556` `http_route/3` -> `v6/tsv2/cli/0_inventory.ts:17-21`; `v6/tsv2/runtime/types.ts:603` `rows(rel: string): Observable<readonly IRow[]>`, through the program's own `finalSelect` decode |
| closed decisions carried in | four hashes `h_id`/`h_schema`/`h_rule`/`h_rows`; separator `__`; sha256 truncated to 16 hex behind `module_hash/2` (SWI ships no xxh3); DROP is a named refusal plus `--allow-drop`; the red walk is its own pass rather than a carry-driven tick drain (`drainCap` 100, `v6/tsv2/runtime/tickLoop.ts:43`); verdicts named for what they emit: CREATE, DROP+CREATE, DELETE+INSERT, no statement, DROP |

## 3. Build-vs-buy

### 3.1 Module resolution

| candidate | verdict |
|---|---|
| **Reuse v5's `frontend.rs` resolver** | It is a Rust function over `std::path`, and the v6 compiler is SWI-Prolog, so reuse means a shell hop per `use` or a rewrite. The DESIGN transfers whole (four roots, canonical dedup, col-conflict hard error, `src/frontend.rs:9-23`), the CODE does not cross the language line. Take the design, leave the binary. |
| **SWI `absolute_file_name/3` + `user:file_search_path/2`** | Ships in the base system and is already loaded by every compile. `file_search_path(dl_root, Dir)` gives the four roots as alternatives walked in declaration order with first-match semantics, and canonicalization (symlinks, `..`) is the predicate's own job, exactly the diamond key v5 uses. Cost: `file_search_path/2` is global to the SWI process, so a second concurrent compile sees the other's roots. |
| **Bespoke walk** | `exists_file/1` over a hand-built root list is roughly 12 lines with no global state, so the concurrency hazard vanishes. It reimplements symlink and `..` canonicalization by hand, and that is the part deciding whether a diamond dedups at all. |

**Choice: `absolute_file_name/3`, roots passed as an explicit list.** First-match and canonicalization come from the base system, the root list stays compile-local (pricing out the global-state cost), and the four roots are v5's.

### 3.2 Multi-rel read

| candidate | verdict |
|---|---|
| **graphql-js / Apollo over the rels** | A real library with a client ecosystem, and Door B's `relCatalog` is exactly the schema source a generator wants, so the schema itself would cost little. It costs a resolver per rel and carries the N+1 hazard this repo bans outright, and a GraphQL selection set is an unordered map, contradicting the ordered duplicate-preserving arrival decision at `v6/tsv2/runtime/types.ts:50-53`. Park as a later front once `__rel` is readable. |
| **SQL over HTTP (PostgREST shape, or a raw `POST /sql`)** | Zero design work: the seam is SQLite already and the emitted `finalSelect` already decodes. It costs the boundary, because `finalSelect` is the only decode turning storage rows back into dl values, so raw SQL either skips the decode or re-exports it, and it hands clients the physical table names Door A's mangler is about to rename. Refuse. |
| **Extend `POST /arrivals` with a `read` list** | Reuses the validated body path at `v6/tsv2/serve/4_http.ts:265-306`, the same ordered duplicate-preserving list decision, and the same tick. Costs one body field, one engine method, one response field. The five-route inventory at `registry.pl:552-556` grows by zero. |

**Choice: extend `POST /arrivals`.** `GET /idb/:rel` already suffices for the one-rel case and stays as-is; the missing piece is the batch form, and the read list inherits the arrival batch's settled ordering. graphql-js is the later front, gated on B landing `relCatalog`.

## 4. Door A: signatures, lifetimes, storage

```prolog
%! use_item(-Item, +S0, -S) is semidet.        % v6/prolog/compile/parse_dl.pl
%  ws0, lit_dcg(`use`), ws1, string_literal(Text), ws0, `.` -> Item = use(Text)
%  `use` stays a plain ident, so the lexer gains no keyword

%! include_roots(+EntryPath, -Roots) is det.
%  the four v5 roots, src/frontend.rs:9-19: dirname(EntryPath), $SPREFA_STD,
%  <crate>/std, <exe>/'..'; an absent env var contributes nothing
%! resolve_use_path(+Roots, +UseText, -AbsPath) is semidet.
%  first Root satisfying absolute_file_name(UseText, AbsPath, [relative_to(Root),
%  access(read), file_errors(fail)]); all roots fail -> use_path_unresolved(UseText, Roots)
```

```prolog
%! expand_uses(+EntryPath, +OnStack, +Loaded0, -Loaded, -prog(Decls,Rules), -ModuleTable) is det.
%  parse EntryPath -> Items; partition use(_) from Core items
%  canonical AbsPath in OnStack -> throw(use_cycle([AbsPath | OnStack]))
%  foldl over uses: AbsPath in Loaded0 -> skip, so a diamond parses once; else recurse
%    with [EntryPath | OnStack] and splice the child's Decls/Rules BEFORE the entry's own
%  merge col_type/3 by (Ref, ColumnName): equal type keeps one copy, conflict ->
%    throw(rel_col_conflict(Ref, PathA, PathB))
%  ModuleTable = [module(AbsPath, ModuleName, ModuleHash), ...] in load order

%! mangle_rel_path(+Segments, +ModuleTable, -FlatName) is semidet.
%  atomic_list_concat(Segments, '__', FlatName)  % orchard.tree -> orchard__tree
%  ModuleTable must own FlatName, else module_path_unresolved(Segments)
%  replaces refuse_rel_path_rule at v6/prolog/0_dot_expand.pl:61
```

```ts
// v6/tsv2/runtime/types.ts; 0_compile.ts:98 widens to this, rootDir reaching swipl as compile_dl6/3 arg three
export interface IProgramEntry { readonly text: string; readonly rootDir: string; }
compile(entry: IProgramEntry): Observable<IServedProgram>;
```

- Lifetimes: `Loaded` and `OnStack` are born at the first `expand_uses/6` call and die with that compile; `ModuleTable` lives to the end of `lower_program/2`; `rootDir` lives for one `POST /program`.
- Storage: no new table, since `__rel` carries the module row (`v6/prolog/lower.pl:698`) and every rel row carries `module_id`. Reads: the entry text once, then each resolved file exactly once, keyed by canonical absolute path. Writes: one `gen_served/<digest>.dl6` and one `.ts` under `GEN_SERVED_DIR`.
- Uniqueness: canonical absolute path is the load key; flat name `A__B` is unique per program or `rel_name_collision(Flat, ModuleChain)` fires.

## 5. Door B: signatures, lifetimes, storage

```prolog
%! catalog_rows(+ModuleName, +Rules, +RelPlans, -Rows) is det.
%  the row/11 list catalog_row_ddl/4 already builds at v6/prolog/lower.pl:693-701, lifted
%  out of the format/3 so the INSERT and the emitter read one source
%! rel_catalog_lines(+Rows, -Lines) is det.               % v6/prolog/emit_ts.pl
%  'const relCatalog: readonly IRelCatalogRow[] = [' , one object per row , '];'
%  program_export_lines/2 (emit_ts.pl:2264) gains the line '  relCatalog,'
```

```ts
// v6/tsv2/runtime/types.ts. IGenProgram (:361) gains `readonly relCatalog: readonly IRelCatalogRow[]`.
export interface IRelCatalogRow {
  readonly relId: number; readonly parentId: number; readonly ordinal: number;
  readonly localName: string; readonly kind: "primitive" | "module" | "rel" | "column";
  readonly typeId: number; readonly arity: number; readonly moduleId: number;
  readonly hId: string; readonly hSchema: string; readonly hRule: string;
}
```

```ts
export type RelVerdict = "create" | "recreate" | "refill" | "keep" | "drop";
export interface IReloadPlan {
  readonly verdicts: ReadonlyMap<string, RelVerdict>;  // key = hId
  readonly statements: readonly string[]; readonly refusals: readonly string[];
}
// plan: index both sides by hId over kind === "rel" rows only, then per hId
//   absent in prev -> create CREATE | hSchema differs -> recreate DROP+CREATE
//   hSchema equal, hRule differs -> refill DELETE+INSERT | both equal -> keep
//   absent in next: allowDrop -> drop DROP, else refusals rel_drop_needs_allow_drop(name)
export interface IReloadPlanner {
  plan(prev: readonly IRelCatalogRow[], next: readonly IRelCatalogRow[], allowDrop: boolean): IReloadPlan;
}
```

- `bootServedProgram` (`3_engine.ts:228`) takes the plan and deletes `isAlreadyExists` (`:224`); CREATE text still comes from `program.ddl`, and the plan decides which statements run. Lifetimes: `previous` rows are read at swap time and die when the plan is built; `IReloadPlan` is born before the `switchMap` inner subscribes (`4_http.ts:508`) and dies after boot; `program.relCatalog` lives as long as the imported module.
- Storage: `__rel`, 11 columns, PRIMARY KEY over all 11, `WITHOUT ROWID` (`v6/tsv2/tests/catalogRows.test.ts:57`), plus `__rel_parent (parent_id, local_name)` (`v6/prolog/lower.pl:645`).

Reads then writes, one swap:

1. `SELECT * FROM "__rel"`, a missing table giving `previous = []` so cold boot is the same path with every verdict `create`; then `ReloadPlanner.plan(previous, program.relCatalog, allowDrop)`.
2. `refusals` non-empty -> 400 carrying the refusal term, and the running program keeps turning, matching the compile-before-switch order at `4_http.ts:16-36`; else run `plan.statements`: DROP, CREATE, DELETE, INSERT; then replay the rest of `program.ddl` (indexes, delta tables, `__tick`), all `IF NOT EXISTS` already; then `DELETE FROM "__rel"` and the emitted `INSERT OR IGNORE`.

Uniqueness: `(module_id, h_id)` names exactly one rel across reloads, `ordinal` disambiguates its column children, and primitives sit at reserved ids 1..5 by position (`lower.pl:709-715`), so both sides agree without a hash. `h_rows` and the red walk stay out of this plan.

## 6. Door C: signatures, lifetimes, storage

```ts
// v6/tsv2/runtime/types.ts, beside IArrivalBatch (:42-53). `read` is ORDERED and
// duplicate-preserving, the same decision IArrivalBatch already made.
export interface IRelSnapshot {
  readonly rel: string; readonly rows: readonly IRow[];
  /** the tick these rows decoded at; the same number the SSE lane prints */
  readonly atTick: number;
}
export interface IArrivalRequest {          // both fields may be empty
  readonly batch: IArrivalBatch; readonly read: readonly string[];
}
```

```ts
// ILiveEngine (types.ts:600) gains one method beside rows(rel):
//   concatMap over rels, one finalSelect per entry, request order kept
//   batch non-empty -> submit first, atTick = the last outcome's tick
//   batch empty     -> no tick turns, atTick = the current tick
readMany(rels: readonly string[]): Observable<readonly IRelSnapshot[]>;
// 4_http.ts checkArrivalBody (:265) also validates `read`: absent -> []; not an array ->
//   400 "'read' must be an array of rel names"; outside finalSelect -> 400 "not a readable rel".
// response: { ticks: [...], rows: IRelSnapshot[] }   ticks unchanged from today
```

- Read and write share one tick: `engine.submit(batch)` returns the settle tick plus its drain ticks (`types.ts:596-602`), so `readMany` runs after `toArray()` and observes the LAST of them. An empty batch turns no tick, so a read-only call costs one SELECT per named rel. The SSE lane is untouched: `GET /ticks` keeps one `data:` line per tick (`4_http.ts:380`), and `IRelSnapshot.atTick` is the same number, so a client holding both aligns a snapshot against the stream with no correlation id and no second lane.
- Lifetimes: `IRelSnapshot[]` lives for one response; `readMany` holds no cursor, no subscription field, no cache, so the one-manual-subscribe ratchet is unchanged. Storage, reads, writes, uniqueness: no new table and no new route; exactly one `finalSelect` per entry in `read`, in request order, duplicates included; writes are the arrivals already validated at `4_http.ts:279-303`; no uniqueness is imposed on `read`, because a repeated name yields a repeated snapshot, mirroring duplicate-preserving arrivals.
- Sequencing: C depends on nothing in A, and reads better after B, because `relCatalog` lets the read validator check names against the catalog rather than against `finalSelect` keys. Order: B, then C, then A.

## 7. Step order, per-step fail-first, gate

| step | lands | RED before / GREEN after | gate |
|---|---|---|---|
| B1 | `catalog_rows/4` lifted out of `catalog_row_ddl/4`; `rel_catalog_lines/2`; `IRelCatalogRow` + `relCatalog` on `IGenProgram` | `catalogRows.test.ts` case `relCatalog exposes the same rows the INSERT carries`: RED, `program.relCatalog` is `undefined` | `cd v6 && just typecheck && just tsv2-test` |
| B2 | `ReloadPlanner.plan`, pure over two row arrays | new `v6/tsv2/tests/reloadPlan.test.ts`: RED, the module does not exist | `cd v6 && just tsv2-test` |
| B3 | `bootServedProgram` runs the plan; `isAlreadyExists` deleted | new `v6/tsv2/tests/servedReload.test.ts` case `a reshaped rel changes the table shape`: RED today by `3_engine.ts:224` | `cd v6 && just tsv2-test && just serve-endurance` |
| B4 | `--allow-drop` through `POST /program`; the named refusal on the 400 path | `servedReload.test.ts` case `dropping a rel without --allow-drop is refused by name`: RED, every unknown-rel swap silently keeps the table | `cd v6 && just tsv2-test && just serve-leak-soak` |
| C1 | `IRelSnapshot`, `readMany` on `ILiveEngine` | new `v6/tsv2/tests/readMany.test.ts`: RED, `readMany` is undefined | `cd v6 && just typecheck && just tsv2-test` |
| C2 | `read` on the POST body and `rows` on the response; `registry.pl:553` summary updated | `readMany.test.ts` served cases plus `cd v6 && just bop` inventory parity: RED, `read` is ignored | `cd v6 && just tsv2-test && just sweep` |
| A1 | `use_item//1` in the text door | text-door fixture `use_item_parses_a_sibling_path`: RED as `unsupported_surface` | `cd v6/prolog && bash compile/scripts/text_door_receipt.sh` |
| A2 | `include_roots/2` + `resolve_use_path/3` | plunit cases in `v6/prolog/compile/test/plunit_tests.pl`: RED, predicate undefined | `cd v6 && just plunit` |
| A3 | `expand_uses/6`: splice, diamond-once, cycle guard, col-conflict throw | new `v6/prolog/conformance/fixtures/7_use_include.pl`: RED, `expand_uses/6` undefined | `cd v6 && just conformance` |
| A4 | `mangle_rel_path/3` replaces `refuse_rel_path_rule` | the three `7_module_path.pl` fixtures move manifest bucket `unsupported` -> `compiled` | `cd v6/tsv2 && bash scripts/sweep.sh`, then grep `7_module_path` in `v6/prolog/compile/out/manifest.json` |
| A5 | `IProgramEntry`; `compile_dl6/3`; `0_compile.ts:98` widened | served case `a POSTed program that uses a sibling file compiles`: RED, `compile` takes a string | `cd v6 && just tsv2-test && just green-all` |

B1 -> B2 -> B3 -> B4 is a chain and B1 depends on nothing. C1 -> C2, and C1 wants B1 for catalog-backed name
validation. A1 -> A2 -> A3, A4 needs A3's `ModuleTable` for the ownership check, A5 needs A2's root list. Each step lands green alone.

## 8. Test plan: the MIN/MAX matrix, and the refusals it names

Empty, one, many, and the boundary that breaks it, per new predicate and per new TS seam. Prolog cells name the fixture
and the `v6/prolog/compile/out/manifest.json` bucket it must reach; TS cells carry no bucket, because the manifest scores compiler fixtures only.

### 8.1 Door A, `expand_uses/6` + `resolve_use_path/3`, in `conformance/fixtures/7_use_include.pl`

| case | fixture name | bucket / named refusal |
|---|---|---|
| empty: zero imports | `use_absent_program_unchanged` | `compiled` |
| one import | `use_one_sibling_splices_in_file_order` | `compiled` |
| many: three-deep chain | `use_chain_three_deep_keeps_load_order` | `compiled` |
| many: diamond | `use_diamond_parses_each_file_once` | `compiled` |
| same name, same cols | `use_same_rel_same_cols_dedups` | `compiled` |
| boundary: cycle | `use_cycle_refuses_naming_the_chain` | `unsupported`, `use_cycle(PathChain)` |
| boundary: self-import | `use_self_refuses` | `unsupported`, `use_cycle([Self])` |
| boundary: missing file | `use_missing_file_refuses_naming_the_roots` | `unsupported`, `use_path_unresolved(Text, Roots)` |
| boundary: same name, conflicting cols | `use_same_rel_conflicting_cols_refuses` | `unsupported`, `rel_col_conflict(Ref, PathA, PathB)` |
| boundary: two chains, one flat name | `use_two_chains_one_flat_name_refuses` | `unsupported`, `rel_name_collision(Flat, ModuleChain)` |

COUNT rail, because a naive recursive loader is exponential over a diamond chain and end-state equality alone would hide it: `use_diamond_parses_each_file_once` asserts a per-canonical-path parse counter of exactly 1, and the three-deep chain asserts 4 parses for 4 files. `module_path_unresolved(Segments)` survives A4 as the ownership failure, so `7_module_path.pl` keeps a fixture that still throws it.

### 8.2 Door B, `ReloadPlanner.plan`, in `v6/tsv2/tests/reloadPlan.test.ts`

| case | test name | verdict / refusal |
|---|---|---|
| empty: `previous = []` | `cold boot is all create` | every `create` |
| one: no change | `an unchanged program keeps everything` | all `keep`, `statements.length === 0` |
| column added | `a column added recreates` | `recreate` |
| column dropped | `a column dropped recreates` | `recreate` |
| type changed | `a type change recreates` | `recreate` |
| key changed | `a key change recreates` | `recreate` (`schema_hash/4` takes `KeyOrNone`, `lower.pl:659`) |
| rule body changed | `a rule body change refills` | `refill` |
| rel added | `a new rel creates` | `create` |
| rel dropped, `allowDrop` false / true | `a drop without allow-drop is refused by name` / `a drop with allow-drop drops` | `rel_drop_needs_allow_drop(localName)` / `drop` |
| many: two changes in one load | `a reshape and a rule change in one load` | one `recreate` plus one `refill`, both present |
| boundary: a load failing mid-DDL | `servedReload.test.ts`: `a mid-DDL failure leaves the previous program answering` | 500, and `GET /idb/:rel` still serves the previous program's rows |

COUNT rail, because today every swap replays the whole `program.ddl` list (`3_engine.ts:228-235`): a no-change reload asserts `plan.statements.length === 0` AND a served counter of zero `seam.runner.execute` calls for schema statements across the swap.

### 8.3 Door C, `readMany` + the `read` body field, in `v6/tsv2/tests/readMany.test.ts`

| case | test name | expectation |
|---|---|---|
| empty: `read: []` | `an empty read list returns no snapshots` | `rows: []`, zero extra SELECT |
| one rel | `one named rel matches GET /idb/:rel` | byte-equal to the single route's payload |
| many rels | `three rels return three snapshots in request order` | order preserved |
| many: duplicate name | `a repeated rel yields a repeated snapshot` | duplicates preserved, matching `IArrivalBatch` |
| write plus read | `the read observes the tick the write produced` | `atTick` equals the last `ticks[]` entry |
| read only | `an empty batch reads at the current tick and turns none` | tick number unchanged |
| boundary: unknown rel | `an unknown rel is a 400 naming it` | 400, `'x' is not a readable rel` |
| boundary: read during a swap | `a read mid-swap answers 409 or the new program, never a mixed shape` | one program's shape only |

COUNT rail, because a per-rel loop that re-derives is the N+1 this repo bans: `reading K rels runs exactly K select statements` asserts the statement counter equals `read.length`, and an EXPLAIN check on the multi-read path asserts SEARCH rather than SCAN on the keyed rels.

## 9. Open, needs Chris

| # | decision | recommended default | cost of getting it wrong |
|---|---|---|---|
| 1 | Does the HTTP door learn `__rel`, or do users curl the flat name? | Flat name only for now; add `GET /rel/:hId` when a caller asks. | Teaching the door `__rel` early freezes the hash format into the public API, and that format is 16 hex chars picked for compile speed rather than for humans. |
| 2 | Monomorphized generic instantiation: `kind='instance'` with `parent_id` = the generic, or a mangled `kind='rel'` row with no link back? | `kind='instance'`, `parent_id` = the generic. | A mangled row loses the edge back to the generic, so "reshape the generic, reshape its instances" needs a name-parse to recover what a column would have held. |
| 3 | Primitives: reserved ids 1..5 as shipped, or looked up by name at seed? | Keep 1..5 (`v6/prolog/lower.pl:709-715`). | Positional ids keep a recompile byte-stable, which the emitted-module byte-identity receipt depends on; name lookup buys extensibility nobody has asked for and costs that receipt. |
| 4 | Dotted head fork: A contribute-only (`orchard.tree(X) <- ...` requires `orchard` to declare `tree` already) or B create-on-write (the head mints the rel)? | Fork A. | B lets a typo in a head mint a silent new rel, the exact failure v5's col-conflict hard error exists to stop (`src/frontend.rs:21-23`). A costs one extra decl per rel and makes the ownership check in `mangle_rel_path/3` total. |
| 5 | Does `--allow-drop` ride the POST body, a query parameter, or a server-start flag? | Query parameter `?allowDrop=1` on `POST /program`. | A server-start flag makes every swap for that process droppable, so one careless boot removes the guard for the whole session. |
| 6 | Does the `read` list belong on `POST /arrivals`, or does a read-only call deserve its own route? | Keep it on `POST /arrivals`; a read-only call sends an empty batch. | A sixth route splits the tick-alignment story across two handlers, and `registry.pl:552-556` plus `0_inventory.ts` would then describe two ways to read one rel. |
