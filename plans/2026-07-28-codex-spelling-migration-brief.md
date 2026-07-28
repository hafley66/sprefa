# CODEX BRIEF: spelling migration wave 1 (luna-class, semantics-preserving)

Ruled inputs (rulings.pl): lifecycle_arm_vocabulary = rx Observer words;
match_block_word = match (match blocks themselves are NOT in this brief).
Session rulings: only() dies into latest(); combine is the explicit
spelling of the unmarked default; zip is reserved. Key decomposition is
EXPLICITLY OUT OF SCOPE (separate semantics arc).

Every change here is semantics-preserving. The grade is byte-identity:
if any tick log differs anywhere, STOP and report; do not adapt expected
outputs.

## Transform 1: only() -> latest() INVERSION (not a rename)

Current: `only(A)` marks A as the sole trigger; other body atoms are
sampled. New surface: triggers are BARE atoms; sampled atoms wear
`latest(...)`. Mechanical rule, applied per edge rule (`<+` bodies only):
- A rule with one or more only-wrapped atoms: unwrap them (they stay
  bare = triggers); wrap every OTHER positive non-special body atom in
  `latest(...)`.
- A rule with no only(): unchanged (all bare = combine default, C2b).
- `not(...)`, `pre(...)`, comparison/bind goals are NEVER triggers and
  NEVER get latest-wrapped; leave them untouched.
Apply to: conformance/fixtures/*.pl (term form), the reference engine's
trigger selection (engine.pl trigger_items/2 + wherever only/1 is
consumed: triggers = positive body atoms NOT wrapped in latest; the
marked/unmarked split collapses), compile/analyze.pl's use/4 marking,
parse_dl.pl, print_dl.pl, SYNTAX.md, regenerated dl_view/ + out/ +
gen_emitted/ via the existing scripts only.

## Transform 2: departed() -> finalize() rename (pure rename)

R4's departure wrapper takes its ruled rx name. Rename the functor in
fixtures, engine, analyze.pl, parser, printer, SYNTAX.md. The ARCH.pl
construct name `departure_form` stays (it names the construct, not the
spelling); update its side comments if they cite the old functor.

## Transform 3: reserved words, parsed + printed, refused by the compiler

- `combine(A, B, ...)` in a `<+` body = sugar, desugars to bare A, B, ...
  at parse/analyze time (identical semantics to writing them bare).
- `zip(A, B)` parses and prints; the compiler gate throws
  `unsupported_construct(zip)`; SYNTAX.md documents the future lowering
  (the min-ordinal pending-queue pattern already present in the scopes
  fixtures) without implementing it.
- `next(A)` in a `<+` body = sugar for bare A (the ruled default arm).
- `unsubscribe`/`complete`/`subscribe`/`error` as atom wrappers: parse +
  print + `unsupported_construct(lifecycle_arm(Name))` from the gate.
  No semantics.

## Grades (all must pass, run by you, in this order)

1. conformance go.pl: 110 PASS with UNCHANGED expected logs (the
   transform preserves semantics; expected outputs in fixtures are not
   edited except where a fixture literally spells only/departed in a
   comment or expected-log term text — those term texts must not change
   because only/latest/departed/finalize never appear IN logged rows).
2. sweep.sh: RUN total=31 identical=28 wrong=0 run_error=2
   no_oracle_log=1, per-fixture buckets unchanged.
3. roundtrip.sh: ALL GRADES PASS over the regenerated dl_view/.
4. plunit 17/17 (update test terms mechanically where they spell only()).
5. cd v6/tsv2 && pnpm test 6/6 + check-imports.sh OK.
6. grep receipts: zero remaining `only(` in fixtures/compile/parser
   surface paths (engine may keep an internal only-compat shim ONLY if
   grade 1 requires it, and then say so); zero remaining `departed(`.

## Laws

Descriptive prolog variables. No em dashes in prose. Banned words:
provenance, substrate, load-bearing, regime. One transform per commit,
`git commit -n`, no push, no merge. Any grade failure whose fix is not
obviously mechanical: STOP, record in final summary. Final summary:
per-transform commits, grep receipts, all six grade results, skips.
