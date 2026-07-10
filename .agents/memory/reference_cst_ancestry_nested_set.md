---
name: reference_cst_ancestry_nested_set
description: "CST node/child ancestry + containment via nested-set span interval predicate, not closure(child)/SCC; browser document-position analogy"
metadata: 
  node_type: memory
  type: reference
  originSessionId: 33cc2e4c-37a8-4f52-954c-4741b241fd77
---

The v5 CST-as-relation feature (christmas #3): `node(id,kind,file,lo,hi,parent)` +
`child(parent,child)`, built by `refresh_node_rels` walking every named tree-sitter
node. Lives on branch `worktree-agent-a8cd173feeff2a4ef` (4 commits, NOT merged as of
2026-06-25; held while main takes parallel json-decl work). Path-scoped incremental
walk landed (`refresh_node_rels_delta`, `_node_path(id,path)` side table, structural
guard test `last_node_files_walked==1`, `DL_TICK_LOG_MS` LSP surfacing).

KEY INSIGHT (the keeper from the Chromium-layout digression): a CST is a forest +
acyclic + **properly nested spans** (child `[lo,hi]` ⊆ parent's). That is the
nested-set encoding. So ancestry/containment is a RANGE JOIN on byte spans, NOT a
transitive closure:

    anc(a,b) <- node(a,_,f,alo,ahi,_), node(b,_,f,blo,bhi,_), alo<=blo, bhi<=ahi, a!=b.

This sidesteps `closure(child)` entirely — closure() always SCC-condenses
(engine.rs `dedup_edges`, "one condensation per graph"), and on an acyclic 136K-node
CST that condensation is pure waste (measured ~0.82s of an incremental tick). Same
trick browsers use for `compareDocumentPosition` via pre/post-order numbering.

MEASURED 2026-06-25 (corrects the earlier "retire closure" guess — verify, don't assert):
- FULL `anc` materialization (1721 nodes, src/spine.rs): closure(child) 91ms BEATS the
  unindexed range-join 484ms (O(n²) self-join; would HANG at 136K). Parity near-exact
  (range over-counts ~15 equal-span wrapper pairs both directions). => closure(child) WINS
  for full transitive ancestry; do NOT retire it. A `node(file,lo,hi)` index would only make
  the range-join competitive, and even then it's output-bound (anc is inherently large).
  LANDED 2026-06-25 (ce7adf5): optional `node_file_span_idx ON rel_node(file, lo, hi)`,
  declared next to the node rels (idempotent), so the POINT/containment query below is a
  range scan not a full table scan. Test `point_containment_query_uses_span_index`
  (cst_node_perf.rs) asserts EXPLAIN QUERY PLAN picks it. Did NOT index for the full
  range-join (closure still wins there).
- POINT containment (innermost node containing a byte coord, full 142K-node corpus): the
  interval predicate `lo<=C, C<hi` is a single scan, cost in the noise vs the ~5.8s walk —
  NO closure/SCC at all. THIS is the LSP-common query and where the nested-set wins decisively.
VERDICT: steer point/containment/innermost queries to the interval predicate (free, no SCC);
keep `closure(child)` for genuine full-ancestry needs. closure-incremental is complementary,
not retired.

Browser analogies that survive: hit-testing = 1D innermost-containment; relayout
boundary / dirty-subtree = the path-scoped incremental tick. What does NOT transfer:
layout constraint-solving (we never compute positions; the parser gives byte spans).

See [[project_v5_dl_engine]]. Practical uses: anchor-finding for codemods (node.id IS
a _where_bytes edit coordinate, joins ref/string), innermost-containment, scope/free-
var (#10), structural lints (cross-kind ancestor joins), join CST ↔ call_site/
type_entity. #3 is the codemod foundation feeding Phase D edit algebra (#4).
