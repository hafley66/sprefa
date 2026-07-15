# A note to my future self: comments as architectural space

_Personal note, 2026-07-15. These are my thoughts to come back to and explore
later. This is not a settled design, syntax, roadmap, or implementation promise._

I have been imagining `comment_node` for years without necessarily having a
name for the whole idea. A comment does not have to be dead prose beside the
program. It can be a small piece of architectural space: human-readable, close
to the thing it describes, precisely located, and optionally understood by a
tool.

Sprefa is already using comments this way in several different forms. I want to
remember the larger pattern rather than treating each form as an unrelated
feature.

## What I am already doing

`comment_node(path, line, col, end_line, end_col, text, kind)` is the raw
substrate. It knows that text is really a grammar-recognized comment rather
than comment-looking text inside a string. From that one fact, the `.dl`
programs currently build several kinds of meaning:

| Use | Existing technique | What the comment becomes |
| --- | --- | --- |
| Architecture | `ARCH {"url": ...}` in `std/arch.dl` | A named node, parent hierarchy, sibling order, and arbitrary architectural fields |
| Local policy | `dl-disable-*` in `std/suppress.dl` | Scoped suppression, reason, exact editor span, malformed/unused diagnostics |
| Documentation metadata | `README(anchor): text` in `examples/gen-readme.dl` | Source-owned gallery prose and generated documentation |
| Cross-language registration | `LANG-JUNCTION(slug): meaning` in `examples/gen-lang-skill.dl` | A distributed registry and a generated language-support map with drift rails |
| Planning and debt | `todo(category): text`, `TODO`, and `FIXME` in `examples/gen-plans-index.dl` | Indexed work, backlinks, generated plan views, and untriaged-debt diagnostics |
| Generated ownership | `BEGIN: gen name` in `examples/gen-zone-info.dl` | An exact “machine-owned region” explanation in the editor |
| Lint integration | `examples/lint-unwrap.dl` through `std/suppress.dl` | A finding that can be locally governed without hard-coding policy in Rust |

There are adjacent versions of the same instinct too: `doc_comment` and
`doc_tag` attach documentation to symbols; checked notes use antijoins to make
claims about symbols and call edges go stale loudly; generated zones make
ownership visible; graph/deck relations project source facts into other views.

The repeated shape is:

```text
ordinary source comment
        |
        v
grammar-backed location and span
        |
        v
an opt-in marker convention
        |
        v
typed relations with source provenance
        |
        +--> diagnostics / rails
        +--> documentation / indexes
        +--> generation boundaries
        +--> architecture / graph views
```

The best current query technique is the `LANG-JUNCTION` one: use a rich text
match to capture fields, then join it to `comment_node` at the same path and
line. The match extracts; the grammar-backed row witnesses that the marker is
actually a comment. Exact `(path, line, col, end_col)` coordinates matter when
the result should point back into the editor.

The conventions are intentionally small and local. Most comments should stay
ordinary prose. Only an explicit prefix opts a comment into a protocol.

## The larger idea

Comments can act like foreign keys into an architecture.

The location is part of their meaning. A note can attach to the nearest
function, type, call, dataflow node, generated region, or module. Its explicit
identifier can then connect that source location to another representation.
Variable names and nearby structure can help infer the attachment, but they
should be supporting evidence, not identity. Names change and repeat; a stable
ID plus a structural/source anchor is stronger.

A generic conceptual layer might eventually look like this:

```text
note(id, namespace, kind, text, path, span)
note_attaches(note, code_entity, method, confidence)
correspondence(note, from_entity, model, to_entity)
```

That does not mean replacing every existing marker with one universal schema.
`ARCH`, suppressions, todos, and generation zones have useful domain-specific
relations. The common layer, if it earns its keep, is provenance and attachment:
what the note is, where it lives, and what it claims to describe.

There are useful levels of commitment:

1. Free prose, for humans only.
2. A named marker that can be queried.
3. A structured payload that becomes typed metadata.
4. Two representations paired by a stable correspondence ID.
5. An executable comparison using shared fixtures or observable traces.

Each higher level adds evidence and maintenance cost. None of them turns a test
into proof.

## The other direction: author the feature space first

I do not have to begin with code structure and claim that two implementations
are one-to-one. I can author an independent space of features, ideas,
questions, and intended behavior, then let code and non-code evidence attach
to positions in that space.

