# Lane brief: docs facet, the shape plus the rust arm (issue extract-docs-facet-shape)

First action: `git merge --ff-only 988e2b514204735869ce2964008bdbea8ad91bc8`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Do not ask a question before starting. Everything you need is stated below with
a `path:line`. If a receipt does not match what you read, say which one and stop.

## The gap, and the receipts that already grade it

v5 binds a doc-comment block to each declared entity. v6 emits nothing.

**The v5 oracle baselines already carry the rows.** They are committed and
byte-comparable today:

| baseline | `doc` rows |
|---|---|
| `v6/sprefa-extract/tests/fixtures/rust/docs.v5.jsonl` | 5 |
| `v6/sprefa-extract/tests/fixtures/go/docs.v5.jsonl` | 6 |
| `v6/sprefa-extract/tests/fixtures/ts/docs.v5.jsonl` | 8 |

Verify with `grep -c '^doc\t' <file>`.

All three fixtures are ALREADY registered cases in the parity test:
`v6/sprefa-extract/tests/golden_parity.rs:66-69` (ts docs), `:87-90` (rust docs),
`:101-104` (go docs). The only reason the rows are not asserted is that `"doc"`
is absent from the `PORTED` list at `golden_parity.rs:123-131`.

**The exact oracle row, verbatim from `fixtures/rust/docs.v5.jsonl`:**

```
doc	v6/sprefa-extract/tests/fixtures/rust/docs.rs::enum::Mode	16
doc	v6/sprefa-extract/tests/fixtures/rust/docs.rs::function::make_engine	30
doc	v6/sprefa-extract/tests/fixtures/rust/docs.rs::function::trim	25
doc	v6/sprefa-extract/tests/fixtures/rust/docs.rs::method::Engine.mode	38
doc	v6/sprefa-extract/tests/fixtures/rust/docs.rs::struct::Engine	11
```

Tab-separated. Three fields: the literal `doc`, then `<path>::<kind>::<name>`,
then the 1-based line. Note that this facet carries the PATH, unlike `type_node`
which does not (`type_node\tstruct\tEngine\t11`). Note the method spelling
`Engine.mode`: owner type, a dot, the method name.

Other receipts:

| thing | file:line |
|---|---|
| v5 `DocFact { sym, line, text, tags }` | `src/graph/typegraph/mod.rs:161-166` |
| v5 `DocTag { tag, arg, text }` | `src/graph/typegraph/mod.rs:168-177` |
| v5 rust walker, THE thing you port | `src/graph/typegraph/rust/mod.rs:519` `rust_docs_from`, called at `:36` and `:72` |
| v6 `TypeFAux` you extend | `v6/sprefa-extract/src/types.rs:328-332` |
| `ConstValue`, the nearest existing twin | search `pub struct ConstValue` in `v6/sprefa-extract/src/types.rs` |
| how a const flattens, copy this shape | `v6/sprefa-extract/src/wire.rs:120-128` |
| the SCHEMA line to copy | `v6/sprefa-extract/src/schema.rs:32` |
| the v6 normalize block you extend | `v6/sprefa-extract/tests/golden_parity.rs:145-230` |
| `line_of`, the 1-based line helper already written | `v6/sprefa-extract/tests/golden_parity.rs:135-142` |

## Scope

The SHAPE plus the RUST arm. The ts, go and kotlin walkers are a separate issue
and a separate lane. Do NOT write them, and do NOT add `"doc"` to `PORTED` in a
way that makes the ts and go docs cases red; see step 6.

## Exact fix, six steps in this order

### 1. Types, in `v6/sprefa-extract/src/types.rs`

In the TYPE plane section, beside `ConstValue`:

```rust
/// A doc block bound to a declared entity. `owner` is the entity node's span,
/// the join key; v6 identity is the span, never v5's `sym` string.
pub struct DocFact {
    pub owner: Span,
    pub text: NameId,
    pub tags: Vec<DocTag>,
}

/// One structured doc tag. `tag` is the bare tag word (param, returns,
/// deprecated, throws, or section for a rustdoc `# Heading`); `arg` is the name
/// a `@param name` carries, None when the tag takes none.
pub struct DocTag {
    pub tag: NameId,
    pub arg: Option<NameId>,
    pub text: NameId,
}
```

Derive exactly what `ConstValue` derives. Add `docs: Vec<DocFact>` to `TypeFAux`.
Every string is a `NameId` through the interner; no `String` on a `types.rs` row.

### 2. Wire, in `v6/sprefa-extract/src/wire.rs`

Two `FlatFact` variants and two flatten loops, right after the const loop at
`:120-128`, same style:

```
FlatFact::Doc     { family, owner: SpanOut, text: String }
FlatFact::DocTag  { family, owner: SpanOut, tag: String, arg: Option<String>, text: String }
```

Update the `Vec::with_capacity` at `wire.rs:100`.

### 3. Schema, in `v6/sprefa-extract/src/schema.rs`

Two lines in RECORD SHAPES right after `record=const` at `:32`, aligned with the
block:

```
  record=doc      family=type  owner={start,end}  text=<string>
  record=doc_tag  family=type  owner={start,end}  tag=<string>  arg=<string|null>  text=<string>
