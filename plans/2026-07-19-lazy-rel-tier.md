# Lazy rel tier + amplification budget (2026-07-19)

User question driving this: a 22.4MB corpus amplifies to an 873MB db (39x).
Which bytes are structural, which are speed-for-space, and what does a
lazy/materialize-on-demand tier buy? Receipts below are from `dl daemon health`
(new this session) against the live sprefa root, 2026-07-19 morning.

## Where the 873MB actually is

| bucket | MB | share |
|---|---|---|
| rel tables (data) | 269 | 31% |
| pk autoindexes | 261 | 30% |
| auto-demand `idx_` | 228 | 26% |
| internal tables (`_strings` 92, `_call_raw_site` 18, ...) | 106 | 12% |
| engine indexes | 9 | 1% |

**Indexes are 57% of the file** (261 + 228 + 9 = 498MB). Every table a lazy
tier removes deletes its data AND its pk autoindex AND its `idx_` pool entries.

Top rels (data + own indexes):

| rel | rows | MB | kind |
|---|---|---|---|
| `_strings` | 1.36M | 92.5 | dictionary (engine) |
| `named_call_site` | 468k | 61.3 | USER, rails.dl:61, materialized join |
| `df_node` + `_rev` + `_repo_rev` + `_in_fn` | ~1.1M | 163.2 | engine df family, 4 projections of one fact set |
| port family (`port_*`, hop staging, reach) | ~1.5M | ~155 | USER, flow-panel.dl |
| `scip_occurrence` + `scip_binding` | 417k | 44.9 | scip import |
| `flow_edge` | 370k | 27.6 | engine |

Confirmed pure duplication (EXCEPT both directions = 0/0): `port_of_reach ==
port_of_reach_rec` (291k rows, 21.7MB), `member_edge == bom_edge` (25k rows).
Static copy-rule scan found 24 distinct single-atom rename rules in this root's
program set (health output, section COPY RULES). Deletion in flight this
session.

Growth is live: 814MB post-VACUUM this morning -> 877MB by 09:00 (daemon
ticking, freelist ~0, so it is new pages, not churn).

## Library research (build-vs-buy, candidate-by-candidate)

Research run 2026-07-19 (web, versions checked). Baseline: rusqlite 0.32
bundles SQLite 3.46.0.

| candidate | status | fit for "heavy derived rels go lazy" | disqualifier |
|---|---|---|---|
| SQLite VIEW (incl. recursive CTE body) | in bundled 3.46 | yes for cheap/rarely-read rels; DDL-only change | full re-run per read; no predicate pushdown into recursion; no indexes on views |
| `dbsp` (Feldera) 0.322.0, MIT, active | eager-incremental circuit engine | no | parallel state store outside SQLite (doesn't shrink anything); hand-built circuits; API churn |
| `differential-dataflow` 0.25.1 | same shape as dbsp | no | arrangements are RAM-resident |
| Materialize | BSL server | no | not embeddable |
| stock-SQLite IVM extension | does not exist | - | pg_ivm is Postgres; OpenIVM is a research prototype |
| Turso `CREATE MATERIALIZED VIEW` (0.5.0, DBSP-backed) | experimental flag | mechanism fits | whole-engine swap off rusqlite/SQLite; feature marked experimental |
| DuckDB + sqlite ATTACH (crate 1.10504.0) | official, MIT | yes as recompute-on-read tier | second engine in-process; extension autoload from network; no incrementality |
| TEMP schema / `memdb` VFS | in bundled 3.46 | yes within one process | invisible to other connections (panel/CLI readers break) |
| `CREATE TABLE AS` + `DROP` on demand | in bundled 3.46 | yes; primitives only | file never shrinks without VACUUM; eviction policy not provided by anything |
| `sqlite-zstd` 0.3.5 | LGPL-2.0+ | no | text/blob columns only; our bloat is int columns + indexes; LGPL; breaks `changes()` |
| `sqlite_zstd_vfs` | C++, low activity | page-level covers indexes | self-described young/risky; no Rust packaging |
| ZIPVFS/CEROD | proprietary | page-level | paid, closed |

Verdict from the table: the only zero-cost library mechanism that removes
table + autoindex + `idx_` bytes at once is the plain SQL VIEW, and
`create_rel_view` machinery already exists for the text layer. Everything
incremental is either an engine swap (Turso), a second engine (DuckDB), or a
second state store (dbsp/DD). The one genuinely missing piece any
demand-materialize design needs is the eviction/bookkeeping policy — SQLite
provides the primitives, no library provides the policy.

## What a `lazy` rel tier would buy (receipts-based)

Candidates that are non-recursive derived rels with few readers:

| rel set | MB now | as VIEW |
|---|---|---|
| `named_call_site` (join of call_site x call_name) | 61 | 0 (recomputed per read) |
| port hop staging (`*_2hop/3hop/4hop/5hop`, `*_seed`, len1-5 unions) | ~71 | 0 |
| df projections (`df_node_rev`, `df_node_repo_rev`, `df_node_in_fn`) | ~111 | 0 |
| rename layers (deleting now) | ~43 | 0 |

Sum ~286MB of today's file is derived-from-derived staging a view tier could
hold at zero bytes, BEFORE dense ids (1a) touches `_strings` and every
remaining index. 873 - 286 - VACUUM ≈ high-500s MB; 1a on top of that attacks
the remaining ~460MB of rows+indexes wholesale.

Costs to price honestly:
- A view re-runs its body per read. The panel reads `rel_*` tables directly;
  a hot panel layer over a 5-hop union view would pay the union every query.
  Per-rel choice, not a global switch.
- Recursion: a rel in a recursive component cannot be a view of itself.
  Restriction: `lazy` only on non-recursive heads (typecheck rejects
  otherwise).
- `@next` carries, effects, and digest-skip read materialized state; a lazy
  rel cannot feed them. Same restriction class, enforceable at typecheck.
- Semantics stay identical otherwise: a view IS the rule body's SQL — the
  lowering already produces exactly this SELECT.

## Open decisions (user's)

1. Surface syntax: `rel lazy foo(...)` vs `@lazy` annotation on the rule.
2. Default polarity: everything eager unless marked lazy (safe, opt-in), vs
   heavy-rel warning from `dl daemon health` suggesting candidates.
3. Whether demand-materialize-with-eviction (the bespoke-policy variant) is
   wanted at all, or VIEW-only covers the need. VIEW-only ships with zero new
   dependencies and no policy code.
4. df-family projections are ENGINE tables — folding them into views is an
   engine change independent of the user-facing lazy tier.
