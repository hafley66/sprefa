# Lane brief: the `unresolved` runtime-computed edge markers, TS/JS (issue extract-unresolved-markers)

First action: `git merge --ff-only 2a9759272ebaccea3dd99f8e381313fbe498fc36`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

That base sha is the head of the sibling branch `feature/extract-docs-facet-lang-arms`,
whose PR is posted and not yet merged. Its four commits touch `src/lang/ts.rs`
(the doc walker) and will appear in your branch's history. Leave them alone:
never revert, amend, squash or reformat them, and never re-run their tests as
yours. Your PR targets `main` as usual.

Do not ask a question before starting. Everything you need is below with a
`path:line`. Line numbers are from the base sha and may be off by a few lines;
locate by the SYMBOL NAME given and continue. If a symbol is absent or the code
contradicts the receipt, say which one and stop.

## The gap

v5 emits a marker row wherever an edge EXISTS but its target is computed at
runtime. v6 emits nothing, so a dynamic `import(expr)`, an `obj[key]()` call and
a `f(...args)` spread are each silently dropped by walks that already see them.

| fact | receipt |
|---|---|
| v5 rel + its closed vocabulary + the reason each bucket exists | `src/engine/family/mod.rs:552-570` (REPO ROOT) |
| v5 emitter, the thing you port, ~70 lines | `src/graph/typegraph/ts/text.rs:234-330` (`UnresolvedRef`, `ts_unresolved_refs`, `TsUnresolvedWalker`) |
| v6 has none | no hit for `UnresolvedRef` or an `unresolved` record under `v6/sprefa-extract/src` |
| the same blind spot from the deps side | `v6/sprefa-extract/src/deps.rs:33-37` |

Scope is TS/JS only, matching v5's v1. A fourth reason needs its own issue.

## The pinned design. Every judgment call below is already made.

### 1. Types, `v6/sprefa-extract/src/types.rs`, beside `Specifier` in the CallF section

```rust
/// An edge that exists in the source but whose target is computed at runtime.
/// `span` is the computed expression itself, so `detail` is exactly the source
/// text at `span` and the enclosing statement is recoverable by containment.
pub struct Unresolved {
    pub span: Span,
    pub reason: UnresolvedReason,
    pub detail: NameId,
}

/// The closed v5 vocabulary (`src/engine/family/mod.rs:552-570`). A fourth
/// reason needs its own issue, never a silent addition.
pub enum UnresolvedReason { DynamicImport, ComputedMemberCall, SpreadCallArgs }
```

`as_str` returns `dynamic-import`, `computed-member-call`, `spread-call-args`,
matching the `SpecifierKind::as_str` shape (`src/types.rs:493-504`). Derive
exactly what `Specifier` derives. Add `unresolved: Vec<Unresolved>` to
`CallFAux` (`src/types.rs:516-523`).

Note the SPAN choice differs from v5, which recorded a 1-based line for the
enclosing expression and a detail slice from a sub-span. v6 is byte-span native
and every other row keys on the span of the thing it describes; the span IS the
detail's span. State that in the type's doc comment, one line, no history.

### 2. Wire, `v6/sprefa-extract/src/wire.rs`, in `flatten_call` (`:155`)

One `FlatFact` arm, in the `Specifier` style:

```
#[serde(rename = "unresolved")] Unresolved { family, span: SpanOut, reason: String, detail: String }
```

### 3. Schema, `v6/sprefa-extract/src/schema.rs`, after `record=specifier` (`:33`)

```
  record=unresolved  family=call            span={start,end}   reason=<slug>  detail=<string>
```

Plus a FIELDS entry for `reason` and `detail`, and one KIND VOCABULARIES line:

```
  unresolved reason  dynamic-import | computed-member-call | spread-call-args
```

### 4. Emission, `v6/sprefa-extract/src/lang/ts.rs`

Port `TsUnresolvedWalker` (`src/graph/typegraph/ts/text.rs:273-330`) as an
`oxc_ast_visit::Visit` walker inside the CallF projection, run over the SAME
`Program` the projector already holds. No second parse. Its four rules, verbatim
from v5, each with the v5 comment's constraint preserved in one line:

