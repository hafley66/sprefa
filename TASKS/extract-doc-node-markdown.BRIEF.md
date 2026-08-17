# Lane brief: markdown `doc_node` plus the `doc_ref` bridge (issue extract-doc-node-markdown)

First action: `git merge --ff-only 6b483939bcfabb329f0cc424ce85aec709acadc3`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

That base sha is the head of the sibling branch `feature/extract-kotlin-type-plane`,
whose PR is posted and not yet merged. Its commits touch `src/lang/kotlin.rs`,
`src/project.rs` and `src/types.rs` and will appear in your branch's history.
Leave them alone: never revert, amend, squash or reformat them, and never re-run
their tests as yours. Your PR targets `main` as usual.

The two `project.rs` comments that claimed `Resolve::resolve` has a `todo!()`
default are ALREADY corrected on this base. Do not re-correct them.

Do not ask a question before starting. Everything you need is below with a
`path:line`. Line numbers are from the base sha and may be off by a few lines;
locate by the SYMBOL NAME given and continue. If a symbol is absent or the code
contradicts the receipt, say which one and stop.

## The gap

`MarkdownSource` projects the CST and stops (`v6/sprefa-extract/src/lang/markdown/_0_source.rs`,
`extract` fills `output.cst` only). v5's document family adds two rels: the
structure rows, and the doc-to-code bridge where a heading names a declared
entity.

| fact | receipt |
|---|---|
| v5 rels + their columns | `src/engine/family/mod.rs:492-499` (REPO ROOT): `doc_node(file, line, kind, name, parent)`, `doc_ref(file, line, sym)` |
| v5 walker, the thing you port, ~150 lines | `src/ingest/mod.rs` (`MarkdownDoc::extract_docs`, `heading_text`, `fenced_code_block_parts`) |
| the heading-stack rule, verbatim | `src/ingest/mod.rs:79-84` ("a new heading at level L pops any entry with level >= L before pushing, so `parent` is the nearest enclosing heading of a strictly lower level") |
| v6 markdown, cst only | `v6/sprefa-extract/src/lang/markdown/_0_source.rs:1-7`, `:105-130` |
| the corpus index the bridge joins against | `ProjectCx.indexes.def_index`, built by `build_def_index`; the pure lookups are at `src/types.rs:1063` (`containing_def_site`) and the `DefIndex` struct above it |
| the CLAUDE.md row claiming v6 has no markdown extractor is STALE | `MarkdownSource` is in the roster at `src/lang/mod.rs:46` and `source_for(".md")` returns it |

## The pinned design. Every judgment call below is already made.

### 1. Types, `v6/sprefa-extract/src/types.rs`, beside `DocFact` (`:328-345`)

```rust
/// One structural node of a document: a heading or a fenced code block.
/// `name` is the heading title, or the fence language (empty when the fence
/// names none). `parent` is the enclosing heading title, None at top level.
pub struct DocNode {
    pub span: Span,
    pub kind: DocNodeKind,
    pub name: NameId,
    pub parent: Option<NameId>,
}

pub enum DocNodeKind { Heading, CodeBlock }
```

`as_str` returns `heading` and `code_block`. Derive exactly what `DocFact`
derives. Add `doc_nodes: Vec<DocNode>` to `TypeFAux` (`:346-355`).

v5's `DocNode` also carries the code-block BODY text, used for name-matching
symbols inside a fence (`src/ingest/mod.rs:26-30`). v6 does not carry it: the
bridge below matches HEADINGS only. Name that as a follow-up in the PR body; do
not add the field.

### 2. `TypeEdgeKind::DocRef`, `src/types.rs` (search `pub enum TypeEdgeKind`)

One variant, slug `doc_ref`. Before adding it, run
`rg -n 'TypeEdgeKind' v6/ --include=*.rs` and confirm no exhaustive match
outside the enum's own `as_str` breaks; a break is a report-and-stop. Update the
`resolved_type_edge kind` vocabulary line in `src/schema.rs:122`.

### 3. Emission, `src/lang/markdown/_0_source.rs`

`extract` currently returns early unless `mask.cst`. Rework so the block parse
happens once and feeds BOTH planes: `cst` when `mask.cst`, a `TypeF` bundle
carrying `doc_nodes` when `mask.types`. One parse, never two. The familymask law
holds: a masked-off family stays `None`, never an empty bundle.

Port the heading stack and the two helpers from `src/ingest/mod.rs` verbatim in
behavior, with byte spans instead of 1-based lines: `span` = the heading or
fenced-code-block node's own span (the existing `span()` helper at
`_0_source.rs:23-28`).

The markdown arm emits NO TypeF nodes and NO sigs. Only the aux rows.

### 4. Wire + schema

`src/wire.rs`, in `flatten_type`: one arm, in the `Doc` style (`wire.rs:131-146`):

```
#[serde(rename = "doc_node")] DocNodeOut { family, span: SpanOut, kind: String, name: String, parent: Option<String> }
```

`src/schema.rs`, after `record=doc_tag` (`:32`):

