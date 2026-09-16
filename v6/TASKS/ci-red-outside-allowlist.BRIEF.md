# ci-red-outside-allowlist

## Goal
origin/main gate legs that fail and are NOT in `.github/CI-KNOWN-RED.md` are the
real signal. Fix each, or (only when the defect is real and out of scope) add a
row with exact failure text + throw site. Ship one PR.

## Suspects (from 2026-08-17 coordinator session, unmeasured since)
- rust-grade: ratchet `recursive_closure_passes_both_build_guard_arms`, and `no such function: reverse`
- dd-grade: `typegen_list_element_ladder`, `upper_folds_ascii_untouches_nonascii`
- engine-rs `cargo fmt --check`: 7 sites in `v6/sprefa-engine-rs`
- v6-gates legs: text-door, staleness-gate, getting-started, flagship, dd-grade,
  extraction-live, typecheck, rtkq-golden
- cargo-dist plan red: soopy path dep `v6/Cargo.toml:95` (REPORT ONLY, Chris owns)

## Legs (v6/justfile)
`green` at v6/justfile:484, `green-all-serial` at :495. Run legs ONE AT A TIME,
`just <leg>` from `v6/`, never the whole gate. Measure each red leg 3 times
before touching code and 3 times after.

## Rails
- Read `.github/CI-KNOWN-RED.md` first. Legs already allowlisted are out of scope.
- CPU cap: `cargo build -j4`, one leg at a time. Nothing beachballs the machine.
- 10-second law: a leg over 10s that is not SCIP or a multi-fixture battery is
  a defect to note in the PR body.
- Comment budget: comments state constraints code cannot show. No dates, no arc names.
- Banned words prose+identifiers: provenance, substrate, load-bearing, regime, refusal.
- No `eprintln!` in `src/**`.
- Rust fmt: `cargo fmt` only the crate the leg names.
- Never edit CI-KNOWN-RED.md to make CI green; a new row needs exact failure
  text + throw site + why the fix is out of scope.

## Worktree provisioning (before first commit)
The pre-commit comment-budget rail needs: `cargo build --release -p sprefa-extract`
(binary), `pnpm install` in `v6/tsv2` and `v6/sprefa-store/js`.

## Deliverable
Branch `fix/ci-red-outside-allowlist` off origin/main. Commit. `gh pr create`
with a body: table leg / before (3 runs) / after (3 runs) / fix or allowlist row.
Do NOT merge. Report the PR number, the table, and anything you left red.
