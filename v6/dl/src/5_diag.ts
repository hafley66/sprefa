/**
 * The builtin `diag` rel (v5 9-col schema verbatim, src/engine/decls.rs:263), the
 * v5 LSP compat view, and the full builtin decl set every later package imports.
 *
 * The view IS the LSP interface: v5 `dl --lsp --diag-db` polls PRAGMA data_version
 * and publishes per file. Head defaults (severity='warn', end_line=line, end_col=col,
 * code/hint null) are the bridge's job (0_ast_bridge.ts).
 *
 * Every decl below is declared EDB at decl time; the bridge flips a rel to IDB when
 * a program rule heads it (0_ast_bridge.ts).
 *
 * This module is the one source every later package (ingest, http) imports builtin
 * decls from: spine + diag declared once, no copies.
 */

import { edbRel } from "sprefa-store-engine/src/lower/ast.ts";
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";

/** v5's 9-col diag schema, positional and in order: the compat contract with v5's
 *  `src/lsp.rs` reader (--diag-db mode) and with DIAG_V5_VIEW_SQL below; changing it
 *  requires updating both together. */
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

/** The v5 LSP compat view. IF NOT EXISTS so a runtime reboot (program swap) stays
 *  idempotent. NULL severity reads as 'warn' at the view layer as a defense-in-depth
 *  default; the bridge is expected to fill severity at write time, so this COALESCE
 *  is normally a no-op. Passed by the integration layer into DlRuntime.boot's
 *  extraDdl (3_runtime.ts). */
export const DIAG_V5_VIEW_SQL: string =
  "CREATE VIEW IF NOT EXISTS diag_v5 AS SELECT path, line, col, end_line, end_col, " +
  "COALESCE(severity,'warn') severity, code, msg, hint FROM rel_diag";

/** The builtin spine rels ingest (4_ingest.ts) writes and programs join against,
 *  columns verbatim. Declared EDB; the bridge flips any of these to IDB if a
 *  program rule heads it. `file`'s content_hash column is the witness a salted
 *  probe joins against, so it re-fires on a content edit rather than reusing a
 *  stale cached response. */
export const spineDecls: readonly RelDecl[] = [
  edbRel("file", ["path", "content_hash"]),
  edbRel("node", ["path", "family", "start", "end", "kind", "name"]),
  edbRel("edge", ["path", "family", "kind", "from_start", "from_end", "to_start", "to_end"]),
  edbRel("sig", ["path", "owner_start", "owner_end", "slot", "pos", "ty"]),
  edbRel("site", ["path", "start", "end", "callee", "callee_path"]),
  edbRel("const", ["path", "owner_start", "owner_end", "field", "text", "kind"]),
  edbRel("span_line", ["path", "start", "line", "col"]),
];

/** Every builtin decl this package ships: spine + diag, in one place. */
export const builtinDecls: readonly RelDecl[] = [...spineDecls, diagDecl];
