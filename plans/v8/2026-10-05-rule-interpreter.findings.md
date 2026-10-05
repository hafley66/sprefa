# Findings: rules as rows, the interpreter mode

Brief: `plans/v8/2026-10-05-rule-interpreter.brief.md`. sprefa `feat/rule-interpreter` (rebased on
`1a615c617`); sqlite_ivm clone `sqlite_ivm/`, branch `feat/rule-interpreter` at main `14675e8`
(no sqlite_ivm change). No clock anywhere below; every number is a count.

## 1. Design

### Prior art and library candidates

| candidate | what it offers here | used |
|---|---|---|
| Soufflé interpreter (`src/interpreter/`, RAM tree walked per tuple) | rules stay a tree interpreted in host code, compiled per program | no: it re-plans per program; the host loop is Rust code, not an IVM program |
| Datalog meta-interpreters (`solve(Goal) :- clause(Goal, Body), solve(Body)`) | rules as facts, one fixed evaluator | the shape taken: rule and goal rows, one fixed program |
| egglog | rules as e-graph rewrites, schedule in host code | no: host-side schedule, no IVM |
| Materialize / DD "rules as data" (`WITH MUTUALLY RECURSIVE` over a rules table) | one dataflow whose joins read rule rows | the shape taken, expressed in `ivm_ir` so both engines run it |
| a library that interprets Datalog inside DD or SQLite | none found that runs over `ivm_ir`; the interpreter is an `ivm_ir::Program` built by `rule_interp::shape` | - |

### Type signatures

```rust
// src/_6_eval/_9a1_rule_interp.rs
pub enum RulesMode { Interpret, Compile }
pub fn init_rules_mode() -> Result<RulesMode, String>   // dl8 main reads DL8_RULES once
pub fn rules_mode() -> RulesMode

pub fn rule_reason(u: &Universe, program: &Program, rule: &Rule) -> Option<String>
pub struct Split { compiled: Program, interpreted: Program, feeds: HashSet<(u32, usize)>, reasons: BTreeMap<String, usize> }
pub fn split(u: &mut Universe, program: &Program) -> Result<Split, String>
pub struct Plan { rules: Vec<RulePlan>, levels: u32, keys: BTreeSet<(u32, usize)> }
pub fn plan(u: &mut Universe, interpreted: &Program) -> Result<Plan, String>
pub fn bucket(levels: u32) -> u32                      // next power of two
pub fn shape(bucket: u32) -> Shape                     // the fixed ivm_ir::Program
pub struct Interner { conses, items, rule_ids }        // dictionary ids Rust minted
impl Interner { fn rows(&mut self, engine, shape, plan, seeds: &[Row]) -> BTreeSet<(RelId, Vec<Cell>)> }
pub fn decode_facts(u, engine, shape) -> Vec<Row>

// src/_6_eval/_9b_ir_eval.rs
enum Placement { Compiled, Interpreted(Plan), Split(Program, Plan, HashSet<(u32, usize)>) }
fn placement(u, program) -> Placement                  // logs `dl8::rules` "rules mode"
trait InterpRun { bucket, settle_rows(u, plan, seeds), change(plan, signed rows), closure(u), counters() }
fn run_interpreted(..)  // seeds, frontiers, live on the interpreter alone
fn run_split(..)        // compiled engine, then interpreter over each compiled closure
```

```text
IrSession::evaluate(program)                 // comptime rounds
  match placement(program)
    Compiled                 -> compiled session (fingerprint reuse, as before)
    Interpreted(plan)        -> interp.settle_rows(plan, program.seeds)
    Split(c, plan, feeds)    -> rows = compiled session evaluate(c)
                                interp.settle_rows(plan, rows filtered by feeds)
                                merge(rows, interp rows), sorted as snapshot_closure
run_fresh(program, frontiers, live)          // dl8 eval / run / query
  same placement; Split runs the compiled session with a leading empty frontier (the start
  closure) and wraps live.on_settle so each compiled closure settles the interpreter
```

