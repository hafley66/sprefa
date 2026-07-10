---
name: project-instance-leak-memory-control
description: "Plan to fix App.instances Vec leak and add effect-system memory control to v4; covers L1–L7 leak taxonomy, 8 invariants, 5 phases, watch_this_file() at the end"
metadata: 
  node_type: memory
  type: project
  originSessionId: f849c0bf-7290-428a-9e46-cc5c7bc71cad
---

DRAFT plan at `plans/2026-05-20-instance-leak-and-memory-control.md`. Not yet implemented.

**Why:** `App.instances: Mutex<Vec<Arc<PipeInstance<Cursor>>>>` (`v4/src/app.rs:508`) has no
key, no dedup, no eviction. Every `lsp_change` duplicates a file's pipes (L6). `lsp_close` does
not evict (L2). Memo/source_gen/files rows orphan forever (L3/L4/L5/L7). User wants durable
execution with memory ceiling.

**How to apply:** Five phases ordered so each compiles and gates GREEN alone:
1. `InstanceRegistry` (HashMap by (source_uri, ast_hash)) + `_files` GC by GROUP BY + debug Replay write assert
2. `_live_owners` table + `lsp_close` evicts + `ingest` atomic swap
3. Seam liveness gate (Skip if owner not live) + dirty-source wake JOIN `_live_owners`
4. `gc_owners` / `gc_sources` / `gc_files` sweeps + `App::gc()`
5. `notify` watcher + `watch_this_file()` op

Eight invariants (I1–I8) numbered in plan §3. Open questions in §24 (GC cadence,
`hash_shape_into` opt-in for components, owner_op_id stability across schema bumps, watcher
test stub). Depends on the Generation/SourceId domain-tag work in
[[project-clock-seam-invariants]] landing first; the salt strategy in `hash_pipe` reuses the
length-prefix discipline from that plan's Fix A.

Related: [[project-lsp-maintenance-debt]] (this touches the LSP ingest path),
[[project-recursion-surface-gaps]] (recursive owner subscribe is the existing pattern the
wake-filter generalizes).
