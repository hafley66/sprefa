# One concept: a rel with external arrivals

Issue `@one-concept-rel`, epic `@cheap-fast-analysis`. Measured at `107f292dd`.
Every claim carries a `path:line` or a command whose output is pasted.

## Contents

1. [Gates this lane ran](#1-gates-this-lane-ran)
2. [The decision already exists](#2-the-decision-already-exists)
3. [Inventory: every construct today](#3-inventory-every-construct-today)
4. [What the corpus spends](#4-what-the-corpus-spends)
5. [The blocker: the proposed surface is already taken](#5-the-blocker-the-proposed-surface-is-already-taken)
6. [The collapse, type signatures first](#6-the-collapse-type-signatures-first)
7. [Instance lifetimes](#7-instance-lifetimes)
8. [Storage layout, reads and writes, uniqueness](#8-storage-layout-reads-and-writes-uniqueness)
9. [Migration table: every consumer site](#9-migration-table-every-consumer-site)
10. [The `emit_ts.pl` byte-identity argument](#10-the-emit_tspl-byte-identity-argument)
11. [Risks, each with a probe](#11-risks-each-with-a-probe)
12. [Three-step lane plan](#12-three-step-lane-plan)
13. [What needs the user before code](#13-what-needs-the-user-before-code)

---

## 1. Gates this lane ran

| gate | command | result |
|---|---|---|
| plunit | `cd v6 && just plunit` | `PLUNIT jobs=12 declared=995 results=1041 passed=1041 failed=0 timeout=0 wall=5.60s` |
| conformance | `cd v6/prolog/conformance && swipl -g go -t halt go.pl` | 439 PASS, 0 FAIL, rc=0 |
| dead-module rail | `bash v6/dl/deadcode/dead-module-rail.sh ~/projects/hafley-rs 'crates/*/src/*.rs'` | `findings=0 unproven=16 unreachable=0`, rc=0 |

The rail gate does not run inside a boop worktree. `v6/sprefa-engine-rs/Cargo.toml:28`
spells `soopy = { path = "../../../hafley-rs/crates/soopy" }`, which from
`.boop-worktrees/plan/one-rel-with-arrivals/v6/sprefa-engine-rs` resolves to
`.boop-worktrees/plan/hafley-rs` and does not exist, so `cargo build` fails
before a tick runs. The number above was measured in the main checkout at the
identical sha `107f292dd`. A lane that has to run this rail needs that path
made worktree-safe first; it is a one-line request in the PR body, not part of
this plan's scope.

## 2. The decision already exists

`v6/prolog/conformance/rulings.pl:301` `ruling(edb_definition,
never_headed_rel_is_pure_subject, user, ...)` reads, verbatim from the file:

> EDB is DEFINED BY ABSENCE: a rel (enum-shaped or not) that no rule ever heads
> is pure input -- an rx Subject the world pushes into (schedule rows, binds,
> host responses). No decl word marks it; being un-headed IS the mark.

`rulings.pl:291` `ruling(no_policy_suffix_words, ...)` says the same thing from
the other side: a rel with no specificity word is just a table.

The compiler already implements it. `compile.pl:262`:

```prolog
subtract(AllRefs, [CatalogName/CatalogArity | DerivedRefs], ArrivalTargets),
```

Arrival targets are every ref the program mentions minus the compiler-owned
catalog minus the rule-headed ones. A `rel` with no rule head is an arrival
target with no declaration word asked for.

`sh` and `bind` are the two decl words that contradict that decision. They mark
a rel as world-pushed with a keyword, which is exactly what `edb_definition`
and `no_policy_suffix_words` say no decl word does. The user's 2026-08-21
statement is not a new design; it is the two decisions finished.

Two more decisions point the same way. `rulings.pl:199`
`ruling(spine_residency, stdlib_rels_and_binds_not_kernel, ...)` and
`rulings.pl:212` `ruling(clock_residency, world_fed_bind_not_construct, ...)`
both put the clock and the spine in the language rather than the kernel;
collapsing `bind` into `rel` is the last step of that.

## 3. Inventory: every construct today

| construct | parsed at | lowered at | runtime reader | what only it carries |
|---|---|---|---|---|
| `sh` declaration | `compile/parse_dl_dcg.pl:1027-1041` (`sh_head//2`, `sh_decl_stmt//1`); dispatched `:524` | `1_host_expand.pl:183-200` `compile_host_decl/2`; port rows `lower.pl:1616-1632` | `hosts.rs:1438` `collect` reads `HostPlanData` | the input/output split and the template |
| `sh` with no `->` | `compile/parse_dl_dcg.pl:1043-1049` | named unsupported `host_decl_inferred` | none | nothing; it is a diagnostic |
| dotted host path | `compile/parse_dl_dcg.pl:1027-1030`, `record_host_path/2`; `:712` `module_path_name/2` | atom is the `__` join | `hosts.rs:500-508` `scip_namespace` | two evidence namespaces under one interface |
| `bind interval` | `compile/parse_dl_dcg.pl:1013-1017`; dispatched `:521` | `1_host_expand.pl:405-419` `validate_bind_decl/3`; port row `lower.pl:1633-1640` | TypeScript door only | a clock cadence with no demand rel |
| `bind watch` | same | same | TypeScript door only | continuous discovery with a delete arrival |
| host demand rel | generated, `1_host_expand.pl:562-582` `generated_host_decls/7` | `1_host_expand.pl:371-375` `host_relation_refs/3` | `hosts.rs:1441-1451` matches `plan.demand_rel` | `identity_digest`, `witness_digest`, salts |
| host response rel | same, `keyed(Ref,[1,2])` at `1_host_expand.pl:579` | same | `hosts.rs:1409` writes it | `ordinal`, and the inputs echoed back |
| adapter row | `.adapters.json` sidecar, `types.rs:646-651` | not compiled at all | `types.rs:664-676`, `driver.rs:119-130` | the executor binding, out of band |
| `host_input_contract/3` | `compile/registry.pl:342-478`, 28 rows | `registry.pl:525-533` `host_input_roles/3` | none | the identity/freshness split, keyed on the NAME |
| `host_output_contract/3` | `compile/registry.pl:512-514` | scip namespaces only | none | one column list shared by two host names |
| template string | `compile/parse_dl_dcg.pl:1060-1070` | `1_host_expand.pl:310-319` `validate_template/4` | `hosts.rs:1515` `fill_template`, `:1516` grouping key | the applicative fold key, and nothing else on the Rust door |
| `--arrive` seed | `bin/emit_rust_harness.rs:61-115` | not compiled | `bin/emit_rust_harness.rs:259` | a first-tick row from the command line |
| scripted `__host_response_*` | schedule file | `check_world_shapes` at `compile.pl:245` | `driver.rs` replay path | a fixture answer with no executor |

Twelve prolog files name `sh_decl` or `bind_decl`, 52 lines total:

```
grep -n "sh_decl\|bind_decl" v6/prolog/*.pl v6/prolog/compile/*.pl | wc -l   -> 52
```

`0_ast_expand.pl:185`, `0_program_check.pl:802,805`, `2_subscribe.pl:79`,
`print_dl.pl:231,232,307,316`, `emit_rust.pl:624`, `lower.pl:1608-1633,7122`,
`use_resolve.pl:223,224`, `1_host_expand.pl:48,54,55,184,405,602,606`,
`compile/parse_dl_dcg.pl:47,59,165,166,272,282,297,329,521,524,990,991,1013,1032,1043`,
`emit_ts.pl:507,519,524`, `ARCH.pl:209,406,433,588,590,711,890`,
`compile/registry.pl:194,196,304`.

### The Rust door has no `bind` at all

```
grep -c "bind" v6/prolog/emit_rust.pl   -> 0
```

`emit_rust.pl:633-663` builds `ProgramDict` with `host_plans` and no
`bind_plans`. `emit_ts.pl:515-525` `world_plan_lines/2` builds both.
`bind interval` and `bind watch` exist only on the paused TypeScript door.
Any program using them is already dead on the door the user has chosen. The
collapse is what makes them live again.

## 4. What the corpus spends

| measure | command | count |
|---|---|---|
| `sh` declarations, authored `.dl6` | `grep -rhoE "^\s*sh [a-z_.]+\(" --include=*.dl6 v6/` | 179 |
| `bind` declarations, authored `.dl6` | `grep -rhoE "^\s*bind [a-z_]+\(" --include=*.dl6 v6/` | 28 (16 `watch`, 12 `interval`) |
| distinct `sh` names, top | `extract` 9, `comment_fact` 8, `call_ref` 8, `files` 7, `call_node_at` 7 | |
| `.adapters.json` sidecars | `find v6 -name "*.adapters.json"` | 15 |
| adapter names in use | `sprefa_extract` 24, `soopy` 2, `sprefa_scip` 2, `soopy_files` 2, `cargo_metadata` 1, `fixture` 1 | 6 distinct |
| `host_input_contract/3` rows | `grep -c "^host_input_contract(" v6/prolog/compile/registry.pl` | 28, plus one clause over 4 scip names at `registry.pl:504-507` |
| manifest entries | `v6/prolog/compile/out/manifest.json` | 433: 335 `compiled`, 98 `unsupported` |
| manifest rows naming host or bind | reason grep | 4, all `unsupported` |

The four manifest rows:

| fixture | bucket | reason |
|---|---|---|
| `host_output_column_shadows_runtime_ordinal` | unsupported | `host_column_shadows_runtime(look,output,ordinal)` |
| `host_input_column_shadows_runtime_witness` | unsupported | `host_column_shadows_runtime(peek,input,witness_digest)` |
| `duplicate_host_name_is_refused` | unsupported | `duplicate_host_decl(look)` |
| `repo_on_bind_watch_is_refused` | unsupported | `bind_repo_column(watch)` |

No `compiled` fixture's reason names a host or a bind, so no `compiled` bucket
row moves on a surface change alone. The four above are the whole exposed
surface in the manifest, and each is a check that has to keep firing under the
new spelling.

## 5. The blocker: the proposed surface is already taken

The brief's candidate is:

```
rel extract(path: text, digest: text) -> (record: text, family: text, callee: text).
```

That line compiles today, rc=0, no named unsupported construct, and means
something else.

`compile/parse_dl_dcg.pl:582-596` `relation_arrow_output//4` reads a
`type_expr` after `->` and appends ONE column called `return`.
`compile/parse_dl_dcg.pl:764-770` `type_base//1` reads a `(`-led group as an
ANONYMOUS PRODUCT TYPE. Measured output for the two-column form:

```
rel_columns:
  extract                                : path, digest, return
  __anon_extract_return_ed2df8f08021f1d6 : record, callee
rel_column_types:
  extract : text, text, ref
arrival_targets: __anon_extract_return_ed2df8f08021f1d6, extract, want
host_plans: []
```

So `->` on a `rel` is spent. The brief's stated budget, "`rel` syntax plus the
existing `->` arrow, no new keywords", cannot be met without taking the arrow
back from the anonymous product type.

### How much it costs to take it back

| spelling | occurrences in authored `.dl6` | where |
|---|---|---|
| `rel N(...) -> <scalar or applied type>` | 13 | generics fixtures, `Result(...)` returns |
| `rel N(...) -> ( ... )` | 1 | `v6/dl/fixtures/anonymous-type-syntax.dl6:5` |
| anonymous product in COLUMN position | still legal, untouched | `v6/sprefa-extract/tree-sitter-dl6/fixtures/anonymous-types.dl6:5` `rel resident(result: (a: int, b: text)).` |

Exactly one authored line uses the parenthesized arrow form, and the anonymous
product already has a second spelling in column position that the tree-sitter
fixture uses. The disambiguation is decidable standing at the `(`: a
parenthesized group after `->` on a `rel` is a response column list; anything
else after `->` is the existing return column.

This is a language design call and `CLAUDE.md` says those happen with the user
in the room. Section 13 states it as a fork with its cost, not as a decision.

## 6. The collapse, type signatures first

### 6.1 The surface term

```prolog
%! arrival_decl(+Name:atom, +Inputs:list(col), +Outputs:list(col),
%!              +Identity:key(list(int))) is det.
%
%   Name      the rel name the author wrote; the probe goal binds to it.
%   Inputs    the left of the arrow, in authored order.
%   Outputs   the right of the arrow, in authored order. [] = a seed rel.
%   Identity  positions within Inputs that identify the answer. Every other
%             input position is freshness. Absent = every input is identity,
%             which is registry.pl:531-533 identity_roles/2 unchanged.
%
% BODY, as pseudo-code:
%   parse `rel N(Ins) -> (Outs) key(Ps).`
%   -> emit sh_decl(N, Ins, Outs, template('')),
%           arrival_identity(N, Ps)
%   and nothing else. Every phase after the parser sees the term it sees today.
```

The new surface desugars INTO `sh_decl/4`, not the other way round. That
direction is what keeps `emit_ts.pl` byte-identical; section 10 argues it.

### 6.2 The declaration forms after the collapse

```dl6
# An arrival rel with a demand: rows of the right side arrive from the executor
# keyed by this rel's name, once per distinct witness of the left side.
# `key(1)` says column 1 identifies the answer, so column 2 is freshness.
#
# rx: demand$ = source$.pipe(map(row => ({ path: row.path, digest: row.digest })),
#                            distinct(demand => witnessOf(demand)));
#     response$ = demand$.pipe(mergeMap(demand => executorFor('extract')(demand)),
#                              mergeMap(rows => from(rows.map(withOrdinal))));
rel extract(path: text, digest: text) -> (record: text, family: text, callee: text) key(1).

# A seed rel: no arrow and no rule head, so rows arrive from --arrive or a
# schedule. This is what compile.pl:262 already builds and needs no new word.
#
# rx: want$ = new Subject<{ glob: string }>();
rel want(glob: text).

# The clock. `bind interval(period, bucket)` today; an arrival rel whose
# executor is the interval one after. Column 1 is the configuration column
# (registry.pl:309-314) and stays identity; the bucket is what arrives.
#
# rx: interval$ = of(...periods).pipe(mergeMap(p => interval(p * 1000).pipe(
#       map(n => ({ period: p, bucket: n })))));
rel interval(period: int) -> (bucket: int) key(1).

# The watcher. `bind watch(glob, path, digest)` today. The digest IS the salt
# (rulings.pl:158 salt_minting = content_addressed), so an unchanged save is a
# zero delta at the rel boundary and nothing downstream re-derives.
# Presence and absence ride the arrival SIGN, never a second column.
#
# rx: watch$ = of(...globs).pipe(mergeMap(g => watcher(g).pipe(
#       map(ev => ({ glob: g, path: ev.path, digest: ev.digest, sign: ev.sign })))));
rel watch(glob: text) -> (path: text, digest: text) key(1).
```

Three surface forms become one. The arrow says a demand exists; its absence
says the rows come from outside with no demand.

### 6.3 Where each piece of the old machinery lands

| carried today by | after the collapse | smallest change |
|---|---|---|
| `sh` keyword | the arrow's presence on a `rel` | `rel_stmt//1` gains one alternative |
| `bind` keyword | the arrow plus an adapter row naming `live_interval` / `live_watch` | `registry.pl:327-328` `bind_executor/2` rows become adapter names |
| template string | `template('')` internally, never authored | `validate_template/4` at `1_host_expand.pl:310-319` becomes conditional on a non-empty template |
| `host_input_contract/3` roles | `key(...)` positions on the declaration, falling back to the registry row | `host_input_roles/3` at `registry.pl:525-529` gains one clause ahead of the existing two |
| `host_output_contract/3` | unchanged; it is the scip interface table, not a per-program fact | none |
| executor binding | the adapter row, unchanged | none |
| witness digest | unchanged, `1_host_expand.pl:517-521` `digest_expr/6` | none |
| response projection by column presence | unchanged, `hosts.rs:1240-1271` | none |

The `key(...)` proposal reuses an existing modifier slot. `key_clause//1` at
`compile/parse_dl_dcg.pl:863-864` parses `key(<int>, ...)` and takes integer
positions only, so `key(1)` parses today and `key(path)` does not. Whether the
lane widens `key_clause` to accept column names is a separate small call; the
positional form is enough to land.

### 6.4 Where the four planning layers disagree

Stated because the planning protocol asks for it.

| layer | says |
|---|---|
| type signature | `arrival_decl/4` is one term with an optional identity list |
| pseudo-code body | it desugars to TWO terms, `sh_decl/4` and `arrival_identity/2` |
| instance lifetime | the SURFACE is one rel; the LOWERING is still two rels, `__host_demand_N` and `__host_response_N` |
| storage layout | `key(...)` means UNIQUE positions on an ordinary stored rel (`lower.pl:1095-1146` `set_rel_pk_sql/7`) and demand identity on an arrival rel. One word, two jobs |

The last row is the sharpest disagreement and the lane has to hold both
readings without letting either leak into the other's code path.

## 7. Instance lifetimes

| holder | lives | held today at | held after |
|---|---|---|---|
| `HostLiveRunner.claimed` | one `run_schedule_live` call, all ticks | `hosts.rs:1285`, keyed `"{plan}|{witness}"` at `:1388-1390` | unchanged |
| `HostLiveRunner.plans` | borrowed from `GenProgram` for the run | `hosts.rs:1282`, built `driver.rs:126-130` | unchanged |
| `HostLiveRunner.adapter_rows` | cloned once per run | `hosts.rs:1284`, read `types.rs:664-676` via `DL_ADAPTERS_DIR` | gains the two clock/watch adapter names |
| `SprefaExtractExecutor.batches` | process, `LazyLock` static | `hosts.rs:469` `Mutex<BTreeMap<String, soopy::GitBatch>>`, one `git cat-file --batch` per repository root | unchanged |
| `SprefaExtractExecutor.roots` | process, `LazyLock` static | `hosts.rs:476`, directory to repository root; `soopy::discover` measured 28ms and asking per file cost 2.29s of a 3.55s run | unchanged |
| `ScipNamespaceExecutor.folds` | process, `LazyLock` static | `hosts.rs:534` `Mutex<HashMap<String, Arc<ScipFold>>>` | unchanged |
| `ScipNamespaceExecutor.sets` | process, primed per tick at `hosts.rs:1481-1483` | `hosts.rs:532` | unchanged |
| clock state | does not exist on the Rust door | nothing | NEW: an `IntervalExecutor` needs a tick counter per period, per run, not per process |
| watcher handles | does not exist on the Rust door | nothing | NEW: one recursive watcher per glob, per run; `1_host_expand.pl:377-400` states the handle budget is per working tree and that is why `repo` on a watch is a named stop |

The two NEW rows are the whole runtime cost of the collapse. Everything else in
the table is already built and does not move.

## 8. Storage layout, reads and writes, uniqueness

The surface collapses; the lowering does not. `1_host_expand.pl:562-582`
`generated_host_decls/7` keeps minting two rels per arrival rel with a demand,
and section 6.4 names that as a deliberate disagreement with the surface.

### Layout

| rel | columns, in order | arity | uniqueness |
|---|---|---|---|
| `__host_demand_N` | `identity_digest`, `witness_digest`, identity inputs, freshness salts | `2 + inputs + salts` (`1_host_expand.pl:566`) | none declared; the demand rule is a level rule and the level dedupes |
| `__host_response_N` | `witness_digest`, `ordinal`, identity inputs, outputs | `2 + inputs + outputs` (`1_host_expand.pl:568`) | `keyed(Ref, [1, 2])` at `1_host_expand.pl:579`, so late answers replace rows positionally |

`witness_digest` is not a hash. It is the literal concatenation built by
`digest_expr/6` at `1_host_expand.pl:517-521`. Measured on the dead-module rail
against `~/projects/hafley-rs`:

```
__host_demand_files
  identity_digest  identity|files|glob:text=crates/*/src/*.rs
  witness_digest   witness|files|glob:text=crates/*/src/*.rs
  glob             crates/*/src/*.rs

__host_response_files  (ordinal 0)
  witness_digest   witness|files|glob:text=crates/*/src/*.rs
  ordinal          0
  glob             crates/*/src/*.rs
  path             crates/boop-acp/src/channel.rs
  digest           589d1271765202c7cdc505fb0e64930bc58102c8

__host_demand_extract
  identity_digest  identity|extract|path:text=crates/boop-acp/src/channel.rs
  witness_digest   witness|extract|path:text=crates/boop-acp/src/channel.rs|digest=589d1271765202c7cdc505fb0e64930bc58102c8
  path             crates/boop-acp/src/channel.rs
  digest           589d1271765202c7cdc505fb0e64930bc58102c8
```

The identity digest names only identity inputs; the witness digest appends the
freshness ones with `salt_digest_parts/2` at `1_host_expand.pl:547-550`, which
writes `|name=value` with no type segment, while identity inputs carry
`|name:type=value` from `input_digest_parts/3` at `:523-526`. That asymmetry is
observable in the rows above and any lane touching `digest_expr/6` moves every
witness in the corpus.

The reserved column names are `witness_digest`, `ordinal`, `identity_digest`
(`1_host_expand.pl:300-302`). An author column that collides is a named stop at
`:304-308`, and `1_host_expand.pl:257-291` records the fail-first receipt: a
host output called `ordinal` used to VANISH, because two identical `col_type/3`
terms were folded by `dedupe_terms/2` while the arity kept the slot.

### Sequence of reads and writes, one tick

| step | who | reads | writes |
|---|---|---|---|
| 1 | level rule from `expand_probe_rule/5` (`1_host_expand.pl:456-472`) | the body goals before the probe | `__host_demand_N` rows |
| 2 | `HostLiveRunner::collect` (`hosts.rs:1440-1452`) | this tick's `+deltas` on `demand_rel` | nothing |
| 3 | `claim_once` (`hosts.rs:1388-1390`) | `claimed` | `claimed` |
| 4 | applicative grouping (`hosts.rs:1495-1524`) | filled template plus ordered inputs | the group key |
| 5 | `IHostExecutor::run` (`hosts.rs:1536`) | the world | `Vec<HostRow>` |
| 6 | `select_columns` (`hosts.rs:1248-1271`) | the answer rows, filtered by `carries_every_column` | the projected rows |
| 7 | `project` (`hosts.rs:1392-1434`) | `rel_columns[response_rel]` | `Arrival` rows on `__host_response_N` |
| 8 | next tick | `__host_response_N` | the author's rule head |

Step 6 is why one executor run can answer three declarations: a host selects
its own columns out of a shared stream by column presence, and a row missing
any declared output column is dropped (`hosts.rs:1240-1244`). The dead-module
rail depends on this and says so at `dead-module-rail.dl6:33-34`.

Step 4 keys on the FILLED COMMAND, not the executor name (`hosts.rs:1510-1516`).
With the template gone from the surface, an arrival rel's group key has to be
built from the executor name plus the ordered inputs alone. That is the one
place where deleting the template from the surface changes runtime behaviour,
and section 11 gives it a probe.

## 9. Migration table: every consumer site

| file:line | what it does | change |
|---|---|---|
| `compile/parse_dl_dcg.pl:521` | dispatches `bind_decl_stmt` | keep; `bind` stays as sugar |
| `compile/parse_dl_dcg.pl:524` | dispatches `sh_decl_stmt` | keep; `sh` stays as sugar |
| `compile/parse_dl_dcg.pl:540-560` `rel_stmt//1` | the `rel` grammar | ADD one alternative: a parenthesized group after `->` |
| `compile/parse_dl_dcg.pl:582-596` `relation_arrow_output//4` | appends the `return` column | GUARD: only when the right side is not a parenthesized column list |
| `compile/parse_dl_dcg.pl:863-864` `key_clause//1` | `key(<ints>)` | optionally widen to column names |
| `compile/parse_dl_dcg.pl:47,59` `cst_shape/2` | CST node shapes | ADD a row for the new statement |
| `compile/parse_dl_dcg.pl:165-166,282-297,329` | host presence guards | unchanged, they test `sh_decl` terms which the desugar still produces |
| `compile/parse_dl_dcg.pl:990-993` | `declared_column_type_name/2` reads host and bind columns | unchanged |
| `compile/parse_dl_dcg.pl:272-274` | `declaration_source_ref/2` for `sh_decl` | unchanged |
| `compile/parse_dl_dcg.pl:319-337` `normalize_host_leaf/3` | body atom to `probe/4` | ADD a stop when an ordinary rel of the same name is declared (risk 2) |
| `compile/registry.pl:194,196` | `surface/5` rows for `sh_decl/4`, `bind_decl/2` | keep; ADD a row for the new surface term |
| `compile/registry.pl:324-328` | `bind_definition/2`, `bind_executor/2` | step 3: the two executor names become adapter names |
| `compile/registry.pl:342-478` | 28 `host_input_contract/3` rows | keep as the fallback; a `key(...)` on the declaration wins |
| `compile/registry.pl:525-529` `host_input_roles/3` | picks contract roles or all-identity | ADD one clause ahead: read `arrival_identity/2` first |
| `1_host_expand.pl:43-66` `prepare_program/5` | collects host and bind plans | unchanged |
| `1_host_expand.pl:183-200` `compile_host_decl/2` | validates | `string(Template)` still holds for `''` |
| `1_host_expand.pl:310-319` `validate_template/4` | every identity input must appear in the template | GUARD on a non-empty template |
| `1_host_expand.pl:405-419` `validate_bind_decl/3` | bind shape plus the `repo` stop | unchanged in steps 1-2; step 3 routes it |
| `1_host_expand.pl:562-582` `generated_host_decls/7` | mints the two rels | unchanged |
| `0_ast_expand.pl:185` | matches `sh_decl` with a template | unchanged |
| `0_program_check.pl:802,805` | column types declared by hosts and binds | unchanged |
| `2_subscribe.pl:65-82` `host_edge/3` | the demand-to-response edge no rule body carries | unchanged |
| `lower.pl:1607-1645` `catalog_port_plane_rows/6` | port and port_response catalog rows | unchanged |
| `lower.pl:7122` | `functor(Decl, sh_decl, _)` | unchanged |
| `use_resolve.pl:223-224` `merged_prog/4` | picks `prog/2` or `program/3` | unchanged |
| `print_dl.pl:231-232,307-319` `decl_line/5` | round-trips `sh`/`bind` text | ADD a clause for the new spelling; the ROUND TRIP is the byte risk (risk 5) |
| `emit_rust.pl:622-627` | collects host plan dicts | unchanged |
| `emit_ts.pl:507,519,524` | host and bind plans on the paused door | unchanged, see section 10 |
| `ARCH.pl:406,433` | `construct(bind_decl, t1, kept)`, `construct(host_decl, t5, new)` | ADD the new construct row; the arc gate reads it |
| `hosts.rs:1510-1516` | applicative group key from the filled template | CHANGE: fall back to executor plus ordered inputs when the template is empty |
| `hosts.rs:42-55` `executor_for` | the roster | step 3: two new names |
| `types.rs:646-651` `HostAdapterRow` | adapter, demand_rel, response_rel | unchanged |

### Fixtures that move bucket

None, on steps 1 and 2. The desugar produces the terms every existing check
reads, and the four manifest rows in section 4 fire on `sh_decl` and
`bind_decl` terms that the desugar still produces. Step 3 adds rows rather than
moving them. A lane that finds a `compiled` fixture moving bucket has found a
defect in the desugar, not a migration cost, and should stop.

## 10. The `emit_ts.pl` byte-identity argument

The brief floats the opposite direction: "the old forms keep parsing as sugar
that desugars to the new one at parse time". That is wrong for this codebase,
and the reason is three lines.

`emit_ts.pl:519` and `:524` match on the TERMS:

```prolog
Decl = sh_decl(_, _, _, _),
...
( member(bind_decl(Name, Columns), Decls),
  bind_read_literals(Rules, Name, Columns, Literals) )
```

If `sh` desugars into a NEW term, both `findall/3` calls answer `[]` for every
program in the corpus, `host_plans` and `bind_plans` empty out, and the paused
door's output moves for every one of the 179 `sh` declarations. The user's
2026-08-21 decision says `emit_ts.pl` output for unchanged programs stays
byte-identical, so that direction is closed.

Desugaring the NEW surface INTO `sh_decl/4` and `bind_decl/2` gives byte
identity for free: for an unchanged program the term list is not merely
equivalent, it is the same list, so nothing below `parse_dl_dcg.pl` can
observe the change. `emit_ts.pl` needs no edit in any of the three steps.

One caveat the lane must check rather than assume. `print_dl.pl:307-319`
re-renders declarations as text and `compile/dl_view/*.dl6` is committed
output of that path. A new `decl_line/5` clause must render the new spelling
and must NOT change how `sh_decl` and `bind_decl` terms render, or 440
committed `dl_view` files move. Test it by diffing `dl_view` before and after,
not by reading the clause.

## 11. Risks, each with a probe

### Risk 1: the arrow is already the anonymous product type

Highest severity, and the reason section 5 exists. Probe, measured:

```
rel extract(path: text, digest: text) -> (record: text, callee: text).
-> compiles rc=0
-> extract/3 with columns path, digest, return:ref
-> __anon_extract_return_ed2df8f08021f1d6 with columns record, callee
```

The probe file is `v6/prolog/conformance/fixtures/one_rel_with_arrivals_probe.dl6`,
which `go.pl:18` skips because it loads only `*.pl` from that directory.

### Risk 2: a rel and a host may share one name and the host wins silently

Probe, measured, in the same file:

```
rel files(glob: text, path: text, digest: text).
sh files(glob: text) -> (path: text, digest: text) = `git ls-files -- '{glob}'`.
seen(Path) <- want(Glob), files(Glob, Path, _Digest).
```

Compiles rc=0. `rel_columns` carries `files`, `__host_demand_files` and
`__host_response_files`; `arrival_targets` carries `files`. The body atom is
rewritten to `probe/4` by `normalize_host_leaf/3` at
`compile/parse_dl_dcg.pl:319-337` because a `sh_decl` of that name exists, so
the declared table is created, is writable by a schedule, and is never read.
Two declarations, one name, no diagnostic.

At a NON-matching arity the same program is a named stop:
`unsupported_construct(probe_mismatch(probe(files,[_],[_],[])))`. The stop
depends on arity accident, not on the name collision.

The collapse removes the class: one name is one declaration. The lane must add
the stop at `normalize_host_leaf/3` in step 1, or the new spelling inherits the
defect.

### Risk 3: a host name colliding with a module path

Probe, measured:

```
rel scip__call(a: text).
sh /scip/call(repo: text, path: text, digest: text) -> (...) = `x {repo} {path}`.
? scip__call(a).
```

Compiles rc=0. `module_path_name/2` at `compile/parse_dl_dcg.pl:712` joins
segments with `__`, so the dotted host and a hand-spelled `scip__call` rel land
on the same atom. They do not collide directly because the host mints
`__host_demand_scip__call` and `__host_response_scip__call`, but the body-atom
shadow of risk 2 applies at matching arity. The step-1 stop covers both.

### Risk 4: a rel with an arrow AND rules

Probe, measured:

```
rel f(a: text) -> text.
f(A, B) <- seed(A), B := concat([A, '!']).
```

Compiles rc=0 today. Under the collapse an arrival rel with a rule head is two
sources writing one rel, which is the shape `dead-module-rail.dl6:101-105`
already works around by hand:

> The union both liveness planes read. One rel carrying arrivals AND a rule
> head would make every derived row arrive twice, so the union is its own rel.

`CLAUDE.md` says the v5 "one rel = one rule kind" bail does not exist in v6 and
the oracle silently returns a duplicated row. So an arrival rel that is also
rule-headed must be a named stop in step 1. `compile.pl:262` already subtracts
`DerivedRefs` from `ArrivalTargets`, which means today such a rel is silently
NOT an arrival target: the arrow would be ignored rather than doubled. Either
way it is a stop, and which of the two shapes it takes should be measured
before the stop is written.

### Risk 5: `print_dl.pl` round trip moves the committed `dl_view` corpus

440 files in `v6/prolog/compile/dl_view/`. `decl_line/5` at
`print_dl.pl:307-319` renders `sh` and `bind` text. A new clause that reorders
`decl_order_item/2` at `print_dl.pl:231-232` moves committed bytes. Gate: diff
the whole `dl_view` directory, do not read the clause.

### Risk 6: the applicative group key loses the template

`hosts.rs:1510-1516` builds the group key as
`"{execution}|{command_line}|{ordered_inputs:?}"` and the comment states the
reason: three extract-shaped hosts sharing one template fold into one run,
while two hosts whose templates differ are two different questions. With an
empty template every arrival rel on one executor with equal inputs folds
together. For `sprefa_extract` that is correct and is the point. For a future
executor answering several questions from one name it is a wrong fold.

Probe before writing step 1: take `dead-module-rail.dl6`, blank the four
`sprefa_extract` templates, and check the rail still prints
`findings=0 unproven=16 unreachable=0`. If it does, the empty-template key is
safe for the corpus as it stands.

### Risk 7: the `fixture` executor is template-only

`hosts.rs:141-163` `FixtureExecutor::run` parses the COMMAND LINE and accepts
only `printf '<json>'`. An arrival rel with no template cannot use it. One
program uses the `fixture` adapter today. Either the fixture answer moves into
the adapter row as a payload field, or `fixture` keeps requiring the `sh`
spelling. State the choice in step 1 rather than discovering it in step 3.

### Risk 8: the TypeScript door

Covered in section 10. No `emit_ts.pl` edit in any step; verify by diffing its
output, not by reading it.

## 12. Three-step lane plan

Each step lands green on its own PR.

### Step 1: the grammar and the desugar

OWNS: `compile/parse_dl_dcg.pl`, `compile/registry.pl` (`surface/5` row only),
`1_host_expand.pl` (`validate_template/4` guard only), `print_dl.pl`,
`ARCH.pl`, new fixtures under `conformance/fixtures/`.
FORBIDDEN: `emit_ts.pl`, `emit_rust.pl`, `lower.pl`, everything under
`sprefa-engine-rs/`.

Lands: the new `rel N(ins) -> (outs) key(...)` spelling desugaring to
`sh_decl(N, Ins, Outs, template(''))`; the same-name stop from risk 2; the
arrow-plus-rules stop from risk 4; a `print_dl.pl` clause.

Gates: plunit 1041 pass 0 fail; conformance 439 PASS 0 FAIL; `git diff --stat`
on `v6/prolog/compile/dl_view/` is EMPTY; manifest bucket counts stay
335 compiled / 98 unsupported plus the new fixtures.

### Step 2: the roles move into the program

OWNS: `compile/registry.pl` (`host_input_roles/3`), `1_host_expand.pl`,
conformance fixtures.
FORBIDDEN: everything else.

Lands: `arrival_identity/2` read ahead of `host_input_contract/3`; the 28
registry rows stay as the fallback for every `sh` declaration.

Gates: the step-1 set, plus `grade.sh` 439/335 rc=0, plus the dead-module rail
`0/16/0` after re-spelling its five `sh` declarations in the new form. That
re-spelling is the acceptance test: the rail's numbers must not move.

### Step 3: the clock and the watcher become executors

OWNS: `sprefa-engine-rs/src/hosts.rs`, `src/types.rs`,
`compile/registry.pl` (`bind_executor/2`), `v6/dl/fixtures/served-watch-rail.dl6`.
FORBIDDEN: `emit_ts.pl`, `parse_dl_dcg.pl`.

Lands: `IntervalExecutor` and `WatchExecutor` in `LINKED_EXECUTORS`
(`hosts.rs:39-40`); `bind interval` and `bind watch` desugared to arrival rels;
`bind` works on the Rust door for the first time.

BUILD-VS-BUY, mandatory before any code in this step. The watcher is a
common-shaped problem and `CLAUDE.md` forbids asserting "write our own" without
a written candidate-by-candidate analysis. Candidates to price: `notify`,
`notify-debouncer-full`, `watchexec-supervisor`, and `soopy`'s own
`crates/soopy/src/_8_watch.rs`, which the dead-module rail already names with
39 defs. The interval side is `tokio::time::interval` and needs no research.
The analysis goes in the step-3 PR body before the first line of executor code.

Gates: the step-2 set, plus `oracle-rustc`, plus `oracle-knip`, plus a new
conformance fixture proving one interval tick mints one bucket row.

## 13. What needs the user before code

1. **The arrow.** Does a parenthesized group after `->` on a `rel` mean
   response columns (cost: one authored line, `v6/dl/fixtures/anonymous-type-syntax.dl6:5`;
   the anonymous product keeps its column-position spelling), or does the
   arrival marker go somewhere else (cost: the program text stops saying which
   columns are the demand)? Section 5 has the counts. `CLAUDE.md` says language
   design happens with the user in the room, so no lane picks this.

2. **`key(...)` doing two jobs.** On a stored rel it means UNIQUE positions
   (`lower.pl:1095-1146`). On an arrival rel it would also mean demand
   identity. Same word, two readings, and section 6.4 names it as a deliberate
   layer disagreement. Acceptable, or does the identity split want its own
   spelling?

3. **`sh` and `bind` after the collapse.** They stay as parse-time sugar in
   this plan because that is what keeps `emit_ts.pl` byte-identical. Is
   deleting the two keywords from the surface a later arc, or never?

4. **The `fixture` executor.** Risk 7. Move the constant answer into the
   adapter row, or keep `fixture` on the `sh` spelling?

Not blocking: everything in sections 6 through 12 that does not depend on the
answer to question 1 is specified and dispatchable as written.

## 14. The batching seam, measured (arrivals-and-ticks lane, 2026-08-21)

The brief asked whether an input column typed `list(text)` can reach an
executor as one demand. Measured answer: NO, twice over, with a working
alternative already in the language.

| leg | result | site |
|---|---|---|
| `list(text)` host input, compile | named stop `refused_host_decl` | `1_host_expand.pl:261` (`atom(Type)` in `validate_columns/2`) throwing at `:230` |
| list value at the host seam, runtime | `BoundaryError::ListAtScalarSeam` | `types.rs:273`, reached from `hosts.rs` `demand_of` |
| `json` host input carrying `json_group_array(ep)`, oracle | `non_display_in_concat` | `conformance/body.pl:303-304` `text_piece/2`: the demand digest concat cannot render a compound json value |
| `json` host input, SQL door | expected to work (a json column stores canonical text, `||` concatenates it; `lower.pl:1036` comment states the contract) | unmeasured end to end, blocked on the oracle leg above |
| executor side, 6 endpoints in one `eps` JSON array | ONE `HttpFetchExecutor::run` call answers 6 rows, each echoing its `ep` | `executors/fetch.rs`; COUNT receipt `tests/executors.rs` `http_fetch_batches_six_endpoints_in_one_executor_call` |

So the language-native batching spelling is `json_group_array` folding the
endpoint set into ONE `json` demand input, and the executor half is built and
count-tested. What blocks landing it end to end is ONE gap: the reference
engine's digest concat refuses compound json values while the SQL door would
concatenate the stored canonical text. Closing it means giving `text_piece/2`
the canonical json rendering (the same bytes `ticklog.pl` `json_value_json/2`
writes) and proving both doors mint identical witness digests for a json
input.

USER DECISION WANTED: is a json value legal in a witness digest (its
canonical text participates, oracle taught to render it), or must a host
input stay scalar and the batch travel as an ordinary `text` column the
program builds with `group_concat`? The first is one oracle predicate; the
second needs no engine change but spells the batch as text, not json.
