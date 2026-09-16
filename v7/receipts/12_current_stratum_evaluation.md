# Current-stratum evaluator scheduling

## Status

Blocked by an observable compiler semantic change. The approved current-only
candidate was tested, then removed. `v7/src/1_libtime/0_evaluator.pl` exactly
matches base `0ed06ee176ec50fbd549d7810e1878eb059444b8`. No new scheduler,
commit, budget, cache, phase, type, workflow, or lower-row lifetime change remains.

Final changed files are `v7/test/1_entrypoints.test.pl` and this receipt.
Next is parent and user review of a separate dependency-closure design.

## Failed candidate

`PlainRules = CurrentRules - AggregateRules` installed
`31,11,39,3,3,26,2` rules rather than all-through
`31,42,81,84,87,113,115` at levels 0 through 6.

```text
before    wall_ms=2638  inferences=15,629,373  rows=14,586  diagnostics=[]
candidate wall_ms=883   inferences=6,482,876   rows=0
candidate diagnostic=missing_derived_bind(module(file(...)),'Name',0)
candidate last closure=15,108; corresponding baseline closure=15,110
```

Per evaluator invocation, assertions fell from `553` to `115`: exact reduction
`438`, candidate repeated lower definitions `0`, restored shipping repeated
lower definitions `438`. The failed candidate never reached an equivalent
successful second compiler round.

## Counterexample

Completed `LowerRows` omit answers that a later, more instantiated call can
produce. Nearest-shadow has `(: Name (Option text))`. A free lower-stratum
`Option(Source, Result)` call reaches `cons(Source, Empty, Arguments)` with
`Source` unbound and produces no row. At the later `:/4` stratum, the caller
binds `Source = ref(primitive(text))`; shipping reinstalls `Option/2`, which
then constructs and interns `application(Option,[text])`.

The added positive cross-stratum test pins the upper result and intern request.
The bound lower answer itself is absent from the general collected closure
under the current variant-tabled evaluator.

## Read-only dependency-closure counts

Input is the successful nearest-shadow runtime: 122 rules, existing strata and
`depends/3` terms. Exact counts use rule-level body edges from nonaggregate
rules because relation-level `depends/3` merges edges from aggregate and plain
rules sharing a head. All candidates remain limited to lower/equal strata.

| level | current nonagg | positive closure | all-body closure | all-through |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 31 | 31 | 31 | 31 |
| 1 | 11 | 25 | 32 | 42 |
| 2 | 39 | 55 | 67 | 81 |
| 3 | 3 | 21 | 35 | 84 |
| 4 | 3 | 19 | 25 | 87 |
| 5 | 26 | 75 | 97 | 113 |
| 6 | 2 | 2 | 2 | 115 |
| total | 115 | 228 | 289 | 553 |

Positive closure repeats `113` lower rules and removes `325` all-through
assertions. Literal all-body closure repeats `174` and removes `264`.
Relation-level `depends/3` over-approximates the positive counts as
`31,27,56,26,19,83,23`; it was retained as a cross-check, not the exact count.

`Option/2` is retained in the level-5 positive closure because:

- `:/4` is level 5 and `Option/2` is level 0;
- existing dependency term: `depends(ref(kernel(:)), ref(Option), positive)`;
- the current derived `Name` edge directly calls `Option(text, Result)`;
- both `Option/2` definitions are selected, including the `cons` plus `intern`
  definition and its `intern_snapshot` definition.

## Negative, aggregate, hosted, and kernel paths

This is execution-path evidence, without choosing scheduler semantics.

- Plain negative goals call `ground(Call), \+ evaluation_lower(...)` in
  `satisfy_goal/2`; installed rule definitions are not queried there.
- Current aggregate rules are handled before installation by
  `derive_aggregate_rows/4`; `completed_body_holds/2` uses `member/2` and
  `memberchk/2` only over `LowerRows`.
- Aggregate rules by level are `0,2,1,2,0,1,1`. Distinct negative body-relation
  edges in current plain rules are `0,5,4,2,1,1,0`.
- Following negative edges yields the all-body column. Following positive edges
  yields the positive-closure column. User approval is required before choosing.
- Nearest-shadow has `0` hosted rows and `0` host-port rows. Host metadata lowers
  to ground empty-body fact rules; no hosted free-enumeration blocker occurs here.
- `kernel(cons)` requires ground `List` or ground `Head` plus `Tail`.
  `kernel(edge_ref)` requires ground `Owner` plus `Label`.
  `kernel(intern)` requires ground `Constructor` plus `Arguments`.
  These three cannot enumerate from a fully free call. `kernel(nil)` can.
- Concrete blocking rule: `Option(Source,Result) :- nil(Empty),
  cons(Source,Empty,Arguments), intern(Option,Arguments,Result)`.

## Output and validation

Canonical lane artifacts, written with `write_canonical/2`, full stop, newline:

```text
before/restored sha256=c1dd35044a4b9a1b7543ceda9ba1265df35055920957defb11a43796c4a761b5
candidate sha256=0884603dec5d02c89d32ccab8f6bf25f6771a1671dbe7609ee39eddec2eda0d5
before vs candidate=DIFFERENT; before vs restored=IDENTICAL_BYTES
```

```text
five direct evaluator closures: 5/5 passed after restoration
focused evaluator cleanup/index/trace: 11/11 passed
timeout 20 swipl -q -s v7/test/3_compiler_trace.test.pl -g run_tests -t halt: 3/3 passed
timeout 20 swipl -q -s v7/test/20_compiler_performance.test.pl -g run_tests -t halt: 17/17 passed
timeout 20 just -f v7/justfile compiler-perf-gate: passed
cold=2622ms/15,629,367inf; warm=23ms/2,144inf; rows=14,586; diagnostics=[]
git diff --check: passed
```

Corpus comparison stopped at its first candidate difference, nearest-shadow.
Tests 15, 18, 19 and `2_partial.dl7` were not run under the invalid candidate.
CI coverage change: five evaluator cases added; zero changed or removed.
