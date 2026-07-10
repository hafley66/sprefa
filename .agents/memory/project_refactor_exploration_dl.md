---
name: project_refactor_exploration_dl
description: dl-as-refactor-tool formula; validation gauntlet killed name-scatter; signature-shape buckets -> latent traits is the winning signal for "imperative soup"
metadata:
  node_type: memory
  type: project
  originSessionId: a6a2eb6e-f477-44b1-97e7-8fd0568577f0
---

Formula for pointing dl at a god file (engine.rs: 7168 ln, 236 methods, one
~4766-ln impl Engine). Written up at v5/docs/refactor-exploration.md; the one
surviving file-cut tool is examples/call-seams.dl (both committed bd7128a).

**Extraction idiom** (self-contained, no SCIP): `match` over the file for fn
decls + `self.X(` call sites; attribute a call to its enclosing fn via
nearest-decl-above (`above` minus `has-closer` — same shape as recompute-guard.dl
and the call-seams tool). call_edge is a RESERVED builtin name -> call yours
`cedge`. node2vec(g) on the symmetrized call graph; scc(cedge) for cycles.
GOTCHA: a running `dl --daemon` HIJACKS ad-hoc `dl prog --root .` (returns the
daemon's OWN loaded program's queries, e.g. stray `? diag`) — always --no-daemon
for exploration.

**Validation gauntlet (run BEFORE trusting any layout/cluster metric):**
1. scatter-vs-random: shuffle family positions ~200x, ratio actual/random
   (~1=names ignored, ~0=clustered). engine.rs=0.32 (already 3x better than chance).
2. call-distance under reordering: median |rank(caller)-rank(callee)| over cedge.
   engine.rs current=18, name-family-sort=60, random=80. Reordering by name
   WRECKS call locality. KILLED the "gather by name / scatter" metric — the file
   is already laid out by who-calls-what (better principle than names).
Rule: a metric must beat random AND not worsen call-distance, else it's cosmetic.

**The winning signal = signature-shape buckets** (receiver,#args,return) expose
latent TRAITS. engine.rs: 19 rel families each have X_rel_decls + X_rels_used(prog)
+ refresh_X_rel (~60 methods) + 106 hand-written self.refresh_* dispatch calls =
ONE `RelKind` trait + 19 impls + a registry loop. Also `BodyOp`
(scc/node2vec/closure eval_*_rule, all fn(&mut self,&Rule)->Result<()> — kills the
per-operator for-loops in tick) and `DeclProvider` (23 fn()->Vec). Sub-structs
from field-coupling (LCOM, `self.field` graph, exclude hub fields db/rels/root):
repos/closure_cache/gen_journal/rev_cache each own their state+methods. ~40
already-assoc fns are free functions wearing an impl.

REFRAME insight: node2vec's pass-1 "refresh_* supercluster" I dismissed as "same
role, low value" WAS the RelKind trait. same role = same trait; wrong frame
(split the file) hid the right one (extract the trait). The self-call graph is a
DAG (0 multi-member SCCs) and the impl core has NO cheap balanced cut (~35% of
call traffic crosses any; 64% of that is just tick/tick_paths = a facade boundary,
correct to leave imperative).

Cross-file calibration: generated oxc ast_builder.rs name-scatter 3.6 (control,
emitted in order); sqlite3.c 90% naming coverage; SillyTavern script.js scatter
32.8; world-info.js 20% coverage. LOW coverage (<=25%) is the ONLY regime where
the expensive embedding earns its keep (no naming structure to recover) — reserve
node2vec for that quadrant. Related: [[project_node2vec_graph_embed]],
[[project_v5_dl_engine]], [[feedback_build_dont_analyze]].
