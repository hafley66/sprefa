# 2026-08-12 green-all triage, plain

## what

Ran every red leg on the leaderboard 3 times, one at a time, machine quiet.
Wrote down what actually happens. Rewrote the red list so stops-lying.

## the count

- real and failing 3/3: 11 legs
- was listed, now green 3/3: 1 (extraction-live)
- flaky 2 of 3: 1 (serve-leak-soak)
- green on a clean temp dir, was never listed: 1 (leak-soak)
- was hiding a real fail that is not on the list: 1 (roundtrip)

## who is actually red (3 of 3)

```
compiler / runtime unit tests:   plunit           6 of 621 fail
print/parse round trip:          roundtrip        1 of 392 fails
compile cost gate:               compile-speed    16 regressions
error text vs the doc:           getting-started  block 24 drifted
golden corpus moved:             flagship         digest changed
golden coverage:                 golden-flex      json_object excuse stale, json_patch missing
grid host decode:                tsv2-test        zero-row demand answers with a row
lsp diag delivery:               lsp-diags        b.ts diag never arrives
row order vs golden:             rtkq-golden      two rows swapped order
emitter stmts pin:               scale-floor      flat, but pin is +2 stale
storage growth:                  memory-soak      sqlite pages double 24.8 -> 49.5
```

## the one that was hiding

roundtrip was failing and the list never mentioned it. It is now added.

## the one that should leave

extraction-live passes 3/3 once the extractor binary is there. The old red was
the gate racing the build, not a code bug. Removed from the list.

## the wobble

serve-leak-soak passed twice, failed once. A temp `setImmediate` handle was
still around at the exact instant it samples. Allowlisted as flaky so a blip
does not red the whole gate.

## leak-soak

Passes 3/3 when each run gets its own clean temp dir. The leg leaves behind a
file named literally `dl-perf.XXXXXX.jsonl` (the template never substitutes),
so the next run in the same dir trips over it. Not a real defect.

## what contradicts the night check

The coordinator said lsp-diags passed. It fails 3/3 here, every run, at the
same phase. Also plunit has six failures, not the one the list carried, and
getting-started fails at block 24, not the warning the list carried. Those are
the better findings.

## fix order, cheap first

1. flagship - rewrite the golden (the gate prints the command)
2. getting-started - fix the doc text for block 24
3. scale-floor - accept the steady [39,43] pin
4. golden-flex - drop the stale json_object excuse, add json_patch
5. rtkq-golden - sort the two rows
6. roundtrip - repair print/parse for one fixture
7. tsv2-test - fix the zero-row decode
8. plunit - trace the six fails, one change likely drives most
9. compile-speed - re-check then re-baseline
10. lsp-diags - fix b.ts diag over stdio
11. memory-soak - trace sqlite growth

flaky lane: serve-leak-soak. keep allowlisted, do not hold the merge on it.