The rows (one `ivm_ir` cell each; a value is its `TermId`; `NIL = -1`):

```text
icons(id, head, tail)                     constructor: lists
iitem(id, kind, a, b)                     constructor: pattern items
iseed(rel, arity, L)                      source: a fact
irule(R, s, goals, rel, arity, HP)        source: rule R, its stratum s, head pattern HP
igoal(R, i, s, rel, arity, pol, FP, EK)   source: goal i of R
fact pattern FP items: (0,p,_) key position p | (1,p,c) position p holds c | (2,p,q) positions p,q equal
env key EK / head HP items: (0,g,p) position p of the fact goal g matched | (1,_,c) constant c
```

Per stratum copy `s` (copies 0 .. bucket-1, `ifact` = facts of the last copy):

```text
key_lo(R,i,L,K)   fact walk of every fact below s against every goal of s        LetRec + Let
LetRec:
  env(R,i,E)        E = list of matched facts, last first; env(R,0,NIL) per rule of s
  efact(E,g,L)      inherited along E' = icons(L, E)
  ew → keyE(R,i,pol,E,K)   env key walk: values through efact and cells
  fw → keyF over facts this copy derives
  positive goal: keyE ⋈ (key_lo ∪ keyF) on (R,i,K) → E' = icons(L,E), env(R,i+1,E')
  negative goal: keyE ▷ key_lo on (R,i,K)        → env(R,i+1,E)
  hw → new(rel,n,L)  head walk at env(R,goals,E)
  cw → cells_new(L,p,x)
facts_s = threshold(facts_{s-1} ∪ new), cells_s likewise
```

### Instance timelines

| instance | created | read | dropped |
|---|---|---|---|
| interpreter program (`shape(bucket)`) | first `Interpreted`/`Split` evaluation of an `IrSession`, or each `run_fresh` | every settle | `IrSession` drop, an engine error, or a program whose level count exceeds the bucket (reinstall at the larger bucket, logged `rules interpreter install`) |
| compiled part | as before, keyed by the structural fingerprint of the compiled rules only | every settle | fingerprint change |
| rule rows (`irule`, `igoal`) | `Interner::rows` when a rule first appears; `R` is stable per rule text for the session (`rule_ids`) | the copy whose `s` matches | a later evaluation without the rule: retracted (-1) |
| item and list ids | `intern_terms` on `iitem`/`icons`, cached in `Interner` | the walks decode them | never (dictionary rows persist) |
| seed rows (`iseed`) | the compiled closure rows of `feeds`, or program seeds, or frontier/live changes | `key_lo`, `cells` | the next evaluation without them: -1 |
| env, efact, keys, cells | derived inside the copy's LetRec | the same LetRec | retracted by the engines when their inputs retract |

### Storage and the sequence per settle

1. Rust: `split` (stratify once, per-rule reason, relation components, compiled closure); compiled
   rules settle on the compiled session first.
2. Rust: `Interner::rows`: intern new items (one `intern_terms` call), new lists depth by depth
   (one call per list depth), then the wanted row set; diff against `installed`; one settle.
3. Engine: seed cells walk; copy 0 .. bucket-1 in order; `ifact` output.
4. Rust: `ifact` snapshot plus one `icons` dictionary snapshot; lists decoded to rows; merge with
   the compiled closure.

sqlite: every relation is the engine's own tables (`*_n<k>_*`), dictionary rows in `ivm_term` and
the `icons`/`iitem` constructor tables; every LetRec is monotone (DRed path): negation reads only
`key_lo`, which is closed below the copy. dd: one `iterate` scope per LetRec, constructor rows in
the shared interner.

### Uniqueness

- A fact is `(rel, arity, L)`; `L` is unique per argument tuple (dictionary intern), so set
  semantics come from `threshold` at each copy and the LetRec thresholds.
- An env is unique per rule and matched facts; two envs of one rule with one binding cannot both
  exist (a fact differs from another matching one at a new-variable position).
