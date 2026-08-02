# L3 demand: which SWI powers, and does Haskell have them

Scope: all `*.pl` in the repo (git-tracked). Prolog is the compiler layer for
the dl language (v6) plus teaching algorithms (books/v6). Answer per power,
each positive answer backed by a compiled probe in `probes/`.

## Table 1: the feature inventory

Each row is a distinct SWI power the repo actually uses, with an occurrence
count (git-tracked `*.pl`, `xargs grep`) and one or more `file:line` receipts.
Line counts: full `v6/prolog/` tree is 37,074 lines; the coordinator's 14,090
is only the 22 top-level modules (excludes `labs/`, `compile/`, `src/`,
`conformance/`, `tools/`). `books/v6/` is 1,898. Both re-derived with `git
ls-files 'v6/prolog/*.pl' | xargs wc -l | tail -1` (37,074) and
`git ls-files 'books/v6/*.pl' | xargs wc -l | tail -1` (1,898).

| power | occurrence count (files) | receipt |
|---|---|---|
| term_expansion/2 | 1 real (rel_island.pl) | books/v6/rel_island.pl:26 |
| DCG `-->`/2 | 202 occurrences, 13 files | books/v6/algos/marble.pl:13-19, json.pl, sexp_cst.pl |
| phrase/2,3 | 9 files | books/v6/algos/marble.pl:9-10, json.pl |
| `:- table` | 6 top-level decls | books/v6/algos/magic_sets.pl:13, causality.pl:16, lustre.pl:99, dl_in_prolog.pl:235, v6/prolog/src/kernel.pl:9, v6/sprefa-store/bench/swi_reach.pl:23 |
| findall/3 | 60 files | v6/prolog/1_expansion.pl:48, books/v6/rel_island.pl:53 |
| bagof/3 | 1 file | v6/prolog/analyze.pl |
| setof/3 | 4 files | v6/prolog/3_clock_check.pl |
| forall/2 | 63 occurrences | books/v6/algos/magic_sets.pl:31, rel_island.pl:67 |
| aggregate_all/3 | 3 imports, 8 uses | v6/prolog/src/emit_ts.pl:153, books/v6/dl_in_prolog.pl:246 |
| assoc (library) | 2 imports | v6/prolog/0_graph.pl:18 |
| ordsets (library) | 4 imports | various |
| pairs (library) | 7 imports | v6/prolog/analyze.pl, 3_clock_check.pl, strat.pl |
| cut `!` | ~100 sites in core | v6/prolog/0_graph.pl:121, 0_body_walk.pl, 0_seq_expand.pl |
| NAF `\+` | core walkers | v6/prolog/0_graph.pl:196, causality.pl:22 |
| catch/throw | 44/31 sites | books/v6/algos/magic_sets.pl:32-33, rel_island.pl:69 |
| `=@=` (variant) | 7 files | v6/prolog/compile/parse_dl.pl, books/v6/algos/unify_hm.pl |
| occurs_check | 2 files (unify_hm, hm) | books/v6/algos/unify_hm.pl |
| copy_term/2 | 9 files | books/v6/algos/lower_sql.pl:12, dl_to_ts.pl:184 |
| functor/arg/`=..`/univ | 251 core sites | v6/prolog/0_body_walk.pl:94, 0_enum_expand.pl:128 |
| atom/number type tests | atom/ 209 in lower.pl alone | v6/prolog/lower.pl |
| format/2, format/3 | 577 core sites | books/v6/rel_island.pl:70, magic_sets.pl:32 |
| assert/retract | 35 assertz, ~40 assert | books/v6/rel_island.pl:30, dl_in_prolog.pl:189-205 |
| module system | 37 `:- module` in core | v6/prolog/1_expansion.pl:9, 0_graph.pl:4 |
| plunit | 3 imports, begin_tests files | v6/prolog/compile/test/plunit_tests.pl |
| op/3 (operator decl) | 10+ files | v6/prolog/0_coalesce_expand.pl:59, books/v6/rel_island.pl:20 |
| string/atom handling | string_codes, atom_codes, sub_atom | v6/prolog/emit_ts.pl, lower.pl |
| between/3 | 2 imports, 11 uses | books/v6/dl_in_prolog.pl, v6/prolog/src/emit_ts.pl |
| process/readutil/filesex | 9/7/4 imports | 6_profile.pl, sweep.pl, text_door_receipt.pl |
| prolog_xref | 3 imports | v6/prolog/tools/prolog_lint.pl:6 |

