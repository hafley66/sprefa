# D5 — rev-aware type/call/dataflow/doc extraction

Date: 2026-07-04. Predecessor: `plans/2026-07-03-pr-diff-graph.md` (D0–D5b done;
D5 is the engine arc that retires the worktree pair). Route: thread `rev`
through the four extraction families so `diff_pair` becomes `(rev, rev)` on ONE
checkout. In-tree template: `type_edge_rev` / `module_edge_rev` (rev-aware source
of truth; the rev-less rel is the deduped union). This plan extends that split to
the node/link/def/df rels the graph diff actually consumes.

## Goal, restated in one line

Today `type_entity` / `type_link` / `call_def` / `call_site` / `df_*` /
`doc_*` carry no `rev`; comparing two revs needs two physical checkouts
(the worktree-pair hack). Make extraction rev-aware → `diff_pair(base_rev,
head_rev)` on one tree, worktree pair retires, PR-as-rows composes (gh effect →
head sha → `scan` rev slot → graph diff without fetching a working tree).

## What is already rev-aware (do not touch, mirror it)

| rel | shape | legacy twin |
|---|---|---|
| `type_edge_rev(from,to,kind,rev,repo)` | source of truth | `type_edge` = union via `rebuild_legacy_type_rels` |
| `call_edge_rev(caller,callee,kind,rev)` | source of truth | `call_edge` = union via `rebuild_legacy_call_rels` |
| `module_edge_rev(src,dst,rev)` | source of truth | `module_edge` = union via `rebuild_legacy_module_rels` |
| `module_import(file,rev,…)`, `crate_edge(…,rev)`, `module_unresolved_rev(…,rev)` | rev as a plain column | — |

The edge rels chose the **twin** (rev-less legacy for `closure()`/`scc`
consumers, `_rev` for history). The richer already-multi-column source rels
(`module_import`, `crate_edge`) chose **rev-as-column**. D5 applies the twin
choice to every diff-consumed rel, because the diff-consumed rels all have many
positional readers (below) and adding a bare column would break them.

---

## Layer 1 — Type signatures first

### 1a. Rev attribution decision, per rel (twin vs column) — DECIDED

`dl` arity is exact: a positional reader `foo(a, b, c)` breaks the instant `foo`
grows a column. Reader counts (`grep -Eho '\brel\s*\(' **/*.dl`, 142 `.dl`
consumers):

| rel | positional readers | rev via | why |
|---|---|---|---|
| `type_entity` | **60** | **`type_entity_rev` twin** | node rel, 60 readers; twin protects all; sym is line-free → rev is pure attribution |
| `type_link` | 11 | **`type_link_rev` twin** | mirrors `type_edge`/`type_edge_rev` exactly |
| `call_def` | 18 | **`call_def_rev` twin** | node rel; same reason as `type_entity` |
| `call_edge` | 30 | **`call_edge_rev` EXISTS** | wire its legacy already done; nothing to add |
| `type_edge` | — | **`type_edge_rev` EXISTS** | done |
| `df_node` | **57** | **`df_node_rev` twin, id salted by rev** | id embeds `file:line:col`; raw ids collide cross-rev → salt (see 3c) |
| `df_field` | 16 | **`df_field_rev` twin, salted ids** | feeds the diff's `fill` member edges |
| `df_arg` | 11 | **`df_arg_rev` twin, salted ids** | feeds the diff's `param` member edges |
| `df_node_repo` | 4 | **`df_node_repo_rev` twin, salted ids** | repo attribution stays orthogonal to rev |
| `type_sig` | 11 | **defer (WORK-only)** | not in the diff's member_node/edge set; twin later if a signature diff is wanted |
| `call_site` | 13 | **defer** | raw graph, not diff-consumed as sym-keyed edge |
| `call_name` | 22 | **defer** | name-resolution helper; WORK view suffices |
| `call_kind` | 3 | defer | rail helper |
| `df_edge` | 6 | **defer** | flow closure input (taint/interproc run on WORK, not cross-rev) |
| `loop_over`/`allocates`/`nest`/`df_param` | 0/1/1/5 | defer | flow/perf inputs, WORK-only |
| `doc_comment`/`doc_tag`/`doc_node`/`doc_ref` | 3/0/2/2 | defer | doc diff is a later arc; type-family digest already gates docs |

