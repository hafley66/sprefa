# Lane `fix/extract-rust-rss-bisect` (opus): find the +970 MB in the rust checker tier

First action: `git merge --ff-only f71d1ae5e3f62ebb7798e19444c88a1e24dce192`.
If that sha does not exist in your tree, STOP and beep the coordinator; do NOT
guess a different base.

## Goal, one sentence

Name the commit that raised `rust.checker` peak RSS from 3,756 MB to ~4,726 MB
on the 873-file rust-analyzer corpus, say WHY in one paragraph with a
`path:line`, and either fix it or write down why the memory is now required.

## Why (zero-context reader)

`docs/failure-modes.md` entry 105 has the full incident. Short version: the
ratchet's rust RSS ceiling was red on main for six commits and nobody saw it,
because `just extract-ratchet` is LOCAL-ONLY and CI structurally cannot hold
it.

Measured, four times, all on this machine:

| tree | harness | rss_mb |
|---|---|---|
| main @41333391a | old | 4,726 |
| feat/extract-tier-axis | new | 4,729 (loaded), 4,729, 4,609 (quiet) |
| ceiling planted at `f58cc6666` | old | 3,756 |

Wall went DOWN over the same span (13,465 -> ~12,269 ms), so this is memory
alone. Accuracy is byte-unchanged: `84.28 / 83.64` vs codeql and
`64.93 / 98.26` vs the typedecl oracle, both then and now.

Six commits touched `v6/sprefa-extract/src` between the ceiling's
`measured_at_sha` and main's tip:

```
41333391a  #631  checker fallback is loud, refuse zero-file crate graph
1181026cd  #629  rust checker TYPE answers pinned
21ca6d1b8  #625  load a sysroot
56ac1178c  #628  F4 duplicate ts def
fe2a5e479  #626  F2/F3 duplicate + cross-file move
2dde9d4bf  #624  F1 param shadow, F5 same-file duplicate
```

The `3,756` was measured POST-sysroot (the #625 lane measured in its own
worktree before committing), so #625 is NOT the obvious suspect and you must
verify that claim rather than inherit it. #631 holding the crate graph to
refuse a zero-file join is the shape worth trying first. Both of those are
COORDINATOR HYPOTHESES with nothing measured. Doubt them.

## Method

Measure, never reason. The leg is:

```
cd v6/sprefa-extract
cargo test --release --features cli,rust-checker --test ratchet_recall \
  -- --exact --ignored --nocapture ratchet_rust_checker
```

On the older commits the test is named `ratchet_rust`, not
`ratchet_rust_checker`; the rename landed in #632. Read
`tests/ratchet_recall.rs` at each commit rather than assuming a name.

Rules that make the numbers real:

- **Release only.** A dev-profile build reads 4-14x the wall and is
  failure-mode 101.
- **One leg per process.** `rss_mb` is `getrusage(RUSAGE_SELF)`, so a second
  corpus in the same process poisons the high-water mark.
- **Quiet machine.** Failure-mode 104: a busy box reads as a regression. Check
  `boop beep lane list` for live lanes before each measurement and say in the
  PR body what was running.
- **Three measurements per commit, report all three.** Two back-to-back
  whole-gate runs on one tree gave different failing sets under load.
- **10-second law.** Every measurement runs in the BACKGROUND with a log file;
  no foreground wait over 10s. The rust checker leg is ~40 s per internal run,
  3 runs per invocation, so budget ~2 min per commit and poll.
- Background the builds too. The `ra_ap` crate graph is a ~380 s cold build and
  each commit may need one.

`git bisect` is fine, and so is measuring the six commits directly: there are
only six and each is ~2 min once built. Prefer whichever gets a clean answer;
say which you used.

## What "done" looks like

One of these two, never a shrug:

**A. A fix.** RSS returns under 4,131 MB (the 3,756 ceiling plus its 10%
tolerance) with accuracy byte-unchanged: `rust.call.checker.codeql`
`84.28 / 83.64`, `rust.call.checker.oracle` `93.66 / 51.82`,
`rust.call.checker.scip_override` `77.09 / 36.98`,
`rust.type.checker.oracle-typedecl` `64.93 / 98.26`. Then bump
`RATCHET.cost.tsv` down with `RATCHET_BUMP=1` and paste the row.

**B. A written reason the memory is required**, naming the commit, the
allocation site with a `path:line`, and what is held and why. Then the current
ceiling stands and `docs/failure-modes.md` 105 gains an `- **Outcome**:` line
closing the bisect.

Either way, do NOT leave the ledger entry open.

## Acceptance receipts (PR body)

1. A table: commit, three rss readings, three wall readings, what was running.
2. The named commit and the `path:line` of the allocation or the retained
   structure.
3. The four accuracy rows above, byte-unchanged, from a full
   `ratchet_rust_checker` run at your tip.
4. `cargo test --release --features cli,rust-checker,ts-checker` rc=0 over the
   WHOLE suite. #632 shipped a broken `tests/79_rust_type_dump.rs` because only
   one target was built.
5. `git diff --stat origin/main...HEAD` lists only owned files.
6. Whether #625 being post-sysroot holds, since the coordinator asserted it
   without measuring.

## Files you own

- `v6/sprefa-extract/src/lang/rust_checker*.rs`, `src/project.rs` (the fix, if
  there is one; smallest possible diff)
- `plans/extract-bench-2026-08-29/RATCHET.cost.tsv` (only to bump DOWN)
- `docs/failure-modes.md`, entry 105 only

FORBIDDEN: `plans/extract-bench-2026-08-29/RATCHET.tsv` (no accuracy number
moves in this lane; if one moves you introduced a defect, stop and report),
`v6/justfile`, `tests/bench/mod.rs`, every other test file except a new
fail-first one, the corpus checkouts, `v6/prolog`, `v6/tsv2`, root `src/`.

## Laws in force

- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers. The word is "oracle", never "ground truth".
- No `eprintln!` in `src/**`; `tracing` only.
- Comment budget: comments state constraints the code cannot show. No
  change-log narrative, no dates, no arc references.
- Descriptive names, never single-letter.
- Every accuracy number carries the full measure signature of `COMMON.md`:
  `<lang>.<family>.<tier>` vs `<oracle>` on `<corpus, n files>`, metric =
  numerator/denominator row unit. A percent with no fraction is banned.
- **Doubt yourself before asserting.** Every hypothesis in this brief is the
  coordinator's, measured by nobody. Verify against the code and the numbers.
- Never `--no-verify`. A blocked command ends the approach.
- You are a lane: never spawn a subagent.

## Reaching the coordinator

```
boop beep --no-wait --as fix-extract-rust-rss-bisect sprefa-coordinator "<one line>"
```
