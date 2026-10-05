# Findings: macrotime waves inside the engine (`DL8_MACROTIME`)

Brief: `plans/v8/2026-10-05-macrotime-waves-engine.brief.md`. sprefa branch
`feat/macrotime-waves-engine` from `0fe9df6db` (main `1a615c617` plus the brief). sqlite_ivm
unchanged: symlink to `~/projects/sqlite_ivm` at main `14675e8`. Debug dl8 (`[profile.dev]
opt-level = 1`, dependencies `opt-level = 3`), `-j 4`, `--test-threads=1`. Counts only, no clock.

## Switch

`DL8_MACROTIME=rust|engine`, unset = `rust`, any other value exits 2 at start. Read once:
`init_macrotime_mode` (`src/_1_macrotime/_8_waves.rs`, a `OnceLock`, same shape as the
rule-interpreter lane's `DL8_RULES`) is called from `main` in `src/bin/dl8.rs` beside
`configured_backend`; a library caller that never called it reads the env on first use.

| reader of the mode | `rust` | `engine` |
|---|---|---|
| `macrotime_text` (`src/_8_driver/_0_read.rs`) | `macrotime/0..2` | `macrotime/0..2` + `macrotime/3_waves.dl7` |
| `slice_macro_program` (`_7_slice.rs`) | `macro_rules` cone | `wave_rules` cone, wave relation declarations kept |
| `expand_rows` (`_4_expand.rs`) | Rust wave loop, unchanged | `expand_rows_engine` (`_8_waves.rs`) |

## How it works

### Rows in, rows out

```ts
// one unit, engine mode
function expandRowsEngine(rows: GraphRow[]): [GraphRow[], OriginRow[], Diagnostic[]] {
  if (macroDispatch(rows) === "absent") return [rows, [], []]   // same shortcut as rust mode
  const seeds = rows.map(toGraphSeed)          // graph_node, graph_item(owner, ref(child), ord), ...
  session.enterWaves(seeds)                    // one settle: previous unit -1 (or rewind), these +1
  const present = session.base + session.net   // the read relations only
  return readResult(present)                   // diagnostics first, else graph_* and expansion_*
}
```

The unit's syntax graph enters once, as the wave-0 rows of the eight `graph_*` wave relations
(the A attempt's `GRAPH_KINDS` shapes; an item edge drops its `item` label). The settle runs
every wave to the fixed point. Rust reads 21 relations from `base + net`: the eight `graph_*`,
`cur_wave`, `expansion_claim`, `expansion_output`, `replaced`, and the eight stop relations.

### The program

`macrotime/3_waves.dl7` is the A attempt's `3_rewrite.dl7` (validated byte-identical against the
Rust rewrite on the oracle and the registry) with the wave column removed from every in-wave
relation, plus the `next` rules. The macros themselves are unchanged: bridges copy `graph_*`
into `node`, `:` item edges, `syntax_form`, `syntax_atom`, `syntax_literal`, `syntax_variable`,
`syntax_source`, so the whole macro cone becomes in-wave relations.

| class | relations | per wave |
|---|---|---|
| wave (`next` heads) | `cur_wave`, `graph_*` (8), `wave_hist_*` (8 + `wave_hist_wave`), `expansion_claim`, `expansion_output` | wave W + 1 = what the `next` rules derive at W |
| in-wave, macros | `syntax_claim`, `:` expansion edges, minted `GeneratedSyntax` nodes, ... | recomputed over wave W's graph |
| in-wave, rewrite | walk, slots, reach, `removed_*`, `added_*`, stop relations, `wave_post_*`, `wave_state_*`, `wave_differ` | recomputed over wave W's graph |

Gates carry the wave number (`rewrite_pending(W)`, `rewrite_stop(W)`, `rewrite_ready(W)`,
`rewrite_cycle(W)`, `rewrite_apply(W)`), each joined from `cur_wave`; nothing else does.

```
rewrite_ready(W) :- rewrite_pending(W), not rewrite_stop(W).
rewrite_apply(W) :- rewrite_ready(W), not rewrite_cycle(W).
next graph_k(x)  :- rewrite_apply(W), wave_post_k(x).            ; post = graph - removed + added
next graph_k(x)  :- cur_wave(W), not rewrite_apply(W), graph_k(x).
next cur_wave(N) :- rewrite_apply(W), int.add(W, 1, N).
next cur_wave(W) :- cur_wave(W), not rewrite_apply(W).
next expansion_claim(N, M, W) :- rewrite_apply(W), rewrite_hit(N, M).   ; plus carry
```

A wave that claims nothing, stops on a diagnostic, or repeats a graph changes no wave relation,
so the LetRec reaches its fixed point at that wave and the final in-wave values are that wave's.

### Lowering

`DL8_IR_DUMP` of the macro program (oracle `list_literal_nested_0`): one wave LetRec, 21 ids
(`__ir_wave_started` + 20 wave relations), `limit` 64, 9 nested LetRecs (one of 16 ids: the macro
cone's mutual recursion through `:`/`node`/`syntax_*`; one of 3: `reach` with
`rewrite_identity_query`/`rewrite_identity`; seven of one relation, among them the rewrite's
`rewrite_up`, `rewrite_path`, `rewrite_walk`, `rewrite_candidate`, `slot_start`). After the
wave LetRec the cone lowers again as ordinary strata over the final wave (130 strata, 5,242 nodes,
10 LetRecs in total), so the cone is evaluated once more after the waves.

### Intern construction in a wave rule (brief item 2)

The macros mint output nodes (`GeneratedSyntax`, `intern` with the output unbound): an IR `Mint`
plus an `intern/3` request row. `_9a_ir_lower.rs` refused any intern in a wave or cone rule
(`intern readback in a wave rule`). Now:

| case | result |
|---|---|
| wave or cone rule constructs, no readback anywhere in the program | lowered: `Mint` inside the LetRec body; the request rows go to the one `intern/3` request stratum after the LetRec, over the final wave's values |
| wave or cone rule reads back (`intern` with constructor or args unbound) | `unsupported_construct(intern readback in a wave rule)` (unchanged) |
| wave or cone rule constructs a sum-readback constructor (snapshot) | `unsupported_construct(intern snapshot in a wave rule)` |
| wave or cone rule constructs and another rule reads back | `unsupported_construct(intern construction in a wave rule beside an intern readback)`: per-level request relations would see only the final wave |

The macro program has no readback, so the first row applies.

### Stop diagnostics in engine mode

| diagnostic | rust mode | engine mode |
|---|---|---|
| macro's `syntax_diagnostic` on an active node | read before claims, wave stops | `rewrite_reported` -> `rewrite_stop`, no rewrite, fixed point; Rust reads it first |
| `conflicting_macro_claims`, `non_dense_expansion_ordinals`, `unknown_expansion_output` | from the walk | stop relations; ordinals of a non-dense node read from `replaced` |
| `expansion_cycle(W)` | `Graph::hash` of the rewritten graph in `seen` (initial and every applied graph) | in the rules: `wave_hist_*` keeps the graph of every applied wave, `wave_state_*` = history plus wave W's graph, `wave_differ(V)` when either side holds a row the other does not, `rewrite_cycle(W)` when some state V has no difference; no rewrite, fixed point; Rust reads `expansion_cycle(W)` |
| `expansion_round_limit(64)` | `for wave in 0..64` ends with a claim | wave LetRec `limit: 64`: evaluations 1..=64 hold waves 0..63, a change in evaluation 65 (a rewrite at wave 63) fails the settle with `wave_limit(64)`, mapped by `failed_terms` |

The bound matches exactly: 63 rewrites settle, 64 fail, in both modes
(`fixtures/macrotime_waves/{1_round_limit,2_last_wave}.json`).

Differences left (no test, oracle case or census program reaches them):

- More than one rewrite diagnostic in one wave: engine mode prints them in term order, rust
  mode in walk order (and twice for a node the walk reaches twice).
- `non_dense_child_ordinals`, `non_dense_frontier_ordinals`, `missing_node_row`: engine mode
  only; they guard the running-sum slot start and the derived-node reach (A findings, same
  rules).
- `expansion_cycle` is exact in engine mode; rust mode compares 64-bit hashes.
- Only a period-1 cycle is exercised (`0_cycle`: a macro whose expansion of a form is the form).

## Parity

Suites: `DL8_ENGINE=<e> DL8_MACROTIME=<m> cargo test -j 4 --workspace --no-fail-fast -- --skip
_7_book --test-threads=1`. Main baseline 177 (2 + 175) per engine.

| engine | mode | result |
|---|---|---|
| ir:sqlite | rust | 179 passed, 1 failed (below), 1 ignored (`work_counts_rust_vs_engine`); 181 tests (2 + 179) |
| ir:dd | rust | 179 passed, 1 failed (below), 1 ignored |
| ir:sqlite | engine | 179 passed, 1 failed (below), 1 ignored |
| ir:dd | engine | 179 passed, 1 failed (below), 1 ignored |

The full runs were made on a tree-wide `cargo fmt` of the same code; the commit carries only the
feature files, unformatted elsewhere. On that tree the one failure per run was
`_9h_build::string_kernel_files_are_current` (fmt reformatted the generated
`src/_6_eval/_4a_str_ops.rs`, byte-identical to its emitted form in the commit). On the commit,
`_9h_build` and `_9l_macrotime_waves` pass on both engines.

New tests (both engines x both modes inside each test, so every suite run covers both modes):
`tests/it/_9l_macrotime_waves.rs` (`0_cycle` -> `expansion_cycle(0)`, `1_round_limit` ->
`expansion_round_limit(64)`, `2_last_wave` settles; `dl8 expand` stdout byte-identical across the
four runs), and `_9h_build::tsi_registry_sqlite_work_counts` now pins both modes.

| check | result |
|---|---|
| macrotime oracle `every_macrotime_case_matches_v7` (24 cases) | equal to v7 in all four suite runs |
| `macrotime_cases_share_one_engine` (24 cases, one session) | passes in engine mode: 1 prepare, 0 reprepares, 1 fingerprint |
| registry `cmp` against `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs` | identical, 4 of 4 |
| census, `dl8 compile` stdout and exit, rust vs engine | equal for 132 of 132 programs on both engines |

## Work counts, rust vs engine

Source: `cargo test -j 4 --test it -- _9l_macrotime_waves::work_counts_rust_vs_engine --ignored
--nocapture --test-threads=1` (Rust; parses the `ivm_sqlite::work`, `dd install`, `dd settle`
events of `RUST_LOG=ivm_sqlite::work=info,ivm_dd=debug,dl8::macrotime=info`). "total" sums every
event of the run; "macrotime session" sums from the install before the first `macrotime prepare`
to the next install after the last `macrotime unit` line. The total includes the macro library's
own compile, which in engine mode carries `3_waves.dl7`. Two runs (a throwaway tabulation, then the test) gave
identical counts. sqlite rows: totals over the install,
settle and between phases; the registry per-phase split is pinned in
`_9h_build::tsi_registry_sqlite_work_counts` for both modes.

Engine-mode cost sources visible in the IR: the whole macro cone runs inside every wave (rust mode
runs it once per settle over the changed rows), the cone is evaluated a second time after the
wave LetRec over the final wave (every in-wave relation is visible), the cycle check keeps a
history copy of the graph per applied wave, and ivm-sqlite takes the recompute path for any
LetRec with a limit.
### registry, total

| count | ir:sqlite rust | ir:sqlite engine | ir:dd rust | ir:dd engine |
|---|---|---|---|---|
| dd install arrangements | - | - | 1474 | 3008 |
| dd install nodes | - | - | 5766 | 14121 |
| dd install operators | - | - | 21603 | 57729 |
| dd settle arranged_records | - | - | 115730 | 487705 |
| dd settle changes_in | - | - | 37225 | 39702 |
| dd settle changes_out | - | - | 18997 | 48422 |
| dd settle epochs | - | - | 9 | 8 |
| dd settle steps | - | - | 949 | 1921 |
| sqlite total autoindexes | 23363 | 100881 | - | - |
| sqlite total creates | 4963 | 11199 | - | - |
| sqlite total executions | 83894 | 125175 | - | - |
| sqlite total fullscan_steps | 2506709 | 7092260 | - | - |
| sqlite total prepared | 11213 | 26225 | - | - |
| sqlite total prepared_bytes | 5502919 | 12595533 | - | - |
| sqlite total reprepares | 3 | 3 | - | - |
| sqlite total rows_returned | 105687 | 135051 | - | - |
| sqlite total schema_rows_parsed | 620094 | 2478066 | - | - |
| sqlite total sorts | 16321 | 77343 | - | - |
| sqlite total unfolded_bytes | 5755706 | 12885048 | - | - |
| sqlite total vm_steps | 42216224 | 183410526 | - | - |
| sqlite total written_catalog | 39 | 40 | - | - |
| sqlite total written_d | 221502 | 763858 | - | - |
| sqlite total written_i | 108610 | 398945 | - | - |
| sqlite total written_scratch | 95433 | 184528 | - | - |
| sqlite total written_source | 23381 | 25020 | - | - |
| sqlite total written_term | 38964 | 41870 | - | - |
| sqlite total written_x | 0 | 0 | - | - |
| sqlite total zero_row_executions | 20185 | 45830 | - | - |

### registry, macrotime session

| count | ir:sqlite rust | ir:sqlite engine | ir:dd rust | ir:dd engine |
|---|---|---|---|---|
| dd install arrangements | - | - | 334 | 1101 |
| dd install nodes | - | - | 1069 | 5242 |
| dd install operators | - | - | 3936 | 21987 |
| dd settle arranged_records | - | - | 87074 | 451888 |
| dd settle changes_in | - | - | 28063 | 28266 |
| dd settle changes_out | - | - | 13612 | 41994 |
| dd settle epochs | - | - | 6 | 5 |
| dd settle steps | - | - | 195 | 1153 |
| sqlite total autoindexes | 22596 | 100114 | - | - |
| sqlite total creates | 1011 | 4126 | - | - |
| sqlite total executions | 46373 | 77048 | - | - |
| sqlite total fullscan_steps | 1195791 | 5553603 | - | - |
| sqlite total prepared | 2367 | 10563 | - | - |
| sqlite total prepared_bytes | 892438 | 4786316 | - | - |
| sqlite total reprepares | 0 | 0 | - | - |
| sqlite total rows_returned | 41931 | 65721 | - | - |
| sqlite total schema_rows_parsed | 59499 | 989098 | - | - |
| sqlite total sorts | 4403 | 57784 | - | - |
| sqlite total unfolded_bytes | 938723 | 4878899 | - | - |
| sqlite total vm_steps | 23130897 | 161059794 | - | - |
| sqlite total written_catalog | 17 | 17 | - | - |
| sqlite total written_d | 136854 | 660962 | - | - |
| sqlite total written_i | 74695 | 353184 | - | - |
| sqlite total written_scratch | 70428 | 152146 | - | - |
| sqlite total written_source | 17761 | 18169 | - | - |
| sqlite total written_term | 28105 | 29039 | - | - |
| sqlite total written_x | 0 | 0 | - | - |
| sqlite total zero_row_executions | 15208 | 36033 | - | - |


Census: 132 programs, 112 exit 0 in all four runs; stdout and exit equal between modes for all 132 on both engines.
### census (112 programs), total

| count | ir:sqlite rust | ir:sqlite engine | ir:dd rust | ir:dd engine |
|---|---|---|---|---|
| dd install arrangements | - | - | 119579 | 291387 |
| dd install nodes | - | - | 495060 | 1430820 |
| dd install operators | - | - | 1720156 | 5766268 |
| dd settle arranged_records | - | - | 2149637 | 5667987 |
| dd settle changes_in | - | - | 1021513 | 1330995 |
| dd settle changes_out | - | - | 527532 | 817971 |
| dd settle epochs | - | - | 538 | 533 |
| dd settle steps | - | - | 12754 | 23680 |
| sqlite total autoindexes | 352479 | 819960 | - | - |
| sqlite total creates | 388293 | 1086725 | - | - |
| sqlite total executions | 2453508 | 5296872 | - | - |
| sqlite total fullscan_steps | 59338859 | 129309840 | - | - |
| sqlite total prepared | 645500 | 2234078 | - | - |
| sqlite total prepared_bytes | 318849726 | 1068512159 | - | - |
| sqlite total reprepares | 24 | 24 | - | - |
| sqlite total rows_returned | 3116674 | 4043228 | - | - |
| sqlite total schema_rows_parsed | 27172309 | 235265173 | - | - |
| sqlite total sorts | 520760 | 3593689 | - | - |
| sqlite total unfolded_bytes | 341150646 | 1092537125 | - | - |
| sqlite total vm_steps | 985513625 | 2397701206 | - | - |
| sqlite total written_catalog | 3745 | 3964 | - | - |
| sqlite total written_d | 4689512 | 11385064 | - | - |
| sqlite total written_i | 2617620 | 6363104 | - | - |
| sqlite total written_scratch | 2097456 | 3830483 | - | - |
| sqlite total written_source | 647847 | 841208 | - | - |
| sqlite total written_term | 1067456 | 1400018 | - | - |
| sqlite total written_x | 0 | 0 | - | - |
| sqlite total zero_row_executions | 654151 | 2265988 | - | - |

### census (112 programs), macrotime session

| count | ir:sqlite rust | ir:sqlite engine | ir:dd rust | ir:dd engine |
|---|---|---|---|---|
| dd install arrangements | - | - | 37408 | 123312 |
| dd install nodes | - | - | 119728 | 587104 |
| dd install operators | - | - | 440832 | 2462544 |
| dd settle arranged_records | - | - | 849918 | 3566236 |
| dd settle changes_in | - | - | 350899 | 405693 |
| dd settle changes_out | - | - | 129693 | 303316 |
| dd settle epochs | - | - | 269 | 264 |
| dd settle steps | - | - | 6267 | 15625 |
| sqlite total autoindexes | 281001 | 748482 | - | - |
| sqlite total creates | 113232 | 462112 | - | - |
| sqlite total executions | 798310 | 2453802 | - | - |
| sqlite total fullscan_steps | 11624190 | 56088403 | - | - |
| sqlite total prepared | 209064 | 1034250 | - | - |
| sqlite total prepared_bytes | 85968777 | 477280809 | - | - |
| sqlite total reprepares | 0 | 0 | - | - |
| sqlite total rows_returned | 516060 | 818326 | - | - |
| sqlite total schema_rows_parsed | 6663888 | 110778976 | - | - |
| sqlite total sorts | 169102 | 2386239 | - | - |
| sqlite total unfolded_bytes | 90624252 | 484714496 | - | - |
| sqlite total vm_steps | 277057516 | 1323519826 | - | - |
| sqlite total written_catalog | 1394 | 1501 | - | - |
| sqlite total written_d | 1739626 | 6391402 | - | - |
| sqlite total written_i | 940863 | 3359595 | - | - |
| sqlite total written_scratch | 795026 | 1701829 | - | - |
| sqlite total written_source | 237471 | 292960 | - | - |
| sqlite total written_term | 384088 | 495786 | - | - |
| sqlite total written_x | 0 | 0 | - | - |
| sqlite total zero_row_executions | 240472 | 1312469 | - | - |

ok


## Overlap with `feat/rule-interpreter`

| file | this lane | rule-interpreter (`30c3db902`, `0f0ea0d1d`) |
|---|---|---|
| `src/bin/dl8.rs` `main` | `.and(init_macrotime_mode())` on the `configured_backend()` match | `.and(init_rules_mode())` on the same expression: textual conflict, both chains keep |
| `src/_6_eval/_9a_ir_lower.rs` | intern refusal split | untouched there |
| `_2_stratify.rs`, `_9b_ir_eval.rs`, `mod.rs` | untouched | changed |

Its `rule_reason` sends any `next` head to compiled mode ("next head (waves)"), so the engine-mode
macro program stays compiled under `DL8_RULES=interpret` if that switch reaches the macro session.