The simplest authoring language might be an ordinary Markdown hierarchy:

```markdown
- reactive-runtime
  - bounded-ingestion
    - scan-files
    - parse-concurrently
    - commit-batches
  - retraction
    - remove-stale-facts
```

Each position gets an address derived from its path:

```text
/reactive-runtime/bounded-ingestion/scan-files
/reactive-runtime/retraction/remove-stale-facts
```

This is like giving every HTML element an addressable ID, except the hierarchy
itself forms a URL-shaped coordinate system. The address says where an idea
lives in the feature space. It does not have to claim that the node corresponds
exactly to one function, type, or file.

That space can contain both presence and absence:

```text
feature-space node
    |
    +--> implemented by source entity
    +--> explained by comment_node marker
    +--> demonstrated by prototype or fixture
    +--> documented by Markdown/HTML element
    +--> supplied by an API or imported dataset
    +--> planned but not represented in code
    +--> intentionally unsupported
```

This matters because a code-derived graph can only describe what it can see.
An authored feature map can also represent negative space: desired behavior,
unanswered questions, competing ideas, and features that do not exist yet.

The useful model may therefore be a mix of declared coordinates and observed
evidence:

```text
feature(id, parent, order, title, status)
feature_claim(feature, claim_kind, text)
feature_evidence(feature, source_kind, source_id, span)
```

`source_kind` could be code, comment, document element, prototype, test result,
API record, generated artifact, or something added later. A `comment_node`
marker would be one convenient way for source code to say “this thing over here
is evidence for that feature-space address.” It would not be the only way.

Markdown could be the pleasant source format, HTML the navigable rendering,
and relations the join layer. Code extraction could suggest attachments while
explicit markers settle the important ones. A feature may have many pieces of
evidence, and one source entity may support several features. That is more
honest than forcing a one-to-one mapping.

Do not hard-code this to plain Markdown or build a document editor from
scratch. Obsidian already demonstrates useful document-space primitives:
wiki links, headings, block IDs, frontmatter, tags, transclusion, backlinks,
and a plugin ecosystem. The Oxide plugin and other document languages/tools
are more prior art and possible integration surfaces to investigate. AsciiDoc,
Org, reStructuredText/MyST, mdBook-style systems, and future formats may expose
different syntax for the same underlying concepts.

Treat each document system as an adapter:

```text
Obsidian block / Markdown heading / HTML element / other document node
                              |
                              v
                  canonical document anchor
                              |
                              v
                  feature-space address + evidence
```

Native selectors should be preserved when they are useful. A canonical anchor
should add interoperability, not erase the document system's own identity,
links, rendering, or editing experience. Sprefa's role could be indexing,
joining, checking, and projection while the document tools keep doing document
authoring well.

## A typed projection and renderer layer

This needs prebaked visualization integrations in the VS Code surface. A query
should not require a custom webview every time. Its result shape, sprefa types,
and provenance should be enough to select a useful standard view, with an
explicit override when inference is wrong.

The existing flow panel is already a good seed. The same node/edge rows feed
list, canvas, and trace modes; `_node`/`_edge` relation pairs are discovered
from the SQLite schema; and the host boundary is only query, hover, and open.
Generalize that convention instead of starting over.

```text
query + result schema + sprefa types + provenance
                       |
                       v
               projection contract
                       |
          +------------+-------------+
          |            |             |
          v            v             v
        table       hierarchy      node/edge
                       |             graph
             +---------+--------+      |
             |                  |      +--> SVG/canvas
             v                  v      +--> Cytoscape
          file tree         symbol tree
             |                  |
             +------- edges ----+
```

There should always be a boring table fallback. Beyond that, standard shapes
can route automatically:

| Result/schema shape | Default projection |
| --- | --- |
| ordinary typed columns | Sortable/filterable table with source provenance |
| `path` or declared delimiter hierarchy | File/tree viewer |
| `parent` + stable `id` | General hierarchy/tree |
| `src`, `dst`, `kind` | Common graph viewer |
| two hierarchy domains plus edges | Paired trees with cross-pane edges |
| `before`, `after` or two revisions | Diff/two-sided view |
| time/revision/ordinal | Timeline or ordered trace |
| symbol/type identity | Type-aware symbol cards and containment |
| file + line/span | Source list with hover/open navigation |

