# Lane brief: the kotlin type plane, candidates plus `Resolve<TypeF>` (issue extract-kotlin-type-plane)

First action: `git merge --ff-only dab50a2df37cdbd9b0db2481a9e7e4ee10f71af6`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

That base sha is the head of the sibling branch `feature/extract-unresolved-markers`,
whose PR is posted and not yet merged. Its three commits touch `src/types.rs`,
`src/wire.rs`, `src/schema.rs` and `src/lang/ts.rs` and will appear in your
branch's history. Leave them alone: never revert, amend, squash or reformat
them, and never re-run their tests as yours. Your PR targets `main` as usual.

Do not ask a question before starting. Everything you need is below with a
`path:line`. Line numbers are from the base sha and may be off by a few lines;
locate by the SYMBOL NAME given and continue. If a symbol is absent or the code
contradicts the receipt, say which one and stop.

## The gap

Kotlin is the only roster language with a `Resolve<CallF>` arm and no
`Resolve<TypeF>` arm, and it emits no type-edge candidates, though v5's kotlin
front-end DOES emit `type_edge`.

| fact | receipt |
|---|---|
| kotlin has CallF resolve only | `v6/sprefa-extract/src/lang/kotlin.rs:1145`; no `impl Resolve<TypeF>` in the file |
| kotlin imports no `TypeEdgeCandidate` | `v6/sprefa-extract/src/lang/kotlin.rs:39-42` (`go.rs:31-33` and `rust.rs:36-40` both do) |
| the deferral note to correct | `v6/sprefa-extract/src/lang/kotlin.rs:27-31` |
| the status table row to flip | `v6/sprefa-extract/src/types.rs:2174`, the `type_edge (field/impl/variant/uses/generic)` line, whose tail reads `kotlin DEFERRED to the traits/codegen arc`. The NEXT line (`:2175`, `resolved caller -> callee`) is a DIFFERENT deferral (the scip-kotlin ratchet) and does NOT flip. |
| v5 source, at the REPO ROOT | `src/graph/typegraph/kotlin.rs`, `kotlin_decl_edges` |
| the type dispatch that skips kotlin | `v6/sprefa-extract/src/project.rs`, the `RESOLVE_ARMS` table (`types: None` on the kotlin row) and the comment above it |

## The oracle you are matching: 10 rows, verbatim

`v6/sprefa-extract/tests/fixtures/kotlin/sample.v5.jsonl`:

```
type_edge	Color	Color::GREEN	variant
type_edge	Color	Color::RED	variant
type_edge	Repo	Base	impl
type_edge	Repo	Cache	field
type_edge	Repo	Entity	generic
type_edge	Repo	Item	field
type_edge	Repo	Meta	field
type_edge	Repo	Pricing	impl
type_edge	Repo	Store	field
type_edge	Single	Pricing	impl
```

Tab separated: `type_edge`, owner entity name, target as written, kind. Four
kinds only: `field`, `impl`, `variant`, `generic`. A row v6 emits that v5 does
not is as much a failure as a missing one.

## The pinned shape: mirror the go arm exactly

Go is the closest twin (same front-end family, same byte-offset span story).
Read these four go anchors whole before writing:

| step | `src/lang/go.rs` |
|---|---|
| 1. phase-1 candidates on `TypeFAux.candidates` | `:177-274` (`go_edge_candidates`, `push_candidate`) |
| 2. the candidate accessor the parity test zips against | `:1501-1520` (`GoSource::type_edge_candidates`) |
| 3. the resolve helper | `:1521-1543` (`resolve_type_dst`) |
| 4. `impl Resolve<TypeF>` | `:1544-1613` |

`TypeEdgeCandidate` is `{ owner: Span, to: NameId, kind: TypeEdgeKind }`
(`src/types.rs:319-323`). No new type is needed anywhere: `types.rs` changes are
limited to the ONE status-table line at `:2174`.

