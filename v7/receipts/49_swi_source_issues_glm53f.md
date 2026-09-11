# 49. SWI-Prolog source, releases, and performance issue map

Date: 2026-09-11. Read-only research report on SWI-Prolog implementation behavior and
performance guidance, written for the DL7 compiler effort (V7). No compiler, test,
skill, workflow, or other receipt was edited. No delegation. Claim labels used
throughout:

- **[documented]** — stated in the official reference manual or release notes, cited inline.
- **[source-verified]** — read in the SWI-Prolog source tree at tag `V10.1.14` (raw GitHub).
- **[measured]** — observed on this machine, SWI-Prolog 10.0.2 arm64-darwin, the same
  install used by receipts 45 and 46; command shown.
- **[inferred]** — my interpretation; explicitly marked, never load-bearing.
- **[local-measured]** — measured on the sprefa DL7 workload (receipts 42/43/45/46); may
  be workload-specific.

## TOC

- [1. Releases and recent changes](#1-releases-and-recent-changes)
- [2. Implementation map](#2-implementation-map)
- [3. Official performance traps: issues, discussions, maintainer statements](#3-official-performance-traps)
- [4. Shipped versus proposed behavior](#4-shipped-versus-proposed-behavior)
- [5. Failure-pattern catalog](#5-failure-pattern-catalog)
- [6. Inspection recipes](#6-inspection-recipes)
- [7. Cross-check against receipts 42, 43, 45, 46](#7-cross-check-against-receipts-42-43-45-46)
- [8. Skill deltas](#8-skill-deltas)
- [9. Source inventory and search log](#9-source-inventory-and-search-log)
- [10. Coverage gaps](#10-coverage-gaps)
- [11. Validation](#11-validation)

## 1. Releases and recent changes

Two release trains exist; both are relevant because the DL7 work runs 10.0.2 locally:

| Train | Latest | Released | Evidence |
| --- | --- | --- | --- |
| Stable | 10.0.2 | 2026-03-14 | https://www.swi-prolog.org/download/stable (binaries and source 10.0.2), ChangeLog https://www.swi-prolog.org/ChangeLog?branch=stable shows last stable entry `[Mar 14 2026]` |
| Development | V10.1.14 | 2026-08-30 | https://github.com/SWI-Prolog/swipl-devel/releases (tag `V10.1.14`, commit `6977543`, released 30 Aug 09:40); V10.1.13 2026-08-07, V10.1.12 2026-07-19, V10.1.11 2026-07-05, V10.1.10 2026-06-25. Recent 10.1.x releases are build/packaging only; no engine behavior changes in their bodies. |

Relevant recent changes [documented, both from the stable ChangeLog and the 10.0
download page]:

- 10.0 page states: "Improvements in clause indexing and compilation results in a
  10-30% performance improvement" over 9.x (https://www.swi-prolog.org/download/stable).
- 10.0.2 (Mar 2026) added `library(tableutil)`: "toplevel utilities ... to dump
  tables, their relations and statistics" (https://www.swi-prolog.org/ChangeLog?branch=stable).
  Manual section: https://www.swi-prolog.org/pldoc/man?section=tableutil.
- 10.0.2 added thread *classes* (`thread_property/2` gained `debug_mode/1`, class
  assignment in the VM; ChangeLog Mar 5-6 2026).
- 10.0.2 clarified `create_prolog_flag/3` thread interaction and added the
  `local(true)` option (ChangeLog Mar 11 2026).
- 10.0.2 changed `visible/1` default to include `cut_call` in the tracer (ChangeLog Mar 6 2026).
- The site itself reports "Powered by SWI-Prolog 10.1.14" on every manual page,
  confirming the documentation server runs the development train.

The V7 environment (receipts 45, 46) runs **10.0.2**; the JITI, GC, and tabling
mechanisms described below were verified against tag `V10.1.14` and locally on
10.0.2 where marked [measured]. The 10.0/10.1 indexing internals are shared; the
10.0 release note's "clause indexing improvements" refer to the 9.x→10.0 jump.

## 2. Implementation map

Files below are under `src/` in https://github.com/SWI-Prolog/swipl-devel, tag
`V10.1.14`. Symbol names were read from the actual files [source-verified].

### 2.1 Clause indexing / JITI — `src/pl-index.c` (+ `src/pl-index.h`, `src/pl-supervisor.c`)

Algorithm [source-verified, pl-index.c:60-110 and the call-selection order in
the manual https://www.swi-prolog.org/pldoc/man?section=jitindex]:

1. *Special purpose code*: compiled single-clause predicates and the
   `[]` / `[_|_]` two-clause case (supervisors; `src/pl-supervisor.c` has
   `createSingleClauseSupervisor()` and the static `S_VIRGIN` supervisor).
2. *Linear scan on the primary index argument* when the call has an
   instantiated primary argument and **fewer than 10 clauses**
   (`ci_min_clauses`, [measured] default 10); the scan uses `MAX_LOOKAHEAD`
   (`ci_max_lookahead`, [measured] default 100) to declare determinism.
3. *Hash lookup / creation*: assesses every instantiated argument, picks the
   best by *speedup* (unique values / (std-dev of duplicates + 1)), falling
   back to multi-argument combinations (`find_multi_argument_hash()`,
   `MAX_MULTI_INDEX` args). Index creation is gated by `MIN_SPEEDUP`
   (`ci_min_speedup`, [measured] 1.5) and `MIN_SPEEDUP_RATIO`
   (`ci_min_speedup_ratio`, [measured] 3.0: needs `nclauses/speedup` above the
   ratio, pl-index.c:564).
4. *Deep indexing*: when a single-argument index bucket holds multiple
   compounds with the same name/arity and a nonvar sub-argument, a list index
   (`L` flag) is created and recursion applies per sub-argument
   (`ctx->depth < MAXINDEXDEPTH`, pl-index.c:383; depth limit 7
   [documented], https://www.swi-prolog.org/pldoc/man?section=deep-indexing).

Key gating facts [source-verified + documented]:

- `MAX_VAR_FRAC` (`ci_max_var_fraction`, [measured] 0.1): an argument is not
  indexed when >10% of clauses have a variable at that position ("Clauses that
  have a variable at an otherwise indexable argument must be linked into all
  hash buckets", manual section=jitindex).
- Dynamic predicate index invalidation [documented, manual section=jitindex]:
  "The indexes of dynamic predicates are deleted if the number of clauses is
  doubled since its creation or reduced below 1/4th." Running predicates
  cannot have their index deleted; the index moves to a *removed index list*
  and is reclaimed by `garbage_collect_clauses/0`. Source: `replaceIndex()`,
  `deleteIndex()`, `unallocClauseIndexTable()`, `jiti_tried` bookkeeping
  (pl-index.c:1467,1497,3159).
- The JITI attempt counter `clist->jiti_tried` limits repeated multi-argument
  index searches (pl-index.c:3154-3159) [source-verified].
- `mode/1` declarations suppress index examination on `-` arguments and never
  change semantics (manual https://www.swi-prolog.org/pldoc/man?section=prologjiti,
  `jiti_suggest_modes/1`).
- Inspect with `library(prolog_jiti)`: `jiti_list/0,1`, `jiti_suggest_modes/0,1`
  (same URL). Column grammar: `A+B` multi-argument, `P:L` deep index
  (e.g. `1/2:2+3`), `L` list index, `V` virtual/not-yet-materialized.

### 2.2 Dynamic predicates and the logical update view — `src/pl-assert.c`, `src/pl-pro.c`

[documented] Manual section "Update view"
(https://www.swi-prolog.org/pldoc/man?section=update): SWI-Prolog adheres to
the ISO *logical update view*. Every database change increments a global
*generation*; each goal is tagged with its starting generation; a clause is
visible iff its `[created, erased)` generation interval encloses the goal's
generation. Erased clauses are reclaimed by clause GC (below).

- `asserta/1` performance history note [documented, pldoc history on
  assertz/1]: "improve performance of asserta/1 when there are lots of erased
  clauses" landed in 8.1.7
  (https://github.com/SWI-Prolog/swipl-devel/commit/2c1efca6099ab52d47cf548ddc438ee908430b2c);
  `assertz/1` was protected against C-stack overflow in 8.5.2 (commit
  `f0e2614`); `assert/1` on a defined non-dynamic predicate raises
  permission_error since 8.1.1 (issue #81, commit `5deb1b0`).
- `retractall/1` removes matching clauses but keeps the predicate and its
  properties; `abolish/1` removes all clauses and properties
  (https://www.swi-prolog.org/pldoc/man?predicate=retractall/1).

### 2.3 Atom and clause garbage collection — `src/pl-atom.c`, `src/pl-gc.c`

- Atom GC [source-verified, pl-atom.c header comment, lines ~89-207]: atoms
  are reclaimed by `collectAtoms()` in two passes (invalidate via CAS, then
  `destroyAtom()` guarded by `pl_atom_bucket_in_use()` to avoid a
  lookup/insert race). Foreign references require explicit
  `PL_register_atom()`/`PL_unregister_atom()`. Stack references are found by
  a **conservative scan** (`markAtomsOnStacks()`), so a dead atom reachable
  only through stale stack slots survives until the next AGC.
- AGC and normal GC must not run concurrently; they synchronize through
  `LD->thread.scan_lock` (pl-atom.c comment "Atom GC and multi-threading")
  [source-verified]. Implication [inferred]: GC and AGC serialize, so both
  can appear as one pause.
- Default GC thread [documented, manual section=update]: "the clause garbage
  collector runs in a thread named `gc`, together with the atom garbage
  collector", controlled by the `gc_thread` Prolog flag.
  [measured] `current_prolog_flag(gc_thread, X)` yields `true` on 10.0.2.
- Clause GC is "scheduled automatically, based on time and space based
  heuristics" [documented, manual section=jitindex];
  `garbage_collect_clauses/0` and `garbage_collect_atoms/0` are manual triggers
  (https://www.swi-prolog.org/pldoc/man?predicate=garbage_collect_clauses/0).

### 2.4 Tabling / trie storage — `src/pl-tabling.c`, `src/pl-trie.c`

[documented] Chapter 7, https://www.swi-prolog.org/pldoc/man?section=tabling:

- Variant tabling memoizes answers per subgoal variant; answers are **not
  ordered** for a generator (see the discourse thread
  https://swi-prolog.discourse.group/t/are-there-table-ordering-guarantees/3458,
  referenced from the manual annotations).
- Mode-directed / answer subsumption: https://www.swi-prolog.org/pldoc/man?section=tabling-mode-directed.
- Incremental (invalidate dependent tables on `incremental` predicate change),
  monotonic (propagate new answers), shared (tables shared between threads:
  https://www.swi-prolog.org/pldoc/man?section=tabling-shared), tripwires
  (restraint on subgoal size, answer size, answer count:
  https://www.swi-prolog.org/pldoc/man?section=tabling-restraints).
- Storage [documented, https://www.swi-prolog.org/pldoc/man?section=tabling-about]:
  answer tables are tries (section 4.14.4); suspension uses delimited
  continuations; the wrapper `table/1` directive generates
  `start_tabling(user:variant, renamed_body)`. The "Status of tabling"
  subsection still calls the implementation "merely a first prototype" and
  lists shared tables and automatic invalidation as *future* work; that text
  is stale relative to the shipped chapters (shared/incremental/monotonic
  tabling exist) [source-verified inconsistency, flag it when reading].
- [source-verified, pl-tabling.c]: answer subsumption is implemented as a
  trie map (`TRIE_ISMAP`, pl-tabling.c:1234); answer completion removes
  positive loops from delay lists (`answer_completion/2`, pl-tabling.c:1516,
  1875-1899); SCC/dependency bookkeeping keeps `variant` trie nodes per
  table (`idg_init_variant`).

### 2.5 Term hashing and comparison — `src/pl-termhash.c`

[documented] https://www.swi-prolog.org/pldoc/man?section=hashterm:

- `term_hash/2` fails (leaves unbound) on non-ground terms; hash range 0..2^56;
  stable across invocations and versions since 9.3.28; cycle-safe; **hash
  differs between big/little-endian machines and between GMP/LibBF for big
  integers**. `term_hash/4` bounds depth and range.
- `variant_sha1/2` and `variant_hash/2` hash up to variable renaming (used by
  tabling to index variants); both raise on cyclic terms; `variant_hash/2`
  treats attributed variables as normal variables.
- Structural terms have no hash-consing: every `==/2` or index probe on a
  compound re-walks it [source-verified mechanism: hashes are computed per
  call in pl-termhash.c; no interning of ground compounds]. This is the
  documented rationale for the `assert_x/1` hash-column pattern shown in
  section=hashterm.

### 2.6 Profiler and tracer — `src/pl-prof.c`, `src/pl-trace.c`, `library(statistics)`, `library(prolog_profile)`

[documented] https://www.swi-prolog.org/pldoc/man?section=profile:

- `profile/1,2` run `once(Goal)` under the kernel profiler: a **gprof-style
  dynamic call tree** built from three kernel hooks (`profCall`, `profExit`,
  `profRedo`) plus *statistical time sampling* via `setitimer()/SIGPROF`
  (Windows: MM timer thread). Default sample rate 200/s
  (`profile_sample_rate`), `time(cpu|wall)`, `ports(true|false|classic)`,
  `top(N)`, `cumulative(Bool)`.
- `profile_data/1` returns the raw dict (`summary` with `samples`, `ticks`,
  `accounting`, `time`, `nodes`, `sample_period`; `nodes` with
  `ticks_self/ticks_siblings/call/redo/exit/callers/callees`).
  `profile_procedure_data/2` filters per predicate. `show_profile/1` re-renders.
- Recursion detection: "recursive procedures increment the 'recursive' count"
  only when *the same predicate with the same parent* appears higher in the
  call graph; mutual recursion through `call/1` is generally not detected
  [documented, section=profilegather within section=profile].
- Consequences [inferred from the documented mechanism, plus one measured
  trap]: (a) a goal that finishes inside **one sample period** yields zero
  samples; the profiler's accounting then divides by the tick count and
  raises `zero_divisor` — [measured] on 10.0.2,
  `profile(b, [top(3)])` on a goal that finishes in <5 ms raised
  `ERROR: //2: Arithmetic: evaluation error: 'zero_divisor'`, while the same
  predicate under a 300k-iteration loop produced a normal report
  (commands in section 6.4). (b) C builtins accumulate ticks to the calling
  VM instruction's node, so a hidden C-level scan appears as time in the
  Prolog predicate that called it — consistent with the local-measured
  `memberchk/2` wall/inference split (receipt 45).
- The call-counting profiler is `library(prolog_profile)` (different from the
  sampling kernel profiler; used by V7 receipts to count calls);
  `library(prolog_wrap)` provides wrapper interception (used in receipt 45/46).
- Tracer determinism diagnostics: `det_goal_error`/determinism warnings live
  in the VM (`src/pl-comp.c`, `det_goal_error`); `% ... left a choice point`
  messages are the classic symptom of an index not ruling out an alternate
  clause (issue #1386 below).

### 2.7 Threads — `src/pl-thread.c`

- Threads have private stacks and private (non-shared) dynamic predicates
  unless declared `(:- dynamic p/1).` in a shared module context; the shared
  tabled-predicate story is the "Shared tabling" section
  (https://www.swi-prolog.org/pldoc/man?section=tabling-shared) [documented
  chapter exists; not deep-read in this report — see gaps].
- `thread_statistics/2` (library(statistics)) returns status, time, and
  stack-size dict per thread; fails silently on a dead thread [documented,
  section=statistics].
- AGC/GC serialization with all thread stacks is the main multi-thread GC
  cost (2.3) [source-verified].

### 2.8 Transactions — `src/pl-transaction.c`

[source-verified, pl-transaction.c module comment]:

- Transactions reuse the logical-update generation space: generations
  `1..GEN_TRANSACTION_BASE` are globally visible; each thread starting a
  transaction reserves a segment at
  `GEN_TRANSACTION_BASE + TID * GEN_TRANSACTION_SIZE` (2^32 generations per
  thread) and sets `LD->transaction.generation` to the segment base.
- Clause updates inside a transaction are recorded in
  `LD->transaction.clauses` (values `GEN_ASSERTA`/`GEN_ASSERTZ` or the local
  offset of the deleted generation); retracts bump `clause.tr_erased_no`.
- Visibility rule: a clause is visible if it was created inside the
  transaction, or was visible at transaction start and was not retracted in
  this transaction. Nested transactions keep a `tr_stack` parent with saved
  clause/predicate tables and a `table_trail` for changes to tables.
- [documented] manual entry points: https://www.swi-prolog.org/pldoc/man?section=transactions
  and impact notes https://www.swi-prolog.org/pldoc/man?section=transaction-impact
  (not deep-read; see gaps).

### 2.9 Foreign records and the recorded database — `src/pl-rec.c`

- The recorded database (`recorded/3`, `recorda/3`, `recordz/3`, `erase/1`)
  is implemented in `src/pl-rec.c` [source-verified file presence].
- [documented, cited in receipt 45 from https://www.swi-prolog.org/pldoc/man?predicate=recorded/3]:
  lookup is hashed on the first argument; `recorded/3` is semi-deterministic
  only when a *reference* is given; `retractall/1` does not remove recorded
  terms, `erase/1` does. [local-measured, receipt 45]: recorded lookup
  enumerated at 643 ms for 34,497 keyed lookups vs 35 ms for JITI dynamic
  facts on the same workload.

## 3. Official performance traps

High-signal items, each with link, shipped behavior, and the trap.

| # | Source | Behavior / maintainer statement | Trap for a Datalog compiler |
| --- | --- | --- | --- |
| 1 | Manual section=jitindex (URL above) | JITI builds indexes on first call with an instantiated argument; indexes on dynamic predicates are **deleted when clauses double or fall below 1/4**; outdated indexes of *running* predicates go to a removed-index list, reclaimed by clause GC | Assert/retract churn between compiles keeps re-triggering index rebuilds; a long-running goal pins stale indexes |
| 2 | Manual section=jitindex; `ci_max_var_fraction` | >10% variable clauses at an argument disables that index | Facts written with a trailing unbound argument kill the index on the useful prefix; order arguments or use `mode/1` |
| 3 | Manual section=jitindex, deep indexing | Deep indexing limited to 7 levels, single-argument indexes only, per-level independent decisions | Deep keys cost; flatten or `term_hash` the key (manual recommends exactly this) |
| 4 | https://github.com/SWI-Prolog/swipl-devel/issues/1386 | "Clause indexing regression": a 2-clause static predicate left an unwanted choice point after success, introduced in v9.3.19; argument-pattern dependent | A predicate that *should* be det-finished by indexing can still leave a choice point after an engine change; do not pin determinism to a version without a test |
| 5 | https://swi-prolog.discourse.group/t/are-there-table-ordering-guarantees/3458 (manual annotation) | Tabled predicates "may well change the order of solutions if used as generator"; no ordering guarantee | Variant tabling must not be used where first-match order is semantics; mode-directed `first/1` preserves it [local-measured, receipt 45 checksum] |
| 6 | Jan Wielemaker, tabling chapter annotation (section=tabling) | "If you table predicates with infinitely many solutions you simply run out of (table) memory." | Table explosion is the documented failure mode; use tripwires (section=tabling-restraints) |
| 7 | https://github.com/SWI-Prolog/swipl-devel/issues/611 and #1115 | Cross-version perf regressions (~30% in 7.7→8.3; ~2x in 8.4.3→9.0.x) reported and worked through on the tracker | Version-to-version regression risk is real for index- and GC-sensitive workloads; pin the version in the benchmark |
| 8 | Manual section=hashterm | `term_hash/2` is unbound on non-ground terms by design ("the lookup to be fast if Term is ground and correct (but slow) otherwise") | A hash-keyed store silently degrades to enumeration when a key is accidentally unbound |
| 9 | pldoc history on assertz/1 | 8.1.7: "improve performance of asserta/1 when there are lots of erased clauses" | Building a store by assert into a predicate with a long erased history pays; prefer fresh predicates per store (receipt 46's per-arena-id + `retractall` of the specific id avoids partial retention) |
| 10 | Manual section=update | Logical update view via generations; erased clauses stay visible to older goals | A long-running evaluation that starts before `retractall` keeps the old clause set alive; scope stores so no goal straddles the cleanup boundary |
| 11 | AGC comment in src/pl-atom.c | Conservative stack scan; AGC cannot run concurrently with normal GC | Atoms and terms reachable from stale choice points are retained; leftover choice points (receipt 42's `leaves_choicepoint` flag) directly increase memory retention |
| 12 | pldoc annotation on statistics/1 | "Inference statistics are often a few off" [documented] | Use inferences as a *gate*, treat small deltas as noise only with a fixed threshold policy |

## 4. Shipped versus proposed behavior

- Shipped: JITI over multiple arguments, deep indexing to depth 7, dynamic
  index invalidation at double/quarter clause counts, logical update view
  via generations, generation-segmented transactions, trie-backed tabling
  with variant/subsumptive/mode-directed/incremental/monotonic/shared modes,
  `library(tableutil)` for table dumps (10.0.2). All with links in sections
  2 and 3 [documented/source-verified].
- Proposed (manual section "Future directions",
  https://www.swi-prolog.org/pldoc/man?section=indexfut): extended special
  cases for low clause counts, "an efficient decision diagram for selecting
  between low numbers of static clauses", better judgement between deep and
  plain indexes [documented]. `jiti_suggest_modes/1` notes mode declarations
  "have no effect on the semantics" but "This may change in the future"
  [documented, section=prologjiti].
- Stale doc: the tabling "Status" subsection still describes shipped features
  (shared tables, incremental) as future work; treat the chapter body, not
  the status blurb, as current [source-verified inconsistency].

## 5. Failure-pattern catalog

Compiler-oriented rows; counters and tools verified where marked.

| Symptom | Observable counter | Likely cause | Confirming query/tool | Safe candidate mechanism | Semantic risk |
| --- | --- | --- | --- | --- | --- |
| High wall, low inferences on lookups | `memberchk/2` at 94k calls, 13.5M scanned entries but only ~9 charged inferences/call [local-measured, receipt 43] | C-level list scan hidden from the inference counter | `library(prolog_profile)` call counts + `call_time/2` wall vs inferences | Dynamic facts + JITI built once per compile (receipt 45 matrix: 35 ms vs 246 ms wall) | First-match duplicates: assert in source order, keep `(Cond -> L ; default)` fallback |
| Inference budget rises after adding an index | +30,136 inferences (receipt 43) | Index build charged in Prolog per invocation; scan was C | `DL7_TRACE=steps` before/after, same fixture | Build the store once at the compile boundary, not per checker call (receipt 45 lifetime matrix: 335 vs 2,213 ms) | Store lifetime must be closed by `setup_call_cleanup/3` or facts leak across compiles |
| Lookup never hits an index | `jiti_list/1` shows no row, or `V` (virtual) flag only | Argument never called instantiated, or `ci_max_var_fraction` exceeded | `jiti_suggest_modes/1`; add `mode/1` for `-` args | Reorder arguments so the key leads; `term_hash` column for compound keys | `mode/1` is advisory; a mode-declared `-` argument called bound loses the index silently |
| Lookup hits, then degrades over the run | `jiti_list/1` buckets/speedup change between samples; speedup collapses | Dynamic index invalidated after clause count doubled / fell below 1/4 (manual section=jitindex) | Sample `jiti_list/1` before and after the mutation phase | Keep stores immutable within a compile; one fact family per store id | Index rebuild is lazy: first query after churn pays the build |
| Tabled goal returns answers in different order than source | Checksum mismatch on ordered output [local-measured, receipt 45: variant tabling failed the first-match checksum] | Variant tabling gives no ordering guarantee (discourse 3458) | Compare `findall` order against list baseline; `table_statistics/2` for table shape | Mode-directed `:- table p/2 as first/1` or indexed facts | `first/1` subsumption still merges variant-identical answers; duplicate-key semantics must be pinned by a test |
| Memory grows across compiles | `statistics/2` `heapused`/global stack rising; residual facts after run | Store not torn down; recorded terms only removable via `erase/1`; atoms retained by stale choice points (AGC conservative scan) | Residue probe: count clauses of each `arena_*` predicate after cleanup (receipt 46 measured `facts=0 scopes=0`); `garbage_collect_atoms/0` then re-measure | `setup_call_cleanup(open, ..., close)` with `retractall` scoped by store id | `retractall/1` does not free recorded terms (manual recorded/3); long-running goals pin erased clauses via generations |
| Pause spikes mid-run | GC times from `statistics/2` (key `garbage_collection`, a `[Count, Time]` pair [measured, 10.0.2; `statistics(gc,..)` is a domain error on 10.0.2]) | Clause GC + AGC serialized in the `gc` thread, triggered by time/space heuristics (manual section=update, section=jitindex) | `statistics(garbage_collection,[N,Time])` before/after phases | Fewer, bigger store builds; avoid per-call assert/retract | Forcing GC manually costs the pause it avoids later; do it only between phases |
| Determinism test flips between versions | `% ... left a choice point` message; choicepoint flag from receipt 42 tooling | Index-selection or special-case supervisor change (issue #1386 pattern) | `library(prolog_wrap)` probe on the predicate; `jiti_list/1` diff across versions | Pin determinism with `once/1` at the *evidence* boundary (never in the library body) and add a regression test | Masking a genuine nondeterminism with `once/1` hides a real logic bug |
| Profiler shows nothing / crashes on fast goals | `zero_divisor` from `//2` [measured on 10.0.2 for sub-5 ms goals] | Zero samples in one 5 ms sample period break profiler accounting (documented sampling design) | Repeat with a heavier fixture, or use `call_time/2` | Scale the workload up; use `time/1`, `call_time/2,3` for short phases | Sampling profiler under-reports C-level work; combine with call counters |
| Meta-call overhead suspected | Calls counted through a wrapper exceed semantic callers (receipt 45: `prolog_wrap` added 20-39k inferences) | Wrapper interpreter per lookup; per-call store rebuild | Compare wrapped vs unwrapped inference totals on the same hash | Thread the store, install once per compile (receipt 45 conclusion) | Wrappers change error attribution and can alter determinism observables |

## 6. Inspection recipes

Verified on SWI-Prolog 10.0.2 [measured] unless noted.

### 6.1 JITI state

```prolog
?- use_module(library(prolog_jiti)).
?- jiti_list(p(_, _)).                 % or jiti_list/0 for everything
?- jiti_suggest_modes(user:p(_, _)).   % proposes mode/1 declarations
?- forall(predicate_property(p(_, _), indexed(Index)),
          format("index: ~w~n", [Index])).
```

Output columns [documented + measured]: `Predicate #Clauses Index Buckets
Speedup Coll Flags`; `Index` grammar `A+B`, `P:L`; `L`=list index,
`V`=virtual (not materialized).

### 6.2 Predicate properties and determinism

```prolog
?- predicate_property(p(_,_), indexed(I)), writeln(I).
?- predicate_property(p(_,_), number_of_clauses(N)).
?- findnsols(2, t, p(k, _), Ts), length(Ts, 2).   % >1 solution ⇒ choice point survives
```

Choicepoint residue detection is receipt 42's `leftover_choicepoint/2`
(capped probe + `call_cleanup/3` flag read before commit) [local-measured].

### 6.3 Resources and GC

```prolog
?- statistics(inferences, I).
?- statistics(garbage_collection, [Count, Time]).   % NOT statistics(gc, ..) on 10.0.2
?- statistics(cputime, C), statistics(walltime, W).
?- statistics(global, Used), statistics(trail, T).
?- thread_statistics(Thread, Stats).                 % per-thread residue [documented]
```

### 6.4 Profiling

```prolog
?- use_module(library(statistics)).
?- profile(compile_fixture(F), [top(15)]).           % sampling profiler; needs ≥1 sample period
?- show_profile([top(15), cumulative(true)]).
?- profile_data(D).                                  % raw dict incl. ticks/accounting
?- call_time(Goal, T).                               % wall/cpu/inferences dict
?- time(Goal).                                       % prints, reports per answer
```

Measured trap: goals under one sample period raise `zero_divisor` in
`profile/2` on 10.0.2 (section 5, row 9). For call counts use
`library(prolog_profile)`; for argument-shape probes use `library(prolog_wrap)`.

### 6.5 Tables

```prolog
?- use_module(library(tabling)).
?- table_statistics(p/2, Subgoals, Answers).         % [documented, section=tabling-preds]
?- use_module(library(tableutil)).                   % 10.0.2+: dump tables + stats [documented]
?- abolish_all_tables.                               % full reset between phases
```

### 6.6 Clause counts and references

```prolog
?- predicate_property(p(_,_), number_of_clauses(N)).
?- predicate_property(p(_,_), erased_clauses(E)).    % erased-but-visible clauses
?- garbage_collect_clauses, garbage_collect_atoms.   % force reclaim, then re-measure
```

### 6.7 Thread-local residue

```prolog
?- forall(current_thread(T, _),
          ( thread_statistics(T, S), format("~w ~w~n", [T, S.get(status)]) )).
```

Shared dynamic predicates and per-arena store ids (receipt 46) keep
simultaneous-thread isolation; verify by asserting distinct ids in two
threads and counting `arena_stratum(_, Id, _)` per id.

## 7. Cross-check against receipts 42, 43, 45, 46

| Receipt finding | Official verdict | Generalizes? |
| --- | --- | --- |
| **42** (`determinism_evidence`): wrapper + `findnsols/4`-with-marker classification; choicepoint flag via `call_cleanup/3`; non-replayable stateful calls | Sound against documented semantics: `predicate_property/2`, `call_cleanup/3`, and `findnsols/4` have stable documented contracts; issue #1386 shows determinism *itself* is version-sensitive, so classification per run is the right shape | Generalizes; determinism results are version- and fixture-specific by nature (issue #1386) |
| **43** (`origin_index`): assoc origin index removed 5.5M scanned entries but charged +30,136 inferences; rejected | Explained by mechanism: `memberchk/2` scans in C and under-charges inferences (receipt 45's split of wall vs inferences; profiler doc: ticks attributed to the calling node, C loops cheap in charged terms) [source-verified mechanism + local-measured] | The *wall* win generalizes (O(log N) vs O(N) scan); the *inference-gate rejection* is workload/policy-specific to V7's inference budgets |
| **45** (`indexing_storage_lab`): JITI dynamic facts cheapest on wall and charged inferences; variant tabling breaks first-match; `first/1` preserves it; `retractall/1` leaves recorded terms | All confirmed by official sources: JITI column semantics (section=prologjiti), table ordering guarantee absence (discourse 3458), `retractall/1` vs `erase/1` (recorded/3 doc), index invalidation thresholds (section=jitindex) | Generalizes with the duplicate-key caveat: first-match reproduction depends on `assertz` order, which is documented insertion order (manual assertz/1: "assert the clause as last clause") |
| **46** (`jiti_stratum_arena`): per-compile fact store with arena ids; +343 charged inferences; exact output parity; zero residue | Matches documented invalidation model: small stable fact families never cross the double/quarter invalidation thresholds, so the JITI index stays realized; residue measurement (`facts=0 scopes=0`) matches the documented `retractall/1` behavior on dynamic clauses | The +343 delta is fixture-specific [local-measured]; the *pattern* (scope stores at an owning boundary, unique id per store) generalizes and matches the documented generation/update-view model |

Workload-specific remainders: all inference-delta numbers; the conclusion
that JITI beats `assoc` *in charged inferences* depends on V7 charging the
gate by inferences; on a wall-only budget `assoc` was competitive (42 ms vs
35 ms, receipt 45 matrix).

## 8. Skill deltas

Terse additions proposed for `swi-prolog-performance-jutsu` (skill file NOT edited):

- Gotcha routing: "high wall / low inferences ⇒ C-level scan; switch to
  dynamic facts + JITI"; "inference rise after an index ⇒ store rebuilt per
  invocation; hoist to the compile boundary"; "checksum changed under
  tabling ⇒ ordering guarantee absent; use `as first/1` or facts".
- Diagnostic commands: `jiti_list/1` (read `V` as *not yet materialized*),
  `jiti_suggest_modes/1` + `mode/1` for `-` arguments,
  `statistics(garbage_collection, [N,T])` (10.0.2 rejects the `gc` key),
  `table_statistics/2` and `library(tableutil)` (10.0.2+) for table residue,
  `library(prolog_profile)` for call counts (the sampling profiler
  attributes time, not calls).
- Counterexamples: variant tabling `once/1` fails a first-match checksum on
  duplicate keys (receipt 45 measured); `profile/2` on a sub-5 ms goal
  raises `zero_divisor` (measured 10.0.2); `statistics(gc, _)` is a domain
  error on 10.0.2 (measured); `retractall/1` leaves recorded terms (recorded/3
  doc); >10% variable clauses at an argument disables that JITI index
  (`ci_max_var_fraction` = 0.1, measured flag default).
- Version pin: JITI thresholds (`ci_min_speedup` 1.5, `ci_min_speedup_ratio`
  3.0, `ci_min_clauses` 10, `ci_max_lookahead` 100) measured on 10.0.2;
  issue #1386 shows determinism changed within a 9.3.x patch series; any
  determinism or inference gate must record the SWI version.

## 9. Source inventory and search log

Manual pages read (swi-prolog.org, server version 10.1.14):

1. `pldoc/man?section=prologjiti` — jiti_list/0,1, jiti_suggest_modes/0,1, column grammar.
2. `pldoc/man?section=jitindex` — clause selection order, thresholds, invalidation, deep indexing, body-code indexing, portability.
3. `pldoc/man?section=update` — logical update view, generations, clause GC thread, gc_thread flag.
4. `pldoc/man?section=hashterm` — term_hash/2,4, variant_sha1/2, variant_hash/2, hash-column design pattern.
5. `pldoc/man?section=profile` — profiler design, options, profile_data/1 fields, recursion detection, Windows variant.
6. `pldoc/man?section=statistics` — library(statistics), thread_statistics/2, call_time/2,3, "inferences often a few off".
7. `pldoc/man?section=tabling` — chapter index, memoizing, ordering annotations (incl. discourse t/3458), infinite-table memory note.
8. `pldoc/man?section=tabling-about` — implementation (delimited continuations, tries, wrapper translation), status blurb (stale).
9. `pldoc/man?predicate=assertz/1` — assert family, history notes, retractall-vs-abolish.
10. `pldoc/man?section=dynamic-predicates` — 404 (wrong section id; correct chapter reached via `section=dynpreds` links; recorded in gaps).
11. `pldoc/man?predicate=retractall/1` (via assertz page links), `predicate=recorded/3` (cited via receipt 45's verified table).
12. `download/stable` and `ChangeLog?branch=stable` — 10.0.2, library(tableutil), thread classes.
13. GitHub releases page — V10.1.10..V10.1.14 dates and contents.

Source files read (raw.githubusercontent.com, tag `V10.1.14`), with key symbols:

- `src/pl-index.c` (4,128 lines): `hashDefinition`, `createIndex`,
  `find_multi_argument_hash`, `consider_better_index`, `MIN_SPEEDUP`,
  `MIN_SPEEDUP_RATIO`, `MAX_VAR_FRAC`, `MIN_CLAUSES_FOR_INDEX`,
  `MAX_LOOKAHEAD`, `jiti_tried`, `realize_clause_index`, `replaceIndex`,
  `deleteClauseBucket`, `hashIndex`, `MAXINDEXDEPTH` use at :383.
- `src/pl-atom.c` (2,880 lines): AGC header comment, `collectAtoms`,
  `markAtomsOnStacks`, `PL_register_atom`, `considerAGC`, `scan_lock` note.
- `src/pl-gc.c` (5,924 lines): GC/stack scanning structure (header).
- `src/pl-tabling.c` (9,472 lines): `TRIE_ISMAP` answer subsumption (:1234),
  `answer_completion` (:1516, :1875), `idg_init_variant`, worklist/`depend_abolish`.
- `src/pl-trie.c` (3,526 lines), `src/pl-termhash.c` (911 lines),
  `src/pl-prof.c` (1,397 lines: `profile()`, `thread_prof_ticks`, `ticks`,
  `sibling_ticks`), `src/pl-thread.c` (8,845 lines), `src/pl-comp.c`
  (9,223 lines), `src/pl-hash.c`, `src/pl-rec.c` (recorded db),
  `src/pl-transaction.c` (generation segments, `tr_stack`, `GEN_ASSERTA/Z`),
  `src/pl-supervisor.c` (`createSingleClauseSupervisor`, `S_VIRGIN`),
  `src/pl-assert.c`, `src/pl-index.h`, `src/pl-comp.h`, `src/pl-global.h`.
- File listing via GitHub contents API confirmed `pl-record.c` and
  `pl-trx.c` do not exist at this tag; recorded db is `pl-rec.c`,
  transactions are `pl-transaction.c`.

Issue and forum search (GitHub issues API, discourse search API):

- Search "jiti OR indexing in:title" → issues #1386, #463, #148.
- Search "performance in:title" → #1115, #611, #1067/#1069 (mpz_gcd), #1018.
- Discourse search "tabling memory" → 9482 (dynamic subsumptive table),
  announcement threads 9803/9781/9747/9544.
- Issue bodies read: #1386 (full), #611 (full), #1115 (report body).

Local measured probes (SWI 10.0.2, this machine): `jiti_list/1` output
columns; `ci_*` flag defaults; `gc_thread=true`;
`statistics(garbage_collection,[N,T])` valid and `statistics(gc,_)` a domain
error; `profile/2` `zero_divisor` on fast goals and normal report on a
300k-iteration goal; `table_statistics/2` existence via library load.

## 10. Coverage gaps

- Threads chapter (section=threads) and transaction-impact
  (section=transaction-impact) not deep-read; claims there rest on the
  source files and the update-view section.
- `src/pl-gc.c` and `src/pl-termhash.c` read only at header/structure level;
  no GC-threshold constants extracted.
- AGC scheduling thresholds (`considerAGC`) not extracted.
- Deep-indexing depth macro `MAXINDEXDEPTH` definition file not located at
  this tag (manual documents 7; trusted as documented).
- Discourse threads beyond the search hit list not read in full (only the
  ordering-guarantee annotation and titles).
- `library(prolog_profile)` internals (call-count profiler) not read; used
  only as a documented tool name.
- The 10.1.x release notes between 10.1.0 and 10.1.9 were not enumerated
  (releases page showed only the five newest); possible mid-cycle engine
  changes between 10.0.2 and 10.1.x are therefore partially unmapped.

## 11. Validation

- Report path: `v7/receipts/49_swi_source_issues_glm53f.md`, Markdown.
- Every version/date claim cites the release page or ChangeLog (section 1).
- No conflicting sections; "shipped" and "proposed" are separated (section 4);
  generalization limits stated per row (section 7).
- One stale-doc inconsistency flagged (tabling status blurb, section 4).
- `git diff --check` run for this file: clean (trailing whitespace / conflict
  markers absent).
