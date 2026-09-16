# BRIEF: drive RUST-GRADE up from 103/392, cause by cause

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha; that is your base. The ordering is not a gate. If a procedural
line in this brief seems to forbid otherwise-correct work, the work wins: note
the conflict in your report and keep going.

Your starting point is PR #215's branch `feature/emit-rust-close-the-loop`. If
that PR has merged, branch off main. If it has not, branch off it. Check with
`gh pr view 215 --json state`.

## One sentence
`emit_rust` is byte-clean on 103 of 392 fixtures; find out WHY the other 289
fail, group the causes, and fix them in descending order of fixtures unblocked.

## The measurement you inherit, verify it before trusting it

```
RUST-GRADE graded=392 byte-clean=103
```

| verdict | fixtures |
|---|---:|
| clean | 103 |
| diff (runs, wrong answer) | 55 |
| runtime-error | 122 |
| unsupported (emitter declines) | 106 |
| error / compiled-only | 6 |

Reproduce it with `cd v6/sprefa-engine-rs && bash grade.sh` before you change
anything. A number you did not reproduce is not your baseline.

## Deliverable 1, and NOTHING else ships before it

`graded.tsv` is two columns: fixture and verdict. It records no reason. So
nobody, including you, can tell whether the 122 runtime-errors are ONE cause or
a hundred. That is the single most valuable thing missing.

Add a third column carrying the failure reason, truncated to something
groupable (the panic message, the emitted-code compile error, the first diff
line). Then print a cause histogram:

```
RUST-GRADE graded=392 byte-clean=103
  runtime-error 122
     87  no method `drain_arrivals` for aggregate rel
     21  index out of bounds in level fold
     14  <other>
```

Commit that alone, with the histogram pasted in the commit message. The
histogram IS the work plan for everything after it, and it is worth landing
even if you fix nothing else.

## Deliverable 2: fix causes, biggest first

For each cause you take:
- one commit
- the commit message names the cause, the fixture count it unblocks, and the
  before/after `byte-clean=` number
- `graded.tsv` is updated in the same commit, so the ratchet moves with the fix

Do not chase a single interesting fixture while a cause blocking 80 sits
untouched. Order by fixtures unblocked, and say in your report if you deviated
and why.

## On the 106 `unsupported`

These are constructs the emitter declines. Each one is a hypothesis, never a
settled limit. For at least the top three by frequency, trace the decline to its
throw site in `emit_rust.pl` and say in the plan doc whether it encodes a real
impossibility or unfinished work. "The emitter does not support X" is a claim
that needs the throw site cited. Fix the ones that are merely unfinished.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| raise `byte-clean` by narrowing the corpus | the denominator is 392 and it stays 392 |
| mark a fixture `unsupported` to get it out of `runtime-error` | that is moving the failure, not fixing it |
| fix the one fixture whose name you liked | order by fixtures unblocked |
| report `byte-clean` without rerunning `grade.sh` | rerun it, paste the line |
| loosen the byte comparison | byte-identical to the oracle is the whole test |
| skip the histogram because fixing feels more productive | the histogram is deliverable 1 for a reason |

Report the real number even when it moves little. 103 to 141 with a named cause
is worth more than a decorated claim.

## File ownership. Yours alone:
- `v6/prolog/emit_rust.pl`
- `v6/sprefa-engine-rs/**`
- `plans/2026-08-12-emit-rust-climb.md`

## Forbidden, other lanes own these right now:
- `v6/prolog/compile/registry.pl` and `v6/prolog/lower.pl` (string-std lane, LIVE)
- `v6/boop/**` (boop transport lane, LIVE)
- `v6/prolog/emit_ts.pl` (do not touch the working emitter)
- `v6/prolog/compile/**`
- `v6/tsv2/**`

If a fix genuinely requires a forbidden file, STOP and report the exact line and
the reason. Do not work around it.

## Validation
- `cd v6/sprefa-engine-rs && bash grade.sh` — the `RUST-GRADE` line, every time
- `cd v6/tsv2 && bash scripts/sweep.sh` — must stay `MANIFEST_REASON_DIFF` all
  zero; you are adding a backend, not changing the front door
- conformance stays 392 PASS / 0 FAIL
- `cd v6/prolog && swipl -g go -t halt ARCH.pl` — all PASS

`just green-all` is RED and has been for days. `.github/CI-KNOWN-RED.md`
allowlists the failing legs. Read it before reporting any leg as broken. A leg
that fails and is NOT allowlisted is the real signal.

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- The word "refusal" is banned in prose; an error for an unbuilt construct is
  "TODO" or "not built yet".
- Comments state ONLY constraints the code cannot show. No change-log narrative,
  no dates, no arc references in source.
- Every new Rust type says what it is on first reading.
- The 10-second law: any operation over 10s is a defect to investigate. Note
  that `grade.sh` compiles a Rust crate per fixture; if the full grade run is
  slow, say how slow and propose the fix rather than normalising it.
- Docs open with a table of contents; output is tables and lists, not prose.

## Worktree setup, before your first commit
The extractor binary and two pnpm installs are absent in a fresh worktree. Run
the repo's prescribed setup before committing; the pre-commit hook needs them.
