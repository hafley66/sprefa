# OPTION-LAB: where option(T) enters the pipeline

Contract file for the option(T) implementation-path scout. Design is settled
(plans/2026-08-08-option-type-design.md + rulings.pl `option_surface`, user
2026-08-09: both spellings legal; none per-instance; one enum per element
type). This lab decides WHERE the desugar runs and proves the first step.

## Candidates

| path | mechanism | door coverage |
|---|---|---|
| A | parse_dl.pl rewrites option decls into enum_decl + retyped col at parse time | text door only |
| B | new 0_option_expand.pl expansion phase (order 5, before enum at 10) | both doors (compile.pl:170 and conformance engine.pl:548 run 1_expansion.pl) |
| C | 0_type_plane.pl learns option(T) as real column storage through lower.pl/emit_ts.pl | both doors, but every downstream consumer of column types must learn it |

## Kill criteria

- A dies if the term door (conformance fixtures, the oracle) cannot spell
  option(T) at all under it, since fixtures are the gradeable record and the
  text-door receipt prints term fixtures back to text.
- B dies if the expansion output cannot be graded by conformance/ticklog.pl
  without special cases, or if the minted enum breaks the text-door byte
  round trip.
- C dies if the count of files that must learn the new type exceeds B's blast
  radius with no capability B lacks. Measure via the list(T) precedent (the
  one existing parametric type).

## Gate list (slices used here)

| gate | command | baseline |
|---|---|---|
| conformance | `swipl -q -l conformance/go.pl -g go -g halt` from v6/prolog | 330 PASS / 0 fail, 0.34s |
| plunit | `swipl -q -l compile/test/plunit_tests.pl -g run_tests -g halt` from v6/prolog | 276 |
| sweep + text door | `bash scripts/sweep.sh` from v6/tsv2 | manifest 330 entries, TEXT_DOOR 0 failures |
| ARCH | `swipl -g go -t halt ARCH.pl` from v6/prolog | green |

## Per-path first step

- A: typed_column_type/3 clause for `option(T)` + `T?`, parser-local mint of
  enum_decl. Only built if B fails; the parse clause itself is shared with B.
- B: 0_option_expand.pl (scalar element -> mint '__opt_<t>' enum_decl +
  retype col; rel-ref element -> companion split rel, drop the col, renumber
  keys; option in key column -> named throw), wire as expansion phase 5,
  conformance fixtures for option(text) and option(<rel-ref>) with arrivals
  and retractions, plus a .dl6 text-door probe.
- C: add column_storage/3 clause for option(T), record the next failure in
  the cascade (sabotage receipt), count consumer files via the list(T) grep.
  Not committed unless it wins.

## Probe receipts (state before any patch)

- term door: `column_storage([], option(text), S)` ->
  `unsupported_construct(column_type_unknown(option(text)))`
  (0_type_plane.pl:127-128). The refusal is a fall-through default clause,
  unfinished work, no impossibility encoded.
- text door: `email: option(text)` -> typed_column_type/3's bare-ident
  fallback (parse_dl.pl:672) eats `option`, leaves `(text)` unconsumed,
  decl fails to parse. Same class: no clause yet, nothing structural.
- `text?` -> `?` is consumed by no rule, parse error. Same class.

## Results (filled as steps land)

- [x] baseline gates recorded
- [ ] probes executed with exact outputs
- [ ] Path B first step + gate slice
- [ ] Path C cascade receipt
- [ ] verdict
