# MATCH + ARROWS + FRONTIERS LAB (planner contract, user go 2026-07-28 PM)

User word: "lets lab this one out ... lets find out where contradictions/
unknowns will occur and how direct to rxjs we can make it and how much sense
does it make with exhaustive tables answering the scenarios and properties/
invariants/lowering mechanics preservation and mixing overloaded syntax and
how could we change the syntax to avoid woes."

## The design under test (as presented in-session, none of it ruled yet)

1. ARMS: every atom is a delta envelope (rx `materialize` framing):
   `next(Row)` = +row (bare atom is sugar for this), `finalize(Row)` = -row
   any cause, `complete` = scope close (DEFERRED, slot below). Arms legal on
   edge-rule trigger positions only under the current sketch.
2. ARROWS: `match` is the source-major view of a rule set; arm arrows mirror
   the rule arrows: `body +> head` mirrors `head <+ body` (event axis,
   append), `body -> head` mirrors `head <- body` (state axis, maintained,
   retracts). The arrow selects the axis; lifecycle arms are `+>`-only.
3. TIMING FRONTIERS (dedalus classes, rx schedulers): Tn current tick
   (deductive / queueScheduler), Ti next drain tick (@next inductive /
   asapScheduler), Ta next EDB tick (@async / asyncScheduler). Defaults:
   edge-write VALUES visible Tn to levels; edge-write OCCURRENCES travel Ti
   (engine.pl CarryIn/CarryOut); `@async` = opt-in Ta. Tn for occurrences is
   REFUSED (unbounded in-transaction loop; breaks one-body-one-time-cut).
   Drain cap exists (tsv2 tickLoop drainCap 100); overflow policy unruled.
4. SUGAR LAW: everything desugars to the existing kernel; a match block =
   one kernel rule per arm. ARCH.pl sugar_grounds_out must hold.
5. FLAGSHIP: transition rule `changed(k,o,n) <+ finalize(cache(k,o)),
   next(cache(k,n))` — keyed replace emits both arms in one boundary.

## Ground rules (lab protocol, standing)

- One self-loading lab: `swipl -q -l <lab>.pl -g go -g halt` exits 0
  printing ONLY `PASS <name>` lines. Multiple .pl files fine, one entry.
- Lab files live in v6/prolog/labs/match_frontier/ (dir recreated for this
  lab; labs DIE on landing — coordinator deletes at merge, hash recorded).
  Durable verdict goes to plans/2026-07-28-match-frontier-lab-verdict.md.
