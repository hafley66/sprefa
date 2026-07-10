---
name: project_perf_reactivity_arc
description: "perf-under-reactivity arc COMPLETE (main 041cf1b pushed 2026-07-02) — one-pass fixpoint, closure guard, telemetry rels, merge fix, THEN gaps A/B/C: extract digest skip + fact cache (warm tick 1.5s->35ms), scoped full tick w/ async: attribution, family change reporting; RelDecl carries group/doc"
metadata: 
  node_type: memory
  type: project
  originSessionId: 1e06c249-1ca4-4665-98b0-d38b5054124a
---

Arc landed across d8da1c3 (measure+rails) and 041cf1b (gaps fixed), both
pushed. Origin: profiling examples/flow-interproc.dl on the sprefa repo itself
(chat_log/20260702.0 for the audit, .2 for the gap-fix session).

Engine facts future sessions should rely on:
- `rebuild_derived` evaluates NON-RECURSIVE derived rules in exactly one pass
  (`rel_components` rel-level Tarjan per stratum; only recursive components
  iterate). stratify groups by NEGATION DEPTH, not SCC — why the split exists.
- Unpinned `?` on a closure head is REFUSED over `DL_CLOSURE_QUERY_MAX_EDGES`
  (default 20k; LIMIT does NOT short-circuit the closure VIEW). Both-pinned =
  `run_reaches_pair` condensation walk. dl_diag `closure-unpinned` = lint twin.
- Telemetry rels `rel_count`/`stmt_ms` (src/rels/perf.rs): own-family excluded
  (self-diff oscillation), closure VIEWS excluded (COUNT(*) materializes).
- **Gap A (FIXED)**: type/call/dataflow/doc refreshers persist an
  `extract:<family>` input digest in `_reldigest` (corpus (repo,path,rev,hash)
  + scip_ref rows + exe (len,mtime) identity — a rebuilt binary re-extracts)
  and skip the whole pass warm, cross-process too; per-file in-memory cache
  (repo,path,hash) -> Arc<facts> re-parses only moved files on a changed tick
  (cache map replaced each refresh = self-evicting). Warm flow-interproc tick
  1.5s -> ~35ms in-engine. `Engine::extract_files_parsed` = counter;
  tests/it/extract_cache.rs pins it.
- **Gap B (FIXED)**: full `tick` scopes rebuild via affected_derived like
  tick_paths. Attribution: `seed_rel_digests` now RETURNS movers; extractor
  families + RelKind + every/clock return change bools; **@async/@stream
  response rels get an `async:<rel>` content digest** — the off-tick drain
  writes them, NOTHING else attributes them; without this gh-cache latest-wins
  + temporal_every broke (resp_latest never re-derived). need_full = program
  digest moved OR @next carry moved OR any derived/closure table empty.
  `Engine::last_derived_rebuilt` = instrumentation; tests/it/scoped_tick.rs.
- **Gap C (FIXED)**: tick_paths marks family rels changed only when the digest
  moved.
- builtin_rel_docs() tuple registry DELETED: RelDecl has `group`/`doc`
  (&'static str, "" for user decls); undocumented_builtins = decls with empty
  doc; README regen was byte-identical (the autogen zone doubled as the
  fidelity check). New-builtin checklist skill updated in assets/.
- `dl setup --project` symlinks every `assets/*.skill.md` to
  `.claude/skills/<name>/SKILL.md` (fresh-clone maintainer skills).
- e2e GOTCHA: reconcile's fast path is (mtime SECS, size) — a same-second
  same-byte-length rewrite is invisible; tests must change content length.
- The .dl join smell: cmp predicate over columns of TWO atoms = per-pair
  cross-product; fix = row-local compute rel + equality join (call_edge_bare).

Remaining backlog: positional arg->param hop (extractor records arg pos,
3 langs); module family still lacks a change report (conservatively marked on
full ticks); refresh_spine_rels likewise.