Deferred rels stay exactly as they are (single-rev, populated at whatever revs
`_file` holds, deduped by their existing keys). The cut is honest: D5 = **graph
diff** (nodes + edges), so the diff-critical set is type_entity / type_link /
call_def / call_edge / df_node / df_field / df_arg / df_node_repo. Everything
else is a flow/doc input that runs on WORK and gains a twin only when a
rev-aware flow/doc diff is specced.

### 1b. New rel schemas (all `_rev` twins; RelDecl + reserved-name guard each)

```
type_entity_rev(repo, sym, name, kind, parent, file, line, rev)   // legacy type_entity = union drop rev
type_link_rev(src, dst, kind, rev)                                // legacy type_link  = union drop rev
call_def_rev(repo, sym, kind, file, line, end, rev)               // legacy call_def   = union drop rev
df_node_rev(id, kind, var, fn, file, line, rev)                   // id salted by rev; legacy df_node = union raw-id
df_field_rev(id, field, value, rev)                              // ids salted; legacy df_field = union raw-id
df_arg_rev(call, pos, arg, rev)                                  // ids salted; legacy df_arg = union raw-id
df_node_repo_rev(id, repo, rev)                                  // ids salted; legacy df_node_repo = union raw-id
```

The legacy rels keep their CURRENT schema byte-for-byte — no served reader
breaks. Legacy is rebuilt in Rust (`DELETE … ; INSERT OR IGNORE … SELECT …
FROM …_rev`), exactly like `rebuild_legacy_type_rels`. Both twin and legacy are
engine-populated builtins, so the one-rel-one-rule-kind law is untouched (legacy
is a Rust projection, not a dl derived rule).

### 1c. Changed Rust fn signatures (`src/engine/extract.rs`)

```rust
// per-rev input digest (was whole-corpus). One digest per (family, rev) so a
// rev whose files didn't move costs ~0. `files_at_rev` is the subset already
// filtered to one rev.
fn extract_input_digest(&self, family: &str, rev: &str,
                        files_at_rev: &[ExtractFile], with_scip: bool) -> [u8; 32];
//   key persisted as  "extract:<family>:<rev>"  in _reldigest.

// rev-scoped write helper (generalizes refresh_module_rels_for_revs's DELETE
// pattern to any twin): wipe only the named revs' rows, insert the fresh set,
// leave other revs' rows in place. Collect-then-flush, one insert_rows.
fn refresh_rel_for_revs(&self, rel: &str, cols: &[&str],
                        rows: &[Vec<Value>], revs: &[&str]) -> Result<()>;

// the four family refreshers keep their signature (`&self -> Result<bool>`)
// but internally iterate revs, skip unchanged, and write via
// refresh_rel_for_revs instead of refresh_rel. The bool is OR of per-rev moved.
pub(crate) fn refresh_type_rels(&self)     -> Result<bool>;
pub(crate) fn refresh_call_rels(&self)     -> Result<bool>;
pub(crate) fn refresh_dataflow_rels(&self) -> Result<bool>;

// resolution scoped per (repo, REV) not just (repo). New key arity:
//   by_name:  HashMap<(&str /*repo*/, &str /*rev*/, &str /*name*/), Vec<&str>>
//   sym_at:   HashMap<(&str, &str, &str /*rev*/, &str /*name*/), &str>
// resolve closure gains a `rev` param:
let resolve = |repo: &str, rev: &str, file: &str, name: &str| -> Option<String> {…};

// scip consulted ONLY at rev == "WORK" (SCIP indexes are working-tree
// artifacts; a committed rev has no index):
if rev == "WORK" { if let Some(def_file) = scip.get(&(repo, file, name)) { … } }

// rev-salt for df ids (mirrors spine::WhereBytesId::salted). raw id in, opaque
// per-rev id out; deterministic so intra-rev joins line up.
fn salt_rev(id: &str, rev: &str) -> String;   // e.g. blake3(id, rev) → short hex, or "{rev}\u{1}{id}"
```