- NO edits to v6/prolog/conformance/** (engine.pl is the shared oracle);
  claims about current engine behavior carry engine.pl/body.pl file:line
  citations (trigger_items, apply_edge_writes, tick/7 carry, boundary diff).
  Build a SMALL model interpreter inside the lab (tick/drain/async queues
  over term-form rules) for scenario runs; it models, never replaces, the
  oracle.
- Reference semantics IN prolog; rx lowering described as data (facts) and
  in the verdict, not mocked in code.
- Descriptive prolog variables. Banned words: provenance, substrate,
  load-bearing, regime. No em dashes in .md output.

## Questions the lab MUST grade (each = exhaustive table + PASS checks)

Q1 LEGALITY MATRIX. Rows: {bare atom, next(), finalize(), complete,
@async-marked atom, comparison guard, row pattern, enum destructure,
not()}. Cols: {`+>` arm, `->` arm, classic `<+` body, classic `<-` body}.
Every cell = legal | refuse(NamedError) | AMBIGUOUS(named slot). Check:
`desugar/2` succeeds on every legal cell and throws the named error on
every refuse cell. No cell left blank.

Q2 FRONTIER MATRIX. {value visibility, occurrence visibility} x {Tn, Ti,
Ta} x head kind {level, edge-set, edge-log, effect demand}: coherent |
incoherent(why) | refused. Include per-cell termination consequence
(bounded / drain-capped / unbounded) and what the tick log shows.

Q3 RX DIRECTNESS. For every legal Q1 cell and every coherent Q2 cell: the
pure-rx lowering as an actual expression string, graded DIRECT (plain
operator composition), ENCODED (needs scan/state-table beyond vanilla rx),
IMPOSSIBLE (semantics exceed rx). The counts are the "how direct" answer.
Check: a lowering/3 fact exists for every legal cell.

Q4 CONTRADICTION HUNT. Model-interpreter scenarios with hand-computed
expected tick logs IN the lab, graded by comparison. Mandatory scenarios:
 a. transition rule under multi-key + multi-replace in one tick
    (multiplicity table: N replaces = ? firings).
 b. finalize cascade cycle (evict -> finalize -> evict): quiescence vs
    drain cap; does spill-to-Ta CHANGE the log vs error-at-cap?
 c. self-retraction: a rule set where finalize of a rel feeds an edge that
    retracts more of the same rel.
 d. `->` arm containing not(): stratification preserved? (relax gap model)
 e. two-axis nesting: match over an effect rel with enum arms {Fresh,
    Error} x lifecycle arms {next, finalize} — is nesting order forced?
 f. Ta indistinguishability: @async carries joining the EDB batch unmarked
    — deterministic tick log under a perturbed schedule? (run the model
    twice with perturbed interleaving.)
 g. one-body-one-time-cut: a rule with finalize(x) AND next(y) atoms —
    which cut does each bind against; is there a coherent reading?
 h. finalize binding values of a row no longer in the table joined against
    settled state that references the same rel (old value vs new value in
    one body).

Q5 SYNTAX OVERLOAD TABLE. Every current use of `->`, `<-`, `<+`, `=>`,
`|`, `match`, arrow-in-effect-signature, prolog builtin ops (`->` if-then-
else, `;`, `|`), the DCG surface (SYNTAX.md) and the term form. Per
collision: parser ambiguity (term form AND DCG, with a real parse test
where feasible), human ambiguity, severity. Then >= 2 alternative
spellings PER woe with pros/cons, staying inside rx/prolog/sql symbol
families (vocabulary law). The lab does not pick winners; it prices
options.

Q6 INVARIANT PRESERVATION. For each standing invariant: sugar grounds out;
one-rel-one-rule-kind; stratification (negation in levels only);
occurrence multiplicity (one firing per occurrence); R7 boundary diff;
retention/keep; content-addressed effect identity + support refcount;
exactly-once endurance (crash between drain ticks: what replays?). Verdict
per invariant: preserved (proof sketch as a check where possible) |
broken (counterexample scenario) | needs-rule (named slot).

## Named ambiguity slots (fill or add, never silently resolve)

SLOT-COMPLETE (complete arm semantics), SLOT-CAUSE (cause column on
finalize), SLOT-SPILL (drain overflow: error vs spill-to-Ta), SLOT-NEST
(two-axis match nesting), SLOT-TA-MARK (async carries marked vs unmarked),
SLOT-ARROW (final glyph choice), SLOT-LEVEL-ARMS (are `->` arms restricted
to guards/patterns, lifecycle arms refused — confirm or refute).

## Prospective fixtures (write as fixture/5 terms in the lab, do NOT add
## to conformance until ruled)

1. departure rename: the existing departure_form fixture re-expressed with
   a finalize arm, expected log unchanged.
2. transition rule: keyed replace driving `changed(k, old, new)`, expected
   log hand-written.

## Deliverables

- v6/prolog/labs/match_frontier/*.pl (self-loading, PASS-only output).
- plans/2026-07-28-match-frontier-lab-verdict.md: verdict line first
  (does the design hold / where it cracks), the six tables, numbered
  ambiguities mapped to slots, syntax-change options priced, and the
  contradiction list ranked by severity.
- Final report: verdict, PASS count, top contradictions, slot fills.

## Validation

Lab: swipl self-load exits 0, PASS lines only. Untouched: conformance
go.pl (110), roundtrip.sh (110/110), tsv2 6/6 — run all three to prove no
drift. No file outside labs/match_frontier/ + the verdict doc.
