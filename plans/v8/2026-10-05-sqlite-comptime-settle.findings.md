# Findings: ir:sqlite comptime settle cost (atlas lever 1)

Brief: `plans/v8/2026-10-05-sqlite-comptime-settle.brief.md`. Base sprefa `ae949f231`, sqlite_ivm
`1074daa`. Debug binaries (`[profile.dev] opt-level = 1`, dependencies `opt-level = 3`).

## Method

Temporary counters (not committed): a process-wide accumulator keyed by phase (`dl8::phase` name
in effect) and item, dumped at exit. Rust timers around each comptime round step
(`_4_comptime/_2_rounds.rs`, `_6_eval/_9b_ir_eval.rs`); in ivm-sqlite, per settle statement the
`prepare_cached` time (first prepare of a SQL text in the process vs repeat) and the execute time,
classed by verb, IR op of the node the statement writes and table suffix; per closure pass time.
`dl8 compile`, 3 runs each, mean ms. Load 11.9-12.1 (`uptime`; a parallel lane building).

Programs: the registry build (`std/tsi_registry_emit.dl7`), one `_5_store_host` fixture
(`fixtures/host_effect/1_settled.dl7`), one `_1_compile_oracles` case
(`oracle/compile/sources/applications/dl6/0_catalog.dl7`, case `applications-dl6-0_catalog`).

## Before

### Comptime phase split (ms, mean of 3)

| item | tsi_registry_emit.dl7 sqlite | dd | 1_settled.dl7 sqlite | dd | 0_catalog.dl7 sqlite | dd |
|---|---|---|---|---|---|---|
| comptime phase total | 1151.1 | 154.3 | 388.4 | 40.9 | 466.2 | 60.6 |
| lowering (ir_lower::lower) | 7.0 | 6.9 | 2.3 | 2.3 | 5.3 | 4.8 |
| install (engine CREATEs) | 322.3 | 20.0 | 84.0 | 9.1 | 98.0 | 10.5 |
| fingerprint + matches | 0.9 | 0.9 | 0.3 | 0.3 | 0.6 | 0.6 |
| bootstrap (term intern + seed frontier) | 18.0 | 7.6 | 8.1 | 1.0 | 9.8 | 1.4 |
| settle (engine) | 735.4 | 65.9 | 276.1 | 12.8 | 327.3 | 21.5 |
|   of which first statement prepare | 397.5 | - | 202.2 | - | 229.1 | - |
|   of which cached prepare | 5.5 | - | 0.2 | - | 1.0 | - |
|   of which closure pass 2+ | 3.9 | - | 0.0 | - | 2.8 | - |
| answer loop decode + settles | 2.5 | 2.4 | 0.0 | 0.0 | 0.0 | 0.0 |
| snapshot + decode (closure) | 13.2 | 12.1 | 1.5 | 1.9 | 3.3 | 3.8 |
| Rust round work: colon_rows | 0.4 | 0.4 | 0.1 | 0.1 | 0.2 | 0.2 |
| intern_rows | 0.4 | 0.4 | 0.0 | 0.0 | 0.1 | 0.1 |
| assemble_generated_program | 1.1 | 1.0 | 0.2 | 0.3 | 0.5 | 0.5 |
| call_rules | 0.5 | 0.5 | 0.1 | 0.1 | 0.4 | 0.3 |
| check_resolved_rules | 2.0 | 1.9 | 0.6 | 0.6 | 1.4 | 1.5 |
| round_seeds + round_program | 1.6 | 1.6 | 0.4 | 0.4 | 1.1 | 1.1 |
| strip_snapshot_rows + stable_outcome | 2.4 | 2.3 | 0.9 | 0.9 | 1.1 | 1.1 |

`-`: engine-internal item, sqlite only. dd's settle is ivm-dd internal work.

### Where the sqlite settle goes

First prepare of settle statements is 54 % (registry), 73 % (fixture) and 70 % (catalog) of the
settle. Closure passes after the first: 3.9 ms, 0, 2.8 ms (the `plain_inner` pass-gate
candidate is below 1 % of the settle on these programs; the catalog and fixture main settles
end in one pass already).

| program | settle ms | first prepares | SQL bytes prepared | first-prepare ms |
|---|---|---|---|---|
| registry | 735.4 | 3610 | 3707810 | 397.5 |
| fixture | 276.1 | 435 | 1214748 | 202.2 |
| catalog | 327.3 | 689 | 1479140 | 229.1 |

Prepare cost by statement size (registry): 3198 statements under 2 KB take 72 ms; 387 of 2-10 KB
take 123 ms; 23 of 10-50 KB take 123 ms; 2 over 50 KB take 51 ms.

