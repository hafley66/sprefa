# Lane brief: df aux, the `df_field` and `df_lit` rows on four langs (issue extract-df-aux-fields-lits)

First action: `git merge --ff-only ebe3a141d2a5c049bb35df8f7cee1bd2648401aa`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Do not ask a question before starting. Everything you need is below with a
`path:line`. If a receipt does not match what you read, say which one and stop.

## The gap, with receipts

`DfFAux` carries `params` and `args` only (`v6/sprefa-extract/src/types.rs:557-562`).
v5's dataflow family also carries `df_field` (named value flow into a composite:
struct-literal field, object-literal property, Kotlin named argument) and
`df_lit` (one row per string-carrying df node, kind lit/template/concat).

| fact | receipt |
|---|---|
| v5 rel docs | `src/engine/family/mod.rs:455-474` (REPO ROOT, read it) |
| v5 roster | `src/engine/family/mod.rs:475-491` |
| v5 row payloads | `src/graph/typegraph/mod.rs:370` (`fields: Vec<(NodeIdx, String, NodeIdx)>`) and `:379` (`lits: Vec<(NodeIdx, String, &'static str)>`) with the doc block at `:371-378` |
| the oracle line format | `examples/v5_normalize.rs:155` (`df_fields\t{node_idx}\t{field}\t{val_idx}`) and `:158` (`df_lits\t{node_idx}\t{kind}\t{text}`) |
| v6 deferral notes to correct | `lang/ts.rs:1828`, `lang/ts.rs:1860-1861`, `lang/rust.rs:23`, `lang/go.rs:19-20`, `lang/kotlin.rs:27-28` |
| the status table row to flip | `v6/sprefa-extract/src/types.rs:1869` (`df aux (fields/lits/loops/nests)`) ; flip fields/lits only, loops/nests stay deferred |

## The oracle rows you are matching, verbatim, all 16

```
go/docs.v5.jsonl      df_fields  7   name    8
go/sample.v5.jsonl    df_fields  7   name    8
kotlin/sample.v5.jsonl df_fields 13  host    11
kotlin/sample.v5.jsonl df_fields 13  port    12
rust/docs.v5.jsonl    df_fields  8   name    7
rust/sample.v5.jsonl  df_fields  8   name    7
ts/consts.v5.jsonl    df_fields  2   x       0
ts/consts.v5.jsonl    df_fields  2   y       1
ts/consts.v5.jsonl    df_lits    0   lit     /inner/x
ts/consts.v5.jsonl    df_lits    1   lit     /inner/y
ts/docs.v5.jsonl      df_lits    20  concat  this.x * this.x + this.y * this.y
ts/docs.v5.jsonl      df_lits    5   concat  left + right
ts/lambdas.v5.jsonl   df_lits    28  concat  acc + value
ts/lambdas.v5.jsonl   df_lits    39  concat  outer + 1
ts/sample.v5.jsonl    df_lits    30  concat  this.x * this.x + this.y * this.y
ts/sample.v5.jsonl    df_lits    5   concat  left + right
```

Regenerate that list yourself with
`grep -hE '^(df_fields|df_lits)' v6/sprefa-extract/tests/fixtures/*/*.v5.jsonl`.

Read those rows carefully. Three facts they pin:

1. `concat` is SYNTACTIC, never a type judgment: `outer + 1` is numeric and v5
   still emits `concat` with the RAW source slice of the whole binary
   expression, nested operators intact.
2. `lit` rows are STRING literals only. `/inner/x` is a string; no numeric,
   boolean, null or regexp literal produced a row, though v6 pushes a `lit` df
   NODE for all of them (`lang/ts.rs:1830-1834`).
3. The first column is v5's internal node INDEX. v6 has no such index and will
   never reproduce it, so your parity test compares the PAYLOAD columns only.
   That is stated in the test section below; do not invent an index.

## The pinned design. Every judgment call below is already made.

### 1. Types, `v6/sprefa-extract/src/types.rs`, beside `DfArg` (`:548-554`)

```rust
/// Named value flow into a composite: a struct-literal field, an object-literal
/// property, a Kotlin named argument. `composite` is the node the value lands
/// in, `value` the node it comes from.
pub struct DfField {
    pub composite: NodeRef,
    pub name: NameId,
    pub value: NodeRef,
}

/// The string payload of a string-carrying df node.
pub struct DfLit {
    pub node: NodeRef,
    pub kind: DfLitKind,
    pub text: NameId,
}

/// `Lit` carries the COOKED literal value; `Template` and `Concat` carry the
/// RAW source slice, holes and operators intact.
pub enum DfLitKind { Lit, Template, Concat }
```