The zip discipline the parity tests depend on: `Resolve::<TypeF>::resolve`
returns edges 1:1 IN ORDER with `KotlinSource::type_edge_candidates(out)`. Break
that and the test cannot attribute a row. Preserve it exactly as go does.

## project.rs

`project.rs` is YOURS this round (the lane that owned it landed; PRs #309 and
#310 are merged). READ THE MERGED FILE FIRST: #309 reworked reader plumbing in
it, so the card's pre-merge line numbers are stale. Two edits, no more:

1. the kotlin `ResolveArm` row: `types: Some(|out, cx| Resolve::<TypeF>::resolve(&KotlinSource, out, cx))`;
2. delete the now-false clause in the `ResolveArms.types` doc comment that says
   Kotlin has no arm.

While you are in there, TWO STALE COMMENTS from a landed sibling PR need the
same pass: both `project.rs` comments claiming "the trait's default body is
`todo!()`" (one on `ResolveArms.types`, one above `pub struct ResolveArm`). The
default body was deleted; `Resolve` is non-defaulted and a missing arm is now a
compile error. Correct both to say that. Find them with
`rg -n 'todo!' src/project.rs`.

## The test, a NEW file `v6/sprefa-extract/tests/21_kotlin_type_plane.rs`

`tests/golden_parity.rs` is FORBIDDEN to edit. Its `type_edge_resolve_parity_go`
(`golden_parity.rs:511`) is the function you MIRROR into your new file, for
the kotlin case. Test binaries share no code, so copy what you need:
`with_resolve_cx` (`golden_parity.rs:404`), `owner_name` (`:434`),
`facet_of` (`:146`), and a one-entry `Case` table for
`tests/fixtures/kotlin/sample.kt` + `sample.v5.jsonl`. Adding the kotlin leg to
`golden_parity.rs` itself is a named follow-up in your PR body.

If a candidate's target does not resolve inside the single-file corpus, the zip
lengths diverge; render that loudly (the go test's `ZIP_MISMATCH` row) rather
than filtering it away.

**Fail-first receipt, required.** Write the test first, run it, paste the red
output showing all 10 oracle rows missing from v6. Then land the candidates and
the arm, and paste the green.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 21_kotlin_type_plane; echo "KT rc=$?"
cargo test --features cli --test 1_resolve_cli; echo "RESOLVE rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. `1_resolve_cli` asserts the roster and `RESOLVE_ARMS` agree
both ways; it must stay green.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/lang/kotlin.rs`
- `v6/sprefa-extract/src/project.rs` (the two edits + the two stale comments)
- `v6/sprefa-extract/src/types.rs` (the ONE status-table line at `:2174`)
- `v6/sprefa-extract/tests/21_kotlin_type_plane.rs` (new)

FORBIDDEN, do not open to edit:
- every other `src/lang/**` file
- `src/wire.rs`, `src/schema.rs`
- every EXISTING file under `v6/sprefa-extract/tests/` (new test files only)
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- `v6/sprefa-engine-rs/src/hosts.rs`, `v6/tsv2/goldens/scip_combo/**`
- everything outside `v6/sprefa-extract/`

## Laws that bind you

- Never spawn a subagent.
- **Bare `cargo fmt` is banned.** It reformats files you do not own. Only
  `cargo fmt -- <your owned files>`, named explicitly.
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
- Commit in slices (candidates, resolve arm, dispatch, test). Use
  `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `feature/extract-kotlin-type-plane` in your worktree.
2. Commit in slices.
3. `git push -u origin feature/extract-kotlin-type-plane`
4. `gh pr create` with a body carrying: the 10 oracle rows asserted, the
   fail-first red and the green, both gate runs with per-binary counts, the two
   `project.rs` edits and the two comment corrections quoted, and a `Follow-up`
   heading naming the `golden_parity.rs` consolidation.
5. The PR body ends with the trailer line: `Refs-Issue: extract-kotlin-type-plane`
6. NEVER merge the PR. Report the URL and stop.
