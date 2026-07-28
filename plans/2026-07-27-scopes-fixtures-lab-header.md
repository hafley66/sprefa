# LAB HEADER: scopes fixture promotion — the minimal kernel enters the corpus

Planner-seeded contract per the lab protocol. Worktree agent; you own
`v6/prolog/conformance/fixtures/scopes.pl` ONLY (go.pl auto-loads fixtures/*.pl;
you never edit go.pl or the engine).

## The rulings you promote under (rulings.pl, all user-final)

- `subscription_kernel = minimal_with_coverage_check_and_ghost_view`: ZERO stored
  engine rels, ZERO new tick phases, ZERO new tick-input kinds. switchMap = keyed
  replace on an ordinary program rel; flattening policy = the scope row's key shape
  ([k] = switch, [k, instance] = merge, [k] + guard = exhaust); concat's queue,
  scope_done, demanded/2 are ordinary program rules.
- `salt_minting = content_addressed`: a fill is a cache update addressed to
  (identity, witness); never stale, shared across demanders.
- `stale_fill_policy = not_applicable_under_content_salts`.
- `effect_abort` is runtime world-cost machinery with NO store observability — it
  gets NO fixtures here.

## Source scenarios

`git show ac2aafdc:v6/prolog/labs/switch_flow.pl` (89 checks) and its distillation
`plans/2026-07-27-switch-flow.md` section 7 (the minimal-kernel eliminations, each
already a green scenario). Promote the SURVIVING scenarios into the shared fixture
format (study fixtures/state_machine.pl + operators.pl + FIXTURES.md for the format;
fixture/5 terms, deltas/final expectations, ticks counts).

## The fixture set (12-18 fixtures, named for what they prove)

1. switch-as-keyed-replace: new outer row retracts the old scope's derived cone,
   byte-graded deltas.
2. merge policy: key [k, instance] — two scopes coexist, independent teardown.
3. exhaust policy: key [k] + guard — new row ignored while a scope lives.
4. concat via program queue rel: arrival-ordered dequeue, ONE-tick latency, queue
   rows retract as they drain.
5. scope_done as ordinary rule: terminal-arm spelling, conjunctive (forkJoin)
   spelling, explicit-head spelling — one mechanism, three fixtures or one fixture
   with three rels.
6. completion propagation: a two-stage pipeline whose downstream completes exactly
   at the lattice-predicted tick (pick 2-3 signal schedules from the lab's 21).
7. takeUntil: keyed replace + negated scope_done.
8. state-flap netting: same-tick error-then-fresh flap = zero scope churn.
9. fill-as-cache-update: a response arriving after its original demander's scope
   died still lands in the cache rel (content salt), and a NEW demander of the same
   identity reads it — SWR shape, no staleness anywhere.
10. demand laziness: demanded/2 as a program rule; effect rows exist only while
    some scope's demand row is live.
11. shared demand refcount: two scopes demand one identity; one dies; the demand
    projection survives via the other's support.
12. the zombie-scope NEGATIVE case: the redteam A2b counterexample program, present
    as a fixture asserting today's (unchecked) behavior with a comment block naming
    it as scope_cover_check's future target — when the checker lands this fixture
    flips to a throws() expectation. Never silently drop it.

## Hard constraints

- The reference engine must need ZERO changes. If any scenario cannot be expressed
  with existing constructs + existing fixture vocabulary, STOP that scenario and
  report it as a FINDING AGAINST the minimal-kernel ruling (that is a headline, not
  a workaround target).
- `swipl -q -l v6/prolog/conformance/go.pl -g go -g halt`: all prior 97 stay green,
  plus your new PASS lines, zero failures.
- Fixture comments cite the lab scenario each one came from (name, not line).
- findall never builds trigger lists; descriptive names; banned words: provenance,
  substrate, load-bearing, regime; no em dashes.

## Return

PASS count before/after, the fixture list with one-line what-it-proves each, any
STOPPED scenarios with the exact expressiveness gap, and nothing else.
