# Lane brief: adopt `soopy::ContentId`, delete `BlobHash`

First action: `git merge --ff-only 8baae2dfddfea914ff95ebf86a1c563cd4f591af`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Second action: READ YOUR FULL BRIEF, at this absolute path, and follow it exactly:

    /Users/chrishafley/projects/sprefa/TASKS/soopy-contentid-adoption.BRIEF.md

It is outside your worktree on purpose. Read it whole before you edit anything.
It carries the pinned decisions, the numbered work order with per-file site
counts, the test-file exception table, the file ownership list, and the gate.
If you cannot read it, STOP AND REPORT rather than guessing.

Everything below is a summary; the file above wins wherever they differ.

- Goal: `sprefa-extract` adopts `soopy::ContentId` and DELETES its own
  `BlobHash`. No truncated variant, no wrapper type, no alias keeping the old
  name. Extract's `blake3` dep goes away. `ContentId`'s `Display`
  (`git:<oid>` / `blake3:<hex>`) becomes the wire form.
- `ContentId` is `Clone`, NOT `Copy` (it holds an `Arc<str>`). Drop `Copy` from
  `DefSite` and `ProjectEdge<F>`, keep `Clone`, fix each call site by borrowing
  or cloning explicitly. Report how many call sites you touched.
- **A previous lane on this card surveyed for 12 minutes and wrote NOTHING.**
  Do not repeat that. Start editing early and commit each slice the moment it
  compiles. A partial, committed, honestly-reported result beats an empty lane.
- `v6/sprefa-seed/**` is FORBIDDEN: it declares its own independent `BlobHash`
  and does not depend on `sprefa-extract`. Its hits are correct, leave them.
- The equality claim is a MEASUREMENT, not an assumption: `GitBlob` and
  `Blake3` are different value spaces. State what you measured in the PR body.
  Never fudge it with a normalizing comparison that hides the difference.
- Never spawn a subagent. Never `git commit -n`. Never edit `.githooks/**`.
- Bare `cargo fmt` is banned; only `cargo fmt -- <your owned files>`.
- Comment budget: the pre-commit rail FAILS any staged hunk with more than 2
  consecutive comment lines. Keep doc comments to 2 lines.
- Gate: `cargo build --all-targets --features cli` and `cargo test --features cli`
  TWICE from `v6/sprefa-extract`, plus `cargo build` from `v6/sprefa-engine-rs`.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.
- Push, open a PR whose body ends with `Refs-Issue: soopy-contentid-adoption`,
  and NEVER merge it. Report the URL and stop.
