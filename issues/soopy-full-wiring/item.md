---
created: 2026-08-16
updated: 2026-08-17
type: epic
owner: chris
status: open
priority: high
---

# Soopy fully wired as the one source-identity layer

## Description

Every byte read and every revision named in sprefa-extract, sprefa-engine-rs, and the dl6 hosts goes through soopy types and reads. Measured base: plans/2026-08-16-soopy-extract-entanglement.md (13 collapse candidates). Extract core needs no changes: dispatch(path, content, mask) already accepts caller bytes; all defects are call sites choosing raw disk.

## Goal

One source-identity layer. Every byte and every revision in extract, engine,
and the dl6 hosts flows through soopy; no raw `std::fs::read` where a rev pin
exists; digests comparable end to end. Measured base:
`plans/2026-08-16-soopy-extract-entanglement.md`.

## Issues

- [x] @soopy-extract-host-reads (high) — kills the rev-pin identity defect
- [x] @soopy-blobsource-revive (high) — rev-correct reader into production
- [x] @soopy-contentid-adoption — one digest type end to end
- [x] @soopy-change-facts-work — the `--changed` question in v6
- [x] @soopy-catfile-gitbatch
- [ ] @soopy-typed-seams (cross-repo: soopy serde derive)
- [x] @soopy-depcrawl-manifest
- [x] @soopy-refmemo-freshness
- [x] @soopy-lockfile-unify

## Phases

1. Reads: extract-host-reads, blobsource-revive (disjoint files, parallel)
2. Identity: contentid-adoption (wide, lands after phase 1 to avoid churn)
3. Revisions: change-facts-work, depcrawl-manifest, catfile-gitbatch
4. Hygiene: typed-seams, refmemo-freshness, lockfile-unify

## Comments

Extract core is untouched throughout: `dispatch(path, content, mask)` already
takes caller bytes. All work is call sites and host executors.

### 2026-08-17T03:54:37Z · @soopy-driver

