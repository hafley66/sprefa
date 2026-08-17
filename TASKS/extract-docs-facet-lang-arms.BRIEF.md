# Lane brief: the ts, go and kotlin doc walkers (issue extract-docs-facet-lang-arms)

First action: `git merge --ff-only 6c2dc4af3af6b5747fbbd46b2ff08314ef14c426`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Do not ask a question before starting. Everything you need is below with a
`path:line`. If a receipt does not match what you read, say which one and stop.

## The gap

PR #304 landed the docs facet SHAPE plus the RUST walker: `DocFact` /`DocTag`
(`v6/sprefa-extract/src/types.rs:328-345`), `TypeFAux.docs` (`:351`), the two
`FlatFact` arms (`src/wire.rs:131` and `:138`), the two SCHEMA lines
(`src/schema.rs:31-32`), and `doc_facts` in `src/lang/rust.rs:116-230`.

The ts, go and kotlin walkers are missing, so those three langs emit zero doc
rows. You write exactly those three.

## Scope, fixed

**No change to `types.rs`, `wire.rs` or `schema.rs`.** The shape landed. If you
believe a lang needs a field the shape lacks, STOP AND REPORT with the case;
another lane owns those files.

Three walkers, each emitting into `TypeFAux.docs` from the walk that ALREADY
builds the type entities in that file. Never a second traversal.


## Work order, not negotiable

Write the test FIRST, then one lang at a time, committing each slice:

1. `tests/19_docs_lang_arms.rs` with the 8 ts and 6 go oracle rows as literals; run it; paste the red.
2. The ts arm in `src/lang/ts.rs`; the 8 ts rows go green.
3. The go arm in `src/lang/go.rs`; the 6 go rows go green.
4. The kotlin arm plus `tests/fixtures/kotlin/docs.kt`, self-graded (kotlin has ZERO oracle doc rows; say so).

You need exactly four reads: `src/lang/rust.rs:116-230` (the v6 template),
`src/graph/typegraph/ts/mod.rs:1036` (`ts_docs_from`, called at `:51`),
`src/graph/typegraph/go.rs:681` (`walk_go_docs`, recursion `:755`),
`src/graph/typegraph/kotlin.rs:1034` (`walk_kotlin_docs`, recursion `:1076`).
Do not survey further; every other fact you need is in this brief.

Revert `v6/sprefa-extract/Cargo.lock` before committing: that soopy drift is
already on main and only creates conflicts here.

## The v5 walkers you port

| lang | v5 source, at the REPO ROOT | called from |
|---|---|---|
| ts | `src/graph/typegraph/ts/mod.rs:1036` `ts_docs_from` | `:51` |
| go | `src/graph/typegraph/go.rs:681` `walk_go_docs`, recursion at `:755` | `:27` |
| kotlin | `src/graph/typegraph/kotlin.rs:1034` `walk_kotlin_docs`, recursion at `:1076` | `:31` |

The v6 rust arm (`src/lang/rust.rs:123-230`: `doc_facts`, `doc_lines`,
`doc_sections`) is the shape to mirror: read it whole first. The tag split maps
a section heading to `tag=section` (`src/graph/typegraph/mod.rs:168-171`);
jsdoc-style `@param name text` maps to `tag=param arg=name`.

## The oracle rows you are matching, verbatim

`v6/sprefa-extract/tests/fixtures/ts/docs.v5.jsonl`, 8 rows:

```
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::alias::Vec	23
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::class::Vec2	32
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::enum::Dir	26
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::function::add	12
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::function::mirror	50
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::interface::Point	17
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::method::Vec2.magnitude	36
doc	v6/sprefa-extract/tests/fixtures/ts/docs.ts::method::Vec2.scaled	41
```

`v6/sprefa-extract/tests/fixtures/go/docs.v5.jsonl`, 6 rows:

```
doc	v6/sprefa-extract/tests/fixtures/go/docs.go::alias::Mode	22
doc	v6/sprefa-extract/tests/fixtures/go/docs.go::function::MakeEngine	31
doc	v6/sprefa-extract/tests/fixtures/go/docs.go::function::Trim	25
doc	v6/sprefa-extract/tests/fixtures/go/docs.go::interface::Sizer	17
doc	v6/sprefa-extract/tests/fixtures/go/docs.go::method::Engine.Mode	38
doc	v6/sprefa-extract/tests/fixtures/go/docs.go::struct::Engine	12
```

