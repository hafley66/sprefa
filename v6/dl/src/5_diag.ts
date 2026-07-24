/**
 * 5_diag.ts — the builtin `diag` rel (v5 9-col schema verbatim, src/engine/decls.rs:263)
 * + the v5 LSP compat view + the full builtin decl set every later package imports from.
 *
 * Contract (plan M5, tasks.d.ts): `diagDecl` (path/line/col/end_line/end_col/severity/
 * code/msg/hint, 0-based positions); DIAG_V5_VIEW_SQL creates
 *   CREATE VIEW IF NOT EXISTS diag_v5 AS SELECT path, line, col, end_line, end_col,
 *     COALESCE(severity,'warn') severity, code, msg, hint FROM rel_diag;
 * The view IS the LSP interface — v5 `dl --lsp --diag-db` polls PRAGMA data_version and
 * publishes per file. Head defaults (severity='warn', end_line=line, end_col=col, code/hint
 * null) are the BRIDGE's job (0_ast_bridge.ts, DiagHeadDefaultLaw) — not reimplemented here.
 *
 * `diagDecl` (and every spine decl below) is declared EDB at decl time; the bridge is what
 * flips a rel to IDB when a program rule heads it (BridgeOk.program holds the resolved
 * origin) — that law lives in 0_ast_bridge.ts and is documented, not re-implemented, here.
 *
 * This module is the one source every later package (ingest, http) imports builtin decls
 * from — spine + diag declared once, no copies (tasks.d.ts SpineRelName pin).
 */

import { edbRel } from "sprefa-store-engine/src/lower/ast.ts";
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";

/** The builtin `diag` rel: v5's 9-col schema, positional and in order. This column
 *  list is the compat contract with v5's `src/lsp.rs` reader (--diag-db mode) and with
 *  `DIAG_V5_VIEW_SQL` below — changing it requires updating both together. */
export const diagDecl: RelDecl = edbRel("diag", [
  "path",
  "line",
  "col",
  "end_line",
  "end_col",
  "severity",
  "code",
  "msg",
  "hint",
]);

/** The v5 LSP compat view. IF NOT EXISTS so a runtime re-boot (program swap, M2) is
 *  idempotent. NULL severity reads as 'warn' at the view layer as a defense-in-depth
 *  default; the bridge's DiagHeadDefaultLaw is expected to fill severity at write time,
 *  so this COALESCE is normally a no-op. Passed by the integration layer into
 *  DlRuntime.boot's extraDdl (3_runtime.ts) — the runtime itself is not touched here. */
export const DIAG_V5_VIEW_SQL: string =
  "CREATE VIEW IF NOT EXISTS diag_v5 AS SELECT path, line, col, end_line, end_col, " +
  "COALESCE(severity,'warn') severity, code, msg, hint FROM rel_diag";

/** The builtin spine rels ingest (4_ingest.ts) writes and programs join
 *  (tasks.d.ts SpineRelName pin, columns verbatim). Declared EDB; the bridge flips
 *  any of these to IDB if a program rule heads it. */
export const spineDecls: readonly RelDecl[] = [
  edbRel("file", ["path"]),
  edbRel("node", ["path", "family", "start", "end", "kind", "name"]),
  edbRel("edge", ["path", "family", "kind", "from_start", "from_end", "to_start", "to_end"]),
  edbRel("sig", ["path", "owner_start", "owner_end", "slot", "pos", "ty"]),
  edbRel("site", ["path", "start", "end", "callee", "callee_path"]),
  edbRel("const", ["path", "owner_start", "owner_end", "field", "text", "kind"]),
  edbRel("span_line", ["path", "start", "line", "col"]),
];

/** Every builtin decl this package ships: spine + diag, in one place. The one source
 *  later packages (ingest, http) import from — no copies. */
export const builtinDecls: readonly RelDecl[] = [...spineDecls, diagDecl];
