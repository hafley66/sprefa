/**
 * 5_diag.ts — the builtin `diag` rel (v5 9-col schema verbatim, src/engine/decls.rs:263)
 * + the v5 LSP compat view.
 *
 * Contract (plan M5, tasks.d.ts): `diagDecl` (path/line/col/end_line/end_col/severity/
 * code/msg/hint, 0-based positions); DIAG_V5_VIEW_SQL creates
 *   CREATE VIEW diag_v5 AS SELECT path, line, col, end_line, end_col,
 *     COALESCE(severity,'warn') severity, code, msg, hint FROM rel_diag;
 * The view IS the LSP interface — v5 `dl --lsp --diag-db` polls PRAGMA data_version and
 * publishes per file. Head defaults are the BRIDGE's job (0_ast_bridge.ts).
 *
 * Owned by package M5-TS (diag). Placeholder until that package lands.
 */
export {};
