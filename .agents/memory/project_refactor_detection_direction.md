---
name: project_refactor_detection_direction
description: "Shape-iso refactor detection (type_shape/type_lgg) ruled a dead end 2026-06-27; LLM is the candidate brain, dl is the deterministic executor (LSP/--move/--check)"
metadata: 
  node_type: memory
  type: project
  originSessionId: 597ecb2d-680d-4307-9d5f-311aafddacc8
---

Ruling on 2026-06-27, confirmed independently in two sessions: the deterministic
shape-isomorphism refactor detectors are **not worth pursuing as candidate generators**.

- `type_shape` (field-tree Merkle hash, names dropped) and `type_lgg` (Plotkin
  anti-unification, `(a,b,vars)`) are coincidence-dominated. Same field-tree shape ≈
  zero correlation with same concept. Live run over v5/src: the n=15 bucket lumped
  `Severity`/`Proposal`/`Request`/`PatParser`/`CstNode` (structurally identical,
  semantically unrelated). `type_lgg.vars` is non-monotone — `(None,None)=>1` (two
  leaves) and the divergent `otherwise=>1` fallback collide, so `vars<=2` floods
  (`AggFn` pairs with ~every type at 1). No skeleton-size column to form a holes/size ratio.
- Root cause: projecting keys (names) out for recall destroys the precision signal,
  and the type graph alone has nothing to restore it. All recall, no gate. This is the
  `Point{x,y}` vs `Size{w,h}` false-merge at scale.
- The signal that works lives in **names + usage + co-change** (the reward-oracle side,
  and LLM consensus), not in shape. The 12-agent (6 Opus + 6 Haiku) consensus study and
  the structural-dedup reward oracle (beats LOC 97% vs 50% across 3 repos) are the live
  signal.

**Kept architecture: LLM proposes, dl executes/gates.** The durable asset is dl as a
queryable substrate + LSP + `--move` auto-rewrite + `--check` gate — the deterministic
"hands" (safe, verifiable, incremental rewrites), not an algorithm that ranks candidates.
Chris's own words: "llm mogs this algorithm adventure, at least i can do dl on lsp and
refactorings." Do not re-pitch shape-iso detection. See [[project_v5_dl_engine]].
