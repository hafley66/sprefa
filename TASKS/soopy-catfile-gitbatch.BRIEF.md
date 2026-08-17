# Lane brief: query's blob door becomes soopy::GitBatch (issue soopy-catfile-gitbatch)

First action, before anything else, in your worktree:

```bash
git merge --ff-only f7ed05fa434aab8808c5a833a2bd94cb8448aead
```

Failure or missing tree = STOP AND REPORT. Do not work around it, do not
`--no-verify`, do not copy files in from elsewhere.

Read `CLAUDE.md` at the repo root before you edit.

## The defect

`v6/sprefa-extract/src/0_query.rs:60-77` (`fn cat_blob`) hand-rolls a
`std::process::Command::new("git").arg("cat-file").arg("blob")` spawn and hand-
parses its stderr. The epic's law is that soopy owns every Git call in this
repo. The batched door already exists and is already a dependency of this crate.

| thing | file:line |
|---|---|
| the hand-rolled spawn | `v6/sprefa-extract/src/0_query.rs:60-77` |
| its only caller | `v6/sprefa-extract/src/0_query.rs:52-58` (`fn source_bytes`) |
| the reference batched shape | `v6/sprefa-engine-rs/src/change_facts.rs:193-205` |
| `GitBatch::open` / `read` | `~/projects/hafley-rs/crates/soopy/src/_6_git_batch.rs:15`, `:34` |
| `soopy::discover` | `~/projects/hafley-rs/crates/soopy/src/_2_repository.rs:9` (returns `Repository { root, .. }`) |
| soopy is already a dep | `v6/sprefa-extract/Cargo.toml:92` |
| the single-line error helper | `v6/sprefa-extract/src/0_query.rs:288` (`one_line_text`) |
| exit code 2 on any `run` error | `v6/sprefa-extract/src/bin/extract.rs:252-256` |

## The exact fix

Replace the body of `cat_blob` with the soopy door. Nothing else in the file
changes shape.

```rust
fn cat_blob(oid: &str) -> Result<Vec<u8>, String> {
    let repository = soopy::discover(".")
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let mut batch = soopy::GitBatch::open(&repository.root)
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let bytes = batch
        .read(&soopy::ObjectId(oid.into()))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    Ok(bytes.to_vec())
}
```

Four constraints that are not negotiable, each pinned by a test you may NOT
edit:

1. **The root is the current directory, never `cli.path`.** With `--digest` the
   path argument is a label; `tests/9_query_cli.rs:117-135` runs the binary with
   `current_dir` = a fresh `git init` temp repo while passing a path
   (`tests/fixtures/rust/sample.rs`) that does not exist there. `soopy::discover`
   on that path would walk to a non-existent parent and fail. The old code
   spawned `git` in the process cwd; keep exactly that root.
2. **stderr stays ONE line and still contains the literal `git cat-file blob`.**
   `tests/9_query_cli.rs:139-153` asserts `stderr.lines().count() == 1` and
   `stderr.contains("git cat-file blob")`. anyhow's `Display` can carry a
   newline, so every arm goes through `one_line_text`, as written above.
3. **Exit code stays 2.** That comes from `bin/extract.rs:253`; do not touch it.
4. **Byte-identical stdout between the `--digest` and the plain-path legs.**
   Pinned by `tests/9_query_cli.rs:134-135`.

`GitBatch` kills and reaps its child in `Drop`
(`_6_git_batch.rs:74-79`), so the local binding going out of scope at the end of
`cat_blob` is the whole lifetime management. Do not add a static, a `OnceLock`,
or a cache: this CLI reads exactly one blob per process
(`0_query.rs:52-58` calls `source_bytes` once), so a long-lived batch here would
buy nothing and would be state nobody reads twice.

Delete the now-dead stderr-scraping lines with the old body. Do not leave a
commented-out copy.

## Receipts, required in the commit body

**FAIL-PRE-FIX is not applicable** (this is a door swap, not a bug fix), so the
receipt is the before/after of the two digest tests plus a NEW test that pins
the batched door.

