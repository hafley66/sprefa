---
name: feedback_no_regex_scan_forcer
description: Never use match(/./) as a scan-forcer in .dl; a bare scan rule populates the corpus. Prefer ast/scip over regex match generally.
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 05f36171-99c7-49ed-9fb6-892190668b9e
---

Chris, emphatically (2026-07-02): "no more match, fuck match, use the correct
ast or scip tool, not fucking regex."

**The specific anti-pattern**: `src(p) <- scan("WORK", glob, p, rev), match(p, rev, /./, l).`
The `match(p, rev, /./, l)` matches every line purely to force the scan — it
extracts nothing, `l` is unused. It was copy-pasted across every flow example.

**Why:** using a regex where the AST/SCIP path is correct is offensive to him;
`match` is regex and should only appear when the task IS a regex (a literal
text pattern with no parser handle).

**How to apply:**
- A BARE `scan` rule already populates `_file` and triggers the AST/SCIP
  extraction families (call_name / type_entity / call_edge / df_node / ...).
  `reconcile_sources` (engine/mod.rs) enumerates files into `_file` for ANY
  rule whose body contains a `scan`; extraction reads `_file` (extract.rs
  `extract_file_set` = `SELECT ... FROM _file WHERE path LIKE '%.rs' OR ...`).
  So write `src(p) <- scan("WORK", "src/**/*.{rs,ts,kt}", p, rev).` — no match.
  VERIFIED: bare scan → call_name/type_entity populate, `main`/`helper`
  resolve.
- For actual fact extraction, reach for `ast`/`sg` (tree-sitter) or the
  built-in AST/SCIP rels (call_name, type_entity, type_edge, df_*, scip_*),
  NOT `match`. Only use `match` when the thing genuinely has no parse handle
  (e.g. a codegen banner comment, a per-LINE enumeration like lint-docs.dl's
  `file_line(p, l) <- scan(...), match(p, rev, /./, l)` where `l` IS used).

Stripped the scan-forcer from every owned flow example + checked-notes.dl +
the flow/doc/lsp test .dl strings this session. See [[project_interproc_flow]].
