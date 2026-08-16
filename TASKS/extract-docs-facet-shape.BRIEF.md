# Lane brief: docs facet, the shape plus the rust arm (issue extract-docs-facet-shape)

First action: `git merge --ff-only 988e2b514204735869ce2964008bdbea8ad91bc8`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

## The gap

v5 binds a cleaned doc-comment block and its structured tags to each declared
entity. v6 emits nothing: there is no `DocFact`, no `doc` record, no wire tag.
This lane lands the SHAPE plus the RUST arm. The ts, go and kotlin walkers are a
separate issue and a separate lane; do not write them.

Read these before editing:

| thing | file:line |
|---|---|
| v5 `DocFact { sym, line, text, tags }` | `src/graph/typegraph/mod.rs:161-166` |
| v5 `DocTag { tag, arg, text }` | `src/graph/typegraph/mod.rs:168-177` |
| v5 rust walker, THE thing you port | `src/graph/typegraph/rust/mod.rs:519` `rust_docs_from`, called at `:36` and `:72` |
| v6 `TypeFAux` you extend | `v6/sprefa-extract/src/types.rs:328-332` |
| v6 `ConstValue`, the nearest existing twin | search `pub struct ConstValue` in `v6/sprefa-extract/src/types.rs` |
| how a const flattens (copy this shape) | `v6/sprefa-extract/src/wire.rs:120-128` |
| the SCHEMA line to copy | `v6/sprefa-extract/src/schema.rs:32` (`record=const`) |
| the status table to flip | `v6/sprefa-extract/src/types.rs:1831` |
| the deferred ledger | `v6/sprefa-extract/tests/golden_parity.rs:22-24` |

## Exact fix

### 1. Types, in `v6/sprefa-extract/src/types.rs`

In the TYPE plane section, beside `ConstValue`:

```rust
/// A doc block bound to a declared entity. `owner` is the entity node's span
/// (the join key); v6 identity is the span, never v5's `sym` string.
pub struct DocFact {
    pub owner: Span,
    pub text: NameId,
    pub tags: Vec<DocTag>,
}

/// One structured doc tag. `tag` is the bare tag word (param, returns,
/// deprecated, throws, or section for a rustdoc `# Heading`); `arg` is the name
/// a `@param name` carries, None when the tag takes no name.
pub struct DocTag {
    pub tag: NameId,
    pub arg: Option<NameId>,
    pub text: NameId,
}
```

Derive whatever `ConstValue` derives, no more. Add `docs: Vec<DocFact>` to
`TypeFAux` (`types.rs:328-332`).

Every string goes through the interner as a `NameId`, the way `ConstValue`
does. Do not put `String` on a `types.rs` row type.

### 2. Wire, in `v6/sprefa-extract/src/wire.rs`

Two `FlatFact` variants and two flatten loops, immediately after the const loop
at `:120-128`, in the same style:

```
FlatFact::Doc     { family, owner: SpanOut, text: String }
FlatFact::DocTag  { family, owner: SpanOut, tag: String, arg: Option<String>, text: String }
```

Update the `Vec::with_capacity` at `wire.rs:100` to account for the new rows.

### 3. Schema, in `v6/sprefa-extract/src/schema.rs`

Two lines in RECORD SHAPES, immediately after the `record=const` line at `:32`,
column-aligned with the block:

```
  record=doc      family=type  owner={start,end}  text=<string>
  record=doc_tag  family=type  owner={start,end}  tag=<string>  arg=<string|null>  text=<string>
```

The schema block is documentation of the wire; it must match `FlatFact` exactly
or it is a lie.

### 4. The rust walker, in `v6/sprefa-extract/src/lang/rust.rs`

Port `rust_docs_from` (`src/graph/typegraph/rust/mod.rs:519`). Read the v5
function completely before writing a line. Keep its cleaning and its tag split
byte-identical: the same leading-marker strip, the same blank-line handling, the
same `# Heading` to `tag=section` mapping (`src/graph/typegraph/mod.rs:168-171`).

Emit into `TypeFAux.docs` from the walk that ALREADY builds the type entities.
Never add a second traversal of the syn tree. The owner span is the entity node's
span the existing walk already computed.

Update the header comment at `v6/sprefa-extract/src/lang/rust.rs:23-24`, which
currently lists the docs facet as a deferred follow-up.

### 5. The status table and the ledger

Flip the docs row for rust in the table at `v6/sprefa-extract/src/types.rs:1830-1836`.
Add a docs line to that table if none exists; the table currently has no docs row
and mentions it only in the DEFERRED list at `:1832`.

`tests/golden_parity.rs:22-24` lists `doc` under DEFERRED v5-only, reported and
not asserted. Move rust's doc facet to ASSERTED if and only if the captured v5
oracle baseline carries doc rows. Check the baseline files under
`v6/sprefa-extract/tests/fixtures/` first. If the oracle has no doc rows, LEAVE
IT REPORTED and write one comment line saying the oracle predates the facet.
Do not regenerate a v5 oracle; the v5 crate is not in your build graph.

### 6. Test

Append to `v6/sprefa-extract/tests/golden_parity.rs` or add
`v6/sprefa-extract/tests/16_docs_facet.rs` (your call, one of the two): a rust
fixture with a doc-commented struct carrying a `# Errors` heading and a
doc-commented function, extracted through the library, asserting the `doc` and
`doc_tag` rows land with the right owner spans.

**Fail-first receipt, required.** Run the new test before step 4, paste the red
output into the commit body, then land the walker and paste the green.

## Gate, run each twice, read rc explicitly, never pipe through tail

```bash
cd /path/to/your/worktree/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

`cargo build` ALWAYS runs before any binary gate. Baseline at the base sha is
rc=0 with every leg green, so any red is yours. In particular
`ported_facets_match_v5` and the three `type_edge_resolve_parity_*` legs pass
today and must still pass.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/types.rs`
- `v6/sprefa-extract/src/wire.rs`
- `v6/sprefa-extract/src/schema.rs`
- `v6/sprefa-extract/src/lang/rust.rs`
- `v6/sprefa-extract/src/lib.rs` (re-export lines only)
- `v6/sprefa-extract/tests/golden_parity.rs`
- `v6/sprefa-extract/tests/16_docs_facet.rs` (new, if you choose it)

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/src/scip*.rs`
- `v6/sprefa-extract/src/lang/ts.rs`, `lang/go.rs`, `lang/kotlin.rs`,
  `lang/dl6/**`, `lang/prolog/**`, `lang/markdown/**`, `lang/astgrep.rs`
- `v6/sprefa-extract/src/bin/extract.rs`
- `v6/sprefa-extract/tests/1_resolve_cli.rs`, `tests/8_scip_families_cli.rs`
- `v6/sprefa-engine-rs/**`, `v6/tsv2/**`, `v6/prolog/**`
- everything outside `v6/sprefa-extract/`

Concurrent lanes own the forbidden files. Touching one loses both lanes' work.

## Laws that bind you

- Never spawn a subagent. Fan-out is the coordinator's call.
- Every new type declares itself in the canonical type module (`types.rs`); this
  crate keeps ALL types there and re-exports through shims.
- Comment budget: comments state constraints the code cannot show. No change-log
  narrative, no dates, no arc references.
- No em dashes. Banned words in prose and identifiers: provenance, substrate,
  load-bearing, regime.
- Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before you report done. An uncommitted
  deliverable is an undelivered one.
- Do not push. Do not open a PR. Do not merge. The coordinator lands it.
