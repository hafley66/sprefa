# REPORT.md — alloy semantics in prolog: decl/ref/check/render codegen spike

Worktree: `92756b54dc0cb633e9636234f5358f3324be1ebf` (merge --ff-only
succeeded, rev matches base). Lab files under
`v6/prolog/labs/alloy_semantics/`; nothing committed or pushed.

## 1. Mapping summary

MAPPING.md: **10 mapped, 3 unmapped**. The unmapped set (reactive re-render,
context parent-chain, mono-morphization) all rely on runtime reactivity or
external state a backward-chaining loader does not model. Mono-morphization is
unmapped because neither skill file read for this lab mentions it at all.

## 2. Base verification (FIRST action)

```
$ git merge --ff-only 92756b54dc0cb633e9636234f5358f3324be1ebf && git rev-parse HEAD
Already up to date.
92756b54dc0cb633e9636234f5358f3324be1ebf
```

## 3. Receipts

### (a) GREEN RUN — both targets, import/use lines derived from ref/2

```
$ swipl -q -l run.pl -g run -g halt
export interface StringsRow {
  string_id: number;
  content: string;
}

import type { StringsRow } from "./core";

export interface NodeRow {
  node_id: number;
  family: number;
  file_id: number;
  byte_start: number;
  byte_len: number;
  kind: number;
  name_id: number | null;
}

export interface EdgeRow {
  family: number;
  src_id: number;
  dst_id: number;
  kind: number;
}

---

pub struct Strings {
  pub string_id: i64,
  pub content: String,
}

use super::core::Strings;

pub struct Node {
  pub node_id: i64,
  pub family: i32,
  pub file_id: i64,
  pub byte_start: i64,
  pub byte_len: i64,
  pub kind: i32,
  pub name_id: Option<i64>,
}

pub struct Edge {
  pub family: i32,
  pub src_id: i64,
  pub dst_id: i64,
  pub kind: i32,
}
```

Import (ts) and use (rust) lines are present, derived from `ref/2`: `graph`
references `strings` (node.name_id) across a file boundary, so graph.ts imports
`StringsRow` from ./core and the graph module `use`s `super::core::Strings`.
Edge's references to node stay intra-file and emit no import. No text is built
mid-tree; `ts_file`/`rust_mod`/`ts_interface`/`rust_struct` terms are assembled
then folded in `tree_to_text/3`.

### (b) SABOTAGE 1 — comment out the node decl -> unresolved_ref, no text

```
$ ALLOW_LAB_SABOTAGE_UNRESOLVED=1 swipl -q -l run.pl -g run -g halt
codegen_refused(unresolved_ref(graph.ts,s_node,0))
exit=1
```

The node decl is suppressed so edge's `src_id`/`dst_id` references resolve to a
symbol with zero decls. `codegen_refused(unresolved_ref(...))` throws in the
check pass, which runs before render, so no text is emitted.

### (c) SABOTAGE 2 — duplicate decl name -> duplicate_name, no text

```
$ ALLOW_LAB_SABOTAGE_DUPLICATE=1 swipl -q -l run.pl -g run -g halt
codegen_refused(duplicate_name(core.ts,StringsRow))
exit=1
```

A rendered name is asserted twice in the same target file. Invariant 2 throws
`codegen_refused(duplicate_name(...))` before render, so no text is emitted.
Exit 1 on both sabotages; exit 0 on the green run.

### (d) PARITY — emitted ts vs the real spine

```
diff (real vs emitted) exit=0   (0 = field-for-field identical)
```

All three tables' field names and types match the real checked-in interfaces
(`StringsRow` from spine.ts:63-66; `NodeRow` and `EdgeRow` from
types.ts:80-95). No diff. The only structural differences are cosmetic: the
real file has no import line and carries node/edge row types in types.ts, while
the lab splits into core.ts + graph.ts with the derived import to exercise the
cross-file path. Field-for-field they are identical.

## 4. What broke / what surprised

- **`table` is a SWI-Prolog operator.** `table/1` (tabling, fx, prec 1150) makes
  a bare `table/2` in a module export list a parse error (Operator expected).
  The predicate is `(table)/2` in the export list; the `table(...)` call sites
  parse fine. This is the one deviation from the brief's literal `table/2` spelling,
  and it is forced by the runtime, not a scope choice.
- **`code_type/2` conversion direction.** SWI 10 spells `code_type(Result, to_upper(Input))`
  (result first, input in the arg), the reverse of what I first wrote, so rendered
  names came out lowercase (`stringsRow`) until corrected to `StringsRow`.
- **A standalone text probe needs collect materialized.** `ts_text/1` returned the
  empty string until it ran `collect/1` itself; `run_all` collects before rendering,
  but a bare text goal did not. The parity leg now self-collects.
- **node/edge row types are not in spine.ts:60-105.** The brief's parity range
  shows only `StringsRow`; the real `NodeRow`/`EdgeRow` interfaces live in
  `v6/sprefa-store/js/src/engine/types.ts:80-95`. Parity sourced them from there.
- **The two-file split is a lab fiction.** In the real codebase node+edge live in
  the same spine descriptive set; the core/graph split exists here specifically to
  manufacture one genuine cross-file reference and prove the derived import line.
- **Sabotage mechanism.** Following the `OPENAPI_LAB_DROP` precedent in
  `emit_openapi.pl`, the sabotages are env-var hooks (`ALLOW_LAB_SABOTAGE_*`), so a
  checked-in provenance file is never edited mid-run. They behave as "comment out
  the node decl" and "add a duplicate decl name" respectively.
