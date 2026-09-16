# BRIEF: emit_rust pass 3. All 171 diffs are ONE failure, not 171.

## Base
Confirm the base with `git log --oneline -1` before your first commit. Branch off
origin/main, which now carries PR #222. Try `just boop-start` first; it exists on
main and makes a cold worktree commit-ready in about 2 seconds.

## The state you inherit

```
RUST-GRADE graded=392 byte-clean=109
```

| verdict | count |
|---|---:|
| clean | 109 |
| **diff** | **171** |
| unsupported | 106 |
| error / compiled-only | 6 |

Reproduce with `cd v6/sprefa-engine-rs && bash grade.sh` before changing
anything. A number you did not reproduce is not your baseline.

## The finding that defines this arc

The previous lane added a cause column, which was the right move, but for `diff`
rows it records the raw first-differing LINE. Every one is a unique string, so
nothing groups and the ledger looks like 171 separate problems.

Classified by the coordinator just now, across all 171:

| observation | count |
|---|---:|
| the oracle's line is MISSING from rust output | **171 of 171** |
| rust emitted an EXTRA line | 0 |
| rust emitted a WRONG line | 0 |
| the FIRST differing tick is tick 1 | **163 of 171** |
| first differing tick is tick 2 | 8 |

**Every single diff is absence.** The rust engine never produces a wrong answer;
it produces no answer. And in 95% of cases it has already failed by the end of
tick 1.

Rels most often present in the missing lines: `file` 10, `seen` 9, `demanded` 9,
`metric_doc` 9, `metric_sample` 9, `fpath` 9, `span` 8, `increment` 7.

Verify this classification yourself before acting on it. If it is wrong, that is
the top of your report.

## Deliverable 1: find the ONE cause, or prove there are several

Pick three fixtures from the 171 that differ from each other as much as
possible, and trace each end to end: what the oracle emits at tick 1, what the
rust engine does at tick 1, and where the two part company. Do it by reading and
running, not by reasoning about the emitter.

The hypotheses worth testing first, and there may be better ones:
- arrivals never reach the engine, so tick 1 has nothing to fold
- the tick loop runs but the tick-log writer emits nothing
- the emitted program's boot statements never execute
- a whole phase of the tick order is missing, the way dd-runner once matched
  ONE of twelve phases
- the schedule is parsed but not applied

Write which hypothesis survived and how you falsified the others. If it turns
out to be three causes rather than one, say so with the split.

## Deliverable 2: a diff cause column that actually groups

Replace the raw-line cause for `diff` rows with a CATEGORY plus a short detail:
missing-line, extra-line, wrong-value, wrong-order, and the tick it first
diverges at. Then print a histogram in the grade summary the way the
runtime-error histogram worked.

The test of a good cause column: someone reading `graded.tsv` should see three
buckets, not 171 strings.

## Deliverable 3: fix, biggest bucket first

One commit per cause. Each commit message states the cause, the fixtures it
unblocks, and the before/after `byte-clean=` number, with `graded.tsv` updated in
the same commit so the ratchet moves with the fix.

Do not chase a single interesting fixture while a cause blocking 100 sits
untouched.

## On the 106 unsupported

Out of scope for this arc unless a fix falls out for free. They are a separate
question: which are real impossibilities and which are unfinished work. Leave
them.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| raising byte-clean by narrowing the corpus | the denominator is 392 and stays 392 |
| re-labelling a `diff` as `unsupported` | that moves a failure, it does not fix one |
| a cause column with 171 distinct values | that is the defect you are fixing |
| reporting a number you did not re-run `grade.sh` for | rerun it, paste the line |
| loosening the byte comparison | byte-identical to the oracle is the whole test |
| reasoning about the emitter instead of running a fixture | trace three, end to end |

Report the real number even if it moves little. 109 to 180 with a named cause
beats a decorated claim.

## Validation
- `cd v6/sprefa-engine-rs && bash grade.sh` — the `RUST-GRADE` line, every time
- `cd v6/tsv2 && bash scripts/sweep.sh` — `MANIFEST_REASON_DIFF` must stay all
  zero; you are changing a backend, not the front door
- conformance stays 392 PASS / 0 FAIL
- `cd v6/prolog && swipl -g go -t halt ARCH.pl` — all PASS

`just green-all` is RED and has been for days. `.github/CI-KNOWN-RED.md`
allowlists the failing legs. Read it before calling any leg broken. A leg that
fails and is NOT allowlisted is the real signal.

## File ownership. Yours alone:
- `v6/prolog/emit_rust.pl`
- `v6/sprefa-engine-rs/**`
- `plans/2026-08-12-emit-rust-climb-2.md`

## Forbidden, other lanes own these right now:
- `v6/prolog/compile/registry.pl` and `v6/prolog/lower.pl` (string-std lane)
- `v6/prolog/compile/parse_dl.pl` and `v6/dl/grammar/dl.langium` (grammar lane)
- `v6/boop/**` (price-sync lane)
- `v6/prolog/emit_ts.pl` and `v6/tsv2/**`

If a fix genuinely needs a forbidden file, STOP and report the line and the
reason. Do not work around it.

## Style laws
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- "refusal" banned in prose; an error for an unbuilt construct is "TODO" or
  "not built yet".
- Comments state ONLY constraints the code cannot show. No dates, no narrative.
- Every new Rust type says what it is on first reading.
- The 10-second law: any operation over 10s is a defect to investigate. Note
  `grade.sh` compiles a crate per fixture; if the full run is slow, say how slow
  and propose the fix rather than normalising it.
