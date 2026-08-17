# query-digest-repo-from-path

Repo: `~/projects/sprefa`. All paths repo-relative. ONE small bug, one file.

## First action

```bash
git merge --ff-only 7f11724b4726fa134022b34b22c76d0b4b042584
```

Failure = STOP AND REPORT. Do not work around it.

## Ownership

You own ONLY:
- `v6/sprefa-extract/src/0_query.rs`
- `v6/sprefa-extract/tests/**` (one new test file)

FORBIDDEN, do not open, do not edit, do not create files under: every other
file in `v6/sprefa-extract/src/` (especially `project.rs`, `types.rs`,
`wire.rs`, `lang/**`, `bin/**`), `v6/sprefa-engine-rs/**`, `v6/tsv2/**`,
`v6/prolog/**`, `issues/**`, `plans/**`, `chat_log/**`, `TASKS/**`,
`CLAUDE.md`, `ARCH.pl`, every `Cargo.toml`, every `Cargo.lock`.

## The defect

`extract query --digest <OID> <PATH>` reads the blob from the repository
containing the CURRENT WORKING DIRECTORY, not the one containing `<PATH>`.

`v6/sprefa-extract/src/0_query.rs`:

```rust
fn source_bytes(path: &PathBuf, digest: Option<&str>) -> Result<Vec<u8>, String> {
    match digest {
        Some(oid) => cat_blob(oid),                 // <- `path` is dropped here
        None => std::fs::read(path)
            .map_err(|error| format!("query input '{}': {error}", path.display())),
    }
}

fn cat_blob(oid: &str) -> Result<Vec<u8>, String> {
    let repository = soopy::discover(".")          // <- the cwd, not the path
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let mut batch = soopy::GitBatch::open(&repository.root)
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let bytes = batch
        .read(&soopy::ObjectId(oid.into()))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    Ok(bytes.to_vec())
}
```

The no-digest branch reads `path` directly, so the two branches disagree about
which file the caller asked for. Run from anywhere but the target repository,
the digest branch reads the wrong repository or fails outright.

Find the real line numbers by grepping for `fn cat_blob` and `fn source_bytes`;
do not trust the numbers in this brief.

## The fix, PINNED, do not re-decide

1. `cat_blob` takes the path: `fn cat_blob(path: &Path, oid: &str)`.
2. It discovers from the path's DIRECTORY, not the file itself, and falls back
   to the path itself when it has no parent:
   `soopy::discover(path.parent().unwrap_or(path))`.
3. `source_bytes` passes `path` through.
4. Nothing else in the file changes. Do NOT touch `run`, `query_language`,
   `validate_predicates`, `stream_matches`, `rewrite_predicates`, or any
   predicate code.
5. The error messages keep their EXACT current text (`git cat-file blob {oid}: {error}`).

## The test

New file in `v6/sprefa-extract/tests/`. Run `ls v6/sprefa-extract/tests/` first
and take the next free number, matching the directory's existing naming style.

The test must:

1. Build a scratch git repository in a temp directory, OUTSIDE the current
   working directory. Commit one file with known contents. Read the blob oid
   with `git rev-parse HEAD:<file>`.
2. Run the built binary as `extract query --lang <lang> --query <query>
   --digest <oid> <ABSOLUTE PATH INTO THAT REPO>` from a cwd that is NOT that
   repository. `std::process::Command` has `.current_dir(...)`; point it at
   `std::env::temp_dir()`.
3. Assert the process exits 0 and stdout carries a capture from the committed
   contents.

**Fixture naming rule, this repo has been bitten by it**: a temp directory
named from a clock reading collides across parallel test threads. Name yours
from `std::process::id()` plus an `AtomicU64` sequence, exactly as
`v6/sprefa-engine-rs/tests/git_refs.rs` does (read its `Fixture::build` and
`FIXTURE_SEQUENCE` before writing yours). Remove the directory on drop.

Also set `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME`,
`GIT_COMMITTER_EMAIL` on every `git` invocation, and run
`git symbolic-ref HEAD refs/heads/main` after `git init -q`: machine git
configuration must never reach this test. `git_refs.rs` shows the shape.

Pick the `--lang` and `--query` from an existing query test if one exists
(`grep -rln 'extract query\|QueryCli\|--digest' v6/sprefa-extract/tests/`); a
one-capture tree-sitter query over a tiny file is enough. Do not invent a
language name: the accepted set is in `query_language` in the file you own.

**FAIL-PRE-FIX**: run your new test against the UNFIXED code first, capture the
exact failure output, and put it in your report. It must fail for the stated
reason (wrong repository), not for a typo in the test.

## Validation, run it exactly

```bash
cd ~/projects/sprefa/v6/sprefa-extract && cargo build --release --features cli --bin extract
cd ~/projects/sprefa/v6/sprefa-extract && cargo test --features cli
```

`--features cli` is REQUIRED and is not optional: `Cargo.toml` gates the
`extract` binary behind `required-features = ["cli"]`, so a bare `cargo test`
leaves `CARGO_BIN_EXE_extract` pointing at nothing and reports 8 phantom
failures in `1_resolve_cli`. Run `cargo test --features cli` TWICE and put both
pass/fail counts in the PR body.

## Style laws

- `tracing` only; NO `eprintln!` in `src/**`.
- Comment budget: a comment states only a constraint the code cannot show. At
  most 2 consecutive comment lines in new code. No change-log narrative, no
  dates, no arc references, no restating the next line. Fail-first and sabotage
  receipts belong in the TEST file header, where they are expected.
- BANNED words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base, critical, mode.
- The word "refusal" is banned in prose.
- No em dashes anywhere.
- Descriptive names, never single letters.
- Colocated consistency: match each file's existing style.

## Landing

Branch is already checked out for you. Commit with trailer
`Refs-Issue: @soopy-catfile-gitbatch` and
`Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`, push, and open the PR
with `gh pr create`, receipts in the body.

DO NOT merge. DO NOT push to main. You never spawn subagents.