`cached_facts` and the `FactCache<T>` key are UNCHANGED — the cache is already
`(repo, path, content-hash)`, content-addressed, so a file byte-identical across
two revs parses once and both revs share the `Arc<facts>`. Rev-awareness of the
cache is free (the task's observation); the work is the per-rev digest + the
per-rev resolution maps + the twin writes.

---

## Layer 2 — Pseudo-code bodies

### refresh_type_rels (representative; call/df identical shape)

```
files = extract_file_set()                         // (repo, path, rev, hash), already multi-rev
by_rev = group files by rev
moved = []
for (rev, frev) in by_rev:
    d = extract_input_digest("type", rev, frev, with_scip = (rev == "WORK"))
    if load_rel_digest("extract:type:"+rev) == Some(d): continue   // this rev unchanged → skip
    moved.push((rev, d))
if moved.empty(): return Ok(false)                 // whole family cost ~0

facts = cached_facts(type_facts_cache, files_of(moved_revs), …)   // content-keyed, shared across revs

// per-(repo, rev) resolution barrier (D5a's per-repo scoping extended by rev)
for (repo, path, rev, f) in facts:
    for e in f.entities:
        by_name[(repo, rev, e.name)].push_dedup(e.sym)
        sym_at[(repo, path, rev, e.name)] = e.sym
scip = scip_name_defs()                             // WORK-only, consulted by resolve when rev=="WORK"

// emit rows carrying rev (entity/link now carry rev; edge_rev already did)
for (repo, path, rev, f) in facts:
    entity_rev_rows.push([repo, qsym, name, kind, qparent, file, line, rev])
    for edge in f.edges: type_edge_rev_rows.push([… , rev, repo])       // unchanged
    for edge in f.edges: link_rev_rows.push([src, resolve(repo,rev,path,to), kind, rev])
    …type_sig unchanged (deferred: WORK rows only)…

revs = moved_revs
refresh_rel_for_revs("type_entity_rev", …, entity_rev_rows, revs)
refresh_rel_for_revs("type_edge_rev",   …, edge_rev_rows,   revs)   // was refresh_rel; now rev-scoped
refresh_rel_for_revs("type_link_rev",   …, link_rev_rows,   revs)
rebuild_legacy_type_rels()          // DELETE type_entity; INSERT OR IGNORE SELECT drop-rev FROM type_entity_rev (NEW: entity+link, not just edge)
for (rev, d) in moved: save_rel_digest("extract:type:"+rev, d)
return Ok(true)
```

### refresh_rel_for_revs

```
if revs.empty(): return
CREATE TEMP TABLE _rev_scope(rev PRIMARY KEY); DELETE FROM _rev_scope
insert_rows(_rev_scope, revs)
DELETE FROM {rel} WHERE rev IN (SELECT rev FROM _rev_scope)
insert_rows({rel}, cols, rows)      // rows already only the moved revs' rows
```

### df id salting (refresh_dataflow_rels)

```
for (repo, _, rev, f) in facts:
    for n in f.nodes:
        rid = salt_rev(n.id, rev)                         // rev folded into identity
        df_node_rev_rows.push([rid, n.kind, n.var, n.fn_sym, n.file, n.line, rev])
        df_node_repo_rev_rows.push([rid, repo, rev])
    for e in f.edges: df_edge stays WORK-only (deferred) OR salt both endpoints if promoted
    for (call,pos,arg) in f.args:  df_arg_rev_rows.push([salt(call,rev), pos, salt(arg,rev), rev])
    for (id,field,value) in f.fields: df_field_rev_rows.push([salt(id,rev), field, salt(value,rev), rev])
// legacy df_node = INSERT OR IGNORE SELECT (unsalted raw id) — but we only have salted here,
// so legacy df_node re-emits from the RAW n.id (kept alongside), matching today when rev="WORK" only.
```

The legacy df projection keeps the RAW `n.id` (today's behavior); the `_rev`
twin keeps the salted id. A single-rev daemon (rev = WORK only) sees legacy
df_node identical to today; a diff reads `df_node_rev` where the salt makes
base and head ids disjoint so the derived member-edge diff (name-joined, NOT
raw-id-joined) is honest.

---

## Layer 3 — Instance lifetimes and the sym-vs-column decision

| stateful piece | lifetime | rev handling |
|---|---|---|
| per-file `FactCache<T>` entry | across ticks; replaced each refresh with the current file set | key `(repo, path, hash)` — **rev NOT in key**; identical bytes at N revs = one entry, shared. Free rev-awareness. |
| `_reldigest` rows | durable | key `extract:<family>:<rev>` — **one row per (family, rev)**. Cardinality = watched revs (small). Swept when a rev leaves `_file` (D5.5). |
| `type_entity_rev` / `call_def_rev` syms | durable rows | sym = `{repo}::{file}::{kind}::{name}` — **line-free, rev is a COLUMN, NOT folded into the sym** |
| `df_node_rev` ids | durable rows | id = `salt_rev(file:line:col, rev)` — **rev IS folded into the id** (line-keyed) + a rev column for filter/retract |

**The crux (join consequences):**

- **type/call syms MUST keep rev as a column, never fold it in.** The diff's
  whole job is "is sym S present at head but not base" (`node_added`) — that
  requires S to be the SAME string at both revs. Fold rev into the sym and every
  node is simultaneously added-and-removed; the diff is useless. Intra-rev joins
  (`type_link_rev.src = type_entity_rev.sym` within one rev) work with the plain
  sym + a `rev` equality on both sides. Cross-rev joins (the diff) compare the
  sym across two rev values. This is exactly what `type_edge_rev` / `module_edge_rev`
  already do — rev is attribution, node identity is the sym.

- **df ids MUST fold rev in.** `df_node.id` embeds a line; the same `file:12:4`
  is different code at base vs head. Raw ids in `df_node_rev` would collide two
  revs' rows into one (the `_rev` table's uniqueness key is the id) and the flow
  graph would cross-wire base into head. Salting by rev makes ids disjoint per
  rev, so `df_edge_rev`/`df_arg_rev`/`df_field_rev` (if/when promoted) reference
  the right rev's nodes. The **diff never diffs raw df ids** — it diffs the
  NAME-joined member edges (fill/param/read) derived in `flow-panel.dl`, which
  are already line-free (owner_sym, member_name, kind). The salt is purely the
  table-integrity guard the pr-diff plan's "df ids MUST NOT be diffed raw" note
  demanded. This retires that residual.

---

## Layer 4 — Storage, read/write sequence, uniqueness

### Storage

- New tables: `type_entity_rev`, `type_link_rev`, `call_def_rev`,
  `df_node_rev`, `df_field_rev`, `df_arg_rev`, `df_node_repo_rev`. Created on
  open by reconcile (empty until first tick). Legacy tables unchanged.
- `_reldigest`: rows keyed `extract:type:WORK`, `extract:type:<oid>`,
  `extract:call:<rev>`, `extract:dataflow:<rev>` (doc deferred).

### Per-tick sequence (one family)

1. `extract_file_set()` → multi-rev corpus rows (already carries rev).
2. group by rev; per rev compute `extract_input_digest(family, rev, …)`.
3. skip revs whose digest == stored; collect `moved` revs. If none, return
   `Ok(false)` (family cost ~0 — the recompute guard, per rev).
4. `cached_facts` over the moved revs' files (content-keyed, parses only files
   whose hash is new to the cache).
