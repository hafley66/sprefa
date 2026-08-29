# rust — sprefa-extract corpus battery report (2026-08-28)

- [Setup](#setup)
- [Step 1 — per-file default](#step-1--per-file-default)
- [Step 2 — per-file by family](#step-2--per-file-by-family)
- [Step 3 — --resolve per crate](#step-3---resolve-per-crate)
- [Step 4 — diet_scip](#step-4--diet_scip)
- [Step 5 — scip](#step-5--scip)
- [Perf and RSS](#perf-and-rss)
- [Findings](#findings)
- [Fix landed](#fix-landed)
- [Fix landed: the large-file resource bound](#fix-landed-the-large-file-resource-bound)
- [What stays untested and why](#what-stays-untested-and-why)

## Setup

Worktree `chore/extract-corpus-rust`, merged 8e946ad. Corpus
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (2506 crates). All
extract calls under `timeout 10`. Raw step-1 table: `rust.runs.tsv`.

## Step 1 — per-file default

| metric | value |
|---|---|
| files run | 77,472 |
| rc=0 | 77,246 |
| rc=1 | 225 (all `NotFound`: files deleted from the registry between listing and run; none repro on disk) |
| rc=124 (timeout) | 1: `nickel-lang-core-0.15.3/src/parser/grammar.rs` (29,328,358 B machine-generated) |
| parse errors | 0 |
| total wall ms | 1,671,625 (median ≈ 10 ms/file, process startup bound) |
| total output lines | 402,878,522 |

Macro-heavy crates (`serde_derive*`, `tokio-macros*`, `syn-*`): 957 files, all
rc=0.

## Step 2 — per-file by family

200-file sample (100 largest files + 100 random). One harness note: the
battery's line counter strips the trailing newline, so step-1 "lines" is one
below a fresh `wc -l`; corrected below.

| sum over 100 files | lines |
|---|---|
| default (all families) | 662,977 |
| cst | 580,390 |
| type | 7,469 |
| call | 13,003 |
| df | 62,215 |
| family sum | 663,077 |
| sum minus default | 100 (= 1/file, the trailing-newline counter artifact) |

Per-file, family sum equals the default run exactly. A family whose sum
exceeds the default: none.

## Step 3 — --resolve per crate

300 largest crates by file count, all `.rs` files of the crate as one project.

| metric | value |
|---|---|
| crates | 300 |
| rc=0 | 300 |
| resolved_edge | 1,086,884 |
| unresolved rows | 0 (the rust arm emits no row for ambiguous/absent callees: the documented 4b-iii discipline) |
| total wall ms | 184,226 |
| slowest crates | nickel-lang-core 27,345 ms; windows-0.61.3 17,031 ms; windows-0.62.2 15,316 ms |

Silent-drop note: an unqualified call whose callee resolves to nothing emits
no `--resolve` row at all. The per-file `site` record with `callee_path: null`
carries the fact (fixture `tests/fixtures/rust/corpus_1.rs`); finding F4.

## Step 4 — diet_scip

| metric | value |
|---|---|
| crates | 300 |
| rc=0 | 300 |
| resolved_edge | 1,086,884 (identical to step 3: same name-match leg) |
| total wall ms | 248,555 |

## Step 5 — scip

| root | result |
|---|---|
| `v6/sprefa-extract` | scip_skip: failed — the crate is a workspace member; `cargo metadata` fails outside the workspace root |
| `v6/sprefa-engine-rs` | scip_skip: same cause |
| worktree root `.` | indexed: scip_fn_edge=29,813, scip_def=8,856, scip_ref=14,024 |
| `aho-corasick-1.1.4` (copied to scratch) | indexed: scip_fn_edge=3,925 |

scip vs resolve on `aho-corasick-1.1.4`: scip_fn_edge 3,925 vs resolved_edge
1,549. Name formats differ (qualified scip symbols vs bare names), so the
edge sets were compared by callee name tail. Sample of 20 scip-only edges,
classified:

| class | count | examples |
|---|---|---|
| method call on typed receiver (scip knows the type) | 8 | `Automaton#is_match()` |
| type/const/variant constructor refs (outside the call facet) | 6 | `Anchored#Yes#`, `REGRESSION.` |
| trait-method through impl and dyn | 3 | `Arc<dyn AcAutomaton>::patterns_len` |
| macro site | 1 | `testconfig!` |
| inherent impl same-file (scip splits impl blocks) | 2 | `Input<'h>::haystack()` |

Resolve-only edges (787) are method calls on receivers whose type is not
nameable by the parse (all `is_match`/`follow_transition` impl-site matches)
plus std (`try_from`, `alloc_transition`) and closure/field callees.

## Perf and RSS

- bytes/ms 5th percentile: 19.46 (`serde_with_macros-3.21.0/tests/serde_as_issue_267.rs`, 253 B / 13 ms). Sub-percentile rows are 1-10 B files where process startup dominates: no construct is slow.
- The slow tail is machine-generated tables, not a language construct: nickel grammar.rs (20.8 s unbounded, 13,664,266 output lines), chrono-tz `timezones.rs` (7.2 MB), windows `mod.rs` batch.
- RSS, 20 largest files (`rust.rss_top20.tsv`): peak 3,762,643,968 B (3.74 GB) on nickel `grammar.rs` (128x input size); 824 MB on chrono-tz; 200-500 MB on the windows `mod.rs` set.

## Findings

| lang | class | path:line | repro command | observed | expected |
|---|---|---|---|---|---|
| rust | timeout | nickel-lang-core-0.15.3/src/parser/grammar.rs | `timeout 10 extract <grammar.rs>` | rc=124; unbounded 20.8 s, 13.6M lines | completes or streams bounded |
| rust | rss | nickel-lang-core-0.15.3/src/parser/grammar.rs | `/usr/bin/time -l extract <grammar.rs>` | peak RSS 3,762,643,968 B | bounded footprint |
| rust | wrong_fact (FIXED) | v6/sprefa-extract/src/lang/rust.rs `call_name_match` | `cargo test --features cli --test 60_rust_corpus_scope` | same-file call bound to a foreign file's identical `(name, span)` def (12_rust_scope_helper_a.rs) | same-file def wins |
| rust | missing_fact | `--resolve` pipeline, `CallF` resolve | `extract --family call tests/fixtures/rust/corpus_1.rs` vs `--resolve` with a second file | unresolved call site emits no `--resolve` row (by-design 4b-iii); only per-file `site` with `callee_path: null` carries it | a visible unresolved marker in project mode, or a documented flag |

## Fix landed

- Failing test FIRST: `tests/60_rust_corpus_scope.rs` red (observed edge
  `callee_path: 12_rust_scope_helper_a.rs`), then the fix in
  `src/lang/rust.rs` (`own_file_blob` file-fingerprint join over the whole
  named-def span set of the file), then green.
- Whole crate after the fix: `cargo test --features cli` 354 passed, 0 failed,
  2 ignored. aho-corasick `--resolve` count unchanged (1,549 edges).

## What stays untested and why

- `include!`-expanded sources: no registry crate ships the included files
  outside `build.rs` codegen; not reproducible without cargo build.
- `#[cfg]`-gated modules under their enabled cfgs: needs a target's
  feature set; phase-1 is cfg-blind by design and every cfg form parsed rc=0
  in step 1.
- `mod.rs` vs `foo.rs` scope owner on the real corpus: covered by
  `tests/30_rust_mod_scope_owner.rs` fixtures; no registry-only variant adds a
  shape the fixtures lack.
- scip on the two v6 crate roots directly: impossible until they are
  standalone packages (workspace-membership failure, recorded above); the
  worktree root substitutes.
- Trait-impl method callees under `--resolve`: name-only by the 4a ADDENDUM
  (receiver typing out of scope); scip is the oracle for those edges (step 5).

## Fix landed: the large-file resource bound

Lane `fix-extract-large-files`, base sha `99b8dc79f`, commit "extract: stream
flat facts instead of collecting them". Full write-up and the byte-identity
receipt: `ts.REPORT.md` section 12.

| finding | before | after | test |
|---|---|---|---|
| `rss` on `nickel-lang-core-0.15.3/src/parser/grammar.rs` (29,328,358 B) | 3,610,509,312 B peak RSS | 3,004,694,528 B | `tests/9_large_file_bounds.rs::rs_all_families_rss_is_bounded` |
| wall, same file, no `timeout` | 14.24 s | 12.55 s | (same) |
| `timeout` rc=124 under `timeout 10`, same file | rc=124 | rc=124, **unchanged** | none; blocked, below |

Output on that file is byte-identical before and after: 13,664,266 rows,
`cmp -s` clean.

### The timeout is parse time, not row time

`extract --bench --family <f>` on the same file splits extract from flatten:

| family | extract | serial (flatten) | rows | peak RSS |
|---|---|---|---|---|
| cst | 7.043 s | 224.8 ms | 11,416,699 | 2,039,300,096 B |
| type | 5.119 s | 6.9 ms | 175,430 | 2,419,736,576 B |
| call | 5.494 s | 4.0 ms | 129,247 | 2,292,367,360 B |
| df | 4.475 s | 71.3 ms | 1,942,890 | 2,876,817,408 B |
| data | (0.02 s wall) | | 0 | 34,684,928 B |
| default (all) | | | 13,664,266 | 3,004,694,528 B, 12.55 s |

Flatten is 0.1% to 3% of each family's wall. The 12.55 s is the ast-grep parse
(7.0 s) plus the one shared syn parse and its three projections. No change to
the row plane moves it, and the 3.0 GB floor is the syn AST: `--family type`
peaks at 2.42 GB while emitting 175,430 rows.

The corpus gap is sharp. Second-slowest rust file is `chrono-tz` `timezones.rs`
at 3,789 ms for 7.2 MB; nickel `grammar.rs` is 29.3 MB. One file in 77,472
exceeds 10 s.

### Named size skip: designed, BLOCKED on ownership

The user law is that a silent timeout is a defect and a named skip is a fact.
The row, mirroring `scip_skip`:

```
record=size_skip  path=<string>  bytes=<u32>  limit=<u32>  reason=<over_max_bytes>
```

`extract` emits that one row and exits 0 when the input exceeds the ceiling;
`--max-bytes N` overrides it, `--max-bytes 0` disables it. A ceiling of
16,777,216 B skips nickel `grammar.rs` and no other file in the 77,472-file
rust corpus, no ts/js corpus file, and no fixture, so every golden stays
byte-identical.

Not landed. A `FlatFact` variant lives in `src/types.rs`, its contract line in
`src/schema.rs`, and the flag in `src/bin/extract.rs` plus
`src/bin/extract/help.rs`. `types.rs` and `schema.rs` are shared by every
language arm and are on this lane's forbidden list, so the coordinator was
beeped for the call rather than the files edited. The design above is complete
and needs no further measurement.