Profile of the costliest fixture statement (22137 bytes, 28.6 ms to prepare; replayed in a
Python `sqlite3` 3.53.4 against the dumped schema, `EXPLAIN` to stop after compile, `sample`):
the time is in `sqlite3Select` → `sqlite3SelectPrep` → `selectExpander` / `sqlite3SelectDup` /
`substExpr`, codegen 3427 VDBE ops. SQLite expands each reference to a CTE by duplicating the
CTE's select tree, `MATERIALIZED` or not, so a CTE body is walked once per path from the
statement to it. Inline join deltas are CTEs, and a join delta reads each input delta twice
(`ΔL⋈R_i ∪ L_i⋈ΔR ∪ ΔL⋈ΔR`), so a chain of inline joins doubles per level:

- the 22137-byte statement has 30 CTEs; its CTE tree unfolds to 1346431 bytes (61x);
- a synthetic chain `c_k AS MATERIALIZED (SELECT .. FROM c_{k-1} x JOIN c_{k-1} y ..)` prepares
  in 0.52 ms at k=8, 2.17 ms at k=10, 7.74 ms at k=12, 34.4 ms at k=14.

Across the top 15 fixture statements prepare time tracks unfolded size (15-60 us per unfolded
KB), not text size.

The two other large items are one node: the multi-output bundle's union of every output (684
branches in the registry, 569 CTEs in the fixture). The union is an inline delta read by three
statements (`DELETE .. w = 0`, the `ON CONFLICT` integration, `output_delta`), so each holds all
member CTEs: 196 KB per statement in the fixture, 236 KB in the registry, 30-43 ms to prepare
each, and each execution materializes every member CTE again (`stmt:WITH union _i` 101.9 ms,
`output_delta` 111 ms over the 3 programs, prepare included).

Statements by class, sum of the 3 programs (`prep1:` first prepare, `stmt:` execute; last column rows for `stmt:`, SQL bytes for `prep1:`):

| statement class (sum of 3 programs, sqlite) | ms | runs | rows/bytes |
|---|---|---|---|
| `prep1:WITH join _d` | 295.5 | 799 | 2291717 |
| `prep1:WITH union _d` | 275.8 | 74 | 1018244 |
| `prep1:WITH union _i` | 184.8 | 230 | 1800059 |
| `stmt:SELECT output_delta` | 111.0 | 7 | 9278 |
| `stmt:WITH union _i` | 101.9 | 338 | 14325 |
| `stmt:WITH join _d` | 100.2 | 1823 | 5639 |
| `stmt:WITH union _d` | 26.4 | 182 | 5285 |
| `prep1:INSERT mint _d` | 18.2 | 462 | 305860 |
| `stmt:SELECT count_work` | 12.9 | 961 | 0 |
| `stmt:WITH window _d` | 12.8 | 4 | 160 |
| `stmt:INSERT ` | 11.5 | 948 | 2918 |
| `stmt:INSERT mint _d` | 10.6 | 948 | 8307 |
| `prep1:INSERT ` | 10.1 | 460 | 160494 |
| `meter:INSERT ` | 8.4 | 412 | 14461 |
| `stmt:INSERT source _i` | 6.5 | 23 | 9697 |
| `stmt:INSERT join _d` | 6.1 | 591 | 11140 |
| `stmt:WITH source _i` | 6.1 | 74 | 4764 |
| `prep1:WITH join _i` | 5.9 | 42 | 75435 |
| `stmt:WITH join _i` | 5.3 | 246 | 573 |
| `prep1:INSERT mint _i` | 5.2 | 203 | 91977 |
| `prep1:INSERT join _d` | 4.8 | 186 | 136533 |
| `stmt:INSERT mint _i` | 4.5 | 399 | 3573 |
| `prep1:WITH antijoin _d` | 3.8 | 44 | 67023 |
| `prep1:DELETE mint _i` | 3.7 | 203 | 54332 |

Install (engine CREATEs, lever 4) is the other large comptime item: 322 ms of the registry's
comptime; untouched here.

## Change

sqlite_ivm `crates/ivm-sqlite/src/3_nodes.rs`, `SqlRel::inline_joins` (the former `place` folded
in). The placement pass, which visits nodes readers-first, also counts per node the references
SQLite expands to its delta CTE over all statements that hold it: an output read counts 1, an
integrated node 2, a stored node's statements 1 per reference, and an inline node passes its own
count times its references to each input. Two rules change:

1. A join delta becomes a CTE only when that count is at most 1 (the other conditions are as
   before: one reading statement, counted by it, settle-owned, not recursive).
2. A union read by more than one statement is stored. Before, it was stored only when that let
   two or more join deltas under it inline.

No other planner, SQL text or engine change; sprefa unchanged. The pinned CREATE count of the
c15-shaped install (`crates/ivm-sqlite/tests/6_install_growth.rs`) moves 2232 → 2477: the
newly stored join deltas and unions are tables.

Variants measured before choosing (2-run means, settle ms registry / fixture / catalog): base
733 / 280 / 342; join cap 2 + union by width ≥ 64: 504 / 134 / 170; join cap 4 + same: 518 /
156 / 193; join cap 2, every multi-read union stored: 428 / 108 / 133; join cap 1, every
multi-read union stored: 386 / 82 / 108; no join CTEs at all, every multi-read union stored:
391 / 79 / 101. The cap-1 variant is the change.

