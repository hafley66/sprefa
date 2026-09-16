# soopy-refmemo-freshness

Issue: `issues/soopy-refmemo-freshness/item.md` (epic soopy-full-wiring).
Repo: `~/projects/sprefa`. All paths repo-relative.

## First action

```bash
git merge --ff-only <BASE_SHA>
```

Failure = STOP AND REPORT. Do not work around it.

## Ownership

You own ONLY:
- `v6/sprefa-engine-rs/src/hosts.rs`, and inside it ONLY the `GitRefExecutor`
  block: the `static GIT_REFS` line, `pub struct GitRefExecutor`, its
  `impl GitRefExecutor`, and its `impl IHostExecutor` (currently `hosts.rs:408`
  through `:545`).
- `v6/sprefa-engine-rs/tests/git_refs.rs`

FORBIDDEN, do not open, do not edit: every other region of `hosts.rs`
(`SprefaExtractExecutor`, `SoopyFilesExecutor`, `DepCrawlExecutor`,
`GitRevisionExecutor`, `ChangeFactExecutor`), every other file in
`v6/sprefa-engine-rs/`, `v6/sprefa-extract/**`, `v6/tsv2/**`, `v6/prolog/**`,
`~/projects/hafley-rs/**` (soopy is a sibling lane's), `issues/**`, `plans/**`,
`chat_log/**`, `TASKS/**`, `CLAUDE.md`, `ARCH.pl`.

## The defect

`GitRefExecutor` (`hosts.rs:412-414`) memoises a whole ref snapshot per
repository:

```rust
#[derive(Default)]
pub struct GitRefExecutor {
    snapshots: Mutex<BTreeMap<String, Arc<soopy::RefSnapshot>>>,
}
```

and `snapshot()` (`hosts.rs:442-469`) returns the memo on a hit with NOTHING in
the key but the repo path. It is a `static LazyLock` (`hosts.rs:408`), so the
memo lives as long as the process. Refs move; the memo does not. Every
`git_ref` and `git_tag` demand after the first in one process answers from the
first enumeration forever.

The memo is worth keeping: `soopy::Refs::snapshot`
(`~/projects/hafley-rs/crates/soopy/src/_11_refs.rs:65-79`) runs THREE `git`
subprocesses per call (`for-each-ref` at `_11_refs.rs:139-147`, plus the two at
`:209` and `:222`), and both the `git_ref` and `git_tag` hosts read one
snapshot (`hosts.rs:487` and `:517`).

## The fix, PINNED decisions, do not re-decide

**1. A freshness witness of the ref store, built from stat calls only.**

No subprocess, no watcher thread. The witness is the deterministic map of
(path, mtime nanos, byte length) over the files git rewrites when a ref moves:

- `<git_dir>/HEAD`
- `<common_dir>/packed-refs`
- every regular file under `<common_dir>/refs/` (recursive)

A file that does not exist contributes NO entry, so its appearance or
disappearance moves the witness. Type it:

```rust
/// The ref store's shape as stat data: one entry per loose ref file plus
/// packed-refs and HEAD. Stat-only, because the snapshot it guards costs three
/// `git` subprocesses.
type RefStoreWitness = BTreeMap<String, (u128, u64)>;
```

Keys are paths relative to the repository root (or the absolute path; pick one
and stay consistent). Values are
`metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos()` and
`metadata.len()`. A stat error on one entry SKIPS that entry rather than
failing the whole read; a witness that cannot be built at all is an empty map,
which forces a re-enumeration, which is the safe direction.

**2. `git_dirs` comes from soopy, never re-derived.**

A sibling lane is publishing `pub fn git_dirs(root: &Path) -> Result<(PathBuf, PathBuf)>`
from `soopy` (it returns `(git_dir, common_dir)`; the ref store lives in the
common dir, which is what makes a linked worktree read the right refs). Call
`soopy::git_dirs(&repository.root)`. Do NOT hand-roll `root.join(".git")`: that
is wrong for a linked worktree and for a bare repository.

If `soopy::git_dirs` does not resolve at compile time, STOP AND REPORT that the
dependency has not landed yet. Do not work around it and do not write your own.

**3. The memo stores the witness beside the snapshot.**

```rust
pub struct GitRefExecutor {
    snapshots: Mutex<BTreeMap<String, (RefStoreWitness, Arc<soopy::RefSnapshot>)>>,
}
```

`snapshot()` computes the current witness FIRST, then returns the memo only
when the stored witness equals it; otherwise it re-enumerates and replaces the
entry. Do not hold the mutex across the `soopy::Refs::open(...).snapshot(...)`
call: take it, read, drop, enumerate, take again, insert. Keep the existing
`HostError` shapes and messages for every failure path.

**4. Nothing else moves.** The row-building code in `impl IHostExecutor`
(`hosts.rs:485-543`) stays byte-identical. `ref_kind` and `ref_target`
(`hosts.rs:417-438`) stay byte-identical.

## Tests to add

Extend `v6/sprefa-engine-rs/tests/git_refs.rs`. Its `Fixture` already builds a
repo with branches and both tag kinds (`tests/git_refs.rs:26-80`); follow its
existing shape and its `FIXTURE_SEQUENCE` naming rule.

1. `the_ref_memo_sees_a_moved_ref`: run the `git_ref` host through
   `GitRefExecutor`, then create a new branch in the fixture
   (`fixture.git(&["branch", "later"])`), then run the host again on the SAME
   executor instance and assert the second answer contains `refs/heads/later`.
   THIS TEST FAILS TODAY. Capture its pre-fix failure output for the PR body.
2. `the_ref_memo_still_serves_an_unchanged_store`: two runs with no mutation in
   between return equal row sets. (This one passes before and after; it pins
   that the fix did not simply delete the memo.)
3. A test that a mutation which only rewrites `packed-refs` is seen: run
   `fixture.git(&["pack-refs", "--all"])` between two host runs and assert the
   rows still describe every ref.

Update the file's `//! FAIL-PRE-FIX` / `//! SABOTAGE` header in its existing
style: add ONE sabotage entry for the witness (drop the witness comparison and
always serve the memo: which tests fail, with the real numbers). Run the
sabotage for real; do not guess the counts.

## Measurement receipt required in the PR body

State the witness cost against the cost it guards, from the code: N stat calls
versus three `git` subprocess spawns. If you can measure it, do; if not, cite
the two receipts (`_11_refs.rs:139-147, :209, :222` for the spawns) and say the
measurement was not taken.

## Validation, run it exactly

```bash
cd ~/projects/sprefa/v6/sprefa-engine-rs && cargo test -p sprefa-engine-rs 2>&1 | tail -40
```

rc=0. Run it TWICE and put both pass/fail counts in the PR body.

## Style laws

- `tracing` only; NO `eprintln!` in `src/**`.
- Comment budget: a comment states only a constraint the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next line.
  Sabotage and fail-first receipts belong in the TEST header.
- BANNED words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base, critical, mode.
- The word "refusal" is banned in prose.
- Nothing seizes the machine: no watcher thread, no background task, no
  unbounded directory walk outside `<common_dir>/refs/`.
- No em dashes. Descriptive names, never single letters.
- Colocated consistency: match each file's existing style.

## Landing

Branch is already checked out for you. Commit with trailer
`Refs-Issue: @soopy-refmemo-freshness`, push, `gh pr create` with receipts.

DO NOT merge. DO NOT push to main. You never spawn subagents.