Hierarchy is not always a filesystem. The projection should carry its delimiter
and address kind explicitly: `/` for files or feature URLs, `::` for Rust-like
symbols, `.` for some modules, document heading paths, API resource paths, and
so on. A renderer should receive already separated segments when possible
instead of guessing delimiters from display strings.

The paired-tree view is especially useful:

```text
source filesystem                 generated filesystem

src/                              sdk/
├── model.rs ───── generates ───> ├── model.ts
├── api.rs   ───── corresponds ─> ├── api.ts
└── error.rs ───── projects ────> └── errors.ts
```

The same projection could pair source files with feature-space nodes, Rust
types with TypeScript types, document anchors with source entities, or two Git
revisions. Edge bundling, filtering, and hover should make large mappings
readable; Cytoscape can be one rendering backend, not the data contract.

A conceptual query/view contract might be:

```text
view(id, renderer, title)
view_node(view, id, label, kind, parent, domain, source_anchor)
view_edge(view, src, dst, kind, label)
view_encoding(view, role, column_or_type)
```

`domain` distinguishes the two sides or coordinate spaces. `source_anchor`
retains repo/path/span/document/API provenance so every visual item can explain
where it came from and open the right editor location. The internal sprefa type
system should supply semantic roles where it knows them: path, symbol, type,
revision, span, feature address, document anchor, and so on. Column-name
conventions remain the compatibility fallback.

The VS Code core should host a small renderer registry rather than another
single giant panel:

```text
projection contract
    ├── table renderer
    ├── tree renderer
    ├── paired-tree renderer
    ├── graph renderer (native or Cytoscape)
    ├── trace/timeline renderer
    └── diff renderer
```

Renderers should share selection, pinning, filters, provenance inspection,
hover, and open-location behavior. Presets remain useful for curated joins and
layout decisions, but a normal query should become visual with no custom UI
code.

### Spatial navigation: Obsidian-like panes on the web

The interaction model should feel like Obsidian brought to a web workspace,
not a website that replaces the current page on every click. Clicking an item
opens its representation in a new pane immediately to the right and preserves
the panes that led there.

```text
feature map        paired trees        type detail       source
┌────────────┐     ┌────────────┐      ┌────────────┐    ┌────────────┐
│ ingestion  │ --> │ Rust ↔ TS  │  --> │ ParseBatch │ -> │ batch.rs:42│
│ retraction │     │ file edges │      │ fields     │    │ highlighted│
└────────────┘     └────────────┘      └────────────┘    └────────────┘
```

The row of panes is the user's current reasoning path. Nothing refreshes away
the context on the left. A pane can be a document, feature node, query result,
tree, graph, type card, diff, source span, API object, or generated artifact.
All of them use the same canonical addresses and selection/provenance model.

Useful interaction rules:

- Normal click opens to the right and closes only panes already farther right
  than the clicked pane, creating a new branch from that point.
- A modifier can replace the current pane or pin a pane so it survives
  branching.
- Selecting a graph/tree item can highlight its evidence in every visible pane
  without navigating.
- Back closes the rightmost pane; forward restores it.
- The URL should encode the pane chain, selections, renderer choices, and query
  parameters so a particular exploration is linkable.
- Pane data loads asynchronously and independently. Opening a source location
  must not rerun or rerender the entire workspace.
- Width, scroll position, filters, pins, and expansion state should persist per
  pane address.

This makes a generated visualization an entry point rather than a dead-end
image. I can click from an authored feature to its evidence, from an edge to the
query that produced it, from a type projection to its source and generated
counterpart, and keep the entire explanation visible at once.

The deeper idea: **history is the view**.

The user creates a custom path view simply by navigating. Each pane is a typed
address; each click is a typed transition; the visible pane chain is the
materialized path.

```text
path_view(session-or-name, ordinal, pane_address, renderer, state)
path_edge(path, from_pane, to_pane, action, relation)
```

Saving the path turns an improvised investigation into a named view. Sharing
its URL turns it into an explanation. Replaying it turns it into a tour.
Refreshing its queries turns it into a live dashboard. Annotating a transition
records why the jump mattered.

History is really a branching path graph rather than one flat stack. Going
back and choosing another edge creates another branch; pinned panes can be
shared context across branches. A saved view can preserve only the chosen
spine or include the useful alternatives.

This collapses several products into one interaction primitive:

```text
exploration --save--> custom view
custom view --annotate--> explanation
explanation --order--> tour
tour --keep queries live--> dashboard
path --compare with another path--> diff of understanding
```

