---
name: feedback_never_edit_autogen_zones
description: HARD RULE — never hand-edit any byte inside an autogen BEGIN/END span in any file; regenerate via the dl program that owns it
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 7322e02c-67ee-4fd7-8304-4c7ef80db5d0
---

NEVER hand-edit any byte range inside an auto-generated zone. In this repo those
are regions bounded by `<!-- BEGIN: X -->` / `<!-- END: X -->` markers (e.g.
v5/README.md `op-table` 95-107, `builtin-rels` 264-271). A dl program splices each
block via `comment` (marker bounds) + `gen` (write-only-when-bytes-differ):
examples/builtin-rels.dl, examples/op-table.dl. A daemon (`dl --load <prog>`)
regenerates on the relevant source edit.

**Why:** Chris flagged 2026-06-29 that I edit auto zones "all the time". A hand-edit
is overwritten on the next tick AND corrupts the source-of-truth contract — the
generator is the source, the block is output. The fix for stale generated docs is
to edit the GENERATOR (the .dl program / the engine.rs lines it scans), never the
rendered block.

**How to apply:** before editing any markdown/doc, scan for BEGIN/END markers. If a
change falls inside a span, edit the owning .dl generator or its scanned source
instead, then let the daemon (or `dl <prog> --root .`) re-splice. The hand-maintained
prose table BELOW the builtin-rels block IS editable (README says so explicitly) —
only the marked spans are off-limits. Distinguish them; don't treat the whole file as
locked.

BUILT 2026-06-29 (commits 6fd282a, 89754ff): (1) the autodoc is now self-describing —
engine.rs `builtin_rel_docs()` (single source of (name,group,summary)) + `rel_catalog`
relation (projects docs joined to real decls, can't drift) + `undocumented_builtins()`
completeness test (a new builtin fails CI until documented). `examples/builtin-rels.dl`
reads `rel_catalog` (no source-scrape) and renders ALL 55 rels into the README block.
(2) the lint exists: `examples/lint-no-touch.dl` flags an agent edit inside any
no-touch range via `agent_touch ⨝ changed_line ⨝ no_touch_zone`; covers autogen blocks
AND hand-placed `NO-TOUCH: <reason>` / `END-NO-TOUCH` comment markers (any language).
So the no-touch rule is now machine-enforced, not just behavioral. See
[[reference_scip_multilang_indexers]].