## How it works

SQLite resolves a reference to a CTE by copying the CTE's select tree into the reference
(`selectExpander` → `sqlite3SelectDup`), materialized or not, and then expands the copy's own
CTE references. A join delta `ΔL⋈R_i ∪ L_i⋈ΔR ∪ ΔL⋈ΔR` names each input delta twice, so a CTE
join reading a CTE join doubles the work per level. With the cap, a join delta is a CTE only
where its tree is walked once; any join that would be walked twice writes its own `_d` table in
its own statement, and the statements reading it name a table. The bundle union of every output
now fills one `_d` table in one statement; the `w = 0` delete, the integration and
`output_delta` read that table instead of each carrying (and materializing) all member CTEs.
Rows written, their order and every output are the same; the statements differ in which deltas
are tables and which are CTEs.

## After

### Comptime split (ms; one instrumented build with the same rule, 2-run means)

| program | settle before | settle after | first prepare before | first prepare after | first prepares |
|---|---|---|---|---|---|
| registry | 733.2 | 385.7 | 398.8 | 153.8 | 3610 → 3884 |
| fixture | 279.6 | 81.7 | 204.4 | 46.7 | 435 → 517 |
| catalog | 341.8 | 108.1 | 238.8 | 58.2 | 689 → 853 |

### Phases, uninstrumented binaries, 3 interleaved runs each (ir:sqlite, `dl8 compile`)

Load 6.3-8.2.

| program | install ms (all 3 installs) | macrotime ms | comptime ms |
|---|---|---|---|
| registry | 427-436 → 473-483 | 632-640 → 549-564 | 1106-1112 → 836-841 |
| fixture | 194-195 → 207-212 | 252-255 → 209-219 | 372-386 → 201-207 |
| catalog | 205 → 222-224 | 278-279 → 233-238 | 440-454 → 250-254 |

### Census (104 programs, `dl8 compile`, sums, s)

Load 3.5-5.3. 104 programs exit 0 in all four runs.

| phase | dd before | dd after | sqlite before | sqlite after |
|---|---|---|---|---|
| read | 1.24 | 1.23 | 1.55 | 1.32 |
| macrotime | 5.96 | 6.04 | 32.85 | 25.39 |
| lower | 0.47 | 0.48 | 0.56 | 0.50 |
| check | 0.12 | 0.13 | 0.18 | 0.16 |
| comptime | 7.91 | 8.05 | 56.93 | 31.21 |
| `ir install install_ns` | 2.33 | 2.35 | 24.20 | 24.13 |

### Registry build (`dl8 build --out <dir> std/tsi_registry_emit.dl7`), 5 interleaved rounds

Load 6.0-6.7. Output `cmp`-identical to `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs`
on every run.

| engine | build | real s (sorted) | median | max RSS MB |
|---|---|---|---|---|
| ir:dd | before | 0.37 0.37 0.37 0.41 0.44 | 0.37 | 113-125 |
| ir:dd | after | 0.39 0.40 0.42 0.43 0.49 | 0.42 | 114-120 |
| ir:sqlite | before | 1.81 1.82 1.87 1.98 2.01 | 1.87 | 219-244 |
| ir:sqlite | after | 1.38 1.46 1.47 1.48 1.55 | 1.47 | 181-196 |

dd runs no ivm-sqlite code; its spread is load.

### Byte identity

- Registry `cmp`: identical, both engines, before and after.
- Macrotime oracle: `dl8 expand` on all 24 `oracle/macrotime/*_<n>.json` cases, both engines,
  before vs after stdout+stderr+exit code: 48 pairs, 0 differences.
- The 3 split programs: `dl8 compile` stdout+stderr identical before/after (ir:sqlite).

### Suites

`cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book --test-threads=1`, extension dylib
rebuilt from the matching sqlite_ivm tree before each pair.

| suite | before | after |
|---|---|---|
| ir:sqlite | 166 passed, 0 failed, 260 s (load 5.6) | 166 passed, 0 failed, 173 s (load 5.2) |
| ir:dd | 166 passed, 0 failed, 129 s (load 5.5) | 166 passed, 0 failed, 103 s (load 3.0) |

sqlite_ivm `cargo test --workspace --no-fail-fast -- --test-threads=1`: before all pass; after,
one failure (`c15_shaped_ir_installs_one_shared_plan`, pinned CREATE count 2232 vs 2477) until
the count was updated; `6_install_growth` then 7 passed.

No golden or snapshot file changed.

## Not done

- `plain_inner` closure-pass gate: the extra pass measures 3.9 ms (registry), 0 (fixture),
  2.8 ms (catalog); below the noise of the rows above.
- Install (lever 4) is now the largest comptime item on sqlite: registry comptime 836 ms with
  322 ms of it install #3 (before measurement, unchanged by this rule within noise).
