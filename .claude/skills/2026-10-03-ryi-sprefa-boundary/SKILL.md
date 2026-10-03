---
name: 2026-10-03-ryi-sprefa-boundary
description: Who owns what between ryi (hafley-rs crates/sprefa-extract, scm++) and sprefa (dl8, sqlite_ivm). Read before sprefa code reads ryi output, uses ryi tools, or before proposing engine/recursion work on either side.
---

# ryi / sprefa boundary (fixpoint 2026-10-03, ryi-coordinator <-> claude-44)

Crate law (hafley-rs `crates/sprefa-extract/AGENTS.md`): extract = what is written; programs =
what it means.

| item | ryi (facts + one-shot questions) | sprefa (rules over facts) |
|---|---|---|
| syntax rows, TSI rows, checker witnesses | produces | reads |
| scm++ (non-recursive relations, preorder range joins, `ryii query --scmpp`) | owns | - |
| recursion of any kind (method chain closure, transitive pointer rules) | - | owns (Datalog rules) |
| `graph --slow` demand walk (Call, Method edges; seed -> worklist) | owns | - |
| Passed / TraitImpl edges (checker facts into extern crates) | owns | - |
| bespoke pointer rules within and across repos | `extra` seam only (accepts edges) | writes the rules |
| cross-language queries | - | over shared `tsi.*` relation names |
| DuckDB / DuckPGQ / OpenIVM / DBSP / engine choice and shootouts | - | owns; no engine crate in ryi; DuckDB CLI-only |
| storage: strings interned to integer ids; ids only in rows and indexes | applies | applies |
| refactor tools (`ryi rename/move/cleave`) on sprefa Rust | owns the tools | client only |
| defects found by sprefa using ryi | fixes | files issues in hafley-rs; never edits ryi |

Contact points:
1. ryi walk's `extra` seam takes edges in, never computes them.
2. TSI rows flow ryi -> sprefa only.
3. Contract relations for sprefa's `tests/it/_9j_no_recursion.rs` (runs
   `ryi <root> --resolve --kinds cst,call --sqlite <db>`): views
   `node(family, kind, name, span__start, span__end, _input_path)` and
   `resolved_edge(caller_path, caller_site_start, caller_site_end, callee_path, callee_start)`.
   ryi base tables are private (interned ids after ryi interning phase 2); the two views keep these
   columns. Source of truth: hafley-rs `crates/sprefa-extract/schema/1_facts.tsp`; `ryii schema`.
   A column change is a schema version bump in `1_facts.tsp` plus a note to sprefa. ryi keeps a
   contract test running the `_9j` command shape. sprefa reads only these views, never base tables.