Tab separated: the literal `doc`, then `<path>::<kind>::<name>`, then the
1-based line. A method is `Owner.method`. The oracle drops the doc TEXT and the
tags; only the binding and its line are asserted.

**Kotlin has NO oracle doc rows** (`grep -c '^doc' tests/fixtures/kotlin/sample.v5.jsonl`
is 0). The kotlin walker is therefore graded by your own hand-written
assertions on a NEW fixture, not against v5. Say that plainly in the PR body;
do not claim kotlin parity you did not measure.

## The test, a NEW file `v6/sprefa-extract/tests/19_docs_lang_arms.rs`

`v6/sprefa-extract/tests/golden_parity.rs` is FORBIDDEN to you (another lane
owns the existing test files). Its global `PORTED` flip for `doc` is a named
follow-up in your PR body: after your three arms land, all four langs emit, so
the flip becomes safe. Say exactly that.

Your test:

1. For ts and go: `include_bytes!` the fixture and `include_str!` the oracle
   from `tests/fixtures/{ts,go}/docs.*`, build v6's rows through `dispatch` +
   `flatten`, render each `FlatFact::Doc` into the oracle's line shape, and
   assert set equality with the oracle's `doc` rows. Rendering needs the `kind`
   and `name` of the TypeF node whose span equals the doc's `owner`; the rust
   arm of `golden_parity.rs:239-255` shows that join, and its helper
   `line_of` (`golden_parity.rs:135-142`) is 8 lines you copy into your file
   (test binaries do not share code; copying is correct here).
2. For kotlin: a NEW fixture `tests/fixtures/kotlin/docs.kt` (new file) with a
   KDoc block on a class, a method, a top-level function and a property, and
   literal assertions on the emitted `(owner kind, owner name, line, text)` and
   on at least one `DocTag`.
3. One assertion per lang that a declaration with NO doc comment emits NO row.

**Fail-first receipt, required.** Write the test first, run it, paste the red
output showing 8 ts rows and 6 go rows only in v5. Then land the walkers and
paste the green.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 19_docs_lang_arms; echo "DOCS rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. `golden_parity` stays green: `doc` is filtered out of
`PORTED` on BOTH sides (`golden_parity.rs:323-330`), so emitting new doc rows
cannot make it red. If it does go red, that is a report-and-stop.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/lang/ts.rs`
- `v6/sprefa-extract/src/lang/go.rs`
- `v6/sprefa-extract/src/lang/kotlin.rs`
- `v6/sprefa-extract/tests/19_docs_lang_arms.rs` (new)
- `v6/sprefa-extract/tests/fixtures/kotlin/docs.kt` (new)

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/types.rs`, `src/wire.rs`, `src/schema.rs`,
  `src/project.rs`, `src/lang/rust.rs`
- every other `src/lang/**` file
- every EXISTING file under `v6/sprefa-extract/tests/` (new test files only)
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- `v6/sprefa-engine-rs/src/hosts.rs`, `v6/tsv2/goldens/scip_combo/**`
- everything outside `v6/sprefa-extract/`

Also correct the deferral prose in the three headers you own (`lang/ts.rs`,
`lang/go.rs:19`, `lang/kotlin.rs:27-28`): the docs facet is done, the df
field/lit/loop/nest deferrals stay as written.

## Laws that bind you

- Never spawn a subagent.
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
- Commit one lang per commit, then the test. Use
  `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `feature/extract-docs-facet-lang-arms` in your worktree.
2. Commit in slices.
3. `git push -u origin feature/extract-docs-facet-lang-arms`
4. `gh pr create` with a body carrying: the 14 oracle rows asserted, the kotlin
   no-oracle statement, the fail-first red and the green, both gate runs with
   per-binary counts, and a `Follow-up` heading naming the `PORTED` flip.
5. The PR body ends with the trailer line: `Refs-Issue: extract-docs-facet-lang-arms`
6. NEVER merge the PR. Report the URL and stop.
