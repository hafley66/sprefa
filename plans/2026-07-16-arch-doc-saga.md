# Architecture-as-Document saga — arch space riding the retraction line

## Context

The capstone landed (plans/2026-07-16-capstone-cutover.md): extraction families
are sole writer of the public call rels and the `react_deltas` retract+insert
render is live. The retraction line now needs a second consumer to prove the
family pattern generalizes, and the feature that consumes it is missing:

- **Marker base layer exists.** 11 landed `// ARCH {"url":"sprefa/engine/00-tick"}`
  markers in `src/`, read by `std/arch.dl` — grammar-backed JSON payload,
  slash-path url, `arch_parent`/`arch_order` string surgery (std/arch.dl:60-96).
  No rel knows which comment fulfills which architecture node.
- **Write half exists.** Gen transclusion (`examples/gen-readme.dl`,
  `gen-plans-index.dl`), including per-row zone-name templating:
  `GenTarget::Zone { path_tmpl, name_tmpl }` renders both per row and
  `apply_zones` resolves each rendered name to a `BEGIN: <name>`/`END:` pair
  (src/engine/gen.rs:261-285, 488).
- **Bespoke refreshers remain.** `comment_node`/`doc_node`/`doc_ref` are
  whole-table reloads (src/engine/extract/text.rs:48, doc.rs:14) — the
  per-relation-refresh debt the family pattern retires.
- **Memo is RAM.** `FamilyMemo.rows: Vec<OutRow>` (src/engine/family/router.rs:33-36),
  cloned per rerun (router.rs:125-127), never persisted, no eviction.

Synthesis of two competing 2026-07-16 drafts: the k3 draft is the BASE (its
slash-path ids match the 11 landed markers and std/arch.dl); from the fable
draft exactly two adoptions — `gen(:zone)` name-templated fulfillment zones
(§B.4) and an intermediate memo-spill step before the EXCEPT staging (§C.5).

## Decisions

| # | decision | choice | rejected |
|---|---|---|---|
| D1 | base draft | k3 — slash-path ids (`engine/tick/fixpoint`) match the landed markers and std/arch.dl surgery | fable's dotted ids (`engine.retraction`) — rekeys all 11 markers |
| D2 | per-node fulfillment zones | one `gen(:zone, doc_path, "arch-sites:{node_id}", …)` rule fills every node's zone (name rendered per row, gen.rs:261-285) | comment-op splice form (`comment(path, rev, /BEGIN…/, /END…/, l0, l1)`) — needs the `comment` op over WORK, LIFO pairing, per-zone line coordinates |
| D3 | memo-spill shape | three steps: table-prior reconcile (M1) → EXCEPT staging (M2) → persisted meta (M3) — M1 gives a rollback-able midpoint | straight-to-EXCEPT (new SQL diff + deleted memo in one jump); persisted shadow memo tables (redundant: the public table already is the memo) |

## The kernel

Everything in this saga is one pair of antijoins over two rels:

```
arch_decl(node_id, …)    — nodes a document declares        (doc side)
arch_binding(node_id, …) — nodes a comment marker points at (code side)

unfulfilled(node) <- arch_decl(node, …),    !arch_binding(node, …).   -- doc-first
seed(node)        <- arch_binding(node, …), !arch_decl(node, …).      -- code-first
```

Doc-first and code-first are the **same join read from opposite ends**, not two
mechanisms. A marker referencing an undeclared node is a `seed` (info diag +
draft-generator input, never an error); a declared node with no markers is
`unfulfilled` (info, never red). Both negations auto-clear when the other side
appears — the antijoin-retraction property (skill `sprf-invariants-via-antijoin`;
`examples/checked-notes.dl` is the shipped pattern). Gen transclusion is the
code→doc write half; hand prose + markers are the doc→code half; they meet in
the rels. The saga generalizes the landed `std/arch.dl` work instead of forking it.

## A. Feature design

### A.1 Format — markdown + markers (rec), two rejected

