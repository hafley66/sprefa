---
created: 2026-08-27
updated: 2026-08-27
type: improvement
assignee: chris
status: done
priority: normal
labels: [extract, refactor]
closed: 2026-08-27
---

# extract move: ImportRef.kind enum, moved_names/stem on the Rehome seam

## Description

leaky-types review rows 6 and 20

## Comments

### 2026-08-27T19:44:00Z · @chris

Landed (73634ff73): ImportRef.kind is ImportRefKind { Import, PathLiteral, ManifestTarget, Ext(LangKind) }; single-language kinds live as consts in the rehome files (rust: INCLUDE/USE_PATH/PATH_ATTR/MOD_DECL/MOD_PATH/MOD_RELOCATE_OUT/MOD_RELOCATE_IN/WIDEN_VIS, kotlin: PACKAGE_DECL). moved_names is a Rehome default method keyed off directory_stem (rust "mod", ts "index"); stem lives once in move_cx.rs. Wire text byte-identical (extract move --list over rust_move/relocate and ts_move fixtures, plan-only diff empty). Battery 320 passed / 0 failed; tests/7_import_ref_kind.rs 4/4.