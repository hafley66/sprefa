# Round 2 gaps: soopy-catfile-gitbatch

Round 1 died to repeated provider flakes with its edit uncommitted. The
coordinator graded that edit and committed it for you. Nothing about the source
change is in question; do NOT re-do it, do NOT revert it, do NOT reformat it.

First action, in your worktree:

```bash
git merge --ff-only f7ed05fa434aab8808c5a833a2bd94cb8448aead
git log --oneline -1     # must print a16a16a83 (or a later tip carrying it)
```

Failure or a missing tree = STOP AND REPORT.

Read `/Users/chrishafley/projects/sprefa/TASKS/soopy-catfile-gitbatch.BRIEF.md`
in full. It is still binding. This file names only what is missing.

## What already landed (do not touch)

`a16a16a83` rewrote `cat_blob` in `v6/sprefa-extract/src/0_query.rs` to
`soopy::discover(".")` + `soopy::GitBatch::open` + `batch.read(&soopy::ObjectId(..))`,
every arm wrapped through `one_line_text(format!("git cat-file blob {oid}: {error}"))`.
Coordinator's receipts, run in this worktree:

| leg | result |
|---|---|
| `cargo build --all-targets --features cli` | rc=0 |
| `cargo test --features cli` run 1 | 95 passed, 0 failed |
| `cargo test --features cli` run 2 | 95 passed, 0 failed |
| `cargo test --features cli --test 9_query_cli` | 8 passed, 0 failed |

## Gap 1: the new test file does not exist

`v6/sprefa-extract/tests/9a_query_blob_door.rs` was never written. Write it now.
It is the ONLY file you may add, and you may not edit any other file under
`v6/sprefa-extract/tests/`.

Two tests, both building their own repo under `std::env::temp_dir()`, nothing
reaching the network, nothing written inside the sprefa checkout:

1. `query_reads_a_staged_blob_through_the_batch_door`
   - `git init -q` a temp dir, write a small Rust source into it,
     `git hash-object -w <file>` to get the oid;
   - run `CARGO_BIN_EXE_extract` with `current_dir` = that temp dir and args
     `["query", "--lang", "rust", "--query", "(function_item name: (identifier) @name) @item",
       "--digest", &oid, "<any path label>"]`;
   - assert rc=0 and that stdout carries the expected `@name` capture line.
   - The `current_dir` is what makes this pass: the root comes from the process
     cwd, never from the path argument.

2. `query_rejects_a_non_blob_oid_with_one_stderr_line`
   - in the same temp repo, `git add .` then `git write-tree` to get a TREE oid;
   - run the same command with `--digest <tree oid>`;
   - assert `status.code() == Some(2)`, `stderr.lines().count() == 1`, and
     `stderr.contains("git cat-file blob")`.
   - This is the discriminating case: `GitBatch::read_spec` rejects a non-blob
     header (`_6_git_batch.rs:57-59`), which the deleted hand-rolled spawn could
     not distinguish. A test that only re-checks the happy path is not a receipt.

Copy the temp-repo and `run_in` helper shapes from `tests/9_query_cli.rs:1-40`
into your new file. Do not `mod` or import from that file; duplicate the few
lines you need.

## Gap 2: `cargo fmt --check` is NOT your gate

The brief listed it. That was wrong: `cargo fmt --check` is RED at the base sha
in `v6/sprefa-extract` across 14 files nobody in this arc touched
(`src/lang/dl6/_0_source.rs`, `src/lang/rust.rs`, `src/lib.rs`,
`tests/0_dl6.rs`, `examples/typegraph_d2.rs`, and more). Do NOT reformat the
crate. Confirm only that `git diff` of YOUR files is fmt-clean; the committed
`0_query.rs` already is.

## Gap 3: no PR exists

The branch `chore/soopy-catfile-gitbatch` is pushed and tracking
`origin/chore/soopy-catfile-gitbatch`. No PR is open.

## Your remaining sequence

```bash
cd <worktree>/v6/sprefa-extract
cargo test --features cli --test 9a_query_blob_door; echo "DOOR rc=$?"
cargo test --features cli --test 9a_query_blob_door; echo "DOOR rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
```

Both runs of a leg must print the SAME counts; the whole-crate count must be 97
passed once the two new tests land. Then, from the worktree root:

1. `COMMENT_RAIL_IDLE_MS=3000 git commit` the new test file with the four rc
   lines in the body and the trailer `Refs-Issue: @soopy-catfile-gitbatch`.
2. `git push`.
3. `gh pr create` with a body carrying: the one-line defect statement, the
   receipts table (leg, run 1, run 2), and what the non-blob-oid test pins.
4. **Never merge.** Do not `gh pr merge`.
5. Report `git log --oneline -3` and `git status`, pasted.

Never spawn a subagent. No `eprintln!` under `src/**`. No em dashes. Banned
words in prose and identifiers: provenance, substrate, load-bearing, regime.
`pwd` before every commit; commit only in your worktree.
