# Lane brief: adopt `soopy::ContentId`, delete `BlobHash` (issue soopy-contentid-adoption)

First action: `git merge --ff-only 8baae2dfddfea914ff95ebf86a1c563cd4f591af`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

That base sha is the head of the sibling branch `feature/extract-doc-node-markdown`,
whose PR is posted and not yet merged. Its commits touch `src/types.rs`,
`src/wire.rs`, `src/schema.rs`, `src/project.rs` and `src/lang/markdown/**` and
will appear in your branch's history. Leave them alone: never revert, amend,
squash or reformat them. Your PR targets `main` as usual.

New test files landed by earlier cards in this chain that you may need to fix
MECHANICALLY under the test-file exception if they name `BlobHash`:
`tests/19_docs_lang_arms.rs`, `tests/20_unresolved.rs`,
`tests/21_kotlin_type_plane.rs`, `tests/22_doc_node.rs`. Same rule as the other
three: the type name and its construction only, no assertion values.

Do not ask a question before starting. Everything you need is below with a
`path:line`. Line numbers are from the base sha and may be off by a few lines;
locate by the SYMBOL NAME given and continue. If a symbol is absent or the code
contradicts the receipt, say which one and stop.

## MANDATORY READS, before you touch a line

- `.claude/skills/sql-relational-design`
- `.claude/skills/sqlite-costs`

Any SQLite DDL this change touches keeps INTEGER surrogate keys; content ids
live ONCE in a dictionary table, never as a composite TEXT primary key. If this
change reaches no DDL, say so explicitly in the PR body with the grep that shows
it.

## The gap

`BlobHash` is blake3 cut to 16 raw bytes (`v6/sprefa-extract/src/types.rs:50-73`),
incomparable with `soopy::ContentId`, and it forces extract's own blake3 dep
(`v6/sprefa-extract/Cargo.toml`, the `blake3 = "1"` line). A digest that comes
out of `repo_files_at` and a digest extract computed are two different value
spaces today.

```
rg -n 'BlobHash' v6/ --include=*.rs | wc -l     # 108 at the base sha; measure it yourself
```

## The decisions, already made. You implement them, you do not re-open them.

1. **Adopt `soopy::ContentId` as-is**: `GitBlob(ObjectId) | Blake3([u8; 32])`
   (`~/projects/hafley-rs/crates/soopy/src/_0_types.rs:266-269`). No truncated
   variant. No new wrapper type. No type alias standing in for the old name.
2. **`BlobHash` is DELETED from `sprefa-extract`.** Its 16-byte blake3 form
   survives nowhere in that crate. When you are done,
   `rg -n 'BlobHash' -g '*.rs' v6/sprefa-extract/` returns ZERO hits.
   `v6/sprefa-seed/` declares its OWN independent `BlobHash`
   (`src/_3_extract/_0_shape.rs:45`) and does NOT depend on `sprefa-extract`;
   its 25 hits are correct and FORBIDDEN to you. Measured at your base sha:
   131 hits total across `v6/`, of which 25 are seed's.
3. **Extract's blake3 dep is deleted** from `v6/sprefa-extract/Cargo.toml`.
   Hashing goes through soopy's forms. soopy computes the blake3 arm as
   `ContentId::Blake3(*blake3::hash(bytes).as_bytes())`
   (`soopy/src/_3a_files.rs:75`, `_4_worktree.rs:81`, `_7_source_tree.rs:169`);
   if soopy exposes no public helper for that, adding one to soopy is OUT of
   scope, so call through whatever soopy DOES expose and say in the PR body
   which path you took.
4. **One canonical wire rendering.** `ContentId` already has a `Display`:
   `git:<oid>` / `blake3:<hex>` (`soopy/src/_0_types.rs:573-579`). Use it as THE
   wire form, replacing `BlobHash::to_hex`'s bare 32-char hex. Before you land
   it, `rg -n '"to_blob"' v6/` and grep the goldens for the old bare-hex shape;
   a golden that pins the old form is a report-and-stop, not a regeneration.
5. **The equality claim is a receipt, not an assumption.** The goal is that a
   digest from `repo_files_at` (a `GitBlob` oid) and a digest computed from
   bytes name the same content. `GitBlob` and `Blake3` are DIFFERENT value
   spaces: they compare unequal even for identical bytes unless the byte-side
   digest is computed with git blob object semantics (sha over
   `blob <len>\0` + bytes). Measure it, state the result plainly in the PR body,
   and do NOT fudge equality with a normalizing comparison that hides it. If
   extract can only produce `Blake3` from raw bytes today, the PR body says
   exactly that and names what would close the gap.

## Work order, not negotiable

Round 1 of this card surveyed for 12 minutes and wrote NOTHING. Do not repeat
that. The survey is DONE and its results are in this brief. Start editing in
your first few actions, and COMMIT EACH SLICE AS SOON AS IT COMPILES.

Site counts at your base sha, `rg -n 'BlobHash' -g '*.rs'`:

| file | sites |
|---|---|
| `src/types.rs` | 20 |
| `src/lang/ts.rs` | 7 |
| `src/lang/go.rs` | 7 |
| `src/lang/rust.rs` | 6 |
| `src/project.rs` | 5 |
| `src/lang/kotlin.rs` | 4 |
| `src/wire.rs` | 2 |
| `src/scip.rs` | 2 |
| `src/lang/prolog/_0_source.rs` | 2 |
| `src/lang/dl6/_0_source.rs` | 2 |
| `src/shape.rs` | 1 |
| `src/lib.rs` | 1 |

Slices, in this order, each its own commit:

1. `src/types.rs` + `src/shape.rs` + `src/lib.rs`: swap the type, drop `Copy`
   from `DefSite` and `ProjectEdge<F>`, keep `Clone`. Commit.