- `R` is unique per rule text within one `Interner`; equal rules share `R` and rows.
- Item ids and list ids are unique per content (dictionary); values are `TermId`s, so equality
  of cells is term equality.

### What the interpreter runs and what falls back

A rule is interpreted when `rule_reason` is `None` and its relation component is interpreted:

| reason (rule compiled) | why |
|---|---|
| `kernel goal <op>` (`Int(Lt)`, `IntAdd`, `Tuple`, `Intern`, `str` ops, `TermLt`, fold steps) | values are `TermId`s; the interpreter has no term construction or comparison |
| `intern_snapshot goal`, `intern head/goal`, `effect head/goal` | the compiled program fills these relations from intern requests and served effects too |
| `served relation goal/head` | requests and answers live in the compiled session's answer loop |
| `head aggregate or fold` | no Reduce goal kind yet |
| `next head (waves)` | waves are one LetRec with a limit in the compiled program |
| `head variable no positive goal binds` | no binding to build the head from |
| `read by a compiled rule` | interpretable, but a compiled rule reads its component: the split is compiled first, interpreted second, never back |
| whole program compiled: `stratification diagnostic` | the compiled program reports it |

Every evaluation logs one `dl8::rules` `rules mode` event: `mode`, rule counts, `levels`, and the
reasons map. `RUST_LOG=dl8::rules=info` shows them; the default `warn` filter does not, so stderr
of the suites is unchanged.

## 2. Switch

`DL8_RULES=interpret|compile`, unset is `compile` (user decision 2026-10-05; interpret stays behind the switch). `dl8` main reads it once
(`rule_interp::init_rules_mode`, beside `DL8_ENGINE`); an unknown value exits 2. A library caller
that never called it reads it on first use. Both comptime rounds (`IrSession::evaluate`) and the
runtime paths (`run_fresh`: `dl8 eval`, `dl8 run` ticks, `--frontiers`, live, `dl8 query`) place
rules through `placement`; macrotime (`prepare`/`apply`) stays compiled.

## 3. Parity

Every run below is on `ff2bf361f` plus the `_9l` test fix in this commit.

| suite | ir:sqlite compile | ir:sqlite interpret | ir:dd compile | ir:dd interpret |
|---|---|---|---|---|
| sprefa `cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book --test-threads=1` | 177 passed, 0 failed | 177 passed, 0 failed | 177 passed, 0 failed | 177 passed, 0 failed |
| registry `dl8 build std/tsi_registry_emit.dl7`, `cmp` with hafley_scm `registry.rs` | identical | identical | identical | identical |
| census `dl8 compile`, 132 programs: exit code and stdout | reference | equal | equal | equal |
| census runtime programs `dl8 eval`, 110: output less `counters` | reference | equal | equal | equal |
| macrotime oracle `dl8 expand`, 24 cases | reference | equal | equal | equal |
| rule edit probe, closure after each step | reference | equal | equal | equal |

sqlite_ivm `cargo test -j 4 --workspace --no-fail-fast -- --test-threads=1`: 264 passed, 0 failed
(no sqlite_ivm change; extension dylib built in the clone).

Two census programs print compile output deeper than serde_json's recursion limit; their runtime
programs are not re-read by the `_9l` test (they are in the census row, which compares bytes).