5. build `(repo, rev)`-scoped resolution maps; `scip` consulted only at WORK.
6. emit twin rows carrying rev (df ids salted).
7. `refresh_rel_for_revs(twin, …, rows, moved)` — wipe only moved revs, insert.
8. rebuild legacy union (`DELETE` + `INSERT OR IGNORE SELECT` drop-rev/raw-id).
9. `save_rel_digest("extract:<family>:<rev>", d)` per moved rev (after writes,
   so a failed refresh retries).

### Uniqueness

| rel | row uniqueness |
|---|---|
| `type_entity_rev` | `(repo, sym, rev)` (sym already = repo-relative file::kind::name) |
| `type_link_rev` | `(src, dst, kind, rev)` |
| `call_def_rev` | `(repo, sym, rev)` |
| `df_node_rev` | `(id, rev)` where id is already rev-salted → id alone is unique; rev is a cheap filter column |
| legacy rels | `INSERT OR IGNORE` on the rev-less (or raw-id) projection = rev-deduped union |

Retraction (rev disappears from every scan rule): D5.5 sweep —
`DELETE FROM <twin> WHERE rev NOT IN (SELECT DISTINCT rev FROM _file)` and
`DELETE FROM _reldigest WHERE rel LIKE 'extract:%:'||rev` for the gone revs.
Runs once per tick before the legacy rebuild. (Module twins get the same sweep —
today they lack it; a rev that stops being scanned currently lingers.)

