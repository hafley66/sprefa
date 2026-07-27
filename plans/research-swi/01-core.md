# SWI-Prolog core (language + engine): 9.x -> 10.x, mapped to v6/prolog

Scope: language/engine only (tabling, SSU, delimited continuations, engines,
dicts/strings/rationals, arithmetic, error handling, determinism, indexing,
GC/stacks). Libraries and tooling/embedding are out of scope, owned by sibling
agents.

Local install verified: `swipl --version` -> `SWI-Prolog version 10.0.2 for
arm64-darwin`. `current_prolog_flag(version, V)` -> `100002`.

Context read: `v6/prolog/conformance/engine.pl` (430 lines), `level_eval.pl`
(214 lines), `body.pl`, `go.pl`. The engine is a meta-interpreter: `prog(Decls,
Rules)` is DATA, `solve/2` walks rule bodies, and both the level-rule fixpoint
(`plain_fixpoint/5`, `agg_loop/6` in level_eval.pl) and the tick/drain loop
(`run_ticks/7` in engine.pl) are hand-rolled `findall` + `sort` + compare-equal
loops, not real Prolog clauses that the engine itself could table.

## Feature table

| Feature | Since | Verified locally | Relevance to sprefa v6 |
|---|---|---|---|
| Monotonic tabling (`:- table Q as monotonic`, `:- dynamic D as monotonic`) | ~8.1.x, matured through 9.x | Yes. `mono_tab3.pl`: asserting `link(a,b)` then `link(b,c)` into a monotonic `connected/2` grows the table incrementally (`[a-b]` then `[a-c,a-b,b-c]`) with no full recompute call between asserts. | Would replace the hand-rolled semi-naive loop entirely: `plain_fixpoint/5` (level_eval.pl:144-154) and `agg_loop/6` (level_eval.pl:157-164) both re-run `findall` + `sort` + `Merged == Known0` to detect a fixpoint. Monotonic tabling does this fixpoint bookkeeping in the engine. Requires compiling DSL rules to real dynamic/tabled clauses instead of interpreting `Rules` as data, so this is a redesign, not a drop-in edit. |
| Incremental tabling (`:- table P as incremental`, `:- dynamic D, [incremental(true)]`) | pre-9.2, stable | Yes. `incr_tab2.pl`: after `assertz(d(1))`, `assertz(d(5))`, then `retract(d(1))`, `p/1` tracks `[1]` -> `[1,5]` -> `[5]` automatically via the Incremental Dependency Graph. | Directly models what `engine.pl` hand-rolls for departures: `body_departed_ref/2`, `listened_departure_refs/2`, and the `occurrence_trigger(dep(Row), ...)` clause (engine.pl:162-166) exist only to let a rule react to a `-Row` retraction. Incremental tabling's IDG invalidation is the same idea done by the engine. |
| Answer subsumption / mode-directed tabling (`:- table foo(_, lattice(max/3))`) | pre-9.2 (XSB-derived), stable | No (not exercised; cited from manual section `tabling-mode-directed`). | `agg_compute/3` (level_eval.pl:204-213) hand-folds a collected bag for `count`/`sum`/`min`/`max`/`json_array`/`json_object` after `agg_rule_rows/4` gathers every derivation with `findall`. A lattice-mode tabled predicate keeps a running aggregate per answer instead of collect-then-fold. |
| SSU rules (`Head, Guard => Body`) | 8.3.19 | Yes, three receipts. Guarded dispatch: `classify_ssu(Number,Category), Number < 0 => Category = negative.` chain returns `negative`/`zero`/`positive` correctly. Mixing enforcement: `assertz((mixed(1) => true))` then `assertz((mixed(2) :- true))` raises `permission_error(assert,procedure,mixed/1)`. No-match enforcement: dropping the catch-all clause and calling `only_neg(5,_)` raises `existence_error(matching_rule, only_neg(5,_))` instead of silently failing. | `rel_kind/4` (engine.pl:90-93), `classify_head_arg/2` (level_eval.pl:28-33), `apply_edge_writes/6` (engine.pl:236-254) are all cut-guarded clause chains ending in a catch-all. SSU would make a missing case an `existence_error` at the point of the bad call instead of a silent `fail` that a caller's `->` swallows. |
| `det/1` + `$/0,1` determinism declarations | experimental (naming/semantics may still change per manual) | Yes. `:- det(bad_nondet/2).` over a `member/2`-based clause raises `determinism_error(bad_nondet/2,det,nondet,property)` on the first non-deterministic call. | Nearly every helper in engine.pl/level_eval.pl is written to be deterministic by construction (`key_of/3`, `entry_row/2`, `next_seq/3`, `dedupe_keep_order/2`) but nothing asserts that. `det/1` turns "this predicate secretly went nondet" from a silent correctness bug (extra choicepoint feeding a later `findall`) into an immediate exception at the call site. |
| Delimited continuations (`shift/1`, `reset/3`) | pre-7.4 (VM support improved 7.6.0) | Yes. `reset((X=1, shift(here), Y is X+1, ...), Ball, Cont)` returns `Ball=here`, then `call(Cont)` resumes and prints `X=1 Y=2`. | The tick/drain loop (`run_ticks/7`, engine.pl:367-379) is a hand-written recursive generator with an explicit `drain_cap` guard. A tick could be written as a goal that `shift`s a delta out per iteration instead of the current CPS-by-hand `state(Tick, Store, PrevLevel, PrevAll)` threading, but this is a rewrite for style, not a semantics fix; the current recursion is already correct. |
| Engines (`engine_create/3`, `engine_next/2`, `engine_yield/1`) | pre-9.2 (predates 10.x; single-threaded-build support added 10.0.0 per changelog) | Yes. `engine_create(X, (between(1,3,X), engine_yield(X), fail), Eng)` then three `engine_next(Eng,V)` calls yield `1`, `2`, `3`, then fail on the fourth. Single-threaded builds gaining engine support is a 10.0.0 changelog line, not independently re-verified (this machine's swipl has `threads=true`). | Same shape as delimited continuations for the tick loop: a generator abstraction for `run_ticks/7`'s per-tick delta stream. Lower payoff than tabling changes; the loop already works. |
| JIT clause indexing, deep indexing, `predicate_property(indexed(-Indexes))` | deep indexing since 7.7.4; any-argument primary index since 9.3.18; `indexed(-Indexes)` dict-shaped report new in 10.0.0 | Yes. `predicate_property(foo(_,_), indexed(Indexes))` on a 53-clause dynamic `foo/2` returns `[hash{arguments:[1], buckets:64, collisions:8, ..., speedup:45.8}]`. | Passive win, no code change: the fixture corpus and rule tables in engine.pl/level_eval.pl are plain Prolog terms scanned with `findall`/`member`, not indexed predicates, so this class of improvement does not currently apply to the meta-interpreter's hot path. It would start to matter only if DSL rules get compiled to real indexed clauses (see monotonic tabling row). |
| Fibonacci hashing + collision-free small index tables, `setjmp`/`longjmp` removal from the VM main loop | 10.0.0 | No (changelog-only; both are internal VM changes with no Prolog-level surface to call). Changelog states 6-35% gains depending on compiler and ~12% from the `PL_next_solutions()` change. | Passive win. Free speedup on the existing `findall`-heavy interpreter loop from the upgrade alone; no code change needed. |
| Rational numbers, `prefer_rationals` flag, `1r3` syntax | flag and `1rN` syntax stable pre-9.2 | Yes. `current_prolog_flag(prefer_rationals, false)` (still the default); `X is rationalize(1/3)` -> `1r3`; `1r3 + 1r6` -> `1r2`. | Low relevance. No fixed-point/money arithmetic visible in the conformance fixtures; `agg_compute(sum, ...)`/`count` use plain integers. Worth knowing about if a future fixture needs exact fractional aggregation, not a current gap. |
| Dicts (`Tag{k:v, ...}`) | since SWI 7 | Yes. `point{x:1, y:2}` then `get_dict(x, D, X)` -> `1`. | Low-to-medium relevance. `body.pl`'s json canon form is `obj(SortedPairs)` (a plain compound, per engine.pl:49-53's doc comment), built and read by hand. A dict could replace `obj/1` for JSON object values, but `json_object` aggregation already needs an explicit dup-key check (`agg_compute(json_object, ...)`, level_eval.pl:209-213) that dicts do not remove (dicts forbid duplicate keys at construction time in a way that changes the error path, not just spelling). |
| Strings (`double_quotes` flag defaults to `string`) | default since SWI 7 | Yes. `current_prolog_flag(double_quotes, string)`; `X = "abc"`, `string(X)` succeeds. | Low relevance. `rel_ref/2` and friends work over atoms (`Row =.. [Name \| Args]`); switching literal text fields to strings would touch `rel_ref`/`==`-based dedup (`msort`, `sort/2`, `==`) throughout engine.pl and buys little since these are DSL keys, not user text needing string operations. |
| `occurs_check` default | unchanged | Yes. `current_prolog_flag(occurs_check, false)`. | None. No occurs-check-sensitive unification pattern found in engine.pl (all unification is over finite ground-ish DSL terms); flagging only because the task asked. |
| `unwind(Term)` structured exceptions (for `halt`, `thread_exit`) | 10.0.0 | Yes. `catch(halt, unwind(Term), format(...))` catches `unwind(halt(0))`. | None found. No `halt`/`thread_exit` interception pattern in the conformance engine. |

