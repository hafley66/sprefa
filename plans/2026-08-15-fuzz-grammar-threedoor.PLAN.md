# Grammar fuzzer with three-door differential judging: PLAN

Issue: `issues/fuzz-grammar-threedoor/item.md` (size:large, epic bug-mining).
Every number in this document was measured by the command printed beside it, on
this tree at `e23893b2`.

## TOC

| § | section |
|---|---|
| 0 | [Correction to the issue's premise](#0-correction-to-the-issues-premise) |
| 1 | [Build-vs-buy: candidate table](#1-build-vs-buy-candidate-table) |
| 2 | [What exists today (receipts)](#2-what-exists-today-receipts) |
| 3 | [Generator design](#3-generator-design) |
| 4 | [Judging loop](#4-judging-loop) |
| 5 | [Shrinking](#5-shrinking) |
| 6 | [Budget](#6-budget) |
| 7 | [Phasing: five arcs](#7-phasing-five-arcs) |
| 8 | [Open questions for the user](#8-open-questions-for-the-user) |

---

## 0. Correction to the issue's premise

The issue and the lane brief both say "shrink divergences with the dd arm
(6_isolated_compiler_dd.pl)". That file is not a shrinker. `dd` there is
differential dataflow: `compile/6_isolated_compiler_dd.pl:56` is
`compile_program/5`, the same five-argument emitter seam `emit_ts` uses, and it
writes a dd plan JSON (rels, arrangements, operators, wires, tick_order) that
`v6/dd-runner` replays as a fourth door under two arms
(`v6/dd-runner/grade.sh:11`, `graded.dd-diet-rust-sqlite.tsv`,
`graded.dd-diet-rust-rust.tsv`).

No test-case reducer exists anywhere in the tree:

```bash
grep -rniE 'ddmin|delta.?debug|shrink' --include='*.pl' --include='*.rs' \
  --include='*.ts' --include='*.sh' v6/ | grep -v /out/
```

returns only `0_option_expand.pl`'s `shrink_parent_ref/5` (an arity rewrite),
Rust `Vec::shrink_to_fit` calls, and prose. Shrinking is therefore in scope as
build work, priced in §5 and owned by arc F4, not assumed present.

Second correction, smaller: the issue says the generator reads
`v6/prolog/compile/registry.pl`, "83 constructs". The 83 is right (§2) and the
registry is necessary, but it is not sufficient. Six declaration functors that
appear in the corpus have no registry row at all: `kind/2`, `keep/2`,
`keyed/2`, `rel_path_decl/2`, `rel_template/3`, `interface_decl/2`. The seed
grammar is the registry plus that mined declaration vocabulary (§3.1).

---

## 1. Build-vs-buy: candidate table

Two separable jobs. **A: the trial driver** (loop, seeds, counterexample
reporting, shrink recursion). **B: the program generator** (produce a
declaration/use-coherent, well-typed `.dl6` program). **C: the reducer**.

| # | candidate | what it is | A driver | B generator | C reducer | verdict |
|---|---|---|---|---|---|---|
| 1 | SWI pack `quickcheck@0.3.0` (Hendricks, maint. Gallinal) | property-based testing; exports `arbitrary/2`, `arbitrary_type/1`, `shrink/3`, `quickcheck/1`; `arbitrary/2` and `shrink/3` are **multifile**; 100 trials default; shrink recursion depth-limited to 32 | fits | no type language for our programs | drives our own `shrink/3` | **BUY for A, and as the hook for C.** Define `arbitrary(dl_program, P)` and `shrink(dl_program, P, Smaller)`; the trial loop, counterexample reporting and shrink recursion come free. Risk: 6 downloads at 0.3.0, thin maintenance; the module is small enough to vendor if it breaks, and the fallback (a 40-line `forall` over seeds) is named in arc F1 |
| 2 | SWI pack `probat@0.1` (Azzolini) | property-based testing; `property_test/0,1` with options (trials, depth, ranges); generators for int/float/atom/string/list; automatic shrinking | fits | type list is scalar and list shapes only | generic term shrink, not tree-aware | **NO for B and C, second choice for A.** Its option surface is richer than quickcheck's, its extension surface is not documented as multifile, so a user-defined program type is a fork rather than a hook. Keep as the fallback if candidate 1 breaks |
| 3 | SWI pack `plrand@0.9.13` | skippable/splittable pseudorandom streams and distributions | n/a | seed discipline | n/a | **DEFER.** Splittable streams matter only when N lanes fuzz in parallel and must not correlate. Core `set_random(seed(N))` plus one integer seed per program covers arcs F1 to F4; revisit at parallel fan-out |
| 4 | SWI core `library(random)` | `random_between/3`, `random_member/2`, `random_permutation/2`, `set_random/1` | n/a | the primitive layer | n/a | **BUY.** Every weighted choice in §3 is `random_between/3` over a cumulative weight table. No dependency |
| 5 | SWI core `library(clpfd)` | finite-domain constraints; `labeling/2` accepts `random_value(Seed)` (verified locally) | n/a | candidate for well-typedness by construction | n/a | **NO, with a named reopen condition.** Column-type coherence is equality propagation over a small graph, which plain unification already does; clpfd buys search only when a constraint is numeric and over-determined. Reopen if arc F3 finds the aggregate plus recursion legality conditions need real search rather than rejection sampling |
| 6 | Grammarinator (Python, ANTLR v4) | grammar-based test generator; also recombines and mutates a corpus | no | needs an ANTLR grammar we do not have; documented as context-free, so semantic validity is poor by construction | no | **NO.** The only door is a Prolog DCG (`compile/parse_dl_dcg.pl`); an ANTLR grammar of `.dl6` would be a second grammar to keep in sync, and the CLAUDE.md single-door law exists exactly to prevent that. Its coverage would also be text-only, while the corpus and three of four doors are term-shaped |
| 7 | Nautilus / Gramatron / fuzzingbook `GrammarFuzzer` | grammar fuzzers with mutation, coverage feedback | no | same context-free limit; Nautilus is byte/AFL-shaped | no | **NO, same reason as 6.** Coverage feedback assumes an instrumented binary; our compiler is Prolog and our coverage metric is per-construct by name (§4.3), which we already have |
| 8 | SQLsmith (C++) | random SQL generator that reads the live catalog to emit type-aware and column-aware queries that mostly pass semantic checks; found ~30 Postgres bugs, adopted by CockroachDB, TiDB, RisingWave | no | not reusable as code (Postgres AST) | no | **ADOPT THE ARCHITECTURE, not the code.** Its one idea is the whole design of §3: generate from the catalog rather than from a context-free grammar. Our catalog is `registry.pl` plus the declarations the program itself just emitted |
| 9 | Csmith (C++) | random well-typed C program generator | no | not reusable | no | **ADOPT THE ARCHITECTURE.** Same lesson as 8, plus its safety-by-construction discipline: never generate a program whose meaning is undefined, so any divergence is a real bug |
| 10 | SQLancer (Java) | automated DBMS logic-bug finder, 400+ bugs, oracles NoREC / TLP / DQP | no | no | no | **NO as a dependency, YES as a lesson.** Its oracles exist because a DBMS has no reference implementation. We have one (the Prolog oracle), so the differential judge is already stronger than NoREC or TLP. The lesson worth taking is that oracles are complementary: DQP found 81% of bugs NoREC and TLP missed. Our analogue is the four judging axes in §4.2, not one |
| 11 | picire / picireny (Python) | Delta Debugging and Hierarchical Delta Debugging; picireny parses input with ANTLR v4 and reduces the tree; reduced outputs 25-40% smaller than reference HDD; usable as a library | no | no | fits the algorithm, not the plumbing | **NO for the plumbing, YES for the algorithm.** Picireny needs the ANTLR grammar candidate 6 already ruled out; picire is line-based over text, which for a fixture term means reducing to files that do not read back. The counterexample we hold is already a parsed tree (`prog(Decls, Rules)`), so HDD applies directly to it in Prolog, at roughly 120 lines (§5), with no cross-language process per candidate |
| 12 | Hypothesis (Python) | property-based testing with an integrated shrinker and swarm modes | no | wrong language for the generator | would need the whole toolchain driven per candidate | **NO.** Every trial forks swipl, node and a Rust binary; adding a Python supervisor above that buys a shrinker we can write in Prolog and costs a fourth runtime in the loop |
| 13 | `proptest` / `arbitrary` / `bolero` (Rust) | property testing and structured fuzzing for Rust | no | Rust-side only | Rust-side only | **NO for the language fuzzer, KEEP IN VIEW for the engine.** These are the right tools if a later arc fuzzes `sprefa-engine-rs` internals (SQL seam, value coercions) rather than programs |
| 14 | Swarm testing (Groce et al., ISSTA 2012) | technique: run many configurations, each omitting a random subset of features, instead of one configuration containing everything | n/a | a weighting mode | n/a | **ADOPT.** Free to implement (one bitmask over the construct table per program), and the measured claim is large: 104 distinct compiler crashes in a week versus 73 without, on Csmith. §3.4 |

### Verdict line

**BUY** `quickcheck@0.3.0` for the trial driver and shrink recursion, and core
`library(random)` for weighted choice. **ADOPT** SQLsmith's and Csmith's
catalog-driven, well-typed-by-construction architecture, and Groce's swarm
configurations. **BUILD** two things and only two: `arbitrary(dl_program, P)`,
because no library knows this language's registry, type plane, stratification
rule or declaration/use coherence; and `shrink(dl_program, P, Smaller)`, a
Hierarchical Delta Debugging pass over `prog(Decls, Rules)`, because every
reducer on the market reduces text through a grammar we do not have, while our
counterexample is already a tree.

**What the libraries cannot do**, stated precisely: emit a program in which
(a) every rule body references only declared relations at their declared arity,
(b) every shared variable joins columns of equal declared type
(`lower.pl:347` `join_column_type_mismatch`), (c) every comparison operand pair
satisfies the type contract stored in `registry.pl`'s `expression/5` fifth
argument (`lower.pl:2319` `comparison_type_mismatch`), (d) the derived-relation
graph is a DAG or a legal recursive stratum (`strat.pl:118`
`recursive_stratum_groups/2`), and (e) the arrival schedule mentions only
source relations with rows of the declared column types. Conditions (a) to (e)
are not expressible in any general-purpose generator's type language.

---

## 2. What exists today (receipts)

### 2.1 The registry is 83 distinct constructs

```bash
cd v6/prolog && swipl -q -g 'use_module(compile/registry),
  findall(FA, registry:surface(FA,_,_,_,_), S),
  findall(FA, registry:expression(FA,_,_,_,_), E),
  append(S,E,All), sort(All,U), length(U,N), format("~w~n",[N])' -t halt
```

| measure | value |
|---|---|
| `surface/5` rows | 60 (live 49, reserved 7, refused 4) |
| `expression/5` rows | 31 |
| rows in both (the six ordered comparisons plus `=:=`, `=\=`) | 8 |
| distinct functor/arity | **83** |

Live surface rows by axis: aggregate 11, guard 11, world 4, json 5, decl 4,
sample 3, sugar 3, time 3, bind 2, join 1, read 1, sign 1.
Expression rows by axis: text_scalar 12, ordered_comparison 6, typed_scalar 5,
arithmetic 5, identity_comparison 2, json_scalar 1.

**`expression/5`'s fifth argument is the generator's type rule table**, already
written: `both_number`, `both_int`, `text_only`, `same_type`, `json_only`,
`typed([text,int],text)`. Nothing needs to be restated in generator code.

### 2.2 The corpus, and its coverage holes

| measure | value | command |
|---|---|---|
| fixture files | 61 | `ls v6/prolog/conformance/fixtures/*.pl \| wc -l` |
| fixture terms | 448 | `grep -rhoE '^fixture\(' ... \| wc -l` |
| compiled / unsupported | 341 / 107 (76.1%) | `out/manifest.json` bucket count |

Construct frequency across the 448 fixture programs (walked term-wise, live
registry rows only, top and tail):

| count | construct | | count | construct |
|---|---|---|---|---|
| 999 | `col_type/3` | | 4 | `sum/1`, `min/1`, `match/2` |
| 94 | `type_decl/2` | | 3 | `true/0`, `max/1`, `group_concat/2,3`, `bind_decl/2`, `**/0` |
| 85 | `:=/2` | | 2 | `seq/1`, `next/1`, `json_group_array/2`, `group_concat/1`, `avg/1` |
| 71 | `{}/1` | | 1 | `{}/0`, `ts_query/1`, `=\=/2`, `=:=/2`, `</2` |
| 62 | `not/1` | | 0 | **`is/2`** |

**24 of 49 live constructs appear five times or fewer.** One live construct,
`is/2`, appears zero times in any fixture program. `combine/N` appears four
times, in one file (`fixtures/body_words.pl`).

That tail is the case for the generator, and the repo already recorded what
lives in it: `ARCH.pl:841` (`prolog_main_review`, landed 2026-07-30) finding F1
says the oracle **has no clause** for `combine/variadic` or `next/1`, both
registry-live, so the term door derives zero rows while the compiler emits a
real cross join, and F3 says a level-head expression contradicting its declared
column type is checked by nobody for `int`, `text` and `ref`. Both are exactly
what a weighted generator hits inside its first thousand programs.

### 2.3 The judges

| door | entry point | artifact | replay cost shape |
|---|---|---|---|
| oracle | `conformance/ticklog.pl` `print_ticklog/3`; batch driver `v6/dd-runner/sweep_oracle.pl` `sweep_oracle/2` | `<name>.oracle.jsonl` | in-process, one swipl for a batch |
| TS | `compile/7_emit_ts.pl` via `sweep.pl`; replay `v6/tsv2/scripts/sweep.ts` | `out/<name>.ts` + `<name>.schedule.json` | one node process for a batch, dynamic import per program |
| Rust | `v6/prolog/emit_rust.pl` via `sprefa-engine-rs/grade.pl:9` `generate/2`; replay `target/debug/emit_rust_harness` | `<name>.rs` carrying `PROGRAM_JSON` | **one prebuilt binary, one process per program; no rustc per program** (`src/bin/emit_rust_harness.rs` reads the raw-string JSON out of the module and interprets it) |
| dd (fourth) | `compile/6_isolated_compiler_dd.pl:56` + `v6/dd-runner` | plan JSON | one prebuilt Rust binary, two arms |

The Rust door being an interpreter over `PROGRAM_JSON` is the single most
important throughput fact in this plan: a generated program reaches all three
judges without invoking a compiler toolchain.

### 2.4 Reusable seams, and the one blocker

| seam | path:line | reusable as-is |
|---|---|---|
| read any fixture file | `sweep.pl:56` `read_all_fixtures/2` | yes, takes a path |
| write schedule JSON | `sweep.pl:174` `schedule_json/4` | yes |
| Rust corpus generation | `sprefa-engine-rs/grade.pl:9` `generate/2` | no, calls `sweep:fixture_files/1` |
| oracle batch | `dd-runner/sweep_oracle.pl` | no, iterates loaded `fixture/5` facts |
| per-construct coverage by name | `compile/scripts/golden_coverage.pl` | pattern reusable; today it grades one file, `v6/dl/fixtures/golden-flex.dl6` |
| print/parse round-trip grade (G1) | `compile/scripts/roundtrip.sh` | pattern reusable, hardcoded corpus |
| capped process-group execution | `v6/tools/run-capped.sh` (`capped SECONDS LABEL CMD...`) | yes |

**Blocker, one line of code**: `sweep.pl:39` `fixtures_dir/1` is hardcoded to
`conformance/fixtures`. Every batch driver above inherits it. Arc F1 turns that
into a parameter with the current value as default.

### 2.5 Prior art inside the repo

`v6/tsv2/scripts/golden-flex.sh` grades ONE hand-written program
(`v6/dl/fixtures/golden-flex.dl6`) six ways, including a coverage gate that
fails **by name** when a live registry row is unexercised, and cardinality
schedules at 0 / 1 / 100 rows per input relation
(`v6/tsv2/scripts/golden-schedules.ts`). The fuzzer is the same idea with the
authorship inverted: one program written by hand covering everything, versus a
thousand programs generated covering the crossings a hand-written program
cannot enumerate. The coverage gate and the cardinality ladder are reused
directly.

---

## 3. Generator design

### 3.1 Seed grammar: three sources, not one

| source | contributes | path |
|---|---|---|
| `registry.pl` `surface/5` where status is live | 49 body and head constructs, with axis and lower role | `compile/registry.pl` |
| `registry.pl` `expression/5` | 31 expressions **plus their type contracts** | same |
| mined declaration vocabulary | `kind/2`, `keep/2`, `keyed/2`, `col_type/3`, `type_decl/2`, `enum_decl/2`, `rel_path_decl/2`, `rel_template/3`, `interface_decl/2` | corpus census, §2.2 |

Declaration functor weights measured over the corpus: `col_type/3` 976,
`kind/2` 213, `keep/2` 210, `keyed/2` 145, `type_decl/2` 92, `enum_decl/2` 18,
`rel_path_decl/2` 14, `rel_template/3` 7, `interface_decl/2` 5.
Rule head forms: `<-` 496 level, `<+` 169 edge, `match/2` 4.
Column types: text 405, int 356, json 48, option 30, span (a reference type) 20,
list 20, float 18, bool 4.
Relation kinds: **log 200, set 3**. That ratio is itself a coverage hole; the
generator ships 50/50 and the plan records the corpus number so the first
divergence in a `set` relation is not mistaken for a generator defect.

### 3.2 Program shape, in generation order

```
step 0  seed=S, set_random(seed(S)), swarm mask M drawn (§3.4)
step 1  N_rel  in 2..6      relation names rel_a .. rel_f, arity 1..4
step 2  per relation: column types drawn from the §3.1 distribution,
        kind(Ref, log|set), optional keep(Ref, count(K)), optional keyed(Ref, Cols)
step 3  partition relations: SOURCE (fed by the schedule) vs DERIVED (rule heads)
step 4  fix a topological order over DERIVED; each derived head may read only
        earlier relations (DAG by construction), unless recursion mode is on
step 5  per derived relation, 1..3 rules; per rule pick a body shape from the
        weighted live-construct table masked by M
step 6  bind variables: a shared variable is only placed where both column
        types are equal; guards and expressions typed from expression/5 arg 5
step 7  schedule: 9 ticks at cardinality c in {0, 1, 100}, including one
        departure tick (minus deltas) and one settle tick
step 8  emit fixture(gen_S_c, prog(Decls, Rules), Initial, Schedule, [])
```

Expectations are the empty list on purpose. The generated program has no
expected answer; the oracle is the expectation, and the judge is agreement
between doors (§4).

### 3.3 Well-typedness constraints (the reason this is bespoke)

| # | constraint | enforced by | throw site if violated |
|---|---|---|---|
| C1 | body atoms reference declared relations at declared arity | construction: draw from the declaration table just built | `analyze.pl` unknown reference |
| C2 | a shared variable joins equal declared column types | construction: variables are drawn per type bucket | `lower.pl:347` `join_column_type_mismatch` |
| C3 | comparison operands satisfy the `expression/5` contract | construction: read `both_number` / `text_only` / `same_type` / `typed([...],T)` | `lower.pl:2319` `comparison_type_mismatch` |
| C4 | derived graph is a DAG, or a declared recursive stratum | construction: topological order, recursion is an opt-in mode | `strat.pl:118` |
| C5 | `not/1` only over an atom whose variables are bound elsewhere in the body | construction: negated atom placed last, variables reused | analyze safety check |
| C6 | contextual gates (`latest/1`, `finalize/1`, `now/1`) only in their legal placement | construction: read the `AnalyzeRole` and axis from the registry row | named unsupported constructs in `analyze.pl` |
| C7 | aggregates group on the head columns not under the aggregate | construction | `aggregate_head(...)` |
| C8 | schedule rows carry values of the declared column type | construction | arrival shape mismatch |

The user's **no coercions** decision is what makes C2 and C3 mandatory rather
than optional: an untyped column never silently joins a numeric aggregate, so a
generator that ignores types produces programs that die at the door and grade
nothing.

**Target compile rate: 90% or better**, measured as
`count(bucket == compiled) / count(programs)` over a 1000-program batch, written
to the batch verdict TSV by the same classifier `sweep.pl:168` already uses.
Two rules attach to it: (i) the human corpus sits at 76.1% because it
deliberately contains unsupported-construct fixtures, so 90% is a real
tightening, not a restatement; (ii) every `unsupported` bucket in a generated
batch must carry a reason name that already appears in `out/manifest.json`. A
reason name that has never been seen is either a generator defect or a real
hole, and it is triaged by a human, never suppressed.

### 3.4 Weighting and swarm

Two knobs, both per program, both recorded in the seed line:

1. **Base weights** = the corpus frequency table (§2.2), inverted and clamped,
   so a construct that appears twice in 448 human fixtures is drawn more often
   than `col_type/3`, not less. The corpus tells us what is already covered;
   the generator's job is the complement.
2. **Swarm mask** (Groce): before generating, draw a random subset of the
   construct table and forbid the rest for that program. The measured payoff on
   Csmith was 104 crash classes in a week versus 73. Cost: one bitmask.

Reproducibility: one integer seed per program, `set_random(seed(S))` at step 0,
seed and mask written to `seeds.tsv` beside the verdict. Any program in any
batch is regenerated by its seed alone, which is also what makes a shrink
result attachable to the run that found it.

---

## 4. Judging loop

### 4.1 Flow

```
generate batch -> N fixture terms in $SCRATCH/corpus/gen_NNN.pl
  |
  +-- swipl leg 1: program_plan + lower + emit_ts + emit_rust + dd plan
  |     writes <name>.ts, <name>.rs, <name>.plan.json, <name>.schedule.json,
  |     and compile.tsv (name, bucket, reason)
  +-- swipl leg 2: print_ticklog over the same corpus -> <name>.oracle.jsonl
  +-- node leg:    sweep.ts replays every .ts, diffs against .oracle.jsonl
  +-- rust leg:    emit_rust_harness <name>.rs <name>.schedule.json | diff
  +-- roundtrip:   parse_dl(print_dl(Term)) =@= Term
  -> verdicts.tsv (name, seed, verdict, door, reason)
```

### 4.2 Divergence, defined exactly

| class | comparison | artifact compared | verdict word |
|---|---|---|---|
| D1 | oracle vs TS door | tick-log JSONL, byte equality, then final state | `diff-ts` |
| D2 | oracle vs Rust door | tick-log JSONL, byte equality (`grade.sh:84` already spells this `diff -q`) | `diff-rust` |
| D3 | oracle vs dd door | tick-log JSONL, byte equality, per arm | `diff-dd` |
| D4 | print/parse round-trip | `=@=` variant check on the fixture term | `roundtrip` |
| D5 | door bucket disagreement | term door compiles, text door does not (or the reverse) | `bucket-split` |
| D6 | any door throws or panics | stderr reason, normalized by `grade.sh:67` `reason_text` | `crash` |

D1 and D2 are the headline. D3 is free once the corpus directory is a
parameter. D4 catches the class `ARCH.pl:841` F1 named (a construct the printer
erases, so no corpus fixture can see it). D5 catches the class where the two
front doors disagree about whether a program is in the language at all.

A byte diff on the tick log, not on the final state alone. The retention
fixtures at `fixtures/engine_core.pl:44` exist because grading the end state
alone hid a silently dropped prune for three arcs.

### 4.3 Coverage accounting

Per batch, walk every generated term and count live registry rows hit, in the
shape `golden_coverage.pl` already uses (term walk, plus a text check for the
two `splice_bare` rows `next/1` and `combine/variadic`, which leave no trace in
the term). A batch that leaves a live construct at zero fails the coverage leg
by name. This is the metric that replaces coverage-guided feedback: we cannot
instrument a Prolog compiler cheaply, and we do not need to, because the
language surface is a 49-row table.

---

## 5. Shrinking

No reducer exists (§0), so this is build work, and the algorithm is
Hierarchical Delta Debugging applied to `prog(Decls, Rules)` directly.

| layer | reduction step | invariant to re-establish |
|---|---|---|
| L1 | drop a rule | every remaining head still declared |
| L2 | drop a declared relation and every rule mentioning it | C1 |
| L3 | drop a body item from a rule | C5 (negation safety), C2 (join types) |
| L4 | drop a column from a relation, rewriting every reference | C2, C8 |
| L5 | shrink a schedule: drop a tick, then drop an arrival | C8 |
| L6 | shrink a literal: text to `''`, int toward 0 | C3 |

Each candidate is re-judged by the same predicate that found the divergence, so
the reducer's oracle is the run's own verdict function, not a second
implementation. Interface, one predicate, registered as the multifile hook
candidate 1 exposes:

```prolog
% shrink(+Type, +Program, -Smaller) is nondet.
%   Type = dl_program. Emits candidates in decreasing size order, L1 first.
%   The driver keeps a candidate only when it still reproduces the verdict.
shrink(dl_program, fixture(Name, prog(Decls, Rules), Initial, Schedule, []), Smaller).
```

Budget: a reduction is bounded by candidate count, not wall clock, at 200
judged candidates per level with a global cap of 2000 per counterexample.
Target: a landed counterexample is at most 3 declarations and 1 rule, which is
the size the existing `compile/test/dd/*.dd.pl` cases already have.

Landing: a minimized counterexample becomes a fixture in
`conformance/fixtures/fuzz_<class>.pl` with a FAIL-FIRST header (the repo
convention at `fixtures/engine_core.pl:31`), plus a `docs/failure-modes.md`
entry when it names a real incident class. The generated batch itself is never
committed; only minimized survivors are.

---

## 6. Budget

Measured on this tree, this machine:

| leg | measurement | command |
|---|---|---|
| `program_plan` + `lower_program`, 448 corpus programs | **0.803s total, 1.79 ms/program**, 341 succeed (exactly the manifest's compiled count) | swipl one-shot over `sweep:read_all_fixtures/2` |
| oracle `print_ticklog/3`, 448 programs | **3.777s total, 8.43 ms/program**, 364 produce a log, 84 throw | swipl one-shot with output to a string |
| whole four-stage `sweep.sh` | 8.3s at 196 fixtures, per its own header note | `v6/tsv2/scripts/sweep.sh:46` |
| Rust harness spawn per program | **unmeasured**; arc F1's first receipt | `emit_rust_harness` on one corpus entry, 20 repeats |

Derived budget:

| bound | value | why |
|---|---|---|
| per program, prolog side | ~11 ms (1.79 plan+lower, 8.43 oracle) | measured above |
| per program hard cap | **2s**, enforced by `capped 2 "<name>" ...` from `v6/tools/run-capped.sh` | the 10-second law is per operation; one program is one operation and 2s is ~180x its measured wall |
| per batch | 1000 programs, cap 600s | a multi-program battery is not a single operation (CLAUDE.md), and 1000 x 11 ms is 11s of prolog work plus process spawns |
| per counterexample shrink | 2000 judged candidates, cap 600s | §5 |
| CPU | the batch driver takes an explicit `-j` and defaults to 1 | nothing seizes the machine |

Where results land:

| artifact | path | committed |
|---|---|---|
| generator, driver, shrinker | `v6/fuzz-grammar/` (peer of `v6/dd-runner/`) | yes |
| generated corpus, emitted modules, tick logs | `$TMPDIR/sprefa-fuzz.XXXX/` | no |
| `verdicts.tsv`, `seeds.tsv` per run | `$TMPDIR` run dir, summary line to stdout | no |
| minimized counterexamples | `conformance/fixtures/fuzz_<class>.pl` | yes, one PR per class |
| a divergence that names a design question | a fork in the arc report, per the lang-design law | user decides |

---

## 7. Phasing: five arcs

Each arc owns disjoint files. Sizes are for `pro4` or `flash4` lanes as noted.

### F1. Corpus source seam and the batch judge (small, flash4)

| item | content |
|---|---|
| goal | judge an arbitrary directory of fixture files through oracle, TS, Rust, dd, round-trip; no generator yet |
| files owned | `v6/prolog/sweep.pl` (`fixtures_dir/1` becomes a parameter with today's default), `v6/sprefa-engine-rs/grade.pl` (accept a corpus dir), `v6/dd-runner/sweep_oracle.pl`, new `v6/fuzz-grammar/judge.sh`, `v6/fuzz-grammar/judge.pl` |
| forbidden | `registry.pl`, `lower.pl`, `analyze.pl`, all emitters, all fixtures |
| gate | run the new judge over `conformance/fixtures` and reproduce today's numbers exactly: 448 terms, 341 compiled, 364 oracle logs, and `grade.sh`'s byte-clean count unchanged. Also publish the measured Rust-harness spawn cost |
| receipt | `verdicts.tsv` diffed against `graded.tsv` with zero rows moved |

### F2. Generator core (large, pro4; the creative arc)

| item | content |
|---|---|
| goal | `arbitrary(dl_program, P)` producing declaration/use-coherent, well-typed programs over the plain axes: declarations, level rules, edge rules, guards, arithmetic, `:=`, `not/1` |
| files owned | `v6/fuzz-grammar/gen.pl`, `v6/fuzz-grammar/weights.pl`, `v6/fuzz-grammar/test/gen.plunit.pl` |
| forbidden | everything F1 owns, all compiler modules |
| gate | 1000 programs at seed 0: compile rate >= 90%; every `unsupported` reason already present in `out/manifest.json`; regeneration from `seeds.tsv` is byte-identical; whole batch under 600s |
| note | this arc buys `quickcheck@0.3.0` and declares the multifile hook, or lands the 40-line fallback loop with the reason written down |

### F3. Full construct coverage and swarm (medium, pro4)

| item | content |
|---|---|
| goal | extend the generator to aggregates, the json plane, `match/2` arms, enum and type declarations, `coalesce/2`, `pre/1,2`, `latest/1`, `finalize/1`, `next/1`, `combine/N`, `is/2`; add the swarm mask |
| files owned | `v6/fuzz-grammar/gen_*.pl`, `v6/fuzz-grammar/coverage.pl` |
| forbidden | compiler modules, fixtures |
| gate | a 1000-program batch hits **every live registry row** by name (the `golden_coverage.pl` accounting shape), and the batch reports at least the two known-divergent constructs `combine/N` and `next/1` as `diff-*` verdicts, which is this arc's fail-first proof that the loop can see a real bug (`ARCH.pl:841` F1) |

### F4. Shrinker (medium, pro4)

| item | content |
|---|---|
| goal | `shrink(dl_program, P, Smaller)`, the six-layer HDD of §5, plus the counterexample lander |
| files owned | `v6/fuzz-grammar/shrink.pl`, `v6/fuzz-grammar/land.pl`, `v6/fuzz-grammar/test/shrink.plunit.pl` |
| forbidden | generator files, compiler modules |
| gate | three seeded synthetic divergences (a door mutated in a scratch copy) each reduce to <= 3 declarations and 1 rule within 2000 judged candidates; the reduced term still reproduces; the landed fixture reads back through `read_all_fixtures/2` |

### F5. Rails and continuous run (small, flash4)

| item | content |
|---|---|
| goal | budgets, caps, one `just fuzz` entry, a CI leg that runs a fixed small batch, and the seed ledger |
| files owned | `v6/fuzz-grammar/run.sh`, `v6/justfile` (one recipe), `.github/` workflow leg, `docs/failure-modes.md` entry if F2 to F4 bit |
| forbidden | generator, shrinker, compiler modules |
| gate | `capped` wraps every leg; a wedged program is killed at 2s and named; the CI leg is under 120s and appears in `.github/CI-KNOWN-RED.md` only if it is expected red |

Dispatch order: F1, then F2, then F3 and F4 concurrently (disjoint files), then
F5. F1 is the only arc that edits existing files, so it lands alone.

---

## 8. Open questions for the user

1. **Does a generated divergence get a fixture automatically?** The plan says a
   minimized counterexample lands as `conformance/fixtures/fuzz_<class>.pl` in
   its own PR, one class per PR. The alternative is a quarantine file outside
   the graded corpus. Corpus growth changes every gate's numbers, so this is a
   decision, not an implementation detail.
2. **`combine/N` and `next/1` have no oracle clause** (`ARCH.pl:841` F1). Arc F3's
   gate asserts the fuzzer finds them. Whether the fix is oracle clauses or a
   registry status change is language design, so it comes back as a fork rather
   than being settled in a lane.
3. **Relation kind ratio.** The corpus is 200 `log` to 3 `set`. The generator
   ships 50/50, which will surface `set`-relation behaviour that the corpus has
   never graded. Expect a first batch that is noisy in that axis.