**One NEW test file**, `v6/sprefa-extract/tests/9a_query_blob_door.rs`. You may
NOT edit `tests/9_query_cli.rs` or any other existing file under
`v6/sprefa-extract/tests/`; a sibling lane owns them. Add:

- a temp `git init` repo, `git hash-object -w` a small source file, run
  `extract query --lang rust --query '(function_item name: (identifier) @name) @item'
  --digest <oid> <label-path>` with `current_dir` = the temp repo, assert rc=0 and
  the expected capture line;
- the same call with `--digest` naming an oid that exists in the repo but is a
  TREE, not a blob (`git write-tree`): assert rc=2, exactly one stderr line, and
  that the line contains `git cat-file blob`. That case is what `GitBatch`'s
  header check (`_6_git_batch.rs:57-59`) rejects and the old code could not
  distinguish; it is the discriminating test for the swap.

Both tests build their own repo under `std::env::temp_dir()`; nothing reaches
the network and nothing writes inside the sprefa checkout.

## Gate, run each leg TWICE, echo rc explicitly, never pipe through `tail`

```bash
cd <your-worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 9_query_cli; echo "QUERY rc=$?"
cargo test --features cli --test 9_query_cli; echo "QUERY rc=$?"
cargo test --features cli --test 9a_query_blob_door; echo "DOOR rc=$?"
cargo test --features cli --test 9a_query_blob_door; echo "DOOR rc=$?"
cargo fmt --check; echo "FMT rc=$?"
```

Two runs of the same leg must print the SAME pass/fail counts. A leg that moves
between runs is a finding, report it rather than picking the green run.

The baseline at the base sha is green, so any red is yours.

## File ownership

OWNS, and nothing else:

- `v6/sprefa-extract/src/0_query.rs`
- `v6/sprefa-extract/tests/9a_query_blob_door.rs` (new file)

FORBIDDEN, do not open to edit, a live sibling lane owns each:

- `v6/sprefa-engine-rs/src/hosts.rs`
- `v6/sprefa-engine-rs/src/dep_resolve.rs`
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/src/types.rs`
- `v6/tsv2/goldens/scip_combo/**`
- EVERY existing file under `v6/sprefa-extract/tests/` (you may only ADD the one
  new file named above)
- `v6/sprefa-extract/src/lang/**`, `src/wire.rs`, `src/schema.rs`,
  `src/bin/extract.rs`
- everything outside `v6/sprefa-extract/`

Touching a forbidden file loses both lanes' work.

## Laws that bind you

- Never spawn a subagent. Fan-out is the coordinator's call only.
- No `eprintln!` anywhere under `src/**`. The error text returns as a `String`
  from `run`; the binary prints it.
- Infra is bought, never built. You are deleting a hand-rolled subprocess in
  favour of a library. Never write a replacement batch reader.
- Comment budget: comments state only constraints the code cannot show. No
  change-log narrative, no dates, no arc or issue references in code comments.
- No em dashes. Banned words in prose AND identifiers: provenance, substrate,
  load-bearing, regime.
- `pwd` before every `git commit`; commit ONLY in your worktree, never pipe a
  commit, use `COMMENT_RAIL_IDLE_MS=3000 git commit ...`.
- Commit trailer, required: `Refs-Issue: @soopy-catfile-gitbatch`.

## Landing

1. Your branch is already `chore/soopy-catfile-gitbatch`.
2. Commit with the gate receipts (rc lines, both runs of each leg) in the body.
3. `git push -u origin chore/soopy-catfile-gitbatch`.
4. `gh pr create` with a body carrying: the two-line defect statement, the
   receipts table (leg, run 1, run 2), and the new test's role.
5. **Never merge.** Do not `gh pr merge`. The coordinator lands it.
6. Before you report done: `git log --oneline -3` and `git status` in your
   worktree, and paste both. An uncommitted deliverable is an undelivered one.
