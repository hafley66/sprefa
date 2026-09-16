# Lane brief: dl6 phase-2 arms are never dispatched (issue extract-dl6-resolve-unwired)

First action: `git merge --ff-only 988e2b514204735869ce2964008bdbea8ad91bc8`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

## The defect

`DlSource` implements BOTH phase-2 arms, and the project dispatch never calls
either, because the dispatch is a match on `Source::name()` with no `"dl6"` arm.
Every `.dl6` input to `extract --resolve` therefore emits zero `resolved_edge`
and zero `resolved_type_edge` rows, silently.

Receipts, verify each before editing:

| fact | file:line |
|---|---|
| `impl Resolve<CallF> for DlSource` | `v6/sprefa-extract/src/lang/dl6/_0_source.rs:425` |
| `impl Resolve<TypeF> for DlSource` | `v6/sprefa-extract/src/lang/dl6/_0_source.rs:449` |
| `fn name()` returns `"dl6"` | `v6/sprefa-extract/src/lang/dl6/_0_source.rs:389-391` |
| call dispatch, no dl6 arm | `v6/sprefa-extract/src/project.rs:449-462` |
| type dispatch, no dl6 arm | `v6/sprefa-extract/src/project.rs:467-478` |
| the arm is exercised only by a direct call in a test | `v6/sprefa-extract/tests/0_dl6.rs:143` |

## Exact fix, three steps, in this order

### 1. A declared arm table, replacing the two ad-hoc matches

In `v6/sprefa-extract/src/project.rs`, above `resolve_call_edges`, add one
table that states, for every language, whether it has a `Resolve<CallF>` arm and
whether it has a `Resolve<TypeF>` arm:

```rust
/// Which languages declare a phase-2 arm. Checked against `sources()` by
/// `tests/1_resolve_cli.rs`, so an arm that lands undispatched fails a test
/// instead of silently emitting nothing.
const RESOLVE_ARMS: &[(&str, bool, bool)] = &[
    // (Source::name(), has Resolve<CallF>, has Resolve<TypeF>)
    ("ts", true, true),
    ("rust", true, true),
    ("go", true, true),
    ("kotlin", true, false),
    ("prolog", true, false),
    ("dl6", true, true),
    ("markdown", false, false),
    ("astgrep", false, false),
];
```

Confirm the astgrep source's `name()` string yourself before writing that row;
read `v6/sprefa-extract/src/lang/astgrep.rs:219` and use whatever it returns.
The table must have exactly one row per entry in `sources()`.

### 2. Wire the two dl6 arms

In `resolve_call_edges` add `Some("dl6") => Resolve::<CallF>::resolve(&DlSource, output, cx),`
and in `resolve_type_edges` add the `TypeF` twin. Import `DlSource` from
`crate::lang`. Delete the now-wrong sentence in the `resolve_type_edges` doc
comment at `project.rs:463-466` that says only TS, Go and Rust have one, and
replace it with a pointer to `RESOLVE_ARMS`.

The match arms must agree with the table. Keep the match explicit; the default
body of `Resolve::resolve` is `todo!()` (`src/types.rs:1105-1109`) and a blanket
dispatch would panic.

### 3. Two tests in `v6/sprefa-extract/tests/1_resolve_cli.rs`

Append both to the existing file; do not rewrite what is there.

**Test A, the roster rail.** Iterate `sprefa_extract::sources()`. Assert every
`Source::name()` appears exactly once in `RESOLVE_ARMS` and that every table row
names a live source. Export `RESOLVE_ARMS` as `pub const` from `project.rs` and
re-export it from `lib.rs` beside the other `project::` re-exports so the test
can read it.

**Test B, the CLI leg.** Write two fixtures under
`v6/sprefa-extract/tests/fixtures/dl6/` (create the directory) where one file's
program references a rel declared in the other. Model them on the fixture the
existing `tests/0_dl6.rs` builds for its direct-call test: read that test first
and copy the exact source shapes it already proves resolve. Then run the binary:

```
extract --resolve <fixtureA> <fixtureB> --family call,type
```

Assert stdout carries at least one line with `"record":"resolved_edge"` and at
least one with `"record":"resolved_type_edge"`. Use `env!("CARGO_BIN_EXE_extract")`,
the same way `tests/4_capability_parity.rs:49` does.

**Fail-first receipt, required.** Before wiring step 2, run Test B and paste its
FAILING output into the commit body. Then wire and paste the passing output.
A test that was never red does not count.

## Gate, run each twice, read rc explicitly, never pipe through tail

```bash
cd /path/to/your/worktree/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
```

`cargo build` ALWAYS runs before any binary gate. Both test runs must be rc=0.
Baseline at the base sha is rc=0 with every leg green, so any red is yours.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/src/lib.rs` (the one re-export line only)
- `v6/sprefa-extract/tests/1_resolve_cli.rs`
- `v6/sprefa-extract/tests/fixtures/dl6/**` (new)

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/types.rs`, `src/wire.rs`, `src/schema.rs`
- `v6/sprefa-extract/src/lang/**` (including `lang/dl6/**` — the arms are
  correct, only the dispatch is wrong)
- `v6/sprefa-extract/src/scip*.rs`
- `v6/sprefa-extract/src/bin/extract.rs`
- `v6/sprefa-engine-rs/**`, `v6/tsv2/**`, `v6/prolog/**`
- everything outside `v6/sprefa-extract/`

Concurrent lanes own the forbidden files. Touching one loses both lanes' work.

## Laws that bind you

- Never spawn a subagent. Fan-out is the coordinator's call.
- Comment budget: comments state constraints the code cannot show. No change-log
  narrative, no dates, no arc references.
- No em dashes. Banned words in prose and identifiers: provenance, substrate,
  load-bearing, regime.
- Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before you report done. An uncommitted
  deliverable is an undelivered one.
- Do not push. Do not open a PR. Do not merge. The coordinator lands it.