```

### 4. The rust walker, in `v6/sprefa-extract/src/lang/rust.rs`

Port `rust_docs_from` (`src/graph/typegraph/rust/mod.rs:519`). Read the v5
function completely first. Keep its cleaning and tag split byte-identical: the
same marker strip, the same blank-line handling, the same `# Heading` to
`tag=section` mapping (`src/graph/typegraph/mod.rs:168-171`).

Emit into `TypeFAux.docs` from the walk that ALREADY builds the type entities.
Never add a second traversal of the syn tree. The owner span is the entity node
span the existing walk already computed.

Update the header at `v6/sprefa-extract/src/lang/rust.rs:23-24`, which lists the
docs facet as deferred.

### 5. The normalize arm, in `v6/sprefa-extract/tests/golden_parity.rs`

Add a `FlatFact::Doc` arm to `v6_ported` (`:145-230`) producing exactly the
oracle line:

```rust
set.insert(format!(
    "doc\t{path}::{kind}::{name}\t{}",
    line_of(bytes, owner.start)
));
```

`path` is the `path` parameter `v6_ported` already receives. `kind` and `name`
come from the TypeF node whose span equals `owner`; look it up in the same
`flatten` output, the way the existing arms use what they have. Method names are
`Owner.method`, matching the oracle. `FlatFact::DocTag` gets an empty arm with a
comment saying the oracle drops tags.

### 6. Flip `PORTED`, carefully

Add `"doc"` to `PORTED` (`:123-131`) ONLY IF the ts and go docs cases still pass.
They will not, because their arms are not written: `ported_facets_match_v5`
iterates every case and `PORTED` is global.

So do this instead: leave `PORTED` alone and add a SECOND test,
`rust_doc_parity`, that runs the same set-difference as
`ported_facets_match_v5:284-320` for the single case named `"rust_docs"` and the
single facet `"doc"`. Copy that function body and narrow it. State in a comment
that the global flip lands when the other three arms do.

Also update the DEFERRED note at `golden_parity.rs:20` to say rust is asserted
and ts/go/kotlin are the follow-up.

**Fail-first receipt, required.** Write step 5 and step 6 BEFORE step 4, run the
new test, paste the red output into the commit body. It must show 5 rows only in
v5. Then land the walker and paste the green.

## Gate, run each twice, read rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

`cargo build` ALWAYS runs before any binary gate. Baseline at the base sha is
rc=0 with every leg green. `ported_facets_match_v5` and the three
`type_edge_resolve_parity_*` legs pass today and must still pass.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/types.rs`
- `v6/sprefa-extract/src/wire.rs`
- `v6/sprefa-extract/src/schema.rs`
- `v6/sprefa-extract/src/lang/rust.rs`
- `v6/sprefa-extract/src/lib.rs` (re-export lines only)
- `v6/sprefa-extract/tests/golden_parity.rs`

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/src/scip*.rs`
- `v6/sprefa-extract/src/lang/ts.rs`, `lang/go.rs`, `lang/kotlin.rs`,
  `lang/dl6/**`, `lang/prolog/**`, `lang/markdown/**`, `lang/astgrep.rs`
- `v6/sprefa-extract/src/bin/extract.rs`
- every `.v5.jsonl` baseline (they are the oracle; you never regenerate one)
- `v6/sprefa-extract/tests/1_resolve_cli.rs`, `tests/8_scip_families_cli.rs`
- `v6/sprefa-engine-rs/**`, `v6/tsv2/**`, `v6/prolog/**`
- everything outside `v6/sprefa-extract/`

Three concurrent lanes own the forbidden files.

## Laws that bind you

- Never spawn a subagent.
- Every new type lives in `types.rs`, the canonical type module.
- Comment budget: comments state constraints the code cannot show. No change-log
  narrative, no dates, no arc references.
- No em dashes. Banned in prose and identifiers: provenance, substrate,
  load-bearing, regime.
- Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done.
- Do not push. Do not open a PR. Do not merge.