`as_str` on `DfLitKind` returns `lit`, `template`, `concat`, matching the
`DfEdgeKind::as_str` shape at `:636-642`. Derive exactly what `DfArg` derives.
Add `fields: Vec<DfField>` and `lits: Vec<DfLit>` to `DfFAux`. Every string is a
`NameId` through the interner; no `String` on a `types.rs` row.

### 2. Wire, `v6/sprefa-extract/src/wire.rs`, in `flatten_df` (`:252`)

Two `FlatFact` arms, in the `DfArg` / `DfParam` style (`src/types.rs`, search
`#[serde(rename = "arg")]`), with explicit renames:

```
#[serde(rename = "df_field")] DfFieldOut { family, composite: SpanOut, name: String, value: SpanOut }
#[serde(rename = "df_lit")]   DfLitOut   { family, node: SpanOut, kind: String, text: String }
```

`NodeRef` flattens to the node's span exactly as the existing arg/param loops do.
Update the `Vec::with_capacity` in `flatten_df` if it has one.

### 3. Schema, `v6/sprefa-extract/src/schema.rs`, after `record=arg` (`:27`)

```
  record=df_field  family=df                 composite={start,end}  name=<string>  value={start,end}
  record=df_lit    family=df                 node={start,end}   kind=<lit|template|concat>  text=<string>
```

Plus one KIND VOCABULARIES line in the block at `:110-122`:

```
  df lit kind  lit (cooked string literal) | template | concat (raw source slice)
```

Plus a `text` / `composite` entry in the FIELDS block if the existing wording
does not already cover them.

### 4. Per-lang emission. Same walk that builds the node, never a second pass.

| lang | `df_field` rule | `df_lit` rule |
|---|---|---|
| ts (`src/lang/ts.rs`) | at the ObjectExpression arm (`:1860-1875`), one row per named property: composite = the `new` node, name = the property key, value = the value node | at the literal arm (`:1828-1834`) one `Lit` row for `E::StringLiteral` ONLY, text = the cooked value; at the BinaryExpression arm (`:1946-1957`) a `Concat` row when the operator is Addition, text = the raw source slice of the whole binary expression; at both TemplateLiteral arms (`:2084-2100`) a `Template` row, text = the raw source slice |
| rust (`src/lang/rust.rs`) | at the struct-literal walk, one row per named field | one `Lit` row per `syn::Lit::Str`, text = `lit.value()` (cooked). Rust emits NO template and NO concat rows: `project_df` (`:1216`) has no source bytes, and the v5 oracle has zero rust `df_lits` rows. State that in the header, do not change the signature |
| go (`src/lang/go.rs`) | in `go_flow_literal_fields` (`:1299`), one row per `keyed_element` whose key is a field name; `literal_element` (positional) produces none | one `Lit` row per string literal; `Concat` for a `+` binary expression, raw slice from `src` |
| kotlin (`src/lang/kotlin.rs`) | one row per NAMED argument (`value_argument` with a name), which is what the oracle's `host`/`port` rows are | one `Lit` row per string literal; `Concat` for a `+` binary expression, raw slice from `src` |

The ts source text is on `DfProjector(&'a str)` (`lang/ts.rs:1373`); go and kotlin
carry `src: &[u8]` through the walk. If a lang genuinely lacks the bytes it
needs, STOP AND REPORT with the signature; do NOT thread a new parameter through
half the file on your own judgment.

The go and kotlin `Concat` rows have no oracle row to match (their fixtures
contain no `+` expression), so they are graded by your own test only. Say so in
the PR body.

### 5. Correct the deferral prose in the five headers listed in the receipts
table. Fields and lits are done; loops and nests stay deferred. No dates, no
change-log narrative.

### 6. The TWO authorized edits to an existing test file

`tests/golden_parity.rs` has an EXHAUSTIVE `match fact` over `FlatFact` with no
catch-all arm (`:168-303`). Your two new variants break its compilation.

**6a.** Add the two arms beside `FlatFact::DfParam { .. } | FlatFact::DfArg { .. } => {}`
(`:207`), so the line reads

```rust
            FlatFact::DfParam { .. }
            | FlatFact::DfArg { .. }
            | FlatFact::DfFieldOut { .. }
            | FlatFact::DfLitOut { .. } => {}
```

**6b.** Add a trailing catch-all as the LAST arm of that same match, after the
final `FlatFact::ScipSkipRow { .. } => {}`:

```rust
            // Every remaining wire row is either phase-2 or an opt-in mode; the
            // v5 oracle carries no facet for any of them.
            _ => {}
```

This is a one-line chore the coordinator authorized so later cards stop paying
the compile tax for each new wire variant. Keep 6a anyway: an explicit arm is
the honest spelling for a row this normalize deliberately drops.