The view is therefore not primarily a layout file. It is a durable query and
navigation path through typed, addressable evidence, with renderers chosen for
each stop.

### View hints as non-materialized decorators

Inference needs an escape hatch. There should be built-ins or syntax that give
the projection system hints without creating stored facts or adding work to
the reactive dataflow. Conceptually these are no-op relations: active metadata
consumed by the compiler/UI, not tables that participate in joins.

Possible surface syntax:

```dl
@view("paired-tree")
@domain(left, source_path, delimiter="/")
@domain(right, generated_path, delimiter="/")
@edge(source_id, generated_id, kind)
? generated_mapping
```

Or, using magic decorator-like relations if that fits the language better:

```dl
view_hint("generated_mapping", "renderer", "paired-tree").
view_role("generated_mapping", "source_path", "left.path").
view_role("generated_mapping", "generated_path", "right.path").
view_option("generated_mapping", "delimiter", "/").
```

Both surfaces could lower to the same non-relational manifest:

```text
ViewSpec {
  target: generated_mapping,
  renderer: paired_tree,
  roles: { left.path, right.path, edge.src, edge.dst, edge.kind },
  options: { delimiter: "/" }
}
```

This is a decorator system for relation and query schemas. It should:

- disappear from relational evaluation;
- never create resident rows or trigger a tick;
- be type-checked against the target relation's columns;
- merge with inferred roles, with explicit hints winning;
- be discoverable through the daemon alongside schema metadata;
- travel with saved path views and URLs;
- produce useful errors for unknown renderers, roles, or columns;
- remain optional so every query still has the table fallback.

Static presentation intent belongs here. If color, grouping, labels, or edges
depend on row values, those are real derived data and should remain ordinary
relations. That boundary keeps the hint layer cheap and predictable rather
than creating a second hidden dataflow language.

### Saved paths are data; nested access is typed projection

Do not confuse decorators with saved navigation. A renderer hint is static
program metadata and should not be materialized. A path I save, name, annotate,
share, or graph is user-authored durable data and should become ordinary
queryable relations:

```text
saved_view(id, name, created_at)
saved_pane(view, ordinal, address, renderer, state)
saved_transition(view, from_ordinal, to_ordinal, action, relation, note)
```

Those relations can feed the same graph convention immediately:

```dl
history_node(id, label, kind, parent) <- saved_pane(...).
history_edge(src, dst, kind)          <- saved_transition(...).
```

That means histories can be graphed, compared, searched, generated into docs,
or used as evidence for feature-space nodes. A saved history can also retain
the query/view definition rather than freezing all result rows, so reopening it
shows current data while preserving the route through it.

Eventually these objects need nested values and dot access. Dot should mean
typed field projection, not arbitrary method dispatch, stringly JSON paths, or
an SQL subquery hidden behind syntax:

```dl
pane.address.path
pane.renderer.kind
transition.from.address
```

At lower time, resolve every segment against the logical type:

```text
pane.address.path
   |
   v
project(
  project(PANE, field_id(Pane.address)),
  field_id(Address.path)
)
```

An unknown field is therefore a compile/lower diagnostic. Renaming a field can
be tracked structurally. Render hints can refer to the same typed path:

```dl
@domain(left, mapping.source.path, delimiter="/")
@domain(right, mapping.generated.path, delimiter="/")
```

The current content-addressed `TypeId` arena is a useful base, but it will need
structured record fields (and eventually tuples, lists/maps, and option/union
shapes) in addition to base, named, enum, application, and union nodes. Field
identity belongs in the type arena; do not repeatedly store or compare field
name strings in every value row.

Nested values should also avoid heap-heavy object trees per fact. A portable
logical representation could use compact `ValueId`s plus ordinary normalized
rows:

```text
value_node(value_id, type_id, scalar_or_kind)
value_field(value_id, field_id, child_value_id)
value_item(value_id, ordinal_or_key, child_value_id)
```

The storage adapter may optimize that physical shape later, but the language
semantics should not depend on SQLite JSON functions, custom mmap layouts, or
one database. The planner can lower a field projection to a normalized join,
an indexed lookup, or a decoded compact value depending on the backend.

Important semantic boundaries:

- Dot projects exactly one named field; it does not flatten collections or fan
  out rows.
- Collection traversal is an explicit op (`each`, `members`, or equivalent),
  so cardinality stays visible.
