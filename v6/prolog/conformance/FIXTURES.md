# The shared fixture contract (read before promoting a lab)

One fixture = one `fixture/5` fact in a `fixtures/<lab>.pl` file:

    fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations).

`go.pl` auto-loads every `fixtures/*.pl`; never edit `go.pl` or `engine.pl`.
Validation, from the repo root, must end with zero `fail` lines:

    swipl -q -l v6/prolog/conformance/go.pl -g go -g halt

Every fixture file starts with the three op declarations (copy them from
`fixtures/merge_family.pl`). Prolog variables must be descriptive
(`Endpoint`, never `E`). Banned words apply (see repo CLAUDE.md). No em
dashes in comments.

## Program shape

- `Decls`: `kind(Ref, set|log)` | `keyed(Ref, Positions)` | `keep(Ref, all|count(N))`
- `Rules`: `(Head <- Body)` level | `(Head <+ Body)` edge
- Body goals: atoms, `not(Goal)`, `pre(Atom)`, `only(Atom)` trigger marker,
  `Var := Expr`, comparisons `< =< > >= == \==`, `now(Tick)`,
  `decode(Expr, Pattern)`, `json_each(Expr, Elem)`.
- Exprs: ints, `+ - * / mod` (Int-only, `/` truncates), `concat([..])`
  (the interpolation lowering), braces json literals `{key: Value}`, lists.
- Aggregate HEAD forms (level rules only, head column position only):
  `count/sum/min/max/json_array` of one expr, `json_object(KeyExpr, ValExpr)`.
  Multiplicity is BAG of body derivations (ruling q7).

## Rules the engine enforces (rulings.pl; your expectations must match)

- Every rel that receives edge writes or event arrivals is declared:
  `kind(Ref, log)` needs a `keep(Ref, _)` clause (use `all` unless the lab
  tests retention); an edge head must be a Log rel or a keyed rel; keyed
  Log is a load error.
- Arrivals are an ORDERED list, duplicates meaningful for Log rels. `+Row`
  only; `-Row` into a Log rel throws (allowed for Set source rels).
- Trigger occurrences at tick T = carry from T-1, then outside arrivals,
  then level rows newly true. Occurrences fire edge rules ONE AT A TIME;
  `pre` reads the evolving store, so folds CHAIN within a tick (two same-tick
  increments count 2; re-grade any lab check that assumed the undercount).
- Marked bodies (`only/1`) fire on marked atoms only; unmarked bodies keep
  any-atom (backlog replay is the documented default).
- Keyed writes replace (`-old/+new` at the boundary); equal-row write is a
  no-op; two DIFFERENT rows for one key from ONE occurrence throw
  `keyed_conflict(Ref, Key, Rows)`; across occurrences the later write wins
  (re-grade lab checks that expected same-tick conflicts from two ordered
  arrivals).
- Boundary deltas per tick: Log rels one `+Row` per new stamp (duplicates
  appear twice); Set rels and level views a set diff, removals before
  additions, sorted. Intermediate fold states are invisible.
- Edge-written rows that survive to the boundary AS DELTAS become trigger
  occurrences at T+1 (next-tick, ruling q4); when the schedule runs out the
  engine appends DRAIN ticks while carry remains. Your `deltas(...)` and
  `ticks(N)` expectations MUST include these trailing ticks (usually one
  final `[]` after the last write).

## Expectations

- `final(Ref, Rows)`: msort of the rel's rows at end of run (Log duplicates
  survive; level views included).
- `deltas(Ref, PerTickLists)`: one list per tick INCLUDING drain ticks.
- `ticks(N)`: total tick count including drains.
- `throws(Term)`: the run throws exactly `Term` (do not mix with others).

## Re-grading discipline (AGGREGATE.md 5c)

When a lab check's expectation contradicts the ruled semantics, promote the
fixture under the RULED expectation and keep the lab's original expectation
in a comment directly above it, marked `REJECTED READING`, with one line on
which ruling changed it. Never silently drop a lab check: if it cannot be
promoted (needs an unimplemented feature, tests lab-internal machinery like
desugar equality, or measures a rejected mechanism), list it in a
`not promoted:` comment block at the top of your file with the reason.

## Out of scope for fixtures

Real process spawns, sqlite emission counts, pattern/grammar matching
(astgrep), stdlib string fns beyond `concat`. Model effect fills as canned
scheduled arrivals (the bind-at-link law: canned rows are program-text
identical). Streaming envelopes are ordinary enum-shaped rows.
