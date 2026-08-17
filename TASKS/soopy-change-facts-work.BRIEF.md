# soopy-change-facts-work

Issue: `issues/soopy-change-facts-work/item.md` (epic soopy-full-wiring).
Repo: `~/projects/sprefa`. All paths repo-relative.

## First action

```bash
git merge --ff-only 4531b429769e81b6a1142fdb232805c155836335
```

Failure = STOP AND REPORT. Do not work around it.

## Ownership

You own ONLY:
- `v6/sprefa-engine-rs/src/change_facts.rs`
- `v6/sprefa-engine-rs/tests/change_facts.rs`
- `v6/sprefa-engine-rs/src/hosts.rs`, and inside it ONLY the
  `ChangeFactExecutor` block (`hosts.rs:705-800`, from the
  `// ═══ the rev-pair change plane` banner through the end of its
  `impl IHostExecutor`).

FORBIDDEN, do not open, do not edit: every other region of `hosts.rs`
(especially `GitRefExecutor` at `hosts.rs:409-470`, `SprefaExtractExecutor` at
`:118-...`, `DepCrawlExecutor` at `:235-...`, `GitRevisionExecutor` at `:547-...`
— a sibling lane owns those), `v6/sprefa-engine-rs/src/dep_resolve.rs`,
`v6/sprefa-extract/**`, `v6/tsv2/**`, `v6/prolog/**`, `issues/**`, `plans/**`,
`chat_log/**`, `TASKS/**`, `CLAUDE.md`, `ARCH.pl`.

## The defect

`IRevisionDiffer::diff` (`change_facts.rs:63-65`) is stringly typed:

```rust
pub trait IRevisionDiffer: Sync {
    fn diff(&self, repository_root: &str, rev_base: &str, rev_head: &str) -> Result<RevisionDiff>;
}
```

and `listing_at` (`change_facts.rs:74-88`) hard-wires
`soopy::Revision::Named(Arc::from(revision))`. `soopy::Revision::Worktree` is
therefore unreachable, so v5's `--changed` question (diff a dirty worktree
against a commit) has NO v6 spelling.

`listing_at:83-85` also bails on any non-`GitBlob` content id.

## Receipts you will need

| thing | where |
|---|---|
| `pub enum Revision { Worktree, Named(Arc<str>), Commit(ObjectId) }` | `~/projects/hafley-rs/crates/soopy/src/_0_types.rs:239-243` |
| soopy's own `WORK` spelling: `if value == "WORK" { Revision::Worktree } else { ... }` | `~/projects/hafley-rs/crates/soopy/src/main.rs:126-129` |
| worktree enumeration hashes DIRTY bytes with `git hash-object --stdin-paths`, so it returns `ContentId::GitBlob(oid)` for objects that are NOT in the object database | `~/projects/hafley-rs/crates/soopy/src/_9_git_files.rs:105-139` |
| commit enumeration reads `git cat-file --batch-check`, objects that ARE in the database | `_9_git_files.rs:142-...` |
| the blob-read loop that would fail on such an oid | `change_facts.rs:184-205` |
| `ChangeFactExecutor` and its memo key `{repo}\|{rev_base}\|{rev_head}` | `hosts.rs:713-745` |
| the test fixture (3 commits, 4 change kinds) and its sabotage header | `v6/sprefa-engine-rs/tests/change_facts.rs:1-14, 26-...` |

## The fix, PINNED decisions, do not re-decide

**1. The trait takes `soopy::Revision`.**

```rust
pub trait IRevisionDiffer: Sync {
    fn diff(
        &self,
        repository_root: &str,
        rev_base: &soopy::Revision,
        rev_head: &soopy::Revision,
    ) -> Result<RevisionDiff>;
}
```

`SoopyRevisionDiffer::diff` (`change_facts.rs:151`) follows.

**2. Add the `WORK` mapping in `change_facts.rs`, public:**

```rust
/// The host wire spells a revision as text; `WORK` is the worktree, matching
/// soopy's own CLI spelling.
pub fn parse_revision(value: &str) -> soopy::Revision {
    if value == "WORK" {
        soopy::Revision::Worktree
    } else {
        soopy::Revision::Named(Arc::from(value))
    }
}
```

`ChangeFactExecutor::run` (`hosts.rs:760-765`) calls it on `rev_base` and
`rev_head` and passes the results down.

**3. `Listing` carries the content id, not a String.**

Change `type Listing = BTreeMap<String, String>;` (`change_facts.rs:72`) to
`type Listing = BTreeMap<String, soopy::ContentId>;`, and DELETE the
`let soopy::ContentId::GitBlob(oid) = entry.content else { bail!(...) };`
guard at `:83-85`, storing `entry.content` directly. Rename detection
(`take_renames`, `:92-127`) keeps working unchanged in shape: it groups by the
map's value, and `ContentId` derives `Ord` + `Clone` + `Eq`. Adjust its
`BTreeMap<String, Vec<String>>` grouping key type to `soopy::ContentId`, change
nothing about its logic.

