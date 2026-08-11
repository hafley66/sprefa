# DD plan emitter: mutual recursion in the `dd_plan` stop

## TOC

| # | section | one-line answer |
|---|---|---|
| 1 | [The stop, traced](#1-the-stop-traced) | `reject_mutual_recursion/2` and `rules_reach_ref/3-4` compute one-directional positive-use reachability; nothing downstream of the emitter blocks mutual recursion, the check only runs first. |
| 2 | [Three-backend truth table](#2-three-backend-truth-table) | tsv2 and the dd plan both stop on a positive within-stratum cycle as `recursive_stratum`; the swipl oracle accepts and computes pure positive mutual recursion. |
| 3 | [What differential dataflow does natively](#3-what-differential-dataflow-does-natively) | The recorded docs show one iterative scope carrying one feedback collection; "several mutually-recursive collections in one scope" is not recorded. |
| 4 | [The cost of the stop, measured](#4-the-cost-of-the-stop-measured) | 0 of 370 manifest fixtures and 0 of 804 conformance rules trip it; the only real mutual recursion in the wider corpus is in `examples/`, outside both suites. |
| 5 | [Forks, unranked](#5-forks-unranked) | Four forks, one table each, no ranking. |
| 6 | [What must not be decided here](#6-what-must-not-be-decided-here) | The report stops at cited facts; the scheduling-semantics ruling is the user's. |

## 1. The stop, traced

### 1.1 One-sentence answer

This is unfinished work (an ordering/stratification gate that runs first),
not a real impossibility: the swipl oracle already computes pure positive
mutual recursion, the dd-runner kernel already evaluates all heads jointly,
and nothing downstream of the emitter prevents the rules.

### 1.2 What `reject_mutual_recursion/2` does

```prolog
reject_mutual_recursion(Rule, Rules) :-
    rule_head_ref(Rule, HeadRef),
    rule_body_uses(Rule, Uses),
    (   member(use(HeadRef, _, pos, _), Uses)
    ->  true                                        % direct self-read: allowed
    ;   member(use(BodyRef, _, pos, _), Uses),      % some positive read BodyRef
        BodyRef \== HeadRef,
        rules_reach_ref(BodyRef, HeadRef, Rules)    % BodyRef reaches back to head
    ->  throw(unsupported_construct(mutual_recursion(HeadRef)))
    ;   true
    ).
```

(`v6/prolog/compile/6_emit_dd_plan.pl:460-470`)

It fires per rule inside `rule_operators/5` (`v6/prolog/compile/6_emit_dd_plan.pl:454`),
which runs before `rule_operator_terms/5` builds the rule's operators
(`:454-455`). A direct self-read (`member(use(HeadRef,_,pos,_)`) passes; a
positive read of some other ref that reaches back to the head throws.

### 1.3 What `rules_reach_ref/3-4` computes

```prolog
rules_reach_ref(From, To, Rules) :- rules_reach_ref(From, To, Rules, []).
rules_reach_ref(From, To, Rules, Seen) :-
    member(Rule, Rules),
    rule_head_ref(Rule, From),
    rule_body_uses(Rule, Uses),
    member(use(Next, _, pos, _), Uses),
    (   Next == To
    ;   \+ memberchk(Next, Seen),
        rules_reach_ref(Next, To, Rules, [From | Seen])
    ).
```

(`v6/prolog/compile/6_emit_dd_plan.pl:472-483`)

It answers: is there a rule whose head is `From` that positively reads some
`Next`, where `Next` is `To` or transitively reaches `To` through positive
body uses, visiting each rule head at most once (`Seen`). In the caller this
is the back half of a cycle: `rules_reach_ref(BodyRef, HeadRef, Rules)` asks
"does a rel I read positively lead back, through a chain of positive reads,
to my own head." Combined with the first `member/2` clause it detects a
positive use cycle of length two or more; `Seen` only prevents a rule from
re-entering itself, it does not stop the reachability walk from being
complete over a finite rule set.

### 1.4 Does anything downstream actually prevent it, or does the check run first?

The stop runs first, before any evaluation. Three receipts:

1. **The emitter runs the check before building operators.** `reject_mutual_recursion/2`
   is called at `6_emit_dd_plan.pl:454`, before `rule_operator_terms/5` at `:455`.
   A mutually recursive program never gets operators.
2. **Upstream, `program_plan/2` stops even earlier.** `sql_rule_order/2`
   runs inside `program_plan/3` at `v6/prolog/compile.pl:229`. For `even/0 <-
   odd/0` plus `odd/0 <- even/0` it throws `recursive_stratum([even/0,odd/0])`
   from `strat.pl:topo_order_group/2` (`v6/prolog/strat.pl:96-99`). Verified
   empirically: `fixture_dd_plan_text` returned
   `unsupported_construct(recursive_stratum([even/0,odd/0]))`, not
   `mutual_recursion/1`.
3. **The `mutual_recursion/1` net is a second guard for hand-built plans.**
   `program_plan` always throws `recursive_stratum` first on a positive
   within-stratum cycle, so the emitter's own throw fires only when a caller
   passes a `plan/9` whose `RuleOrder` already contains the cycle. The
   existing test does exactly this: it builds a recursive `RuleOrder` by hand
   and asserts `throws(unsupported_construct(mutual_recursion(left/1)))`
   (`v6/prolog/compile/test/6_emit_dd_plan.test.pl:245-254`).

Downstream of the stop, nothing is a barrier:

- The dd-runner RAM kernel `settle/3` (`v6/dd-runner/src/kernel.rs:86-107`)
  maintains a joint monotone fixed point over *all* operator heads, re-deriving
  every head from base each round until `next == old`. It never assumes one
  recursive head, so it would evaluate mutually recursive rules if a plan
  reached it. Its comment states the coverage: "A bounded monotone fixed point
  covers recursive positive rules; retractions begin each tick" (`kernel.rs:88-89`).
- The dd-runner SQLite arm (`v6/dd-runner/src/main.rs:86-90`) is immature (one
  phase, no fixpoint), but that is a build gap, not a designed barrier. tsv2
  already has a level fixpoint loop for the self case that a mutual group would
  extend (`recompute_levels_fn_lines/3`, `v6/prolog/emit_ts.pl:2072-2085`).

The stop is therefore a scheduling/ordering stop applied at stratify time and
re-applied defensively at the emitter; it is not a semantic impossibility. The
next section shows the reference backend computes the semantics.

## 2. Three-backend truth table

| # | backend | location | mutual recursion compiles? | what it produces |
|---|---|---|---|---|
| 1 | tsv2 (ts + sqlite) | `v6/prolog/emit_ts.pl` over `strat.pl` | NO | throws `recursive_stratum(Heads)` at ordering, `strat.pl:96-99`; self-edge is the sole exemption |
| 2 | rust x sqlite (dd plan emitter) | `v6/prolog/compile/6_emit_dd_plan.pl` + `v6/dd-runner/` | NO | `program_plan` throws `recursive_stratum` first (`compile.pl:229`); the emitter's `mutual_recursion(HeadRef)` is a second net (`6_emit_dd_plan.pl:468`) |
| 3 | swipl oracle (level evaluation) | `v6/prolog/conformance/level_eval.pl` | YES | accepted and computed: grouped into one stratum, joint `plain_fixpoint` iterate |
| 4 | rust x rust (dd-runner RAM kernel) | `v6/dd-runner/src/kernel.rs` | WOULD evaluate it | joint monotone `settle` over all heads (`kernel.rs:86-107`), never reached because the emitter stops |

Details:

- **tsv2 and the dd plan share the upstream gate.** Both build their plan
  through `program_plan/3`, which calls `sql_rule_order/2` (`compile.pl:229`).
  That in turn runs `stratum_groups/2` then `topo_order_group/2`
  (`v6/prolog/strat.pl:81-84`). A pure positive cycle drops both rules into one
  stratum group (positive reads carry `Gap = 0`, `strat.pl:53-54`), and
  `topo_order_group` detects the cycle in `kahn_order/2` and throws
  `recursive_stratum(HeadRefs)` (`strat.pl:96-99`). Self-recursion is exempt
  because it contributes no cross-rule edge (`DependsOnRef \== HeadRef`,
  `strat.pl:92`) and the level fixpoint handles the self edge
  (`emit_ts.pl:2042-2044`, `:2119-2123`).
- **The swipl oracle computes pure positive mutual recursion.** `stratify_level_rules/2`
  in the oracle (`level_eval.pl:97-116`) mirrors the compiler's stratum rule
  (positive `Gap` is lower than strict, `level_eval.pl:118-124`), so `even/0`
  and `odd/0` collapse into one group with no inequality. The group is then
  evaluated by `plain_fixpoint/5` (`level_eval.pl:183-196`), which evaluates
  every rule in the group against the accumulator and iterates until
  `Merged == Known0` (no new row). Verified: with rules `even(X) <- odd(X)`
  and `odd(X) <- even(X)` over base `[odd(0), even(3)]`, `level_closure`
  returned `[even(0),even(3),odd(0),odd(3)]`. Negation or aggregation through a
  cycle throws `not_stratified` (`level_eval.pl:95`, `:178-179`), the
  same-stratum positive case does not.

So the answer to the fork question in the brief is explicit: tsv2 does *not*
accept it, so the dd emitter is not behind its own reference on this point.
The dd emitter and tsv2 stop identically. What accepts it is the oracle
backend, and the dd-runner RAM kernel's `settle` would accept it if reached.

## 3. What differential dataflow does natively

Read from the recorded docs, not the upstream crate.

What the recon recorded about the DD receive side:

- `iterate` "constructs an iterative Timely subgraph with
  `Product<outer_time, inner_round>` timestamps. Differences circulate through
  feedback until they dissipate; the `Variable::new_from` construction
  subtracts the source before connecting the feedback loop"
  (`plans/2026-08-10-dd-source-hunt.RECON.md:42`, and the transfer-fork note at
  `RECON.md:120-125`).
- The benchmark's recursive loop is one collection: `edges.semijoin(reach)`.
  `map(child).concat(roots).distinct()` (`RECON.md:42`). The recon's update
  path describes one feedback collection, not a set of several mutually
  recursive collections sharing one scope.

Does that scope naturally hold several mutually-recursive collections at once,
per the docs? **No prior work found.** The recorded text never states that one
iterative scope holds several mutually-recursive collections simultaneously;
it describes a single collection circulating through feedback. Where the docs
are silent the gap is not reasoned closed.

Which ranked fork would carry it: **fork 1, Timestamped signed-delta fixed
point.** It is the fork whose IR option is explicit about the iterative scope:

- "keep inner timestamps, feedback frontiers, and threshold history as runtime
  rules behind the current iterate operator, or add explicit iterative-scope,
  timestamp, feedback, and threshold-state terms" (`RECON.md:53`).
- The recon names the current emitter state plainly: "Its iterate term contains
  only the head relation and it rejects mutual recursion"
  (`RECON.md:120`, `plans/2026-08-11-dd-line-recon.md:88`).

So the recorded fork-1 extension is where a multi-collection iterative scope
(or the explicit `dd_plan` iterative-scope term) would live. The other three
forks are physical-representation mechanisms (batch compaction, half-join
scheduling, multiplicity consolidation) and carry no recurrence semantics of
their own (`RECON.md:55-77`).

## 4. The cost of the stop, measured

### 4.1 The compile manifest

`v6/prolog/compile/out/manifest.json` has 370 rows (`name` field count = 370;
bucket split 270 `compiled` / 100 `unsupported`). Grep results:

- `mutual_recursion`: **0** rows.
- `recursive_stratum`: **0** rows.

So the shipped compile suite contains no program blocked by either stop name.
Underlying causes among the 100 unsupported rows are type/shape, host, and
relation errors; none is a recursion stop.

### 4.2 The conformance fixtures and the dl6/dl corpus

A derived-ref 2-cycle scan over `v6/prolog/conformance/fixtures/*.pl` found
**0** direct 2-cycles across the 804 rules scanned. The same scan over all
`**.dl6` and `**.dl` files flagged 9 files, verified by reading the rule text:

| file | pair | classified | is real positive mutual recursion? |
|---|---|---|---|
| `examples/arch-expr.dl` | `ax_expr` / `ax_tail` | derived 2-cycle | YES (`ax_expr <- ax_tail`, `ax_tail <- ax_expr`) |
| `examples/npm-crawl.dl` | `dep_at` / `frontier` | derived 2-cycle | YES (`dep_at <- frontier`, `frontier <- dep_at`) |
| `examples/field_matrix.dl` | `eng_field_ref` / `field_name` | scanner noise | NO (one direction only; `field_name` reads `eng_field_ref`, not back) |
| `examples/gh-cache.dl` | `change_log` / `change_log_next` | `@next` accumulator | NO (clock/retention, not same-tick fixpoint cycle) |
| `examples/gh-cache-full.dl` | `reading` / `reading_next` | `@next` accumulator | NO |
| `v6/dl/fixtures/comment-prod.dl6` | `prose_line` / `shebang_line` | negation through cycle | NO (one arm is `not(...)`; that is `not_stratified`, not the positive check) |
| `v6/prolog/compile/dl_view/chain_into_keyed_head_replaces.dl6` | `cache` / `demand_row` | `<+` retention pair | holds a positive cross-read, but it is a compiler-test fixture, outside both suites |
| `v6/prolog/compile/dl_view/concat_program_queue.dl6` | `drained` / `queue_head`; `live_tab` / `open_tab` | negation/`pre` through cycle | NO (`not(...)`, `pre(...)` arms) |
| `v6/prolog/compile/dl_view/exhaust_policy.dl6` | `live_tab` / `open_tab` | negation through cycle | NO (`not(live_tab(...))` arm) |

### 4.3 The number

- Manifest fixtures blocked: **0** (370 rows, none name a recursion stop).
- Conformance-fixture rules that trip the check: **0** (804 rules scanned).
- Real mutual recursion in `examples/` (outside both suites): **2** programs
  (`examples/arch-expr.dl`, `examples/npm-crawl.dl`).

Zero is a finding. The shipped suite is not blocked by this stop today; the
stop is a latent capacity the corpus has not needed. The only real mutual
recursion in the wider corpus sits in `examples/`, which the compile manifest
does not cover.

## 5. Forks, unranked

Each fork is one table: what it would do, what it costs, which code changes
and where, what breaks. No ranking; the user ranks and owns the
scheduling-semantics decision.

### Fork A. Stratify mutual recursion into separate groups like tsv2 already could

| | |
|---|---|
| what it would do | reuse the stratum machinery: keep the `Gap=0` positive grouping, then break a positive cycle into per-group iteration instead of throwing `recursive_stratum` |
| what it costs | a cycle-breaking choice in `topo_order_group/2`; each group becomes its own level fixpoint pass, losing cross-group iteration |
| code | `v6/prolog/strat.pl:topo_order_group/2` `:86-99`, `kahn_order/2` `:106-114`; shared by tsv2 and the dd plan via `compile.pl:229` |
| what breaks | the `recursive_stratum` receipt (`ARCH.pl:739` documents ghcacher `= 2` with that receipt); every emitted program's `RuleOrder` ordering; tsv2 `recompute_levels` single-pass assumption (`emit_ts.pl:2042-2046`) |

### Fork B. Put mutually-recursive rules in one iterative scope the way DD does

| | |
|---|---|
| what it would do | extend the emitter's `iterate/1` term (today one head, `6_emit_dd_plan.pl:663-669`) into an iterative scope holding several heads; remove `reject_mutual_recursion` for in-scope pairs; let the dd-runner `settle` joint fixpoint cover the group (`v6/dd-runner/src/kernel.rs:86-107`) |
| what it costs | a new `dd_plan` notion for iterative scope / inner round / feedback / threshold state; fork 1 of the recorded transfer forks (`RECON.md:53`, `:120-125`); the SQLite runtime arm needs a fixpoint loop, today absent (`main.rs:86-90`) |
| code | `6_emit_dd_plan.pl` `iterate_operators/5` `:663-669`, `reject_mutual_recursion/2` `:460-470`, `rule_operators/5` `:449-458`; `v6/dd-runner/src/main.rs` and `kernel.rs` runtime |
| what breaks | the `mutual_recursion` and `recursive_stratum` receipts; `6_emit_dd_plan.test.pl:245-254`; 3 grade.sh fixtures byte-clean status if output order changes |

### Fork C. Drop the `recursive_stratum` stop by deferring ordering to the runtime

| | |
|---|---|
| what it would do | stop computing a collapsing topological order for cyclic groups; emit grouped rules and let an iterative scope settle them, matching the oracle (`level_eval.pl:187-196`) and the kernel `settle` |
| what it costs | `sql_rule_order/2` today must total-order every group or throw (`strat.pl:96-99`); deferral removes the ordering guarantee tsv2's single-pass emission relies on |
| code | `compile.pl:229`, `strat.pl:sql_rule_order/2` `:81-84`, `topo_order_group/2`; tsv2 `emit_ts.pl` recompute path |
| what breaks | tsv2's one-pass-per-stratum emission for non-recursive modules, which the code keeps identical deliberately (`emit_ts.pl:2044-2046`) |

### Fork D. Keep the stop but name it accurately (the narrowest change)

| | |
|---|---|
| what it would do | leave the semantics untouched; reconcile the two throw names so one stop surfaces, and document that it is an ordering/stratification gate, not a semantic limit |
| what it costs | near zero; the gate already fires as `recursive_stratum` for whole programs and as `mutual_recursion` only for hand-built plans |
| code | documentary: `6_emit_dd_plan.pl:460-470` comment + whether `reject_mutual_recursion` remains a separate net; `emit_ts.pl:2042-2046` comment |
| what breaks | nothing behavioral; only error-surface naming |

Four forks; no ranking. The only fork that changes what programs run is a
combination of A or B/C plus a runtime fixpoint; the rest re-label or
half-implement the same gate.

## 6. What must not be decided here

Per the repo law the language and type-system design happen with the user in
the room, and mutual recursion is a scheduling-semantics decision. This report
records cited facts and four unranked forks; it does not select one, and it
does not change any source file.

<!-- todo(decision): the dd_plan mutual-recursion stop will be re-opened as a scheduling-semantics decision (iterative scope per fork B vs. stratification per fork A vs. re-label per fork D); the user ranks the four forks in plans/2026-08-11-dd-mutual-recursion.md section 5. -->

<!-- todo(bug): three of the four fork tables name v6/dd-runner/src/main.rs and kernel.rs as change sites; those files are owned by another lane and are read-only here. -->

## Verification

- `git merge --ff-only 4dd8ef3a` exited clean; `git log --oneline -1` = the merge of #184 grounded at `4dd8ef3a`.
- `fixture_dd_plan_text` on `even/0 <- odd/0` + `odd/0 <- even/0` returned `unsupported_construct(recursive_stratum([even/0,odd/0]))` (swipl).
- `level_closure` on `even(X) <- odd(X)` + `odd(X) <- even(X)` with base `[odd(0), even(3)]` returned `[even(0),even(3),odd(0),odd(3)]` (swipl oracle).
- `manifest.json`: 370 `name` rows; grep `mutual_recursion` = 0; grep `recursive_stratum` = 0.
- Conformance fixture scan: 804 rules, 0 direct 2-cycles.

No source file was edited. Two plan documents were written:
`plans/2026-08-11-dd-mutual-recursion.md` (this file) and
`plans/2026-08-11-dd-mutual-recursion.visual.human.unga.md`.

## Staffing

- Lane: `dd-mutual-recursion-research`, read-only.
- Base: `4dd8ef3a` (main).
- Scope: research only; two plan documents; no source edits; no subagents; no external-library research.
- Open scheduling-semantics decision: left to the user per the brief.
