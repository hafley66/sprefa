---
name: 2026-10-03-ryi-sprefa-boundary
description: Who owns what between ryi (hafley-rs crates/sprefa-extract, hafley_scm, scm++) and sprefa (dl8, sqlite_ivm). Read before sprefa code reads ryi output, calls ryi, uses ryi tools, or before proposing traversal, scope, SQL generation, store, edit, or engine work on either side.
---

# ryi / sprefa boundary (fixpoint 2026-10-03, ryi-coordinator <-> claude-44, 6 rounds)

Crate law (hafley-rs `crates/sprefa-extract/AGENTS.md`): extract = what is written; programs =
what it means. ryi = extraction + edit application + on-request fact providers + a closed store.
sprefa = every rule, every walk, every scope, the plan IR. Mirror memo: hafley-rs
`.claude/skills/2026-10-03-ryi-sprefa-boundary/SKILL.md`.

## Ownership

| item | ryi | sprefa |
|---|---|---|
| syntax rows, TSI rows, checker witnesses | produces | reads |
| scm++ patterns (non-recursive: preorder range joins, NOT EXISTS stopBy, regexp/instr scalars) | owns the language and its evaluation as a fact provider | uses patterns as syntax atoms in rules |
| recursion with a closed-form encoding (tree ancestry as preorder intervals `d.pre > n.pre AND d.pre <= n.last`) | owns, as an index | - |
| open recursion over graphs (call paths, method chains, points-to, type traversals) | - | owns, as rules (demand lowering: dl8 `_9a_ir_lower.rs` `with_demand_in_strata`) |
| per-body providers (RA `hir`: `edges_out(body)`, binding-exact occurrences) | owns; warm Session in the daemon | calls on request |
| `graph --slow` | thin worklist loop only until the provider protocol exists, then deleted | becomes a canned sprefa program over the providers |
| `extra` seam (rule edges into a ryi fixpoint) | dropped | - |
| Passed / TraitImpl edges (checker facts into extern crates) | owns | reads |
| scope ("which things") | commands take `--scope <relation>` (rows file or view); `--entry`/`--pattern`/`PATH#NAME` are sugar for the same relation | computes scopes as relations |
| plan IR | depends on `ivm-ir` (engine-free, sqlite_ivm repo) via local kellnr registry, pinned | owns `ivm-ir`; publishes it |
| SQL generation | scm++ lowers to `ivm-ir`; no new SQL generator | two renderers beside `ivm-ir`: batch (one-shot) and incremental (ivm-sqlite); dl8 `_5_reify/_7_sqlite.rs` goes once batch exists |
| stores | base facts once, in ryi's CLOSED store (ryi writes; nobody else mutates) | reads in place through the contract; sqlite_ivm holds derived relations only |
| edits to hand-written files | applies (`edit(file, start, end, replacement)`, atomic, verify + rollback); binding-exact sites computed here | policy sites computed as relations |
| generated files | never span-edits them | dl8 `fs.file` whole-writes them |
| TSI / fact schema | `schema/1_facts.tsp` (+ tsi `.tsp`) is the source of truth | dl8 reads the `.tsp` (TypeSpec importer) and regenerates `registry.rs`; no sprefa -> ryi build arrow |
| refactor tools (`ryi rename/move/cleave`) on sprefa Rust | owns | client only |
| defects found by sprefa using ryi | fixes | files issues in hafley-rs; never edits ryi |
| engine choice and shootouts (dd, sqlite_ivm, pg_ivm, DBSP, DuckDB/OpenIVM) | - | owns; no engine crate in ryi; DuckDB CLI-only, never in product binaries |
| storage rule: strings interned to integer ids; ids only in rows and indexes | applies | applies |

## Contracts

1. Seam: ryi daemon (`server_auto`) API, cross-process, is the default. In-process only behind an
   opt-in feature of a thin client crate. dl8 never compiles rust-analyzer.
2. Ids: 64-bit content hash of the text, stable across runs and worktrees; collision = error at
   intern time. The contract carries ids plus `dict(id, text)`; sprefa joins on ids and decodes at
   output. Dense per-run ids stay internal to one run DB.
3. Relations: SQLite views now (Parquet later, same relation set; sprefa reads through one adapter).
   `node(family, kind, name, span__start, span__end, _input_path)` and
   `resolved_edge(caller_path, caller_site_start, caller_site_end, callee_path, callee_start)`
   (read by dl8 `tests/it/_9j_no_recursion.rs`). Views carry the witness tier: a fast-tier gap shows
   as a missing witness, never a silently absent edge. Column change = version bump in `1_facts.tsp`
   plus a note to sprefa; ryi keeps a contract test running the `_9j` command shape.
4. Deltas: ryi owns emission (`ryii watch`): `(relation, row ids, +1/-1)`, an epoch marker per
   settled change; editing a file retracts every row of its old content id and asserts the new.
   sprefa consumes them as sqlite_ivm / dd frontiers.
5. TSI rows flow ryi -> sprefa only.

## Known gaps to respect

- Fast-tier binding gaps: crate-name imports from the own lib, `#[path]` bin modules (hafley-rs
  `issues/ryii-dogfood-self-20261003`, `ryii-dogfood-dl8`). Rules over fast call edges read the
  witness tier.
- Slow tier cost is whole-project RA inference (2,306 fns inferred for a 27-edge answer); salsa
  memoizes per body, so provider calls are cheap only against a warm daemon Session.