No other change in that file: no new test, no assertion edit, no `PORTED`
change, no other comment rewrite. Say in the PR body that both edits were made
under an explicit exception, that df aux stays DEFERRED in that golden, and that
6b is the approved catch-all chore.

## The test, a NEW file `v6/sprefa-extract/tests/18_df_aux_fields_lits.rs`

`v6/sprefa-extract/tests/golden_parity.rs` is FORBIDDEN to you (another lane owns
the existing test files). Its `PORTED` flip is a named follow-up in your PR body.

Your test `include_str!`s the same fixtures and oracle baselines
`golden_parity.rs:52-110` registers (ts sample/consts/docs/lambdas, go
docs/sample, rust docs/sample, kotlin sample) and, per case:

1. builds v6's rows through `dispatch` + `flatten`;
2. asserts the MULTISET of `(name)` from `record=df_field` equals the multiset of
   the oracle's `df_fields` column 3;
3. asserts the MULTISET of `(kind, text)` from `record=df_lit` equals the
   multiset of the oracle's `df_lits` columns 3 and 4;
4. asserts every `df_field.composite` span is the span of a df node whose kind is
   `new`, and every `df_lit.node` span is the span of a df node whose kind is
   `lit`, `template` or `concat`;
5. names the fixture in every failure message, with both sides printed.

Also assert, for one go and one kotlin fixture you write yourself as a new file
under `tests/fixtures/df_aux/` (that directory already exists), a `Concat` row
with the exact raw slice.

**Fail-first receipt, required.** Write the test first, run it, paste the red
output showing the 16 oracle rows with zero v6 rows into the commit body. Then
land the emission and paste the green.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 18_df_aux_fields_lits; echo "NEW rc=$?"
cargo test --features cli --test 2_df_aux_cli; echo "DFAUX rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. `golden_parity` and `2_df_aux_cli` must stay green untouched:
if a new record tag makes either red, that is a report-and-stop, not a fixup.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/types.rs`
- `v6/sprefa-extract/src/wire.rs`
- `v6/sprefa-extract/src/schema.rs`
- `v6/sprefa-extract/src/lang/ts.rs`, `lang/rust.rs`, `lang/go.rs`, `lang/kotlin.rs`
- `v6/sprefa-extract/src/lib.rs` (re-export lines only)
- `v6/sprefa-extract/tests/18_df_aux_fields_lits.rs` (new)
- new fixture files under `v6/sprefa-extract/tests/fixtures/df_aux/` (new files only)

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/src/lang/dl6/**`, `lang/prolog/**`, `lang/markdown/**`,
  `lang/astgrep.rs`, `lang/python/**`
- every EXISTING file under `v6/sprefa-extract/tests/` (new test files only), with
  the ONE authorized arm in `tests/golden_parity.rs` as the sole exception
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- `v6/sprefa-engine-rs/src/hosts.rs`, `v6/tsv2/goldens/scip_combo/**`
- everything outside `v6/sprefa-extract/`

## Laws that bind you

- Never spawn a subagent.
- Every public type lives in `types.rs`, the canonical type module.
- Comment budget: comments state only constraints the code cannot show. No
  change-log narrative, no dates, no arc or commit references.
- Identifiers are descriptive, never single-letter.
- No em dashes. Banned in prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" is banned in prose; say TODO or not built yet.
- No `eprintln!` under `src/**`; `tracing` only.
- N+1 law: collect the set, one push loop; no per-row re-scan of the node vec.
- **The pre-commit rail.** `.githooks/pre-commit` runs
  `v6/tsv2/scripts/comment-budget-rail.sh`: MORE THAN 2 CONSECUTIVE COMMENT
  LINES in a staged hunk FAILS the commit. The waiver is one line
  `// @comment-ok: <one-line reason>` at the end of the offending run; see the
  two in `v6/sprefa-extract/src/types.rs`. Never `git commit -n`, never edit the
  hook or the rail script, never disable it.
- Commit in slices (types+wire+schema, then one commit per lang, then the test).
  Use `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `feature/extract-df-aux-fields-lits` in your worktree.
2. Commit in slices.
3. `git push -u origin feature/extract-df-aux-fields-lits`
4. `gh pr create` with a body carrying: the 16 oracle rows, the per-lang rule
   table as implemented, the fail-first red and the green, both gate runs with
   per-binary counts, and a `Follow-up` heading naming the `golden_parity.rs`
   `PORTED` flip and the go/kotlin concat rows that have no oracle.
5. The PR body ends with the trailer line: `Refs-Issue: extract-df-aux-fields-lits`
6. NEVER merge the PR. Report the URL and stop.