## Top 5 by payoff for this repo

1. **Monotonic tabling.** Targets the exact thing the repo's own header comment
   flags as hand-rolled: the semi-naive fixpoint in `level_eval.pl`
   (`plain_fixpoint/5`, `agg_loop/6`). Highest payoff, highest cost: it only
   pays off if DSL rules compile down to real dynamic/tabled Prolog clauses
   instead of staying `Rules` data walked by `solve/2`.
2. **Incremental tabling.** Same redesign, narrower slice: it directly
   subsumes `body_departed_ref/2` + `occurrence_trigger(dep(Row), ...)`, the
   machinery engine.pl carries solely to let a rule react to a retraction.
3. **Answer subsumption (mode-directed tabling).** Smaller, more local:
   replaces `agg_rule_rows/4` + `agg_compute/3`'s collect-then-fold with a
   per-answer running aggregate, without touching the rest of the
   interpreter's shape.
4. **SSU rules.** Cheapest real win available today, no architecture change.
   `rel_kind/4`, `classify_head_arg/2`, and similar cut-chains become
   `existence_error`-on-miss instead of silently-fails-through-`->`.
5. **`det/1` declarations.** Also cheap and additive: a coverage net over the
   ~dozen helper predicates the code already assumes are deterministic, so a
   regression that adds a stray choicepoint fails loudly at the call site
   instead of corrupting a downstream `findall`.

Honorable mention, lower confidence of net benefit: delimited continuations
and engines could reshape `run_ticks/7` into a generator, but the existing
recursive loop is already correct and readable, so this is a style trade, not
a bug fix or a capability gain.