Drop: `goal_expansion` never appears (0 hits). `=@=` is present but only in
books and a parser comparison, not in the compiler hot path.

## Table 2: the Haskell answer per row

| power | repo uses | Haskell answer | probe | verdict |
|---|---|---|---|---|
| term_expansion/2 | rel_island.pl:26 | Template Haskell quasiquoter | Probe.Sugar | encodable |
| DCG `-->`, phrase | marble.json | parsec parser + printer | Probe.DCG | encodable |
| `:- table` | magic_sets.causality | explicit fixpoint over Data.Set | Probe.Tabling | encodable |
| findall/3 | 60 files | LogicT observeAll | Probe.Quant | direct |
| bagof/3 | analyze.pl | LogicT mplus | Probe.Quant | direct |
| setof/3 | 3_clock_check | sort+nub over LogicT | Probe.Quant | direct |
| forall/2 | magic_sets:31 | filter+null | Probe.Quant | direct |
| aggregate_all/3 | emit_ts.pl:153 | Data.Map fold | Probe.Collections | encodable |
| assoc | 0_graph.pl:18 | Data.Map | Probe.Collections | direct |
| ordsets | 4 imports | Data.Set | Probe.Collections | direct |
| pairs | 7 imports | [(k,v)] + sortBy | Probe.Collections | direct |
| cut `!` | 0_graph.pl:121 | no direct equivalent | unproven | hostile |
| NAF `\+` | 0_graph.pl:196 | Data.Set membership / LogicT guard | Probe.Tabling | encodable |
| catch/throw | magic_sets:32 | Control.Exception | Probe.Exceptions | direct |
| `=@=` variant | parse_dl | custom alpha-equal | unproven | encodable |
| occurs_check | unify_hm | TH/generic, no std | unproven | hostile |
| copy_term/2 | lower_sql:12 | no std; renaming via fresh gensym | unproven | encodable |
| functor/arg/`=..`/univ | 0_body_walk:94 | TH ConE/LitE, or generic | Probe.Sugar | encodable |
| atom/number tests | lower.pl | static types replace runtime tests | whole suite | direct |
| format/2,3 | rel_island:70 | Text.Printf | Probe.Printf | direct |
| assert/retract | rel_island:30 | State/DB; no dynamic predicate store | unproven | encodable |
| module system | 37 modules | Haskell modules | whole suite | direct |
| plunit | plunit_tests.pl | hspec/tasty or tiny runner | Probe.Plunit | direct |
| op/3 | 0_coalesce:59 | TH quasiquoter sugar | Probe.Sugar | encodable |
| string/atom | lower.pl | Data.Text | DCG/Process | direct |
| between/3 | src/emit_ts | [lo..hi] | Probe.Quant | direct |
| process/readutil/filesex | sweep.pl | System.Process, System.IO, System.Directory | Probe.Process | direct |
| prolog_xref | prolog_lint.pl:6 | no equivalent (static analyzer) | none | absent |

Verdict counts: direct 15, encodable 10, hostile 2, absent 1. Five of those
rows (`=@=`, copy_term/2, cut, occurs_check, assert/retract) carry NO probe,
so under the lane's probe rule they are unproven regardless of their stated
verdict; see section 4.

## Section 3: the three that decide it

