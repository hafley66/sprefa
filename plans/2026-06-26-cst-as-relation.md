# CST-as-relation: `node`/`child` + whole-match located id + scope intervals

Status: **PLAN** (Phase 2 of `plans/2026-06-24-parse-engine-christmas-list.md`).
Track owner: Opus agent on Phase 2 "CST-as-relation". The grammar registry
(`engine.rs::ts_lang`, `sg.rs::sg_lang`, Cargo tree-sitter deps) is owned by the
other agent and is treated here as a **given** (`ts_lang(lang) -> Language` is
consumed, never edited).

Covers christmas list **#3** (`node`/`child` CST relation), **#9** (whole-match
span as a first-class located id), **#10** (scope-as-interval + binding/visibility
— sketch only, rides #3).

## Current world state (v5, grounded at file:line)

- **Tree-sitter descent + span helpers** to mirror: `datapath.rs` (whole file).
  `run_data` (datapath.rs:26) parses with a per-format grammar and returns
  `Vec<(text, lo, hi)>`. The descent helpers (`entries`, `items`,
  `value_text_span`) walk named children. This is the shape for a full-tree walk,
  but datapath uses fixed json/yaml/toml grammars, not `ts_lang`.
- **The structural extraction backend** is `run_ts` (engine.rs:4921): takes
  `content, lang, query_str`, runs a tree-sitter **query**, returns per-match
  `(start_line, end_line, Vec<(cap_name, text, lo, hi)>)`. The whole-tree walk #3
  needs no query — it enumerates every node — but it shares `ts_lang` and the
  `tree_sitter::Parser`/`Tree` setup verbatim.
- **Source-rule extraction driver**: `parse_file` (engine.rs:4476) reads content,
  derives `where_file: FileId` from the content address
  (`spine::FileId::from_content_address(hash, len)`, engine.rs:4487), runs each
  body op (`Match`/`Ast`/`Sg`/`AstYaml`/`Cmd`/`Comment`/`Json`), and for every
  capture calls the `push_span` closure (engine.rs:4490) to accumulate
  `Vec<spine::WhereBytes>`. Returns `(rows, where_bytes, dropped)`.