1. `visit_import_expression`: a source that is not a `StringLiteral` gives
   `dynamic-import`, span/detail = the SOURCE expression's span.
2. `visit_call_expression`, bare `require` callee whose first argument is not a
   `StringLiteral`: `dynamic-import`, span/detail = that argument's span.
3. `visit_call_expression`, `ComputedMemberExpression` callee:
   `computed-member-call`, span/detail = the member expression's span.
4. `visit_call_expression`, every `Argument::SpreadElement`: `spread-call-args`,
   span/detail = the spread element's span.

`CallProjector` (`src/lang/ts.rs:838`) is a unit struct and has no source text,
which the detail slice needs. Change it to carry the text exactly as
`DfProjector` does (`src/lang/ts.rs:1373`: `pub struct DfProjector<'a>(pub &'a str)`),
and update its construction in `TsSource::extract`. The only other mention in
the tree is the re-export at `src/lang/mod.rs:29`, which does not change; verify
that with `rg -n CallProjector` before and after and paste both.

### 5. `tests/golden_parity.rs` needs NO edit

Its `match fact` over `FlatFact` ends in `#[allow(unreachable_patterns)] _ => {}`
(`tests/golden_parity.rs:340-343` at your base sha), so a new `FlatFact` variant
compiles there untouched and is never asserted. That file is FORBIDDEN with no
exception. If it fails to compile after your variant lands, report and stop
rather than editing it.

## The test, a NEW file `v6/sprefa-extract/tests/20_unresolved.rs`

New fixture `v6/sprefa-extract/tests/fixtures/ts/unresolved.ts` (new file), under
30 lines, covering all four rules plus their negatives: a STATIC `import("./x")`,
a `require("./y")` with a literal, a plain `obj.method()`, and a call with only
ordinary arguments. Assert:

1. the exact set of `(reason, detail, span)` rows, every value a hand-derived
   literal;
2. zero rows from the negatives;
3. the flattened JSONL line of one row, byte for byte;
4. `--schema` text contains the new record line (read `SCHEMA` from the library,
   no subprocess).

**Fail-first receipt, required.** Write the test first, run it, paste the red
output into the commit body. Then land the walker and paste the green.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 20_unresolved; echo "UNRES rc=$?"
cargo test --features cli --test 7_diet_deps_cli; echo "DEPS rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. `unresolved` is a v6-only facet with no oracle row, so
`golden_parity` must stay green untouched; if it goes red, report and stop.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/types.rs`
- `v6/sprefa-extract/src/wire.rs`
- `v6/sprefa-extract/src/schema.rs`
- `v6/sprefa-extract/src/lang/ts.rs`
- `v6/sprefa-extract/src/lib.rs` (re-export lines only)
- `v6/sprefa-extract/tests/20_unresolved.rs` (new)
- `v6/sprefa-extract/tests/fixtures/ts/unresolved.ts` (new)

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`, `src/deps.rs`
- every other `src/lang/**` file
- every EXISTING file under `v6/sprefa-extract/tests/` (new test files only);
  `tests/golden_parity.rs` has NO exception
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- `v6/sprefa-engine-rs/src/hosts.rs`, `v6/tsv2/goldens/scip_combo/**`
- everything outside `v6/sprefa-extract/`

## Laws that bind you

- Never spawn a subagent.
- **Bare `cargo fmt` is banned.** It reformats files you do not own. Only
  `cargo fmt -- <your owned files>`, named explicitly.
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
- Commit in slices (types+wire+schema, walker, test). Use
  `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `feature/extract-unresolved-markers` in your worktree.
2. Commit in slices.
3. `git push -u origin feature/extract-unresolved-markers`
4. `gh pr create` with a body carrying: the four rules as implemented, the
   fixture's expected row table, the fail-first red and the green, both gate
   runs with per-binary counts, the `rg -n CallProjector` before/after, and a
   `Follow-up` heading for any non-TS lang.
5. The PR body ends with the trailer line: `Refs-Issue: extract-unresolved-markers`
6. NEVER merge the PR. Report the URL and stop.