**4. A worktree side reads bytes from DISK, never through the batch.**

`git hash-object` does not write the object, so a dirty file's oid is not in
the object database and `GitBatch::read` on it fails. Replace the two
`batch.read(...)` calls in the changed-lines loop (`change_facts.rs:189-200`)
with one helper:

```rust
/// A worktree side is dirty bytes with an oid that was never written to the
/// object database, so those bytes come from disk; a commit side comes from
/// the one batch process.
fn read_side(
    batch: &mut soopy::GitBatch,
    root: &std::path::Path,
    revision: &soopy::Revision,
    path: &str,
    content: &soopy::ContentId,
) -> Result<Arc<[u8]>> {
    if matches!(revision, soopy::Revision::Worktree) {
        let bytes = std::fs::read(root.join(path))
            .with_context(|| format!("read worktree {path}"))?;
        return Ok(Arc::from(bytes));
    }
    match content {
        soopy::ContentId::GitBlob(oid) => batch
            .read(oid)
            .with_context(|| format!("read {path} at {revision:?}")),
        other => bail!("a committed revision answered a non-Git content id for {path}: {other}"),
    }
}
```

Adjust the exact `batch.read` argument spelling to whatever compiles against
soopy's signature; read `change_facts.rs:190-199` for how the current call is
built. The `empty: Arc<[u8]>` base for a created path stays.

**5. A `WORK` diff is NEVER memoised.**

`ChangeFactExecutor::diff` (`hosts.rs:716-745`) memoises on
`{repo}|{rev_base}|{rev_head}`. A worktree moves under that key, so a memoised
WORK answer goes stale exactly the way the current key never invalidates. Gate
the memo: when EITHER revision string is `"WORK"`, skip both the memo lookup
and the memo insert and compute fresh. ONE comment states that constraint.
Keep the existing memo behavior byte-identical for two commit revisions
(`the_diff_memo_keys_on_the_whole_triple` in the test file must still pass).

## Known limit, state it in the PR body, do NOT fix it

A tracked file deleted from the worktree makes soopy's
`git hash-object --stdin-paths` pass fail, so `WORK` on a tree with a deleted
tracked file errors rather than reporting a deletion. That is soopy's
enumeration contract (`_9_git_files.rs:105-139`), not this card. Name it in the
PR body as a follow-up and stop.

## Tests to add

Extend `v6/sprefa-engine-rs/tests/change_facts.rs`. The fixture already builds
a three-commit repo; give it a way to dirty the worktree after the last commit
(a `dirty` method beside the existing `write`; read the file's existing
`Fixture` impl first and follow its shape).

1. `work_revision_diffs_the_dirty_worktree`: commit, then edit one tracked file
   and add one NEW tracked-and-committed-then-edited file; diff
   `(Named(head_sha), Worktree)` and assert the edited path is `Modified` and
   carries the right head-side line numbers.
2. `work_revision_is_not_memoised`: run the host through `ChangeFactExecutor`
   twice with `rev_head = "WORK"`, dirtying the file differently in between,
   and assert the SECOND answer reflects the second edit. This is the test that
   fails without pin 5.
3. `parse_revision_maps_work_and_names`: `parse_revision("WORK")` is
   `Revision::Worktree`; `parse_revision("main")` is `Revision::Named`.

Update the file's `//! CONTROL: N passed, 0 failed.` header line to the new
count, and ADD one sabotage entry in the existing header style for pin 5
(memoise WORK anyway: which tests fail and why). Run the sabotage for real and
report the real numbers; do not guess them.

## Validation, run it exactly

```bash
cd ~/projects/sprefa/v6/sprefa-engine-rs && cargo test -p sprefa-engine-rs 2>&1 | tail -40
```

rc=0. Run it TWICE and put both pass/fail counts in the PR body.

## Style laws

- `tracing` only; NO `eprintln!` in `src/**`.
- Comment budget: a comment states only a constraint the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next line.
  Sabotage and fail-first receipts belong in the TEST header, which this file
  already has.
- BANNED words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base, critical, mode.
- The word "refusal" is banned in prose.
- Never a per-row read: the blob batch stays ONE process for the whole loop.
- No em dashes. Descriptive names, never single letters.
- Colocated consistency: match each file's existing style.

## Landing

Branch is already checked out for you. Commit with trailer
`Refs-Issue: @soopy-change-facts-work`, push, `gh pr create` with receipts.

DO NOT merge. DO NOT push to main. You never spawn subagents.