- Optional/union projection must be explicit in the type and query semantics;
  missing dynamic JSON keys must not silently imitate a typed optional field.
- Dynamic external JSON can have a separate checked/dynamic access operator,
  while ordinary `.` remains statically resolved.
- Frequently queried fields should be pushable into indexed relational joins;
  avoid implementing dot as repeated `json_extract` or nested SELECTs.

The surface can feel object-shaped while the engine remains relational and
bounded. Objects are the authoring and typing view; normalized values and field
projections are the execution view.

The two approaches can coexist:

- **Code-first correspondence:** discover a structural entity, then explain or
  pair it with another representation.
- **Feature-first addressing:** declare an idea-space node, then gather code and
  non-code evidence beneath it.

The first is strong for navigation and structural drift. The second is strong
for intent, missing features, and mixed sources. The hybrid is probably the
interesting system: an authored map whose implemented regions are continuously
grounded in extracted code facts.

## The RxJS mirror I keep asking for

When I ask an AI, “what would this Rust pipeline look like in RxJS?”, I am
trying to move between two levels of explanation. If the correspondence is
useful and reasonably direct, I should be able to keep it near the code instead
of asking for it again from scratch.

For example, a Rust stage could carry a lightweight marker:

```rust
// ANALOGY(rxjs, scan-files): mergeMap(file => parse(file), 2)
```

Or, if the metadata needs to grow:

```rust
// CORRESPONDS {"model":"rxjs","id":"scan-files",
//              "role":"mergeMap","target":"examples/rx/scan-files.ts"}
```

A tiny TypeScript/RxJS prototype could carry the same ID. Sprefa could extract
both markers, attach each to its nearby structural node, and show a paired
pipeline:

```text
Rust implementation                    RxJS explanatory model

walk files          <--- scan-files --> from(paths)
bounded parse       <--- parse-pool ---> mergeMap(parse, concurrency=2)
batch facts         <--- fact-batch ---> bufferCount(n)
commit generation   <--- commit -------> concatMap(commit)
```

An orphaned side or changed stage could produce a drift rail. A shared event
fixture could compare traces. That would be useful evidence that the analogy
still teaches the right behavior; it would not claim that Rust and RxJS have
identical semantics.

This could eventually make comments into a navigable correspondence layer
between implementation, pseudocode, prototypes, documentation, and diagrams.

## Prior art to investigate

Nobody appears to combine the entire idea, but several mature systems contain
important pieces:

- **Doxygen (especially C/C++, but also Python and PHP)** lets special comments
  define named topics, nested groups, pages, subpages, anchors, custom commands,
  and membership that differs from source-language structure. This is very
  close to attaching code entities to an independently meaningful semantic
  hierarchy. It remains primarily a documentation generator rather than a
  general relational evidence and drift system.
- **Sphinx** has typed domains and cross-reference roles for Python, C, C++, and
  JavaScript; extensions can add new object types, while Intersphinx joins
  object inventories across documentation sets. This is close to a canonical,
  extensible address space for heterogeneous document/code objects.
- **Cucumber/Gherkin** starts with an authored `Feature -> Rule -> Scenario`
  space, then connects its steps to implementation code in several languages.
  Undefined and ambiguous bindings fail visibly. This is probably the closest
  precedent for “declare the feature space first, then ground it in code,” but
  it is deliberately about executable behavior rather than arbitrary source
  structure, ideas, documents, and evidence.
- **CWEB, noweb, and Org Babel** make named document/code chunks primary, then
  tangle or evaluate them across languages. They prove that document-first
  addressing and structural generation can work. Their center of gravity is a
  literate source that owns generated code, not analysis of an existing
  polyglot codebase.
- **CodeTour** stores an external sequence of Markdown explanations attached to
  directories, files, lines, selections, or content. It is a direct precedent
  for a positional explanatory overlay, but it lacks semantic attachment,
  feature-space relations, and strong drift handling.
- **Language documentation tools** each supply useful local pieces: rustdoc has
  checked intra-doc symbol links; Go has declaration-bound doc comments and
  tool directives; JSDoc/TypeDoc can create virtual members, tutorial trees,
  and external document hierarchies; phpDocumentor combines structured
  DocBlocks with guides that reference API objects; RDoc and Haddock expose
  named anchors, cross-references, and source links.