---

## Migration / compat

- **Existing dbs**: reconcile creates the empty `_rev` twins on open; first tick
  populates. **No legacy rel changes schema**, so every served reader keeps
  working with zero migration. This is the entire reason for the twin choice
  over a bare column.
- **Served `.dl` programs**: unchanged (they read legacy rels). A program opts
  into history by reading a `_rev` twin.
- **flow-panel / graph-diff consumers**: `plans/2026-07-03-pr-diff-graph.md`'s
  `.dl/graph-diff.dl` moves from prefix-strip-over-two-repos to rev-filter over
  one repo:
  - `diff_pair(base_slug, head_slug).` → `diff_pair(base_rev, head_rev).`
    (`diff_pair("main", "WORK").` for local; `diff_pair("<base_sha>",
    "<head_sha>").` for a PR).
  - `bare_edge(rev, a, b, kind) <- type_link_rev(a, b, kind, rev).` (no
    `replace_re` prefix strip — syms are already bare per-rev).
  - `edge_added(a,b,k) <- bare_edge(HEAD,a,b,k), !bare_edge(BASE,a,b,k).`
    with `diff_pair(BASE, HEAD)` binding the rev pair.
  - node diff over `type_entity_rev` / `call_def_rev` by rev.
  - df-derived member edges: `flow-panel.dl`'s `df_node_repo`/name joins become
    `df_node_repo_rev` + rev filter; the repo-scoping helpers stay (repo axis is
    orthogonal to rev — a multi-repo PR diff wants both).
- **The basename convention retires**: no more root-dir-basename sym prefixes,
  no `diff_pair` carrying folder basenames, no `SPREFA_CONFIG` two-root
  `diff.config.toml`. One root, two revs.

## SCIP — explicitly OUT of scope (permanent constraint, not retired)

SCIP `index.scip` is a **working-tree artifact** — `dl index` runs an indexer
over the checked-out files, so `scip_ref` describes rev = WORK only. There is no
per-rev SCIP and D5 does not invent one.

