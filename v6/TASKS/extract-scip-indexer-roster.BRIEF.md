# Lane brief: SCIP indexer roster, port the three missing languages (issue extract-scip-indexer-roster)

First action: `git merge --ff-only 988e2b514204735869ce2964008bdbea8ad91bc8`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

## The gap

v5's indexer table has six languages. v6's has three. python, kotlin/java and
cpp are undetectable and unindexable from v6. The gap is already named in the
v6 module header at `v6/sprefa-extract/src/scip_ensure.rs:35-40`, which states
the fix shape: "each is one `build` body plus its staging decision, not a new
wire."

Read these before editing:

| thing | file:line |
|---|---|
| v5 python row | `src/scip_setup.rs:66-72` |
| v5 kotlin/java row | `src/scip_setup.rs:80-86` |
| v5 cpp row | `src/scip_setup.rs:87-99` |
| v6 roster (3 rows) | `v6/sprefa-extract/src/scip_ensure.rs:65-88` |
| v6 `Indexer` struct | `v6/sprefa-extract/src/scip_ensure.rs:47-63` |
| existing `ScipTypescript` impl | `v6/sprefa-extract/src/scip.rs:97-139` |
| existing `ScipRust` impl (the STAGED one) | `v6/sprefa-extract/src/scip.rs:140-175` |
| existing `ScipGo` impl (the unstaged one) | `v6/sprefa-extract/src/scip.rs:176-` |
| the budget law | `v6/sprefa-extract/src/scip.rs:31-35` |

## Exact fix

### 1. Three `ScipSource` impls in `v6/sprefa-extract/src/scip.rs`

Add `ScipPython`, `ScipJava`, `ScipClang` beside the three that exist. Copy the
shape of the nearest existing twin, do not invent a new one.

v5 argv, verbatim, with `{out}` the absolute output path and cwd = root:

| lang | argv | staging |
|---|---|---|
| python | `scip-python index . --output {out}` | UNSTAGED. It writes only the `--output` file. Say so in the impl header the way `ScipGo` does. |
| kotlin/java | `scip-java index --output {out}` | STAGED, like `ScipRust`. It drives gradle or maven, which write `build/` and `target/` under the root, and the seam's law is that a corpus is never mutated by reading it. |
| cpp | `scip-clang --compdb-path compile_commands.json -o {out}` | UNSTAGED. It reads the compdb and writes only `-o`. |

EVERY `build` body calls `crate::scip_ensure::run_capped`, never
`Command::output()` or `Command::status()`. The child runs in its own process
group and the whole group dies on the deadline; these indexers fork, so a bound
that reached only the direct child would leak the real worker. Read how the
three existing bodies call it and match them exactly.

Each impl's header comment states, in one or two lines: the v5 argv it carries,
whether it stages and why. Nothing else. No dates, no arc references.

### 2. Three `Indexer` rows in `v6/sprefa-extract/src/scip_ensure.rs:65`

Marker files, `bin` and `install` strings are v5's, VERBATIM from the rows cited
above. Copy them character for character; do not modernize an install hint.

Keep the roster ordered to match v5's table order: rust, typescript, python, go,
kotlin/java, cpp.

### 3. Delete the stale NAMED GAP paragraph

`v6/sprefa-extract/src/scip_ensure.rs:35-40` says the roster carries three of
six. Once six land, that paragraph is false. Replace it with one line stating
the roster is v5's six, or delete it. Do not leave a comment that lies.

### 4. One test, in `v6/sprefa-extract/tests/8_scip_families_cli.rs`

Append; do not rewrite the file.

A root carrying a marker file for a language whose indexer is NOT on PATH must
produce a `scip_skip` row and exit 0. That is the law at
`v6/sprefa-extract/src/scip_ensure.rs:9-12`: a missing toolchain skips a root,
it never kills the caller. Build a temp dir with a `pyproject.toml` and nothing
else, run `extract --family scip <dir>`, assert rc=0 and at least one line with
`"record":"scip_skip"` and `"lang":"python"`.

Also assert `detect()` returns the new rows for their markers, mirroring v5's
own test at `src/scip_setup.rs:640-648`.

**Fail-first receipt, required.** Run the new test before adding the rows, paste
the red output into the commit body, then add the rows and paste the green.

Do NOT try to install scip-python, scip-java or scip-clang. The skip path is the
whole test. If a binary happens to be on PATH in your worktree, the test must
still pass; write it so it asserts on the marker-detect and the skip path with a
binary name that cannot exist, or gate the skip assertion on `which`.

## Gate, run each twice, read rc explicitly, never pipe through tail

```bash
cd /path/to/your/worktree/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 8_scip_families_cli; echo "SCIP rc=$?"
```

`cargo build` ALWAYS runs before any binary gate. Baseline at the base sha is
rc=0 with every leg green, so any red is yours.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/scip.rs`
- `v6/sprefa-extract/src/scip_ensure.rs`
- `v6/sprefa-extract/src/lib.rs` (re-export lines only, if the new types need them)
- `v6/sprefa-extract/tests/8_scip_families_cli.rs`

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/src/types.rs`, `src/wire.rs`, `src/schema.rs`
- `v6/sprefa-extract/src/lang/**`
- `v6/sprefa-extract/src/bin/extract.rs`
- `v6/sprefa-extract/tests/1_resolve_cli.rs`, `tests/golden_parity.rs`
- `v6/sprefa-engine-rs/**`, `v6/tsv2/**`, `v6/prolog/**`
- everything outside `v6/sprefa-extract/`

Concurrent lanes own the forbidden files. Touching one loses both lanes' work.

## Laws that bind you

- Never spawn a subagent. Fan-out is the coordinator's call.
- Infra is bought, never built. You are wiring three foreign binaries behind an
  existing subprocess seam. Never write an indexer.
- Comment budget: comments state constraints the code cannot show. No change-log
  narrative, no dates, no arc references.
- No em dashes. Banned words in prose and identifiers: provenance, substrate,
  load-bearing, regime.
- Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before you report done. An uncommitted
  deliverable is an undelivered one.
- Do not push. Do not open a PR. Do not merge. The coordinator lands it.