**1. term_expansion.** `books/v6/rel_island.pl:26` defines `term_expansion/
((Head <- Body), Clauses)` and at load time rewrites every `<-` clause into a
datalog fact plus a tabled twin. SWI calls it at consult time, globally, for
every clause, no opt-in. Probe.Sugar is the Haskell answer: a Template Haskell
quasiquoter (\`dl\`) expanded at compile
time into a typed `DClause`. The cost is real and is precisely the staging
split: the quasiquoter must live in its own already-compiled module (elected
by GHC's level rule, `DClause` is a 2-line data type) and the surface `<-`
sugar must be re-lexed into something GHC accepts; term_expansion needs
neither. SWI built it in 45 lines of one file; the TH version needs two
modules.

**2. Tabling.** `books/v6/algos/magic_sets.pl:16-23` and `causality.pl:18-20`
twin a left-recursive `reach` under `:- table`. Verified with swipl: both pass
(magic_sets: same_answers, demand_set, cold_stays_cold; causality: counter_ok,
broken_rejected). Probe.Tabling gives the Haskell answer on the same rule set:
a naive fixpoint over `Data.Set` that iterates the one-hop closure to a fixed
point, terminating because the set is finite and the operator monotone. All
three checks pass: cycle-closed {1,2,3}, cold component {7,8,9}, self-reach.
What SWI gives for free (one `:- table` directive, no loop) is here an
explicit `fixpoint` loop plus a termination argument per rule set. The loop is
the cost; no Hackage package reproduces SWI's declarative annotation, and
`datalog`/`souffle-haskell`/`datafix` are either wrapper DSLs or a different
fixpoint framework with their own admissibility laws.

**3. Unbound-mode predicates.** `marble.pl:7-11` is one DCG that both parses
and prints (`marble(String, Events)` handles `var(String)` to generate, ground
String to recognize; verified both PASS under swipl). Probe.DCG gives the
Haskell answer as two functions over one shared `[Ev]` type: `parseMarble`
(parsec, carry the tick) and `printMarble` (dashes to the target tick). Both
single-direction checks pass and the round-trip is coherent. The cost is the
named two functions: unification-driven bidirectional invocation becomes a
parser and a printer joined by the AST type. `parsec` gives the parse
direction for free; the print direction is bespoke. Two directions, two real
functions, one shared type.

## Section 4: the honest verdict

A straight Prolog-to-Haskell port is direct or encodable for 25 of 28 powers
(direct for 15, encodable for 10), but the three compile-time/metaprogramming
powers (term_expansion,
:- table, dynamic-mutating assert/retract) move from "free, built into the
consult" to "explicit machinery with a staging or loop or state cost" and 5
powers end up with no compiled probe behind them because they need a
hand-written equivalent that the probe set did not reach (alpha-variant,
copy_term renaming, cut semantics, occurs-check unification, dynamic clause
store). The repo's core compiler work (reachability, sets, the DCG front,
format, module structure, pairs) maps cleanly onto containers/logict/fgl/
parsec, so the bulk moves with direct equivalents. The replacement cost is
concentrated entirely in the metaprogramming seam - the reader hook, the
tabling directive, and the dynamic store - which is exactly the seam the repo
already isolates into rel_island.pl, magic_sets.pl and causality.pl rather
than sprinkling across the compiler. A port buys dead type checking and
pattern exhaustiveness over the AST but pays runtime unification (the thing
that makes unbound modes and cut and copy_term cheap in Prolog) with explicit
functions and data structures. What cannot be carried over at all is the
global term_expansion consult hook and the dynamic predicate store; both must
be redesigned into compile-time construction with types supplying what the
store gave at runtime.

## Build-vs-buy notes (repo law)

Every "encodable, write our own" answer checked Hackage first:
- Tabling: checked `datalog 0.1`, `souffle-haskell 0.0.1`, `datafix 0.0.1`.
  `datafix` is a dataflow fixpoint framework with lattice/transfer-function
  laws for compiler dataflow, not general SLG tabling; `datalog` and
  `souffle-haskell` wrap the souffle C++ engine and change the evaluation
  model (external binary) rather than tabling a Prolog rule set in-process.
  None reproduces SWI's `:- table` declarative annotation on a left-recursive
  rule, so the fixpoint is hand-rolled (Probe.Tabling, ~10 lines).
- Unbound modes / DCG: `parsec 3.1` covers parse; the print direction and the
  bidirectional seam are bespoke because no library offers unification-driven
  bidirectional functions.
- term_expansion: Template Haskell (in GHC) is the one tool; no alternative
  DSL-at-read-time package was viable.
- Graphs: `fgl 5.3` covers toposort and closure; cycle detection needed a
  back-edge DFS because fgl `topsort` drops cyclic nodes and `trc` is
  reflexive (see Probe.Graph notes).

Probe build: `ghc 9.14.1`, `cabal 3.16.1.0`, deps logict/containers/fgl/
mtl/transformers/text/template-haskell/parsec; 24 PASS, 0 FAIL, 0 warnings.