The opportunity is not to replace these systems. It is to normalize their
addresses and evidence into one polyglot relation space, while keeping their
native authoring and rendering experiences.

### Systems to study next, in priority order

1. **Malloy.** This may be the closest technical precedent for the query/view
   seam. A query returns typed schema metadata alongside results; annotations
   attach to named model/query fields; the renderer interprets those tags; its
   tag language supports nested properties; and Malloy is explicitly designed
   around nested query results. Study its separation of semantic model, result
   schema, annotations, and rendering before designing sprefa's decorators.
2. **Glamorous Toolkit, Phlow, and Lepiter.** GT's “moldable” inspector gives
   objects contextual views instead of one generic representation. Lepiter is
   a graph of live, multi-language notebook snippets, and evaluation can open
   an inspector to the right. This is the closest interaction/philosophy match
   for typed panes, history-as-view, and cheap purpose-built renderers.
3. **JupyterLab, not primarily JupyterHub.** JupyterLab workspaces persist the
   position and state of files, notebooks, sidebars, and panels and can be
   named, exported, imported, and addressed through URLs. Its Rendermime
   registry selects pluggable renderers for typed MIME bundles. JupyterHub is
   useful later for authentication, authorization, spawning isolated per-user
   servers, and sharing—not for the core pane or projection design.
4. **Nextjournal Clerk and Clojure's data-oriented inspectors.** Clerk's viewer
   API separates value transformation from browser rendering and allows viewer
   selection through metadata. The surrounding Clojure `datafy`/`nav` idea is
   worth studying as a protocol for turning arbitrary objects into navigable
   data without baking every domain into the inspector.
5. **Pluto.jl (and reactive notebook descendants).** Pluto statically derives a
   dependency graph from cell definitions/references, reruns only downstream
   cells, and removes deleted definitions rather than retaining hidden state.
   Study the dependency/invalidation contract and IO technique, not the
   notebook product. Sprefa's unit is a relation/query rather than a cell.
6. **GraphQL and DuckDB nested types, as semantics only.** GraphQL is useful for typed nested field
   selection, introspection, and separating a logical object graph from its
   storage/resolvers. DuckDB is concrete implementation prior art for typed
   `STRUCT`, `LIST`, `MAP`, and `UNION` values, dot projection, unnesting, and
   resolving ambiguities between table/column and object-field paths. Keep
   SQLite and the storage seam; do not adopt DuckDB to obtain this syntax.
7. **Vega-Lite, as a chart backend only.** Use it as one model for a declarative rendering grammar:
   typed fields map to visual encoding channels, while a compiler lowers a
   compact specification to a more detailed runtime. Sprefa needs analogous
   defaults plus explicit overrides, although its renderers include trees,
   source, and inspectors rather than charts alone. Vega does not define the
   general renderer or workspace architecture.
8. **Quarto, selectively.** Its typed cross-reference IDs, executable code blocks, document
   filters, multiple output formats, and links between code, computed output,
   and prose are useful prior art for document adapters. The whole system is
   much broader and heavier than the surface sprefa needs.

The immediate reading order should be Malloy → Glamorous Toolkit → JupyterLab.
Together they cover the three hardest seams: typed query annotations,
context-specific inspection/navigation, and persistent extensible workspaces.

### The reactive mechanism is versioned dependency combination

For sprefa, the useful Pluto idea reduces to something closer to React
`useMemo` dependencies or RxJS `combineLatest`: an operator depends on the
latest versions of several inputs, and it runs only when that dependency tuple
changes.

```text
files_version  ──┐
git_version    ──┼── combine latest version tokens
config_version ──┘              |
                                v
                  hash(op identity, config,
                       ordered dependency tokens)
                                |
                    same key? skip : enqueue once
```

The combined inputs are tiny stable tokens, not retained copies of the actual
relations. The data remains in the storage/fact layer and is read through a
consistent generation snapshot when the queued operator runs.

Important details make this more than ordinary UI memoization:

- Ingress computes or advances content versions once; downstream stages should
  not re-hash entire relations to discover change.
- A burst of input events invalidates a stage once. The scheduler coalesces
  repeated invalidations and runs it against the newest complete dependency
  tuple.
- Every tuple belongs to a generation/snapshot so `combineLatest` cannot pair a
  new filesystem state with an old configuration accidentally.
- The memo key includes operator identity, relevant configuration, dependency
  identities, and their versions. Changing the dependency set invalidates the
  key.