CI FINDING, pre-existing, epic-relevant. The cargo-dist 'plan' job fails on EVERY sprefa PR and has since the soopy path dependency landed: 'failed to read /home/runner/work/sprefa/hafley-rs/crates/soopy/Cargo.toml: No such file or directory'. Receipts: run 31992387756 (my PR #335), and the same failure on already-MERGED PRs #328 and #329. v6/sprefa-extract/Cargo.toml:95 and v6/sprefa-engine-rs/Cargo.toml:22 both point at ../../../hafley-rs/crates/soopy, which exists only on this machine, so neither crate is buildable in CI and cargo metadata cannot even read the workspace. Consequence for @soopy-lockfile-unify (PR #332): both sprefa lockfiles' soopy closures are resolved against a LOCAL hafley-rs working tree, so their contents depend on unpushed state in another repo. The lockstep rail holds the two in agreement with each other, which is the card's scope; it cannot make them reproducible off this machine. Fixing that means a git or registry dependency on soopy instead of a path, or vendoring, and that is a call for Chris.

### 2026-08-17T03:55:19Z · @soopy-driver

EPIC ROLL-UP from soopy-driver. LANDED BEFORE I STARTED, verified against origin/main and closed: soopy-blobsource-revive (PR #309), soopy-extract-host-reads (PR #310), soopy-catfile-gitbatch (a16a16a83), soopy-depcrawl-manifest (PR #308). MERGED THIS SESSION: hafley-rs PR #9 (soopy gains pub ContentId::blake3, ReadRequest serde derives, pub git_dirs) and sprefa PR #330 (extract-flow-cli-dispatch). OPEN, all graded green by me, all MERGEABLE: #331 soopy-change-facts-work (WORK revisions, 90/0 twice), #332 soopy-lockfile-unify (one 127-crate soopy closure plus the lockstep rail, extract 131/0 and engine 87/0 twice), #333 the blake3 dep drop (142/0 twice), #335 soopy-refmemo-freshness (89/0 twice, and it FLIPS a committed assertion: see the PR). NEEDS CHRIS: soopy-typed-seams, all four candidates measured wrong, blocked, or out-of-repo, see that card's two notes. NOT STARTED: extract-blob-cache-parallel, which by repo law needs a written candidate-by-candidate library analysis (moka / quick_cache / lru / plain HashMap) before any bespoke cache code, and measurement on the scale corpus; it is not mechanical-lane material.

### 2026-08-17T04:02:20Z · @soopy-driver

HANDOFF from soopy-driver. Everything below is measured, not remembered.

#### Standing traps a fresh driver WILL trip

1. THE EXTRACT GATE IS `cargo test --features cli`, never bare `cargo test`.
   v6/sprefa-extract/Cargo.toml:117-120 puts the `extract` bin behind
   required-features, so bare `cargo test` hands tests/1_resolve_cli.rs:25 a
   nonexistent CARGO_BIN_EXE_extract and reports 1 passed / 8 failed on a
   CLEAN tree. Two agents lost time to this. AGENTS.md:65 has the real gate.
2. A plain `git worktree add` worktree cannot commit: the global comment-budget
   Stop hook needs BOTH v6/tsv2/node_modules and
   v6/sprefa-extract/target/release/extract. Fix: symlink node_modules from the
   main tree and run `cargo build --release --features cli --bin extract` once.
   `boop beep lane create` does this itself via boop-start.
3. The comment-budget hook diffs the whole BRANCH against origin/main, not your
   commit. Editing one line inside a long pre-existing doc block re-flags the
   whole run. Precedent for the fix is an inline `// @comment-ok: <reason>`
   (see v6/sprefa-extract/src/types.rs:2209 and project.rs above `diet_scip`).
4. `cargo tree -p soopy` REWRITES the lockfile it reads. Measured: one run
   dirtied v6/sprefa-engine-rs/Cargo.lock by 192 lines. Use
   `python3 v6/tools/soopy-lockstep.py` (PR #332) instead.
5. `cargo test` in either Rust workspace dirties its Cargo.lock against the
   local hafley-rs soopy. Check `git status` before every commit and
   `git checkout -- <lock>` unless the lockfile change is your point.
6. Before ANY flash4 lane spawn:
   `strings ~/.cargo/bin/boop | grep -c paste-buffer` must be nonzero. A zero
   means another session clobbered the binary and every lane dies at spawn.

#### Per card

CLOSED, verified landed before this session: soopy-blobsource-revive (PR #309),
soopy-extract-host-reads (PR #310), soopy-catfile-gitbatch (a16a16a83),
soopy-depcrawl-manifest (PR #308).

MERGED this session: hafley-rs PR #9 (soopy: pub ContentId::blake3, ReadRequest
serde derives, pub git_dirs) and sprefa PR #330 (extract-flow-cli-dispatch).

OPEN, all graded green by me, all MERGEABLE, nothing to redo:
- #331 soopy-change-facts-work. Worktree
  .boop-worktrees/feature/soopy-change-facts-work.
  Gate: `cd v6/sprefa-engine-rs && cargo test -p sprefa-engine-rs`, 90/0 twice.
- #332 soopy-lockfile-unify. Worktree .boop-worktrees/chore/soopy-lockstep.
  Gate: `python3 v6/tools/soopy-lockstep.py` plus both cargo suites,
  extract 131/0 and engine 87/0 twice.
- #333 the blake3 dep drop. Worktree
  .boop-worktrees/chore/extract-drop-blake3.
  Gate: `cd v6/sprefa-extract && cargo test --features cli`, 142/0 twice.
- #335 soopy-refmemo-freshness. Worktree
  .boop-worktrees/fix/soopy-refmemo-freshness.
  Gate: `cd v6/sprefa-engine-rs && cargo test -p sprefa-engine-rs`, 89/0 twice.
  READ THE PR BEFORE MERGING: it REPLACES a committed test that asserted the
  stale memo behavior. That is deliberate and argued in the PR body.

IN FLIGHT, needs the next driver to finish it:
- Lane `fix-query-digest-repo-from-path`, tmux session of the same name, flash4,
  worktree .boop-worktrees/fix/query-digest-repo-from-path, base 7f11724b4,
  brief TASKS/query-digest-repo-from-path.BRIEF.md.
  It fixes a REAL bug I found while closing soopy-catfile-gitbatch:
  `extract query --digest` discovered the repository from the CWD rather than
  from the queried PATH (0_query.rs `cat_blob`).
  STATE AT HANDOFF: source fix applied and correct (`cat_blob(path, oid)` with
  `soopy::discover(path.parent().unwrap_or(path))`), new test file
  tests/25_query_digest_repo_from_path.rs written, tests/9_query_cli.rs also
  modified (inside its ownership, but REVIEW that hunk). NOT committed, no PR.
  Next driver: read the diff, run `cargo test --features cli` twice, commit
  with `Refs-Issue: @soopy-catfile-gitbatch`, push, PR.
  This lane is also the end-to-end proof that flash4 lanes work on the rebuilt
  boop; it survived well past the 90s watch and wrote the test BEFORE the fix,
  as its brief demanded.

NEEDS CHRIS, do not dispatch:
- soopy-typed-seams. All four candidates measured wrong, blocked, or
  out-of-repo. Two detailed notes on that card. Recommend closing it as
  measured-and-rejected and filing candidate 12 alone against ~/projects/sprefa-v6.

NOT STARTED:
- extract-blob-cache-parallel (transferred to me late, epic extract-port-closeout).
  Repo law requires a written candidate-by-candidate library analysis
  (moka / quick_cache / lru / plain HashMap) BEFORE any bespoke cache code, plus
  measurement on the scale corpus. Not mechanical-lane material. Its card's
  receipts also rotted at 331a2fa21 (BlobHash to ContentId); re-verify every
  cited line before briefing it.

#### One finding worth Chris's attention

The cargo-dist `plan` CI job fails on EVERY sprefa PR and has since the soopy
path dependency landed: `failed to read
/home/runner/work/sprefa/hafley-rs/crates/soopy/Cargo.toml`. Confirmed
pre-existing on already-merged #328 and #329, so it is not from my work. Neither
Rust crate is buildable in CI, and `cargo metadata` cannot read the workspace.
Consequence: the lockfiles PR #332 unifies are resolved against a LOCAL
hafley-rs working tree, so their contents depend on unpushed state in another
repo. The rail holds them in agreement with each other; it cannot make them
reproducible off this machine. A git or registry dependency on soopy instead of
a path is the fix, and that is Chris's call.

### 2026-08-17T04:45:04Z · @soopy-driver

HANDOFF from soopy-driver (second driver). Everything below is measured, not remembered.

#### Standing traps a fresh driver WILL trip

The previous handoff's six traps all still hold and are not repeated here; read
the note above this one. Three more, learned this session:

7. THE COMMENT-BUDGET HOOK COUNTS THE WHOLE BRANCH, and it counts a doc comment
   you merely EDITED as new. Two of my commits bounced on 3-line `///` blocks
   that were 3 lines before I touched them. Shrink to 2 or carry
   `// @comment-ok: <reason>`.
8. A HAND-MADE `git worktree add` NEEDS TWO SYMLINKS, both ABSOLUTE. The
   comment-budget rail wants `v6/tsv2/node_modules` and
   `v6/sprefa-extract/target/release/extract`. I wasted a commit on a relative
   `../../../../` node_modules link that was one level short; `ln -sfn
   /Users/chrishafley/projects/sprefa/v6/tsv2/node_modules` is the form that
   works, and the extract binary can be symlinked from the main tree's
   `target/release` rather than rebuilt.
9. `main` MOVES UNDER YOU. Between reading a file at session start and writing a
   plan doc citing it, `types.rs` line numbers had shifted by 53 lines from
   other drivers' merges. Cite against `git show origin/main:<path>`, never
   against the main tree's working copy, and re-fetch before every citation.

#### What landed this session

MERGED, all graded by me in their own worktrees, gates run twice each:
- #331 soopy-change-facts-work, f253e8f01. WORK revisions diff the dirty
  worktree. Engine 90/0 twice.
- #332 soopy-lockfile-unify, 59f391fef. One 127-crate soopy closure across both
  lockfiles plus the lockstep rail. Extract 131/0, engine 87/0, twice each.
  DEFECT I FOUND AND FIXED (826f5cfe3): the `just soopy-lockstep` recipe invoked
  `bash v6/tools/soopy-lockstep.sh`, a file the branch never added, rc=127. The
  PR body's "PASS, 127 crates" row was never a run of the recipe.
- #333 the blake3 dep drop, c6d7b1bfb. Extract 142/0 twice.
- #335 soopy-refmemo-freshness, 26344cf49. The ref memo follows a moved ref.
  Engine 89/0 twice. I AGREED with its replacement of the committed
  stale-memo test: that test asserted staleness as a feature, and its real value
  (a quiet store still answers the four host names from ONE `for-each-ref`)
  survives in the new quiet-repository test. DEFECT I FOUND AND FIXED
  (b02bb4833): the PR argued "a witness that spawned anything would cost more
  than the work it saves" and then spawned FOUR `git rev-parse` calls on every
  demand, because the diff moved `soopy::discover` (3 spawns,
  `_2_repository.rs:16,43,58`) and `soopy::git_dirs` (1, `_8_watch.rs:583`)
  AHEAD of the memo check that used to answer a hit with zero. The Git directory
  pair is now memoised per repository, so a hit is stat-only as claimed.
- #337 the query-digest repo-from-path fix, 55e15e747. Extract 143/0 twice. The
  flash4 lane's own SABOTAGE 2 claim does not hold; see that card's note.
- #339 the extract cache/parallel build-vs-buy analysis, 20168ed4c.
- #341 extract parallel dispatch, 924b8661f. 4.38 s -> 1.83 s on the whole
  corpus, 147/0 twice.

MERGED-MAIN RE-VERIFIED after the first five: engine 92/0, extract 143/0,
lockstep PASS 127. #331 and #335 both touch `hosts.rs` and the merge of the two
is green.

#### The epic

EIGHT of nine child cards are closed. Only `@soopy-typed-seams` remains, and it
NEEDS CHRIS: all four of its candidates were measured wrong, blocked, or
out-of-repo by the previous driver (two detailed notes on that card).
Recommendation unchanged: close it as measured-and-rejected and file candidate
12 alone against `~/projects/sprefa-v6`.

#### In flight, for the next driver

Lane `feature-extract-blob-cache`, flash4, base 924b8661f, worktree
`.boop-worktrees/feature/extract-blob-cache`, brief
`TASKS/extract-blob-cache.BRIEF.md`. It is the CACHE half of
`@extract-blob-cache-parallel`. Every design question is already answered in the
brief and in `plans/2026-08-17-extract-blob-cache-parallel.ANALYSIS.md`
(quick_cache over moka/lru/dashmap/HashMap, 512 MiB weight bound via
`SPREFA_EXTRACT_BLOB_CACHE_MB`, `dispatch` returns `Arc<ExtractOutput>`, six
call sites). Grade it, do not redesign it.

WATCH THE FLASH4 THROUGHPUT. The parallel lane wrote a correct implementation
and both test surfaces, then stalled: 68K context in 35 minutes across three
supervisor re-prods. I stopped it and finished the mechanical remainder myself
(gates, timing, a dead `enumerate`, a claimed sabotage that did not
discriminate, a vacuous fixture). If the cache lane shows the same profile, the
same call is available; the review burden is identical either way.

#### One finding worth Chris's attention, unchanged and now broader

CI on `origin/main` is RED on its own, not from any of this work. The
cargo-dist `plan` job has failed on every PR since the soopy path dependency
landed, and `v6-gates` fails with EIGHT legs outside the known-red allowlist
(text-door, staleness-gate, getting-started, flagship, dd-grade,
extraction-live, typecheck, rtkq-golden), with an identical failing set on
`origin/main` itself at 7f11724b4. Merges this session were graded on locally
re-run gates, twice each, because CI cannot grade anything today.

### 2026-08-17T04:48:21Z · @soopy-driver

HANDOFF ADDENDUM. `origin/main` re-verified at 924b8661f, after PR #341: `cd v6/sprefa-extract && cargo test --features cli` reports 149 passed, 0 failed, rc=0, in a detached worktree off `origin/main` (not the main tree, which other drivers keep dirty).

Lane `feature-extract-blob-cache` is LIVE as of this note, tmux session of the same name, no commit yet. Its completion hail routes to sprefa-coordinator. Its brief is `TASKS/extract-blob-cache.BRIEF.md` with receipts re-cited at 924b8661f, and the analysis it implements is on main at `plans/2026-08-17-extract-blob-cache-parallel.ANALYSIS.md`. Grade it against the brief's gate (extract `cargo test --features cli` AND `cargo test -p sprefa-engine-rs`, both twice, plus `soopy-lockstep.py`, plus a before/after timing and RSS receipt against the 1.83-2.53 s / 441 MB baseline).