2. `src/wire.rs` + `src/scip.rs`: the wire rendering moves to `ContentId`'s
   `Display`. Commit.
3. `src/project.rs` + the six `src/lang/**` files: fix the borrow/clone fallout.
   Commit.
4. `Cargo.toml`: delete the `blake3` dep. Commit.
5. The six test files in the exception table, mechanically. Commit.

If a slice will not compile after a genuine attempt, commit the slices that DO
compile, then STOP AND REPORT what blocked you, naming the file and the error.
A partial, committed, honestly-reported result beats another empty lane.

## The ripple you must plan for BEFORE editing: `Copy` is gone

`BlobHash` is `Copy`. `ContentId` holds an `Arc<str>` and is `Clone`, not `Copy`.
Everything that copies a blob today changes shape:

| site | `v6/sprefa-extract/src/types.rs` |
|---|---|
| `DefSite` derives `Clone, Copy` and holds `blob` | `:912-917` |
| `ProjectEdge<F>` derives `Clone, Copy` and holds `dst_blob` | `:748-761` |
| `build_def_index(outputs: &[(BlobHash, &ExtractOutput)])` | `:938` |
| `containing_def_site` returns `DefSite` by value | `:1063-1091` |

Pinned resolution: drop `Copy` from `DefSite` and `ProjectEdge<F>`, keep
`Clone`, and fix each call site by borrowing or cloning explicitly. Do NOT
introduce a `Copy`-able id to preserve the old ergonomics; decision 1 forbids
the new type. Report the count of call sites you touched.

## Files that carry the refs

```
src/types.rs  src/shape.rs  src/lib.rs  src/project.rs  src/wire.rs  src/scip.rs
src/lang/{ts,go,kotlin,rust}.rs  src/lang/prolog/_0_source.rs  src/lang/dl6/_0_source.rs
tests/0_prolog.rs  tests/0_dl6.rs  tests/golden_parity.rs
```

**Test-file exception, granted for this lane only.** These five test files
construct `BlobHash` directly and cannot compile without changing. You MAY edit
exactly these, MECHANICALLY: the type name and its construction, nothing else.
No assertion value, no test name, no oracle comparison changes. Every other
existing test file stays untouched.

| test file | BlobHash sites |
|---|---|
| `tests/golden_parity.rs` | 24 |
| `tests/22_doc_node.rs` | 8 |
| `tests/21_kotlin_type_plane.rs` | 5 |
| `tests/13_flow_join.rs` | 5 |
| `tests/0_dl6.rs` | 3 |
| `tests/0_prolog.rs` | 2 |

`v6/sprefa-extract/src/project.rs` is YOURS this round. READ THE MERGED FILE
FIRST: PRs #309 and #310 reworked reader plumbing in it, so any line number from
an older doc is stale.

## Receipts your PR body must carry

1. `rg -n 'BlobHash' v6/ --include=*.rs` before (a count) and after (the hits,
   which must be `v6/sprefa-seed/` prose only).
2. `rg -n 'blake3' v6/sprefa-extract/` after, proving the dep is gone from
   `Cargo.toml` and no direct call remains.
3. `cargo tree -d` before and after; a NEW duplicate group is a
   report-and-stop.
4. The equality measurement from decision 5, stated as a fact with its command.
5. The `Copy` call-site count.
6. Both full test runs (below) with per-binary counts.
7. `cargo build -p sprefa-engine-rs` rc, run from `v6/sprefa-engine-rs`: that
   crate consumes extract's types and must still compile.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cd <your worktree>/v6/sprefa-engine-rs
cargo build; echo "ENGINE rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. A leg that differs between runs is a report-and-stop.

## File ownership

OWNS: every file listed under "Files that carry the refs", plus
`v6/sprefa-extract/Cargo.toml` and `Cargo.lock`.

FORBIDDEN, do not open to edit:
- `v6/sprefa-seed/**` (frozen spec text)
- `v6/sprefa-engine-rs/src/hosts.rs` (build against it, never edit it)
- every EXISTING test file except the three named in the exception
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- `v6/tsv2/goldens/scip_combo/**`
- `~/projects/hafley-rs/**` (soopy is a dependency, not your tree)

## Laws that bind you

- Never spawn a subagent.
- **Bare `cargo fmt` is banned.** It reformats files you do not own. Only
  `cargo fmt -- <your owned files>`, named explicitly.
- Build-vs-buy: you are ADOPTING a library type. Writing a bespoke id type is
  the banned move here, in every form.
- Every public type lives in `types.rs`, the canonical type module.
- Comment budget: comments state only constraints the code cannot show. No
  change-log narrative, no dates, no arc or commit references.
- Identifiers are descriptive, never single-letter.
- No em dashes. Banned in prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" is banned in prose; say TODO or not built yet.
- No `eprintln!` under `src/**`; `tracing` only.
- **The pre-commit rail.** `.githooks/pre-commit` runs
  `v6/tsv2/scripts/comment-budget-rail.sh`: MORE THAN 2 CONSECUTIVE COMMENT
  LINES in a staged hunk FAILS the commit. The waiver is one line
  `// @comment-ok: <one-line reason>` at the end of the offending run; see the
  two in `v6/sprefa-extract/src/types.rs`. Never `git commit -n`, never edit the
  hook or the rail script, never disable it.
- Commit in slices (type swap, Copy fallout, wire rendering, dep removal, test
  mechanical fixes). Use `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe
  a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `refactor/soopy-contentid-adoption` in your worktree.
2. Commit in slices.
3. `git push -u origin refactor/soopy-contentid-adoption`
4. `gh pr create` with a body carrying every receipt above.
5. The PR body ends with the trailer line: `Refs-Issue: soopy-contentid-adoption`
6. NEVER merge the PR. Report the URL and stop.