```
  record=doc_node  family=type              span={start,end}   kind=<heading|code_block>  name=<string>  parent=<string|null>
```

Plus one KIND VOCABULARIES line for the doc node kinds.

### 5. The bridge, `impl Resolve<TypeF> for MarkdownSource`

For every `DocNode` of kind `Heading`, look its `name` up in the corpus
`DefIndex`. Exactly one site: emit `ProjectEdge::new(src, site.blob, site.span,
TypeEdgeKind::DocRef)`. Two or more sites is ambiguous: skip, never guess. Zero:
skip.

`src` is a `NodeRef`, and the markdown arm emits no TypeF NODES, so there is no
node to point at. Resolve that by emitting the heading's INDEX into
`TypeFAux.doc_nodes` as the `NodeRef`, and say so in a one-line doc comment on
the impl: for this arm `src` indexes the aux row, not the node vec. If you
judge that unacceptable, STOP AND REPORT rather than inventing a second shape.

`src/lang/dl6/_0_source.rs:449` and `src/lang/go.rs:1544` are the two
`Resolve<TypeF>` arms to read first.

### 6. Dispatch, `src/project.rs`

`project.rs` is YOURS this round. READ THE MERGED FILE FIRST: PRs #309 and #310
reworked reader plumbing in it, so any line number you were given is stale. The
markdown `ResolveArm` row gets `types: Some(...)`; `call` stays `None`.
`tests/1_resolve_cli.rs` asserts the roster and the arm table agree both ways
and must stay green.

### 7. `tests/golden_parity.rs` needs NO edit

Its `match fact` over `FlatFact` ends in `#[allow(unreachable_patterns)] _ => {}`
(`tests/golden_parity.rs:340-343` at your base sha), so a new `FlatFact` variant
compiles there untouched and is never asserted. That file is FORBIDDEN with no
exception. If it fails to compile after your variant lands, report and stop
rather than editing it.

## The test, a NEW file `v6/sprefa-extract/tests/22_doc_node.rs`

**Fixture placement rail, learned the hard way.** NEVER add a new `.ts` file
under `v6/sprefa-extract/tests/fixtures/ts/`. The ts scip ratchet
(`tests/golden_parity.rs`, `call_resolve_scip_ratchet_ts`) walks that root
RECURSIVELY and asserts every v6 call site has a scip-typescript occurrence, so
a new file there turns `golden_parity` red. Your `.md` fixture is safe under
`fixtures/markdown/`; if your corpus needs a ts or rust file, reference an
EXISTING fixture rather than adding one.

New fixture `v6/sprefa-extract/tests/fixtures/markdown/doc_node.md` (new file):
nested headings at three levels, two sibling headings at the same level, a
fenced code block with a language, a fenced block without one, and one heading
whose text is exactly a type name declared in an existing rust or ts fixture you
include in the corpus.

Assert:
1. the exact `doc_node` row set: `(kind, name, parent, span)`, every value a
   hand-derived literal, including the sibling case (siblings do NOT nest);
2. the CST plane is unchanged from today for the same fixture (extract with a
   cst-only mask and assert the node count against the value you measure at the
   base sha, stated in the test as a literal with a comment naming what it pins);
3. exactly one `doc_ref` edge, to the entity the heading names;
4. an ambiguous heading name (declare the same name in two corpus files) emits
   NO edge.

**Fail-first receipt, required.** Write the test first, run it, paste the red
output into the commit body. Then land the walker and paste the green.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 22_doc_node; echo "DOCNODE rc=$?"
cargo test --features cli --test 11_markdown; echo "MD rc=$?"
cargo test --features cli --test 1_resolve_cli; echo "RESOLVE rc=$?"
cargo test --features cli --test 6_document_formats; echo "DOCFMT rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. `11_markdown` and `6_document_formats` pin today's markdown
behavior and must stay green untouched: if either goes red, that is a
report-and-stop, not a fixup.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/lang/markdown/**`
- `v6/sprefa-extract/src/types.rs`
- `v6/sprefa-extract/src/wire.rs`
- `v6/sprefa-extract/src/schema.rs`
- `v6/sprefa-extract/src/project.rs` (the one arm row)
- `v6/sprefa-extract/src/lib.rs` (re-export lines only)
- `v6/sprefa-extract/tests/22_doc_node.rs` (new)
- `v6/sprefa-extract/tests/fixtures/markdown/doc_node.md` (new)

FORBIDDEN, do not open to edit:
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
- Commit in slices (types+wire+schema, walker, bridge+dispatch, test). Use
  `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `feature/extract-doc-node-markdown` in your worktree.
2. Commit in slices.
3. `git push -u origin feature/extract-doc-node-markdown`
4. `gh pr create` with a body carrying: the row set asserted, the heading-stack
   rule as implemented, the fail-first red and the green, both gate runs with
   per-binary counts, and a `Follow-up` heading naming the code-block body text
   and any lang whose headings could not be bridged.
5. The PR body ends with the trailer line: `Refs-Issue: extract-doc-node-markdown`
6. NEVER merge the PR. Report the URL and stop.