- `resolve()` consults `scip` **only when `rev == "WORK"`**; committed revs use
  the syntactic per-(repo, rev) name-unique fallback (D5a's fix, now rev-scoped).
- `extract_input_digest` folds `scip_ref` **only into the WORK rev's** digest
  (a committed rev's resolution can't move when the index changes).
- **Asymmetry hazard**: a `diff_pair("main", "WORK")` gives the WORK side a
  scip-backed resolution the committed side lacks → the diff could report edges
  that differ only in resolution precision, not in code. Recommendation
  (open-question 2): a diff compares committed-vs-committed by default; when a
  diff is active and one side is WORK, the diff program sets syntactic-only
  (scip suppressed for both sides) to keep resolution symmetric — matching D3's
  final posture that the diff must compare like-with-like.

---

## Task queue

| id | task | size | depends |
|---|---|---|---|
| **D5.1** | per-rev digest infra: `extract_input_digest` gains `rev`; `extract:<family>:<rev>` keys; per-rev skip loop; `refresh_rel_for_revs` helper | M | — |
| **D5.2** | type family rev-aware: `type_entity_rev` + `type_link_rev` twins, per-(repo,rev) resolution, extend `rebuild_legacy_type_rels` to entity+link, RelDecls + reserved guards | M | D5.1 |
| **D5.3** | call family rev-aware: `call_def_rev` twin, per-(repo,rev) resolution, wire `call_edge_rev` legacy (exists); RelDecls + guards | S | D5.1 |
| **D5.4** | df family rev-aware: `df_node_rev`/`df_field_rev`/`df_arg_rev`/`df_node_repo_rev` twins with `salt_rev`, thread salt across every df id reference; legacy keeps raw id | L | D5.1 |
| **D5.5** | rev-retraction sweep across all `_rev` twins (incl. module) + stale `extract:*:<rev>` digest keys | S | D5.2–D5.4 |
| **D5.6** | SCIP-at-WORK-only guard: `resolve()` scip branch gated on `rev=="WORK"`; digest folds scip only into WORK's per-rev digest | S | D5.2, D5.3 |
| **D5.7** | consumer swap: `.dl/graph-diff.dl` → `diff_pair(base_rev, head_rev)` over `*_rev` twins; retire prefix-strip + worktree pair; flow-panel diff preset reads twins by rev | M | D5.2–D5.4 |
| **D5.8** | PR-as-rows: gh effect → head sha → `scan` rev slot populates `_file` at that rev → diff without a working tree | S | D5.7 |
| **D5.9** | tests: two-revs-one-checkout exact diff counts (the D4 four scenarios on the rev axis), per-rev digest-skip assertions, df cross-rev id-disjointness | M | D5.2–D5.7 |

Critical path: D5.1 → {D5.2, D5.3, D5.4 parallel} → D5.5/D5.6 → D5.7 → D5.8/D5.9.

## Retirements (what D5 kills)

1. **Worktree-pair hack** (`plans/2026-07-03-pr-diff-graph.md` D0, the second
   `git worktree add ../sprefa-base` + two-root `diff.config.toml`) — replaced
   by `scan(base_rev)` + `scan(WORK)` on one checkout.
2. **df WORK/HEAD ambiguity residual** (D5b OPERATIONAL note; "df-node ids embed
   line numbers and MUST NOT be diffed raw") — retired by rev-salted df ids +
   name-joined member-edge diff.
3. **`diff_pair(slug, slug)` basename convention** — replaced by
   `diff_pair(rev, rev)`; root-dir-basename sym prefixes gone.

## NOT retired (out of scope)

- **SCIP worktree-only limitation** — SCIP indexes are working-tree artifacts;
  rev-aware SCIP is explicitly out of scope. Non-WORK revs resolve syntactically
  (per repo, rev). Documented as a permanent constraint (see SCIP section).
- **The syntactic per-repo double-registration workaround** (D5a's def-sym
  dedup) — orthogonal to rev; stays.
- **Deferred rels** (`type_sig`, `call_site`, `call_name`, `call_kind`,
  `df_edge`, `loop_over`, `allocates`, `nest`, `df_param`, all `doc_*`) — WORK
  only; gain twins in a later flow/doc-diff arc.

## Open questions (recommendation each)

1. **df twin scope** — all 9 df rels rev-aware, or just the diff-critical
   node/field/arg/node_repo subset? **Rec: subset now** (D5.4). Flow analyses
   (taint, interproc) run on WORK; promoting `df_edge`/`loop_over`/`nest`/
   `df_param` is a follow-on when a rev-aware flow diff is specced. The salt
   helper is written to make promotion mechanical.
2. **SCIP asymmetry when a diff spans WORK** — accept WORK's scip advantage, or
   force syntactic-only? **Rec: syntactic-only when `diff_pair` is active and
   either side is WORK** (symmetric resolution), else compare committed-vs-
   committed. A one-line gate in the diff program, no engine change.
3. **legacy union semantics with multiple revs present** — a multi-rev db's
   `type_entity` is a superimposition of every scanned rev. **Rec: keep
   rev-deduped union** (matches `type_edge_rev` precedent); document that legacy
   rels are closure/point-query targets for the single-rev (WORK) daemon, and a
   diff program reads the `_rev` twins. A single-rev daemon sees legacy ==
   today.
4. **per-rev digest key cardinality** — a db watching many revs accretes
   `extract:*:<rev>` rows. **Rec: bounded by watched revs (small); the D5.5
   sweep drops keys for revs that leave `_file`.** No cap needed.
5. **`type_sig` / `call_site` rev-awareness** — needed for a signature/site-level
   diff, not the node/edge graph diff. **Rec: defer;** add `type_sig_rev` /
   `call_site_rev` twins only when that diff is specced (cheap, same template).