- A hash mismatch schedules work; it does not define correctness. Actual row
  additions and removals still flow as explicit deltas, including deletion of
  the final witness.
- Committing output publishes its new version token only after the output delta
  is durable, then downstream stages become eligible.
- A snapshot-rebuild stage may discard intermediate invalidations and keep only
  the newest dependency tuple. A delta-only stage may not drop events; it must
  compose them into one bounded net delta (add then remove cancels, repeated
  adds combine by weight) before it runs.

```text
event -> update input token -> invalidate dependents -> coalesce
      -> capture consistent tuple -> run bounded work -> commit delta
      -> publish output token -> invalidate next dependents
```

This gives the simple mental model of `useMemo(deps)`/`combineLatest(inputs)`
without importing their unbounded queues, retained payloads, or UI scheduling
semantics.

## A separate but connected branch: structural polyglot code generation

One day I also want to explore polyglot code generation through a type system
that covers the useful common ground of the languages I care about. TypeSpec is
an example of the general instinct: describe types once, then project that
understanding into canonical artifacts in several target languages. I like the
idea even if I do not want to inherit another system's language evolution or
implementation choices.

Sprefa already has analysis and refactoring. Structural code generation is a
different operation, but it can share the same semantic substrate:

```text
source types in one or many languages
                 |
                 v
        canonical type model
   (records, sums, fields, refs,
    generics, nullability, constraints)
                 |
        +--------+---------+
        |        |         |
        v        v         v
      Rust      TypeScript  Kotlin ...
        |        |         |
        +--------+---------+
                 |
                 v
      checked-in canonical files
      + provenance + drift rails
```

The hard and interesting part is not printing syntax. It is defining which
type meanings are portable, which are target-specific, and how a projection
reports loss instead of quietly lying. A future canonical model probably needs
an explicit escape hatch for target-language details and a way to state that a
mapping is partial or lossy.

Comment-space can connect this branch back to real code. A field, type, or
generated region could say which canonical type node produced it. Handwritten
code could carry an override or correspondence note next to the exact element
it affects. Generated files could remain ordinary, greppable source, while
forward and reverse rails detect drift.

This suggests three related but distinct capabilities:

- **Analysis:** recover structure and meaning from existing code.
- **Refactoring:** transform existing structure while preserving intended meaning.
- **Structural generation:** project an explicit semantic model into canonical target-language source.

They should share identities, types, provenance, and rails without being
collapsed into one operation.

## A small experiment for later

Do not begin with a universal annotation language or a universal type system.
Pick one real pipeline and one small type family.

1. Pair four Rust pipeline stages with a tiny RxJS mirror using stable comment IDs.
2. Generate a relation/view that shows both sides and flags missing partners.
3. Run one shared fixture through both and compare only the observable event trace.
4. Define one canonical record + tagged union and project it to Rust and TypeScript.
5. Put provenance markers beside the generated types and add forward/reverse drift rails.
6. Record where meaning is lost, where an override is required, and whether the comments remain pleasant to read.

The question is not “can this generate code?” The question is whether the
correspondence remains understandable, local, inspectable, and cheap enough
that I trust it months later.

## Things to remember when I return

- `comment_node` currently needs files to enter the corpus through a scan.
- Source scanning and derived `comment_node` joins should stay in separate rules;
  mixing them has been a sharp edge.
- Most consumers ignore comment kind; exact spans matter more than kind for UI.
- The raw relation has neither repository nor revision identity. Identical paths
  and spans can collapse across repos/revisions, so correspondence IDs cannot
  safely build on its current identity alone.
- Removing the last supported file can currently leave stale comment rows, and
  extraction failures can look like an empty successful parse. Harden that
  lifecycle before making comment metadata authoritative.
- Proximity is valuable, but explicit stable identity should beat clever inference.
- Variable-name consistency is a useful signal, never a sufficient key.
- Prefer opt-in, domain-shaped markers over interpreting all prose.
- A runnable mirror or test is evidence and a drift detector, not proof.
- Keep the generated output ordinary and inspectable. Make loss and overrides explicit.
- Keep this as an exploration until a tiny end-to-end example demonstrates that it
  reduces explanation and maintenance cost.

The thought I want to preserve is simple: comments can be part of the program's
architectural address space. Once they have grammar-backed spans, stable names,
and typed projections, they can connect how code works, how I explain it, and
what other code should be generated from the same understanding.