- **Markdown doc set + `comment_node`/`doc_node` extraction (REC).** Headings are
  node sections; `<!-- @arch <id> -->` pins identity; prose under the heading is
  the WHY; `BEGIN:`/`END:` zones hold machine-written tables. Both extractors
  already fire on markdown (`doc_node` headings via tree-sitter-md,
  src/ingest/mod.rs:91-98; `comment_node` strips `<!-- -->` grammar-backed —
  gen-plans-index.dl relies on it). Zero engine change for v1.
- `.dl` facts program (`arch_node("engine/tick", "why…").`) — rejected as doc
  format (prose in string literals is unwritable); retained as the derived-rel
  layer underneath.
- Fully generated doc — rejected (no hand-owned home for WHY; doc-first
  impossible); survives as Direction-2's draft file (§B.4), explicitly not the doc.

### A.2 Node identity — pinned id + slug fallback (rec)

A node id is a slash path `engine/tick/fixpoint`. **Pinned**: `<!-- @arch <id> -->`
in a doc declares the node; the heading above supplies the display title; renames
never rekey. **Slug fallback**: headings with no pinned marker still yield nodes,
id from the heading-nesting path (`lower`/`replace_re`, docs/reference/functions.md:12).
A slug-id node that acquires a binding gets an `arch-slug-bound` info diag
suggesting a pin. Hazard: `doc_node.parent` is heading **text** — duplicate
sibling titles are resolved to the nearest same-title heading above (`max`
aggregate, lint-docs.dl precedent); the dup-node rail (§B.5) catches real collisions.

### A.3 Tree projection — id slash-path surgery (rec)

`arch_parent("a/b/c") = "a/b"` reusing std/arch.dl's proven surgery — **not**
`doc_node` heading nesting. The tree is a property of the id space (stable
across doc reorganization); a code-first marker seeds the whole ancestor chain
before any doc exists; title-text parents are ambiguous on duplicates. Heading
nesting stays presentation. Numeric prefixes (`00-entry`, `01-parse`) carry
sibling order into lexicographic id order, so the tree index zone is a flat
id-ordered render. Implicit ancestors (`a/b/c` implies virtual `a`, `a/b`) are
`structural` nodes in the universe (§B.2).

### A.4 Prose attachment — file content, not a rel column

dl has no multi-line string aggregate (`count`/`sum`/`min`/`max` only), so v1
extracts one machine-readable artifact per node: `arch_summary(node_id, summary)`
= first non-empty line of the section (joined at `min(body_line)`). Full prose
is reached by jumping (diag/hover messages carry `file:line`). Deliberate scope
cut: the doc is the prose store; rels carry identity, structure, status.

## B. Binding layer — `std/archdoc.dl`