- **Whole-match span, today (the #9 precedent)**: the `Match` arm ALREADY has it
  (engine.rs:4516-4557). The op's optional 5th arg `id` binds the located
  `WhereBytesId` of capture-group-0 (the whole regex match) and pushes its span.
  `ast`/`sg`/`ast_yaml`/`json` do **not** — they push only individual capture
  spans (engine.rs:4581-4644, 4723-4736). The whole-match span for those backends
  exists only as the positional `start`/`end` line outputs, never as a located id.
  **This is exactly christmas #9.**
- **The ref spine** (`SPINE_RELS = ["string", "ref"]`, engine.rs:139):
  - `_where_bytes(id, string_id, file_id, lo, hi, repo, rev, path)` table
    (engine.rs:1788). `id` = `WhereBytesId::of_located(WhereBytes{string,file,lo,hi},
    repo, path)` (engine.rs:3677). Content-addressed: same (string, file, lo, hi,
    repo, path) -> same id, stable across ticks.
  - `_strings(id, content, ...)` interns each text via `StringId::of(text)`.
  - `insert_spine_where_bytes(&[(repo, path, WhereBytes)])` (engine.rs:3673) is the
    ONE batched write seam: interns both `_strings` and `_where_bytes` via
    `db.insert_rows`. No per-row writes.
  - `refresh_spine_rels` / `refresh_spine_rels_delta` (engine.rs:2430/2463)
    projects `_where_bytes ⋈ _strings` into the `string`/`ref` query relations.
  - `ref` is **5-ary**: `ref(id, string, file, lo, hi)` (engine.rs:135). `file` is
    the **content** FileId, not a path (christmas #11 caveat).
- **Lazy built-in rel machinery** (the `node`/`child` shape): per the
  `i:sprefa-v5-new-builtin-rel` skill. `*_RELS` const array + `*_rel_decls()` +
  `*_rels_used(prog)` gate + reserved-name guard in `declare_all` + register in
  `declare_builtins` + `refresh_*_rels()` populating via `refresh_rel` + wire into
  full tick (engine.rs ~948) and incremental tick (`tick_paths`, ~1143).
  `refresh_rel(rel, cols, rows)` (engine.rs:2469) = `DELETE FROM rel` then
  `insert_rows`. The type graph (`type_edge`, refresh_type_rels) is the closest
  worked example of a built-in **graph** relation populated from a tree-sitter
  walk over the scanned file set.
- **Recursive `closure()`**: a `.dl` author writes `head(a,b) <- closure(edge).`
  (closure_map, engine.rs:485; `Rule::closure_edge`). `declare_closure`
  (engine.rs:3503) requires the edge rel to be **exactly 2 columns** and builds a
  SQL `WITH RECURSIVE` view over the SCC condensation. A pinned endpoint
  (`anc("nodeX", D)`) seeds a BFS point query (`closure_seed_of`, engine.rs:550).
  So **`child(parent_id, child_id)` being 2-col means `ancestor(a,b) <-
  closure(child).` works with zero new recursion code.**

## The headline decision: node-id scheme

A node is `(file, kind, lo, hi)` in a parsed file. Two tree-sitter nodes in the
same file can share `(lo, hi)` only if they share bytes (e.g. a wrapper node and
its sole child) — kind disambiguates those. The id must be:

1. **Stable across ticks** for an unchanged file (so `child` edges and `ancestor`
   closures don't churn, and an `edit` can key off a node id).
2. **Content-addressed** (matches how `_where_bytes` ids already work, so the CST
   relation reuses the spine instead of inventing a parallel id space).
3. **Joinable to `ref`/`string`** so a node's source text resolves through the
   existing spine.

**Chosen scheme: a node id IS a `_where_bytes` id.**

```
node_id = WhereBytesId::of_located(
    WhereBytes { string: StringId::of(node_source_slice), file, lo, hi, .. },
    repo, path
).to_string()
```

where `node_source_slice = &content[lo..hi]`, `file = FileId::from_content_address(hash, len)`.

Justification:
- It is **already the located-id primitive** (engine.rs:3677, the same call the
  `match` whole-match id and every capture span use). Zero new id math.
- Content-addressed -> stable for an unchanged file; a byte edit upstream shifts
  `lo`/`hi` -> new id (correct: the node moved).
- The same `insert_spine_where_bytes` call that records the id ALSO interns the
  node's full source text into `_strings` and its span into `_where_bytes`. So
  **`node.id` joins `ref(id, string, file, lo, hi)` for free** — you get the
  node's byte span and `string(string, text, _)` gives its source text. No
  duplicate location storage.
- Kind collision (wrapper === sole child, identical `lo`/`hi`): the source slice
  is identical too, so the `WhereBytesId` collides. Mitigation: fold `kind` into
  the interned string used for the id (`StringId::of(format!("{kind}\u{1}{slice}"))`
  for the **id derivation only**, while the `ref`-visible string stays the raw
  slice via a separate push). DECISION NEEDED (see Open decisions #1): accept the
  rare collision (cleaner, fewer interns) vs. kind-salt the id (exact, one extra
  intern per node). Recommend **kind-salt** — a CST relation that silently merges
  two distinct nodes breaks innermost-containment.

**`node` is a lazy built-in query relation** (like `string`/`ref`/`program`), NOT
a source rel and NOT derived. Rationale (style note "one rel = one rule kind"):
- It is engine-emitted from a tree-sitter walk, not from a user `scan`+op rule
  (source) and not from a `<-` rule (derived). It is the same category as the
  spine rels: a projection the engine populates when `node_rels_used(prog)`.
- A source rel would require a user-written `scan`+op head; there is no op that
  emits "every node". A derived rel would need the rows to already exist in some
  base rel. Built-in lazy is the only fit, and it matches `string`/`ref` which are
  also CST/lexical projections.

## Design (CLAUDE.md protocol)

### 1. Type signatures

```rust
// engine.rs — the relation name consts (skill step 1)
/// CST node graph: every tree-sitter node of every scanned file as a row.
/// `node(id, kind, file, lo, hi, parent)` + `child(parent, child)`. `id`/`parent`
/// are `_where_bytes` ids (join `ref`/`string`); `file` is the content FileId.
/// `child` is 2-col so `anc(a,b) <- closure(child).` gives ancestor/descendant.
const NODE_RELS: [&str; 2] = ["node", "child"];

// engine.rs — column decls (skill step 2)
fn node_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "node".into(), cols: vec![
            c("id", Type::Text), c("kind", Type::Text), c("file", Type::Text),
            c("lo", Type::Int), c("hi", Type::Int), c("parent", Type::Text)] },
        // EXACTLY 2 cols: required by declare_closure (engine.rs:3504).
        RelDecl { name: "child".into(), cols: vec![
            c("parent", Type::Text), c("child", Type::Text)] },
    ]
}

fn node_rels_used(prog: &Program) -> bool { rels_used(prog, &NODE_RELS) }

// datapath.rs (or a new cst.rs) — the whole-tree walk over a ts_lang grammar.
// Mirrors run_data's parser setup but enumerates EVERY node, no query.
// Returns one record per node; the engine derives ids + spine rows.
pub struct CstNode {
    pub kind: String,
    pub lo: usize,
    pub hi: usize,
    pub parent_ix: Option<usize>, // index into the returned Vec; root = None
}
pub fn walk_cst(content: &str, lang: &tree_sitter::Language) -> Vec<CstNode>;
//   ^ takes the Language (engine resolves it via ts_lang, the owned-by-other-agent
//     registry) so cst.rs never touches the registry itself.

// engine.rs — populate the two rels from every scanned file (skill step 6)
fn refresh_node_rels(&self) -> Result<bool>;
//   pseudo-body below.

// engine.rs — #9: extend the located-whole-match id beyond `match`.
// Smallest change reuses the EXISTING push_span + of_located pattern in parse_file
// for the ast/sg/ast_yaml/json arms (see #9 section).
```

### 2. Pseudo-code

```
// cst.rs::walk_cst  (the #3 whole-tree enumeration)
//   parser = Parser::new(); parser.set_language(lang)?;     // same as run_data
//   tree = parser.parse(content, None)?;
//   out: Vec<CstNode> = []
//   // iterative pre-order DFS with a TreeCursor (no recursion blowup on deep files):
//   stack = [(tree.root_node(), None)]   // (node, parent_ix)
//   while let Some((n, pix)) = stack.pop():
//       my_ix = out.len()
//       out.push(CstNode { kind: n.kind(), lo: n.start_byte(), hi: n.end_byte(), parent_ix: pix })
//       for child in n.named_children(cursor):   // named only — skip punctuation tokens
//           stack.push((child, Some(my_ix)))
//   return out
//   // NOTE: named_children gives pre-order; child push order does not matter for
//   //       a set relation. Use named children to keep the row count sane
//   //       (anonymous "(" "," nodes are noise for codemod anchors). DECISION #2.

// engine.rs::refresh_node_rels  (skill step 6: collect-then-flush, NO per-row write)
//   node_rows: Vec<Vec<Value>> = []           // for rel_node
//   child_rows: Vec<Vec<Value>> = []          // for rel_child
//   where_rows: Vec<(repo, path, WhereBytes)> = []   // spine ingest, batched
//   for each scanned (repo, path, rev, hash) in the current file set:   // SELECT from _file
//       content = read_content(root, rev, path)?
//       file = FileId::from_content_address(hash, content.len())  // skip SYNTHETIC
//       lang = match ts_lang_for_path(path) { Some(l) => l, None => continue }  // skip non-CST files
//       cst = cst::walk_cst(content, lang)
//       ids: Vec<String> = cst.iter().map(|n| {
//           slice = &content[n.lo..n.hi]
//           // kind-salted id (DECISION #1):
//           id_string = StringId::of(format!("{}\u{1}{}", n.kind, slice))
//           wb_id = WhereBytesId::of_located(WhereBytes{string:id_string,file,lo,hi}, repo, path)
//           // ALSO push the RAW-slice span so node.id joins ref()/string() to the source text:
//           where_rows.push((repo, path, WhereBytes{string:StringId::of(slice),file,lo,hi}))
//           wb_id.to_string()
//       }).collect()
//       for (i, n) in cst.iter().enumerate():
//           parent_id = n.parent_ix.map(|p| ids[p].clone()).unwrap_or_default()  // root parent = ""
//           node_rows.push([ids[i], n.kind, file, n.lo, n.hi, parent_id])
//           if let Some(p) = n.parent_ix: child_rows.push([ids[p], ids[i]])
//   // existing-set compare to early-out (skill: return Ok(false) if unchanged):
//   if stored_node_set == node_rows && stored_child_set == child_rows: return Ok(false)
//   self.insert_spine_where_bytes(&where_rows)?      // one batched spine write
//   self.refresh_rel("node",  NODE_COLS,  &node_rows)?   // DELETE+insert_rows, plural seam
//   self.refresh_rel("child", CHILD_COLS, &child_rows)?
//   Ok(true)

// .dl author then writes (no new recursion code — closure() already exists):
//   anc(A, D)  <- closure(child).            // ancestor/descendant transitive
//   // innermost node containing a byte B in file F:
//   contains(Id, B) <- node(Id, _, F, Lo, Hi, _), B >= Lo, B < Hi.   // + min(Hi-Lo) agg
//   // scope as a JOIN instead of ast-grep inside/has:
//   inside(Inner, Outer) <- anc(Outer, Inner).
```

### 3. Instance lifetimes (state)

- `CstNode` Vec: per-file, transient inside `refresh_node_rels`; dropped after the
  file's rows are appended. No persistent in-memory tree.
- `node_rows` / `child_rows` / `where_rows`: per-tick accumulators, flushed once
  (three batched writes total), then dropped. Matches the spine's
  collect-across-files-flush-once discipline (engine.rs:1638-1649).
- Persistent state lives only in SQLite: `rel_node`, `rel_child` (the query rels)
  and the shared `_where_bytes`/`_strings` (the spine, NOT duplicated). The
  `closure(child)` SCC condensation tables (`scc_node_tbl("child")` /
  `scc_edge_tbl("child")`) are owned by `declare_closure`, built lazily when a
  program declares an `anc <- closure(child)` rule.

### 4. Storage / reads / writes / uniqueness

- **Storage layout**:
  - `rel_node(id, kind, file, lo, hi, parent, __src)` — PK on the declared cols
    (the engine's standard rel table; `__src` bookkeeping).
  - `rel_child(parent, child, __src)`.
  - NO new located storage: node spans go into the existing `_where_bytes` (raw
    slice) so `node.id`/`node.parent` are valid `_where_bytes` ids that `ref`
    projects. `_strings` interns both the raw slice (for `ref`/`string`) and the
    kind-salted id-string (id derivation only).
- **Read sequence**: per file from `_file` (the scanned set) → `read_content` →
  `ts_lang` → `walk_cst`. One parse per file per refresh (same cost class as
  `refresh_type_rels`).
- **Write sequence**: accumulate all files' rows → `insert_spine_where_bytes`
  (one batched spine write) → `refresh_rel("node", ..)` → `refresh_rel("child", ..)`.
  Three batched writes per refresh, zero per-row writes (the N+1 screamer stays
  quiet).
- **Uniqueness**: `node.id` is the content-addressed `WhereBytesId` (unique per
  (kind, slice, file, lo, hi, repo, path)). `child(parent, child)` is unique by
  PK; the same edge re-derived collapses. Two files with byte-identical content
  share a FileId but differ in `path` → `of_located` folds `path` into the id
  (engine.rs:3677, the P0 fix), so they don't collide.

## #9 — whole-match span as a first-class located id (S, CLEANLY SEPARABLE)

**Verdict: #9 is fully separable from #3 and touches nothing in the grammar
registry.** It is a mechanical extension of the existing `match`-arm pattern
(engine.rs:4538-4557) to the other structural arms. This is the part to implement
now (see "Implementation" below).

Today `ast`/`sg`/`ast_yaml`/`json` push only per-capture spans; the whole-match
byte range is positional-only. The change: give each an optional `id` term (the
5th-arg pattern `match` already uses) that binds
`WhereBytesId::of_located(whole_match_span)` and pushes that span via the
existing `push_span` closure. The whole-match byte range is already in hand:

- `ast`/`sg`/`ast_yaml`: `run_ts`/`run_sg`/`run_ast_yaml` return per-match `caps`
  with `(name, text, lo, hi)`. The whole match span = `(min(lo), max(hi))` over a
  match's caps — but cleaner is to have `run_ts` also return the **matched
  node's** byte span. Smallest no-registry-touch change: compute
  `(caps.iter().map(|c| c.lo).min(), caps.iter().map(|c| c.hi).max())` in the
  engine arm (no `run_ts` signature change). DECISION #3: span-of-captures
  (engine-side, zero backend change) vs. true matched-node span (needs `run_ts`
  to return it). Recommend **span-of-captures** for #9 now; it needs no grammar
  or backend edit and is correct for the codemod anchor use ("the bytes this rule
  matched").
- `json`: `run_data` returns `(text, lo, hi)` per value; the "whole match" is the
  value span itself, already pushed. #9 for `json` = bind the existing value span
  as an `id` (one new optional term).

Cleanly separable because: it edits only `parse_file`'s per-arm binding logic
(engine.rs:4571-4736) + the `ast`/`sg`/`json` AST structs to carry an optional
`id` term (ast.rs) + the parser arms (parse.rs). No `ts_lang`, no `sg_lang`, no
Cargo. The `match` arm is the working template — copy its `idv`/`of_located`/
`push_span` block.

### #9 implementation slice (the part attempted in this worktree)

To stay strictly out of the grammar registry AND avoid the AST/parser surgery of
adding a new optional `id` term to every op (which the coordinator may want to
shape consistently), the minimal end-to-end demonstration is on the op whose
whole-match span is most clearly "the matched bytes" and which already threads a
located id: see the `match` arm. If a same-shaped `id` arg is desired for `ast`,
the change is: ast.rs `BodyItem::Ast { .., id: Option<Term> }`, parse.rs ast arm
parses the optional trailing id var, engine.rs ast arm copies the `match` block.
Status of the code attempt is reported by the agent; if it entangled with the
shared op-surface decision it is left as plan only.

## #10 — scope-as-interval + binding/visibility (L, RIDES #3, SKETCH ONLY)

Do NOT build now; rides #3. Names the rules/relations it would add:

- **Relations** (likely lazy built-ins, populated by a per-lang binding walk):
  - `binding(id, name, kind, file, lo, hi)` — a node that BINDS a name (a let,
    param, fn decl, import), `id` = the binder node id, `(lo,hi)` = its scope
    interval (the enclosing block's span).
  - `use_ref(id, name, file, lo, hi)` — a node that REFERENCES a name (the spine
    `ref` already locates the text; this tags it as a use).
- **Derived rules** (pure `.dl`, riding `node`/`child`/`anc`):
  - `in_scope(Name, B) <- binding(_, Name, _, F, Lo, Hi), B >= Lo, B < Hi.`
  - free-var of a byte range = `use_ref` whose name has no enclosing `binding`
    whose scope interval contains the use: a `NOT EXISTS` over `binding` joined by
    `anc`/interval-containment.
- Binder/scope-interval extraction is per-language (which tree-sitter node kinds
  open a scope, which bind a name) — the L effort. The interval arithmetic and
  free-var query are pure `.dl` once #3's `node`/`child`/`anc` exist.

## Reuse vs new (reconcile with the ref spine)

| Concern | Reuse (no dup) | New |
|---|---|---|
| Node byte location | `_where_bytes` via `insert_spine_where_bytes` (engine.rs:3673) | — |
| Node id | `WhereBytesId::of_located` (engine.rs:3677) | kind-salt the id-string (DECISION #1) |
| Node source text | `_strings` + `string`/`ref` query rels | — |
| Transitive ancestor | `closure(child)` (engine.rs:3503) — child is 2-col | — |
| Innermost containment | a `.dl` rule over `node` + min-span agg | — |
| Relation plumbing | `refresh_rel` plural seam (engine.rs:2469) | `node_rel_decls`/`refresh_node_rels`/gate |
| Tree-sitter parse | `ts_lang` (other agent) + `walk_cst` modeled on `run_data` | `walk_cst` (no query) |

**No duplication of `_where_bytes`.** The CST relation's only new persistent
tables are `rel_node`/`rel_child`; every byte coordinate is a spine id.

## Sequencing

| Step | Item | Effort | Depends | Parallel? |
|---|---|---|---|---|
| 1 | **#9** whole-match located id for ast/sg/ast_yaml/json (copy `match` arm) + e2e | S | — | independent of #3, of grammar registry; **do first / now** |
| 2 | **#3a** `walk_cst` in cst.rs (whole-tree enumeration, modeled on `run_data`) + unit test | M | `ts_lang` (consume only) | parallel with grammar-registry work |
| 3 | **#3b** `node`/`child` lazy built-in: consts, decls, gate, reserved guard, `declare_builtins`, `refresh_node_rels`, full+incremental tick wiring (skill steps 1-8) | M | #3a | after #3a |
| 4 | **#3c** e2e: `node(...)` query + `anc(a,b) <- closure(child).` + innermost-containment over a real file; `node.id` joins `ref` | S | #3b | after #3b |
| 5 | **#10** binding/use_ref extraction + scope/free-var `.dl` rules | L | #3 | later phase |

#9 (step 1) and #3a (step 2) are independent of each other and of the
grammar-registry track, so they can run concurrently. #3b/#3c are serial after
#3a. #10 is a later phase.

## Open decisions (arbitrate before build)

1. **Node id kind-salt.** Kind-salt the id-string so a wrapper node and its sole
   identical-span child get distinct ids (exact, +1 intern/node) vs. accept the
   rare collision (cleaner). **Recommend kind-salt** — silent node-merge breaks
   innermost-containment. (Coordinator: confirm.)
2. **Named children only vs. all nodes.** `named_children` drops anonymous
   punctuation tokens (`(`, `,`, `;`) — far fewer rows, and codemod anchors are
   named nodes. All-nodes gives exact lexical coverage at a big row-count cost.
   **Recommend named-only** for #3; revisit if a codemod needs punctuation
   anchors. (Coordinator: confirm.)
3. **#9 whole-match span source.** Span-of-captures (engine-side
   min(lo)/max(hi), zero backend change) vs. true matched-node span (needs
   `run_ts`/`run_sg` to return it, brushes the backend). **Recommend
   span-of-captures** for #9 now (no registry/backend touch). (Coordinator:
   confirm; affects whether #9 stays in my lane.)
4. **`refresh_node_rels` cost / scale.** A full-tree walk over every scanned file
   every tick is the same cost class as `refresh_type_rels`, but the row count is
   ~100x (every node, not every type decl). christmas #19 (chunked flush) is the
   eventual mitigation. For the first cut: gate strictly on `node_rels_used` (only
   pay when a program asks). (Coordinator: accept full-walk-when-used for v1?)
5. **Op-surface for #9's optional `id` arg.** Should `ast`/`sg`/`json` get a
   trailing optional `id` var consistent with `match`'s 5th arg, and is that
   arg-position the coordinator wants? This is the entanglement that may keep #9
   as plan-only (the AST/parser surface is shared across ops). (Coordinator:
   confirm the surface before the build.)

## Tests to write

- **#9** (implementing now): an `ast`/`sg` rule with the new `id` arg binds a
  located whole-match id; `ref(id, _, f, lo, hi)` resolves to the match's byte
  span. (e2e, sandbox pattern from `tests/data_ops.rs`.)
- **#3** unit: `walk_cst` on a small Rust/TS snippet yields the expected node
  count + parent links (root has `parent_ix == None`).
- **#3** e2e: `? node(Id, "function_item", F, Lo, Hi, P)` finds a fn; `anc(a,b)
  <- closure(child).` reaches it from the root; innermost-containment rule picks
  the tightest node for a byte; `node.id` joins `ref`.
- **Reserved-name guard**: a `.dl` declaring `node` or `child` bails loudly.
