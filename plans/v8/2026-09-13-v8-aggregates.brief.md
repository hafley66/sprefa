# v8 lane: `sum`, `min`, `max` aggregate heads beside `count`

## TOC
1. Goal
2. Base and first action
3. Files you own, files you never touch
4. Design (decided, do not redesign)
5. Steps with receipts
6. Validation commands
7. Style laws
8. Finish protocol

## 1. Goal
A rule head today may carry one `(count ?X)`. After this lane it may carry
one of `(count ?X)`, `(sum ?X)`, `(min ?X)`, `(max ?X)`. `count` keeps its
frozen JSON shape `{"count": arg}` so every `v8/oracle/**` fixture stays
byte-identical. Three new fixtures run through the real binary.

## 2. Base and first action
- Base sha: `ccbd9502e75a6a0c65206af434c0091892d73423` (`origin/main`, PR #737 merged).
- Branch `feature/v8-aggregates-20260913`, worktree under
  `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only ccbd9502e75a6a0c65206af434c0091892d73423`. Failure = stop and report.
- Crate: `v8/` (package `dl8`). Every command runs from `v8/`.

## 3. Files you own
| file | site | change |
|---|---|---|
| `v8/src/_6_eval/_1_program.rs` | `:40-50` `Arg::Count` | `Arg::Aggregate(AggregateKind, Box<Arg>)`, `is_aggregate`, `aggregate_args` |
| `v8/src/_6_eval/_5_evaluate.rs` | `:372-410` `aggregate_rows` | fold by kind |
| `v8/src/_6_eval/_6_json.rs` | `arg_from_json` (grep `"count"`) | read `count`, `sum`, `min`, `max` keys; write side keeps `{"count": ..}` and adds the others |
| `v8/src/_2_lower/_8_express.rs` | `:606-620` | accept the four head atoms |
| `v8/src/_2_lower/_7_execute.rs` | `:155` | `aggregate_outside_rule_head` for all four |
| `v8/src/_3_check/_2_resolve.rs` | `:170` | all four |
| `v8/src/_3_check/_6_strata.rs` | `:25`, `:205` | all four |
| `v8/src/_4_comptime/_2_rounds.rs` | `:421` | all four |
| `v8/tests/_11_aggregates.rs` | new | real-binary test |
| `v8/fixtures/aggregates/*.dl7`, `*.expected.json` | new dir | |

Never touch: `_6_eval/_0_term.rs`, `_6_eval/_4_kernel.rs`,
`_2_lower/_9_kernel.rs`, `_0_read/**`, `_3_check/_5_kernel.rs`,
`v8/oracle/**`, `v7/`, `v6/`.

## 4. Design (decided)
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AggregateKind { Count, Sum, Min, Max }
// Arg::Count(Box<Arg>) becomes Arg::Aggregate(AggregateKind, Box<Arg>)
```
Every site listed in section 3 that matches the atom `"count"` matches the
four names through one function `AggregateKind::of(name: &str) -> Option<Self>`
in `_1_program.rs`. The lowered term stays `aggregate(<kind atom>, inner)`,
so `aggregate(sum, ?X)` flows through resolve, strata and rounds unchanged
in shape.

Fold in `aggregate_rows`, per group key (plain head positions):
| kind | seed | step | result type |
|---|---|---|---|
| count | 0 | `+1` per proof | `Int` |
| sum | 0 | `+ value`, `checked_add`, overflow is a `Diagnostic` `aggregate_overflow` | `Int` |
| min | first value | `Universe::cmp` Less wins | the inner term |
| max | first value | `Universe::cmp` Greater wins | the inner term |
`sum` over a non-`Int` inner term is a `Diagnostic` `aggregate_type_mismatch`
carrying the term. `min` and `max` accept any term, ordered by
`_0_term.rs:164 cmp`. A group with zero proofs produces no row (same as
count today).

Strata: an aggregate rule already gets gap one (`_2_stratify.rs:36`); no
change there beyond the kind-agnostic `is_aggregate`.

## 5. Steps with receipts
1. `AggregateKind`, `Arg::Aggregate`, every match site compiles.
   Receipt: `cargo test` 22 green, `v8/oracle/**` untouched, `git status` clean
   under `oracle/`.
2. `aggregate_rows` fold. Receipt: fixtures below.
3. Test `v8/tests/_11_aggregates.rs`, same shape as `tests/_0_eval_oracle.rs`:
   `dl8 compile <fixture.dl7>` through `env!("CARGO_BIN_EXE_dl8")`, take
   `program` from stdout, write to `std::env::temp_dir()`, `dl8 eval <tmp>`,
   compare `closure` with the sibling `*.expected.json` as
   `serde_json::Value`. Expected files are hand-written from the program
   text.

Fixtures:
```lisp
; 0_sum.dl7
(: Input (* (: value int)))
(Input 7)
(Input 35)
(Input 100)
(: Total (* (: sum int)))
(<- (Total (sum ?Value)) (Input ?Value))

; 1_min_max.dl7
(: Input (* (: value int)))
(Input 7)
(Input 35)
(Input 100)
(: Lowest (* (: min int)))
(<- (Lowest (min ?Value)) (Input ?Value))
(: Highest (* (: max int)))
(<- (Highest (max ?Value)) (Input ?Value))

; 2_grouped.dl7
(: Score (* (: player text) (: points int)))
(Score "ann" 3)
(Score "ann" 4)
(Score "bob" 10)
(: PlayerTotal (* (: player text) (: sum int)))
(<- (PlayerTotal ?Player (sum ?Points)) (Score ?Player ?Points))
```
Expected closures: `Total 142`; `Lowest 7`, `Highest 100`;
`PlayerTotal "ann" 7`, `PlayerTotal "bob" 10`. Write the JSON by hand in the
`term_to_json` shape (`_6_json.rs:55-70`).

## 6. Validation commands
```bash
cd v8 && cargo build 2>&1 | tail -3
cd v8 && cargo test 2>&1 | grep -E "^test result|FAILED|panicked"
cd v8 && cargo clippy --all-targets 2>&1 | grep -c "^warning\|^error"   # must print 0
cd v8 && cargo fmt --check
git status --short v8/oracle    # must print nothing
git diff --stat ccbd9502e75a6a0c65206af434c0091892d73423...HEAD   # only owned files
```
Every test runs through the real `dl8` binary. No unit fakes.

## 7. Style laws
- Tests only under `v8/tests/`. Files `_<n>_name.rs`. Everything `pub`.
- No `eprintln!` in `src/`. `tracing` only.
- No em dashes. Banned words in prose and identifiers: provenance,
  substrate, load-bearing, regime, honest, ground (as a verb), refusal,
  "ground truth". No shortnames: `kind`, `inner`, `total`, never `k`, `i`,
  `t` in new code.
- Comments state only constraints the code cannot show.
- No function over 70 lines. If `aggregate_rows` crosses 70, split the fold
  into `fold_group(kind, values) -> Result<TermId, Diagnostic>`.
- Commit message ends with
  `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## 8. Finish protocol
1. Commit, push, PR to `main` titled
   `feat(v8): sum, min, max aggregate heads`. Body: fixture names, test
   count before and after, clippy count, `git status --short v8/oracle`
   output (empty). End with
   `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
2. `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "aggregates: PR #<n>, tests <before>-><after>, clippy 0"`.
3. Never merge. Never spawn subagents. Never touch `v7/` or `v6/`.
