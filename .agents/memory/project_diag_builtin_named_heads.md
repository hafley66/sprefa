---
name: project_diag_builtin_named_heads
description: "diag is now a reserved fixed-schema built-in (v0.4.0); never `rel diag`. Named args + bare shorthand + positional literals work in rule heads."
metadata: 
  node_type: memory
  type: project
  originSessionId: 05f36171-99c7-49ed-9fb6-892190668b9e
---

sprefa v0.4.0 (main, pushed 2026-07-02): **`diag` is a reserved fixed-schema
built-in relation**, not a user-declared name. Schema:
`diag(path, line, col, end_line, end_col, severity, code, msg, hint)` — `path`
is TEXT (synthetic origins like `"(engine)"` survive the file check). A
`rel diag(...)` decl now ERRORS. Head it directly, naming only the columns you
set; the rest pad to NULL and default in `Engine::diags` (severity `warn`,
end_line=line, ints 0). Motivation: it was a magic name mapped by column position
that collided in the merged `.dl/` discovery namespace when two files declared it
with different columns.

**Named/pun/positional binding in rule heads (and body/query atoms), the one
rule = "carries a name -> bind by name; nameless -> bind by position":**
- Positional head, term count == rel arity: unchanged, stays positional. Existing
  programs untouched.
- `diag(path: p, line: l, msg: m)` — named by column, out of order OK, rest NULL.
- Bare var puns to its own column (`diag(path, line, msg)` == `path: path, ...`),
  interleavable with `col:` in ANY order (unlike Python). Fully-bare shorthand
  needs no `col:` anchor; only fires when the atom would otherwise be an arity
  error (count != arity, all terms column-named vars).
- Nameless literal fills the next OPEN column (Python-style positional prefix):
  `diag("synth.rs", 1, severity: "error")` -> path="synth.rs", line=1.
- A head can't mix named args with an aggregate call.

Engine pieces: `diag_rel_decls()` + `DIAG_RELS` reserve (engine/mod.rs),
`ast::Value::Null` (new variant) so a padded head column -> SQL NULL through both
the derived (lower.rs) and source-rule (Rust head-projection) paths; parser
`head_atom` collects named; `frontend::resolve_atom` does name/pun-first then
literals-fill-holes.

**Doc generators now live in `.dl/`** (symlinks to examples/op-table.dl,
builtin-rels.dl, gen-reference.dl, gen-skill-ref.dl) since the diag collision is
gone; a plain `dl --root .` tick or a `--daemon` regenerates README + docs +
skill reactively. `gen` is suppressed under `--lsp`/`--check` (the LSP process
won't regen). Renamed each generator's marker-region rel uniquely (op-table
`block`->`op_block`, builtin-rels `block`->`rel_block`) so the merge doesn't
dedup them into one cross-contaminating rel.

Migrated `~/projects/games/smash/.dl` (dl-self-lint.dl, lint-input-device.dl) to
named diag; its stale Jul-1 daemon killed so it respawns 0.4.0. See
[[feedback_no_regex_scan_forcer]] (same session's match-strip rule).