Mode-specific pins. These assert work, not output, and now name their mode:
`_4_sqlite::ir_sqlite_comptime_objects_for_the_prelude` and `_9h_build::tsi_registry_sqlite_work_counts`
run with `DL8_RULES=compile`; `_9_ladder::rung_9` expects 2 comptime installs in interpret mode
(compiled part plus interpreter, each installed once) and 1 in compile mode; `_11_watchers` edit
`rows_written` delta is 3 in interpret mode (the interpreter's settle adds one row) and 2 in compile
mode. `rung_2_join` (join delta rows constant under unread padding rows) holds in both modes after
the interpreter walks cells only of relations a goal reads.

Bugs found by the parity runs and fixed: an interpretable rule that calls a demand-driven compiled
relation (`needs_bound_head`) dropped that relation's demand rows and one `intern` request
(`tests/fixtures/2_partial.dl7`, `fixtures/ladder/0_negation_tuple_intern`, `demand_count`): such a
caller is now compiled (`calls a demand-driven relation`). `cargo fmt` rewrote the generated
`src/_6_eval/_4a_str_ops.rs`; it is kept as `std/str_ops_emit.dl7` builds it.

## 4. Work counts, interpret vs compile

Produced by `cargo test --test it _9l_rule_counts::rule_interpreter_counts -- --ignored --nocapture`
(`RUST_LOG=ivm_sqlite::work=info,ivm_dd=debug,dl8::ir=info,dl8::rules=info`). sqlite counts are the
`ivm_sqlite::work` events summed over install, settle and between; dd counts are the `dd install` and
`dd settle` events. `rules *` rows exist only in interpret mode (the `rules mode` event).

### registry (`dl8 build std/tsi_registry_emit.dl7`, cmp identical in all four)
| count | sqlite compile | sqlite interpret | dd compile | dd interpret |
|---|---|---|---|---|
| dd install arrangements | 0 | 0 | 1474 | 1611 |
| dd install operators | 0 | 0 | 21603 | 24236 |
| dd settle arranged_records | 0 | 0 | 115730 | 195814 |
| dd settle changes_in | 0 | 0 | 37225 | 39458 |
| dd settle changes_out | 0 | 0 | 18997 | 21164 |
| dd settle steps | 0 | 0 | 949 | 1118 |
| ir install nodes | 5766 | 6357 | 5766 | 6357 |
| ir installs | 3 | 5 | 3 | 5 |
| rules compiled | 0 | 320 | 0 | 320 |
| rules evaluations interpret | 0 | 3 | 0 | 3 |
| rules interpreted | 0 | 38 | 0 | 38 |
| rules rules | 0 | 358 | 0 | 358 |
| sqlite total creates | 4963 | 5660 | 0 | 0 |
| sqlite total executions | 83894 | 91196 | 0 | 0 |
| sqlite total prepared | 11213 | 12726 | 0 | 0 |
| sqlite total rows_returned | 105687 | 131080 | 0 | 0 |
| sqlite total vm_steps | 42216224 | 102521862 | 0 | 0 |
| sqlite total written_d | 221502 | 499114 | 0 | 0 |
| sqlite total written_i | 108610 | 247700 | 0 | 0 |
| sqlite total written_scratch | 95433 | 167100 | 0 | 0 |
| sqlite total written_source | 23381 | 25614 | 0 | 0 |
| sqlite total written_term | 38964 | 49154 | 0 | 0 |
### census (`dl8 compile`, summed over programs exiting 0)
| count | sqlite compile | sqlite interpret | dd compile | dd interpret |
|---|---|---|---|---|
| dd install arrangements | 0 | 0 | 119579 | 135307 |
| dd install operators | 0 | 0 | 1720156 | 2022439 |
| dd settle arranged_records | 0 | 0 | 2149645 | 8007855 |
| dd settle changes_in | 0 | 0 | 1021541 | 1194810 |
| dd settle changes_out | 0 | 0 | 527540 | 690492 |
| dd settle steps | 0 | 0 | 12754 | 26597 |
| ir install nodes | 495060 | 563591 | 495060 | 563591 |
| ir installs | 339 | 565 | 339 | 565 |
| rules compiled | 0 | 21882 | 0 | 21882 |
| rules evaluations interpret | 0 | 261 | 0 | 261 |
| rules interpreted | 0 | 3297 | 0 | 3297 |
| rules rules | 0 | 25179 | 0 | 25179 |
| sqlite total creates | 388293 | 467776 | 0 | 0 |
| sqlite total executions | 2453612 | 3046336 | 0 | 0 |
| sqlite total prepared | 645500 | 811108 | 0 | 0 |
| sqlite total rows_returned | 3116822 | 4657357 | 0 | 0 |
| sqlite total vm_steps | 985526057 | 5431544810 | 0 | 0 |
| sqlite total written_d | 4689576 | 25227168 | 0 | 0 |
| sqlite total written_i | 2617660 | 12849486 | 0 | 0 |
| sqlite total written_scratch | 2097488 | 7342861 | 0 | 0 |
| sqlite total written_source | 647875 | 817506 | 0 | 0 |
| sqlite total written_term | 1067504 | 1795387 | 0 | 0 |
132 programs, 112 exit 0 in all four runs; output equal in all four.
### runtime programs (`dl8 eval`)
| count | sqlite compile | sqlite interpret | dd compile | dd interpret |
|---|---|---|---|---|
| dd install arrangements | 0 | 0 | 50691 | 63503 |
| dd install operators | 0 | 0 | 1322043 | 1640214 |
| dd settle arranged_records | 0 | 0 | 24599 | 82215 |
| dd settle changes_in | 0 | 0 | 60705 | 59681 |
| dd settle changes_out | 0 | 0 | 3725 | 4401 |
| dd settle steps | 0 | 0 | 1787 | 10803 |
| ir install nodes | 240559 | 291041 | 240559 | 291041 |
| ir installs | 110 | 220 | 110 | 220 |
| rules compiled | 0 | 11384 | 0 | 11384 |
| rules evaluations interpret | 0 | 110 | 0 | 110 |
| rules interpreted | 0 | 1636 | 0 | 1636 |
| rules rules | 0 | 13020 | 0 | 13020 |
| sqlite total creates | 150572 | 209332 | 0 | 0 |
| sqlite total executions | 355034 | 520331 | 0 | 0 |
| sqlite total prepared | 217620 | 338699 | 0 | 0 |
| sqlite total rows_returned | 139038 | 319610 | 0 | 0 |
| sqlite total vm_steps | 87289938 | 116299712 | 0 | 0 |
| sqlite total written_d | 135716 | 271904 | 0 | 0 |
| sqlite total written_i | 68783 | 147467 | 0 | 0 |
| sqlite total written_scratch | 82592 | 155766 | 0 | 0 |
| sqlite total written_source | 57668 | 56644 | 0 | 0 |
| sqlite total written_term | 119195 | 121765 | 0 | 0 |
110 runtime programs; `dl8 eval` output less its `counters` field equal in all four. 2 census outputs past serde_json's depth limit are not re-read.
macrotime oracle: 24 cases, `dl8 expand` output equal in all four.
### rule edit: first evaluation
| count | sqlite compile | sqlite interpret | dd compile | dd interpret |
|---|---|---|---|---|
| dd install arrangements | 0 | 0 | 368 | 476 |
| dd install operators | 0 | 0 | 7057 | 9011 |
| dd settle arranged_records | 0 | 0 | 133 | 2807 |
| dd settle changes_in | 0 | 0 | 548 | 549 |
| dd settle changes_out | 0 | 0 | 104 | 120 |
| dd settle steps | 0 | 0 | 32 | 301 |
| ir install nodes | 2159 | 2608 | 2159 | 2608 |
| ir installs | 1 | 2 | 1 | 2 |
| rules compiled | 0 | 101 | 0 | 101 |
| rules evaluations interpret | 0 | 1 | 0 | 1 |
| rules interpreted | 0 | 17 | 0 | 17 |
| rules rules | 0 | 118 | 0 | 118 |
| sqlite total creates | 1319 | 1838 | 0 | 0 |
| sqlite total executions | 3049 | 6391 | 0 | 0 |
| sqlite total prepared | 1843 | 3098 | 0 | 0 |
| sqlite total rows_returned | 774 | 1315 | 0 | 0 |
| sqlite total vm_steps | 767413 | 2950720 | 0 | 0 |
| sqlite total written_d | 1662 | 12028 | 0 | 0 |
| sqlite total written_i | 827 | 5313 | 0 | 0 |
| sqlite total written_scratch | 733 | 3934 | 0 | 0 |
| sqlite total written_source | 519 | 520 | 0 | 0 |
| sqlite total written_term | 1052 | 1284 | 0 | 0 |
### rule edit: the edit only
| count | sqlite compile | sqlite interpret | dd compile | dd interpret |
|---|---|---|---|---|
| dd install arrangements | 0 | 0 | 368 | 0 |
| dd install operators | 0 | 0 | 7057 | 0 |
| dd settle arranged_records | 0 | 0 | 133 | 2368 |
| dd settle changes_in | 0 | 0 | 548 | 16 |
| dd settle changes_out | 0 | 0 | 102 | 146 |
| dd settle steps | 0 | 0 | 32 | 260 |
| ir install nodes | 2159 | 0 | 2159 | 0 |
| ir installs | 1 | 0 | 1 | 0 |
| rules compiled | 0 | 101 | 0 | 101 |
| rules evaluations interpret | 0 | 1 | 0 | 1 |
| rules interpreted | 0 | 17 | 0 | 17 |
| rules rules | 0 | 118 | 0 | 118 |
| sqlite total creates | 1318 | 0 | 0 | 0 |
| sqlite total executions | 3150 | 3705 | 0 | 0 |
| sqlite total prepared | 1860 | 293 | 0 | 0 |
| sqlite total rows_returned | 1370 | 1447 | 0 | 0 |
| sqlite total vm_steps | 771158 | 3370029 | 0 | 0 |
| sqlite total written_d | 1654 | 15330 | 0 | 0 |
| sqlite total written_i | 823 | 8827 | 0 | 0 |
| sqlite total written_scratch | 733 | 8397 | 0 | 0 |
| sqlite total written_source | 519 | 16 | 0 | 0 |
| sqlite total written_term | 1052 | 0 | 0 | 0 |
edit closures equal in all four.

Rule placement in interpret mode over the census: 25,179 rules evaluated across 261 comptime
evaluations, 3,297 interpreted; runtime programs: 13,020 rules, 1,636 interpreted. In the registry
build 38 of 358. Reasons, registry comptime program (`rules mode` event): `read by a compiled rule`
48-55, `kernel goal Tuple` 16-18, `intern_snapshot goal` 19, `calls a demand-driven relation` 7,
`head aggregate or fold` 6, `kernel goal Int(Lt)` 5.

The rule edit (`plans/v8/probes/2026-10-05-rule-edit/`, `1_reach.dl7` to `2_reach_edited.dl7`,
the negated goal of `open_reach` reads `?A` in place of `?B`): in compile mode the edited program has
a new fingerprint and installs again (1 install, 1,318 CREATEs, 1,860 prepared on sqlite; 7,057 dd
operators). In interpret mode the compiled part keeps its fingerprint and the interpreter keeps its
bucket: 0 installs, 0 CREATEs, 293 prepared, 16 source rows changed (`changes_in` 16 on dd); the
interpreter's settle does more row work than the compiled program's whole fresh settle (sqlite
vm_steps 3,370,029 against 771,158; written_d 15,330 against 1,654; dd arranged_records 2,368
against 133).

## 5. What is not built

| item | site | fork |
|---|---|---|
| kernel goals as goal kinds (`int.*`, `str.*`, `any.lt`, `intern`, `tuple`) | `rule_interp::rule_reason` | needs the compiled term encoding (dictionary cells) in place of `TermId` cells |
| aggregates and folds as goal kinds | same | a Reduce over env rows keyed by the head's non-aggregate positions |
| more than one compiled/interpreted alternation | `rule_interp::split` | a chain of engines needs the answerer shared across compiled segments |
| interpreter row work: env inheritance (`efact`) copies every matched fact per goal; `key_lo` walks every lower fact per stratum copy | `rule_interp::shape` | per-goal live-variable envs; one `key_lo` shared across copies |
| `next` heads in the interpreter | `rule_reason` | waves as an outer LetRec over rule rows |