New std module: no scans, no `?`, mirrors `std/suppress.dl` conventions; the
importer heads the `arch_doc` contract rel (suppress.dl's lint_candidate pattern).

### B.1 Signatures

```dl
rel arch_doc(path: file).                 # the architecture doc file set (importer-headed)

rel arch_mark(path: file, line: int, col: int, end_col: int,
              node_id: text, form: text). # form ∈ "light" | "json"

rel arch_decl(node_id: text, file: file, line: int,
              title: text, id_kind: text).  # id_kind ∈ "pinned" | "slug"
rel arch_summary(node_id: text, summary: text).

rel arch_binding(node_id: text, path: file, line: int, col: int, end_col: int).

rel arch_parent(node_id: text, parent_id: text).
rel arch_order(node_id: text, ord: int).

rel arch_vtx(node_id: text, kind: text).  # "pinned" | "slug" | "seed" | "structural"
rel arch_cover(ancestor: text, descendant: text).          # recursive
rel arch_status(node_id: text, status: text, direct: int, subtree: int).
  # status ∈ "fulfilled" | "covered" | "unfulfilled" | "seed" | "structural"

rel archview_node(sym: text, name: text, kind: text, file: file, line: int, parent: text).
rel archview_edge(src: text, dst: text, kind: text).
```

Name-collision: std/arch.dl owns `arch_node(path, line, url)`; the universe rel
is `arch_vtx` so both modules import together. `diag` is headed with named args,
never redeclared (built-in 9-col schema, src/engine/decls.rs:253-261) — the
program merges cleanly into `.dl/` discovery mode.

### B.2 Key rules (pseudo-dl, regex bodies elided)

```
arch_mark(path, line, col, end_col, node_id, "light") <-
    comment_node(path, line, col, _, end_col, text, _),
    text =~ /^@arch\s+(?<node_id>[A-Za-z0-9_.\/-]+)\s*$/.
arch_mark(path, line, 0, 0, url, "json") <- arch_node(path, line, url).   # use "std/arch.dl"

arch_binding(node_id, path, line, col, end_col) <-
    arch_mark(path, line, col, end_col, node_id, _), !arch_doc(path).

# decls — pinned: mark in a doc file, title from nearest heading above (max agg)
# decls — slug: headings with no pinned mark, slug-path id; suppressed where a
#         pinned decl anchors the same section (antijoin)
# universe: declared ∪ (binding ids − declared = "seed") ∪ (ancestor closure =
#         "structural"); kind precedence pinned/slug > seed > structural

arch_cover(node, node) <- arch_vtx(node, _).
arch_cover(ancestor, descendant) <- arch_cover(ancestor, mid), arch_parent(descendant, mid).

arch_status(node, "fulfilled", direct, subtree) <- declared(node), direct > 0, ...
arch_status(node, "covered", 0, subtree)       <- declared(node), subtree > 0.
arch_status(node, "unfulfilled", 0, 0)         <- declared(node), no binding in subtree.
arch_status(node, "seed", direct, direct)      <- !declared(node), direct > 0.
```

### B.3 Surfacing — five channels, all dogfooded

1. **Diags at code sites** (the `@op` arc's shape): `info` at each binding mark's
   span, `code: "arch-binding"`, msg carries site count + decl `file:line`.
   Severity is free text; `info` maps to `DiagnosticSeverity::INFORMATION` — no
   engine change.
2. **Diags at doc nodes**: `arch-unfulfilled` (info) at the heading;
   `arch-seed` (info) at the mark; `arch-dup-node` (**error**, id declared
   twice); `arch-mark-malformed` (warn, near-miss first token — the
   `dl-directive-malformed` pattern, std/suppress.dl:239). Only errors trip
   `--check` exit 2, so doc-first work is never red.
3. **Hover**: `hover_note` on the mark span — title + `arch_summary` + site
   counts + decl `file:line`. Verify line-base against src/lsp.rs during
   implementation (rel line bases are undocumented: `comment_node` 1-based,
   diag/hover 0-based per decls.rs:246,268).
4. **Query verb**: `examples/arch-doc.dl` carries `? arch_status(…)` /
   `? arch_binding(…)`; A6 adds `dl q arch <node_id>` — the verb seam is an
   embedded `.dl` program + `q_target("<arg>")` injection (src/verbs.rs:1-20),
   catalogued via `verb_catalog`.
5. **Flow panel**: `archview_node`/`archview_edge` are convention-named, so the
   panel discovers an `archview` layer with zero preset edits (skill
   `sprf-flow-panel-graph-viewer`). Node `kind` = **status** (the legend is the
   fulfillment heatmap); edges `tree` (parent→child) + `fulfills` (node→site);
   site nodes keyed `"{path}:{line}"` carry `file`/`line` for click-to-jump.

### B.4 Gen transclusion — zones via per-row name templating (D2)

Pairing rule is absolute: **gen writes only inside `BEGIN:`/`END:` zones and
draft files; humans own everything else.** `GenTarget::Zone` renders path and
name per row as `{var}`-hole templates and groups rows by the rendered
`(path, name)` (gen.rs:261-285); `apply_zones` (gen.rs:488) resolves each
rendered name to a `BEGIN: <name>` line through the next `END:` line — any
comment prefix works, `END:`'s name is informational (gen.rs:516-517, 578-582).

```dl
# placed zones are visible to extraction (html comments in md are comment_node rows)
rel arch_zone(path: file, name: text).
arch_zone(path, name) <-
    comment_node(path, _, _, _, _, text, _), arch_doc(path),
    text =~ /^BEGIN:\s+(?<name>arch-sites:\S+)\s*$/.

# ONE rule fills every arch-sites:{node_id} zone a node author chose to place.
gen(:zone, doc_path, "arch-sites:{node_id}", "- {site_path}:{site_line}") <-
    arch_decl(node_id, doc_path, _, _, _),
    arch_binding(node_id, site_path, site_line, _, _),
    arch_zone(doc_path, zone_name), zone_name = "arch-sites:" + node_id.
```

The `arch_zone` join keeps the rule opt-in per node and avoids the
`gen :zone … not found` bail (gen.rs:516-517) for nodes whose author placed no
zone. Zones never nest (apply resolves name → first matching pair) — stated in
the module docs. Two singleton zones plus one generator, all precedent-backed:

- **Tree index** (`gen-readme.dl` shape): `- {node_id} — {status}, {subtree} sites`,
  id-ordered = DFS via the numeric-prefix convention.
- **Unplaced-seeds inbox**: every seed id + site list — the doc-visible
  "born from markers" backlog.
- **Draft generator** (Direction-2 bootstrap, opt-in): `gen("docs/arch/_seeds.draft.md", …)`
  assembling one stub section per seed (`## <last-segment>` + `<!-- @arch {node_id} -->`
  + site list) — the gen-reference.dl page-assembly precedent. The draft sits
  **outside** the `arch_doc` glob; a human promotes a stub by pasting it into
  the real doc, flipping `seed → pinned` through the same rels. Never auto-insert
  into the hand doc.

**Convergence / no-feedback proof obligation** (module header + a test): zone
content rows are markdown list items — never headings, `@arch` marks, or `BEGIN:`
lines — so extraction (`arch_decl`, `arch_zone`) is insensitive to gen output;
the draft file is outside the doc glob. The extraction fixed point is a pure
function of hand content + code marks; gen writes are byte-convergent
(`content == old` skip, gen.rs:187/345/374); a second tick writes nothing. Drift
rails read zone content only into diags — still no edge back into bindings.

### B.5 Drift rails

Gen is suppressed under `--check`/`--lsp`; diag rules are not. Forward + reverse
(gen-readme.dl:60-69 / gen-plans-index.dl:117-151 pattern): a binding absent
from its node's zone → `arch-zone-drift` error at the mark; a zone row matching
no binding → `arch-zone-orphan` error at the zone row. `dl examples/arch-doc.dl
--check` is green on a converged tree, exit 2 on dup/drift, silent on
seeds/unfulfilled (info).

### B.6 Staleness — riding the new machinery

- **Marked code edited/deleted**: `tick_paths` re-extracts the file's comment
  facts → (post-A4) the Comment family retracts the mark's `comment_node` row
  via `retract_rows` in the one render tx → `affected_derived`
  (src/engine/tick.rs:601-613) scopes `arch_mark → arch_binding → arch_status`
  for rebuild → binding row, info squiggle, and zone row all disappear **in the
  same tick**. The node flips back toward `unfulfilled`/`seed` automatically.
  Pre-A4 the identical outcome rides bespoke comment refresh + derived recompute;
  A4 makes the built-in half *delta*-reactive instead of reload-reactive.
- **Doc node removed**: heading/`@arch` mark disappears → `arch_decl` retracts →
  live bindings flip the node `pinned → seed`: `arch-seed` info at every mark,
  the unfulfilled dot vanishes with its anchor, the panel node re-colors.
  Delete-code and delete-doc are the same retraction walking two antijoins.
- **Instance lifetimes**: a binding lives exactly as long as its mark's comment
  row; a decl as long as its section; a status row is recomputed each tick; a
  seed is promoted the tick after a human declares it; gen zones converge in
  ≤2 ticks (byte-skip proves rest).

### B.7 Storage + uniqueness

No new tables — derived rels over existing built-ins (`comment_node`,
`doc_node`). Persisted derived tables land as `rel_arch_*` under `--db` like any
program. Uniqueness as rails: node id unique across the doc set
(`arch-dup-node`); one decl anchor per section (slug suppressed under pinned);
binding identity = full tuple `(node_id, path, line, col, end_col)`; ids
case-sensitive, charset `[A-Za-z0-9_.\/-]` enforced at mark parse
(`arch-mark-malformed`).

## C. Continuing the retraction line

### C.1 Families from day one — and not

- **Families (engine work): `comment_node` (Comment) and `doc_node`/`doc_ref`
  (Doc + DocRef)** — the two built-in extracts the feature stands on, today
  bespoke whole-table reloads (text.rs:48, doc.rs:14). They become the second
  consumer of the family pattern.
- **Not families: the `arch_*` rels.** Program-level derived datalog;
  incrementality is `affected_derived`-scoped recompute; true derived-layer
  retraction is Slice D of plans/2026-07-14-delta-reactivity-and-fact-ownership.md
  — explicitly out of scope. At doc-scale row counts, scoped recompute is
  correct and fast; the retraction story lives in the built-in layer.

### C.2 Why comment+doc is the right second consumer

The 2026-07-15 plan nominated `node` (CST) as consumer #2 ("zero new soundness
tests"). Comment+doc is better *now*: (1) **the feature consumes them** — the
cutover ships with a dogfood user and goldens that matter; (2) **it forces the
missing router feature** — `doc_ref` reads `type_entity` (doc.rs:42-52), a
*cross-family read edge*, the top item on the change-cost friction inventory;
`node` would prove generalization without forcing it; (3) smaller surface than
type/dataflow — no resolution machinery to host. Order: **comment+doc second
(this saga), node third** as the generalization proof on the by-then-general
router, then type → module → dataflow.

Structural prerequisites: (a) an **owned input layer** (`_comment_node` /
`_doc_node` tables mirroring the `_call_*` baseline); (b) `Family` impls (4
slots, src/engine/family/mod.rs:206-215) + `register_family!`; (c) **router
generalization** — `flip_call_rels_via_router` is hard-coded to
`call_families()` + `self.call_router` (src/engine/extract/call.rs:389-396);
(d) semantics hazards to pin with goldens *before* cutting: `comment_node` has
no rev column and cross-rev span dedup (text.rs:44-47, 81-84); `doc_ref`
derives empty when type rels are absent.

### C.3 Arc A3 signatures — router generalization + cross-family chaining

```rust
// src/engine/family/mod.rs — Family gains a read-only rel declaration:
fn read_rels(&self) -> &'static [&'static str] { &[] }   // public rels read, not owned

// src/engine/family/router.rs
impl FamilyRouter<'static> {
    /// Rerun families whose (owned inputs ∪ read rels) intersect `moved`;
    /// a family whose render moved rows feeds its public rels back into
    /// `moved`, rerunning families that read them. Each family reruns at
    /// most once per tick. Returns every rerun family incl. empty deltas.
    pub(crate) fn react_deltas_chained(
        &mut self, db: &Db, moved: &HashSet<&'static str>,
    ) -> Result<Vec<(&'static str, RowDelta)>>;
}

// src/engine/mod.rs — the one router; call_router retires into it.
family_router: RefCell<Option<family::FamilyRouter<'static>>>;

// src/engine/extract/mod.rs — sole-writer flip for every registered family;
// `moved` is seeded by ALL refresh paths (bespoke module/type/df refreshes
// report moved public rels exactly as tick.rs:406-410 does for call today).
pub(crate) fn flip_families_via_router(&mut self, moved: &HashSet<&'static str>) -> Result<()>;
```

Chaining loop: `pending = moved`; rerun families whose footprint ∩ pending;
collect non-empty deltas' public rels into pending; repeat until no new deltas;
render all in one tx. Termination by the rerun-once set (≤ F iterations,
F ≈ 12). Dep capture stays rel-footprint (per-row `DepKey` capture remains
future work).

### C.4 Arc A4 storage + read/write sequence

```sql
CREATE TABLE _comment_node(path_sid, rev_sid, line, col, end_line, end_col, text_sid, kind_sid,
                           PRIMARY KEY(path_sid, rev_sid, line, col)) WITHOUT ROWID;
CREATE TABLE _doc_node(repo_sid, file_sid, line, kind_sid, name_sid, parent_sid,
                       PRIMARY KEY(file_sid, line, kind_sid)) WITHOUT ROWID;
```

Write sequence per tick: (1) `refresh_comment_rels`/`refresh_doc_rels` keep
grammar extraction per moved file but stop writing public tables — they
set-diff DELETE + chunked `insert_rows` the owned tables (the
`persist_call_family` shape) and report moved input rels; (2)
`flip_families_via_router(moved)` reruns Comment/Doc/DocRef, reconciles,
renders retract+insert in the one tx; (3) cold process with no `_family_meta`:
derive from the owned baseline, `reload_rel` authoritative. Read sequence
unchanged: user rules, LSP, panel read public `rel_comment_node`/`rel_doc_node`/
`rel_doc_ref` — byte-identical rows proven by goldens frozen **before** the cut
(`tests/fixtures/`, sorted text-decoded TSV, the P3 pattern).

### C.5 The memo-spill arc — three steps, safe midpoint first (D3)

Current state: `FamilyMemo.rows` is a RAM `Vec<OutRow>` (router.rs:33-36);
`react_deltas` clones prior + fresh per rerun (router.rs:125-127); `reconcile`
is a full-tuple set-diff (`row_key`, NULL==NULL, family/mod.rs:125-142); cold =
re-derive from the owned baseline + authoritative `reload_rel`. Key fact:
post-capstone the family is **sole writer** of its public rel, renders commit in
one tx, so `rel_<name>` on disk is byte-equal to `memo.rows` after every render
— the memo is already redundant with SQLite.

- **M1 — reconcile against the public table (the safe midpoint).**
  `react_deltas` reads prior rows from `tbl(name)` (SELECT of out_cols) instead
  of `memo.rows.clone()`. Nothing else moves: `reconcile()` stays Rust, the
  fresh-derive Vec stays resident, `FamilyMemo.rows` stays populated — demoted
  to the differential oracle. Rail: table-read prior ≡ memo rows on every flip
  (asserts the sole-writer invariant; any non-family writer trips it). The
  absent-memo cold branch is subsumed: cold becomes "diff against what disk
  already has". Sketch: `fn prior_rows(db: &Db, family: &dyn Family) ->
  Result<Vec<OutRow>>`; `MemoMode { Resident, Spilled }` until the rail proves
  out, then always spill.
- **M2 — EXCEPT staging (both sides in SQLite).** Fresh derive streams into
  `TEMP TABLE _stage_<family>` (chunked `insert_rows`; col types mirror the
  public decl); `retracted = SELECT cols FROM rel_<f> EXCEPT SELECT cols FROM
  _stage_<f>`, `inserted` = the reverse; render `retract_rows`/`insert_rows` in
  the same tx. `EXCEPT` dedups (== reconcile's set semantics) and treats
  NULL==NULL (== `row_key`). `FamilyMemo.rows` is **deleted**; RAM per family
  drops to one insert chunk; `RowDelta` survives as
  `DeltaSummary { retracted: usize, inserted: usize, sample: Vec<OutRow> }`
  (bounded sample for tests). Sketch:
  `react_deltas_spilled(&mut self, db, moved) -> Result<Vec<(&'static str, DeltaSummary)>>`.
- **M3 — persisted render meta.**
  `CREATE TABLE _family_meta(family TEXT PRIMARY KEY, schema_version INTEGER,
  input_digest TEXT, rendered_tx INTEGER) WITHOUT ROWID;` — digest match ⇒ skip
  derive *and* render (retires the daemon-restart cold re-derive); mismatch ⇒
  M2 path + upsert meta, one tx; schema bump ⇒ `reload_rel`.

How far it goes: the built-in family layer goes all the way — memo, diff, meta
SQLite-resident; only footprints/digests stay process-resident. Not riding:
derived user rels (`rebuild_derived` stays DELETE+rebuild,
src/engine/derive.rs:359-361 — Slice D, a separate epic); extraction itself
(already file-scoped incremental via content-hash). Differential rails: M1's
table≡memo assert; M2's SQL-diff ≡ `reconcile()` property suite (the T1 tests,
family/mod.rs:270-376, generalize; the Rust reconcile stays as test oracle).

## D. Sequencing — six independently landable arcs

```
A1 (pure .dl feature) ──┬── A2 (dogfood sprefa)
                        └── A4 needs A1's fixtures as its consumer-oracle
A3 (router generalization) ──> A4 (comment/doc families) ──> A5 (memo spill M1→M2→M3)
A6 (verb + panel polish) anytime after A1
```

| arc | content | verification |
|---|---|---|
| **A1** `std/archdoc.dl` + `examples/arch-doc.dl` (pure .dl, zero Rust) | everything in §B: mark grammar (light + JSON via `use "std/arch.dl"`), decl/binding/tree/status, five channels, zones + draft generator, drift rails. Also fix-or-document the decls.rs:243 drift ("gen one-shot needs --apply" vs ungated `run_gens`, gen.rs:7) | new `tests/it/arch_doc.rs`: tmp fixture (mini doc + marked code), `arch_status` rows + `--diag-json` snapshot; edit scripts (add/remove mark → statuses flip; remove section → seed appears) — the recompute-path oracle A4 must preserve byte-for-byte; `--check` green, exit 2 on injected dup/drift; second run writes nothing; panel smoke (`archview` layer, site jump) |
| **A2** dogfood sprefa | write `docs/arch/*.md` (pinned ids `sprefa/engine/tick`, `sprefa/engine/family`, …); keep the 11 `ARCH {}` markers as first bindings (seeds until declared — Direction 2 working on day one); add `.dl/arch-doc.dl` to discovery | pre-commit `dl --check` covers arch drift; unplaced-seeds zone committed and honest; docs render |
| **A3** router generalization (Rust, behavior-preserving) | §C.3 signatures + moved-set plumbing from every refresh path | all existing family rails/goldens green untouched (`tests/it/call_golden.rs`, router unit tests src/storage/call.rs:1785-1937); new unit test: two synthetic families (A emits rel_x, B `read_rels` rel_x), chained rerun fires exactly once per family |
| **A4** Comment + Doc/DocRef cutover (Rust) | §C.4; goldens frozen first (`tests/fixtures/text_doc_golden/`, P3 pattern); rev/dedup semantics pinned by a named unit test | golden parity; new `tests/it/retraction_scripts.rs` — `delete_only_the_marked_file`, `add_and_remove_a_mark`, `doc_section_removed_flips_seed` vs the fresh-engine oracle; **A1's fixture tests rerun unchanged on the cut-over path** (the feature never notices — that is the proof) |
| **A5** memo spill M1→M2→M3 (Rust) | §C.5, each M its own commit | M1: table≡memo rail per flip; M2: SQL-diff ≡ `reconcile()` property rail, router tests re-aimed at the spilled path; M3: cold-restart rail (restart, no change ⇒ zero derives, zero writes, `--tick-audit` empty); perf vs the 73 ms/1000-file baseline |
| **A6** verb + polish (small) | `dl q arch <node_id>` (embedded program over `q_target`, catalogued in `verb_catalog`), maybe `dl q arch-seeds` | verb golden; panel status-color smoke |

Rationale: A1 first because the family work needs a consumer and its goldens;
A3 before A4 because the new families need the general router; A5 last because
the new families cut over on the *proven* RAM-memo path — one moving part per
arc. Inside A5, M1 lands first because it changes only where priors come from
while reconcile semantics stay in the proven Rust path: if the table-as-prior
read misbehaves, the arc bisects cleanly (flip `MemoMode` back) instead of
debugging a new SQL diff and a deleted memo at once.

## Open either-ors (recommendations embedded above)

1. **Doc format**: markdown + markers (rec) / `.dl` facts / fully-generated.
   <!-- todo(decision): arch-doc format — markdown + markers (rec) vs .dl facts vs fully-generated doc -->
2. **Node identity**: pinned `@arch` + slug fallback (rec) / slug-only / pinned-only.
   <!-- todo(decision): arch node identity — pinned @arch + slug fallback (rec) vs slug-only vs pinned-only -->
3. **Tree source**: id slash-path surgery (rec) / `doc_node` heading nesting.
   <!-- todo(decision): arch tree source — id slash-path surgery (rec) vs doc_node heading nesting -->
4. **Marker grammar**: one `@arch` token both sides, role by file context (rec) /
   separate decl token (`@node`, rejected — breaks the one-join symmetry) / JSON-only.
   <!-- todo(decision): arch marker grammar — one @arch token both sides (rec) vs separate @node decl token vs JSON-only -->
5. **Severities**: seed=info, unfulfilled=info (rec) / hint / warn; per-node CI-gate
   escalation (`{"url":…,"gate":"error"}` payload) deferred to a later arc.
   <!-- todo(decision): arch diag severities — seed/unfulfilled = info (rec) vs hint vs warn; per-node gate escalation deferred -->
6. **Doc-set declaration**: importer-headed `arch_doc` contract rel (rec) / hardcoded glob.
   <!-- todo(decision): arch doc-set declaration — importer-headed arch_doc contract rel (rec) vs hardcoded glob -->
7. **Direction-2 stubs**: unplaced-seeds zone + opt-in draft file (rec) / gen
   auto-inserts stubs into the hand doc (rejected for v1 — gen never writes
   outside zones).
   <!-- todo(decision): direction-2 stubs — unplaced-seeds zone + opt-in draft file (rec) vs gen auto-insert into the hand doc -->
8. **Spill depth**: full three-step arc (rec) / stop at M1 (RAM Vec stays,
   priors already spilled) / stop at M2 (skip persisted meta).
   <!-- todo(decision): memo-spill depth — M1+M2+M3 (rec) vs stop at M1 vs stop at M2 -->
9. **Doc-internal xref marks**: v1 treats every doc-file `@arch` mark as a decl
   (rec); xref-in-doc as a `form="xref"` binding variant deferred.
   <!-- todo(decision): doc-internal xref marks — every doc-file @arch mark is a decl in v1 (rec); form="xref" variant deferred -->

Beyond this saga (named, not planned): type→module→dataflow family cutovers;
Slice D derived-layer delta algebra; `_call_edge_support` deletion (capstone
follow-up); node (CST) family as the third consumer.

## Repo-laws checklist (stated for every arc)

- One rel = one rule kind: scans live in their own source rules;
  `comment_node`/`doc_node` joins only in derived rules (S6, gen-readme.dl:14
  pattern).
- Never a per-row write: chunked `insert_rows`/`retract_rows` only.
- Recompute guard satisfied: the SQL reconcile runs only on footprint-moved
  families and M3's persisted digest *is* the digest skip — no
  `// @recompute unguarded` waiver needed.
- Banned identifiers absent (`provenance`/`substrate`/`load-bearing`/`regime` —
  this plan says source/base/critical/mode; Slice-D's "per-output-row
  provenance" is quoted as *source tracking* if ever referenced).
- Descriptive dl variable names everywhere; no `?` in `std/` files; `diag`
  headed with named args, never redeclared.

## Verification

Per-arc gates are in the §D table. Cross-cutting:

- **New tests**: `tests/it/arch_doc.rs` (A1), `tests/it/retraction_scripts.rs`
  (A4), chained-rerun unit test (A3), spill property rails (A5).
- **Goldens frozen before every cut**: `tests/fixtures/text_doc_golden/` (A4);
  existing call goldens must stay green untouched (A3).
- **Oracles**: A1's fixture tests are the recompute-path oracle A4 preserves
  byte-for-byte; fresh-engine `assert_matches_oracle` for retraction scripts;
  Rust `reconcile()` stays as the M2 test oracle after deletion from the hot path.
- **Gates**: `dl examples/arch-doc.dl --check` (exit 2 on dup/drift only);
  `cargo test` full suite per arc; second-run-writes-nothing convergence test.

<!-- todo(perf): measure A5 wall time vs the 73 ms/1000-file baseline from plans/2026-07-15-family-derive-reactive-engine.md:187 once M3 lands -->

## Staffing

- One agent session per arc; arcs land as separate commits in dependency order
  (§D diagram). Rust arcs (A3–A5) in worktrees branched from the parent arc's
  merge; pure-.dl arcs (A1, A2) can share a tree.
- Base SHA at plan time: `601210fc`; the T4 property-suite changes to
  src/engine/family/router.rs and tests/it/retraction_props.rs have since
  landed as `2acf0c71` — A5 rebases over that.
- Suite budget: full `cargo test` + the arc's own gates per session; no
 time-boxed perf budget except the A5 baseline measurement above.
