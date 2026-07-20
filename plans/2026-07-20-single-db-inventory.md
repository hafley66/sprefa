# Single-DB Inventory: dl engine (sprefa v5), branch v11, HEAD 538e7f78

Mechanical receipts only. All source read via `git show HEAD:<path>`; all searches
via `git grep -n <pattern> HEAD -- src/`. No working-tree file was read (the tree
carried conflict markers in ~24 files at the time).

Headline counts from raw searches:

| pattern | src/ code-line hits |
|---|---|
| `DELETE FROM` | 126 |
| `DROP TABLE` | 13 |
| `TRUNCATE` | 4 (all `PRAGMA wal_checkpoint(TRUNCATE)`; SQLite has no `TRUNCATE TABLE`) |
| `UPDATE ` | 63 (every one carries a WHERE, verified individually) |
| `INSERT OR REPLACE INTO` | **0** |

## 1. Unscoped table-wide writes

### 1A. The named starting points, confirmed

| file:line | SQL template | enclosing fn | target | class | WHERE |
|---|---|---|---|---|---|
| derive.rs:201 | `DELETE FROM {tbl(head)}` | `eval_node2vec_rule` (@127) | `rel_<head>` node_sim | user rel | none |
| derive.rs:484 | `DELETE FROM {tbl(&head_rel)}` | `rebuild_derived` (@334), mirror branch | `rel_<head_rel>` | user rel | none |
| derive.rs:510 | `DELETE FROM {tbl(rel)}` | `rebuild_derived` (@334), legacy loop | `rel_<rel>` | user rel | none |
| derive.rs:546 | `DELETE FROM {tbl(rel)}` | `rebuild_derived` (@334), orphan loop | `rel_<rel>` | user rel | none |
| derive.rs:1123 | `DELETE FROM {tbl(&head_rel)}` | `try_native_depth_walk` (@822) | `rel_<head_rel>` | user rel | none |
| derive.rs:1536 | `DELETE FROM {tbl(&head_rel)}` | `try_native_halt_bfs` (@1198) | `rel_<head_rel>` | user rel | none |
| declare.rs:989 | `DELETE FROM {tbl("true")}` | `refresh_builtin_rels` (@920) | `rel_true` | builtin | none |
| declare.rs:1013 | `DELETE FROM {tbl("every")}` | `refresh_every` (@1006) | `rel_every` | builtin | none |

Two more matching every criterion, missed by the original list:

| file:line | SQL | enclosing fn | target |
|---|---|---|---|
| derive.rs:2353 | `DELETE FROM {tbl(head)}` | `eval_closure_seed_rule_inner` (@2339) | `rel_<head>` |
| derive.rs:2466 | `DELETE FROM {tbl(head)}` | `eval_scc_rule_inner` (@2451) | `rel_<head>` |

Scratch/mirror, table-wide but not the live rel:

| file:line | SQL | enclosing fn | target |
|---|---|---|---|
| derive.rs:493 | `DROP TABLE {mirror_table}` | `rebuild_derived` (@334) | TEMP mirror, connection-local |
| derive.rs:619, 672 | `DROP TABLE [IF EXISTS] {mirror}` | `eval_component_mirror` (@587) | TEMP mirror |
| derive.rs:2102 | `DELETE FROM {scc_node_tbl(edge)}` | `rebuild_closures` (@2064) | `scc_node_<edge>` |
| derive.rs:2103 | `DELETE FROM {scc_edge_tbl(edge)}` | `rebuild_closures` (@2064) | `scc_edge_<edge>` |

### 1B. Every other table-wide delete/drop

| file:line | SQL | enclosing fn | table | class |
|---|---|---|---|---|
| **db.rs:1254** | `DELETE FROM {table}` | `reload_rel(table, cols, rows)` (@1251) | caller-supplied, used with `tbl(rel)` | **user rel, generic helper: scoping this one covers every `refresh_rel` caller** |
| deltaflow.rs:423 | `DELETE FROM {table} WHERE {key} IN (VALUES ...)` | `batch_delete` (@409) | caller-supplied | scoped by key, not repo |
| extract/mod.rs:556 | `DELETE FROM {tbl("module_edge")}` | `rebuild_legacy_module_rels` (@551) | `rel_module_edge` | user rel |
| extract/mod.rs:560 | `DELETE FROM {unresolved}` | same (@551) | `rel_module_unresolved` | user rel |
| extract/mod.rs:567 | `DELETE FROM {binding}` | same (@551) | `rel_module_binding_resolved` | user rel |
| extract/mod.rs:574 | `DELETE FROM {module_binding}` | same (@551) | `rel_module_binding` | user rel |
| extract/mod.rs:1277 | `DELETE FROM {tbl("type_edge")}` | `rebuild_legacy_type_rels` (@1274) | `rel_type_edge` | user rel |
| extract/mod.rs:1284 | `DELETE FROM {entity}` | same (@1274) | `rel_type_entity` | **has a repo column, still wiped whole** |
| extract/mod.rs:1291 | `DELETE FROM {link}` | same (@1274) | `rel_type_link` | user rel |
| extract/mod.rs:1298 | `DELETE FROM {const_value}` | same (@1274) | `rel_const_value` | user rel |
| extract/node.rs:71 | `DELETE FROM _node_path` | `refresh_node_rels` (@19) | `_node_path` | internal |
| meta.rs:167,170,173,176 | `DELETE FROM _reldigest / _derived_complete / _shapes / _stmt_ms` | `ensure_meta` (@150) | internal | schema-version bump |
| meta.rs:842 | `DELETE FROM _shapes` | `persist_type_decl_shapes` (@774) | `_shapes` | internal |
| mod.rs:897 | `DELETE FROM _write_ledger WHERE tick < ?1` | `flush_write_ledger` (@882) | scoped by tick |
| **repo.rs:393** | `DELETE FROM _repo` | `save_repos_meta` (@379) | `_repo`, the repo registry itself | **table-wide on every call** |
| rpc.rs:210 | `DELETE FROM _program` | `save_program_meta` (@189) | `_program` | internal |
| rpc.rs:361 | `DELETE FROM {tbl(out_rel)}` | `drain_rpc` (@349) | port rel | user rel |
| staged_delta/mod.rs:232-236 | 5 deletes in one `execute_batch` | `stage_unsealed` (@224) | staged-delta scratch | internal |
| staged_delta/mod.rs:280,295,347 | `_stage_ready`, `_plus`, `_minus` | `abort_stage` (@279), `consume_stage_in_tx` (@289) | internal |
| storage/call.rs:561-566 | 6 deletes on `_call_*` | `replace_sqlite_call_baseline` (@549) | internal |
| storage/call.rs:799 | `DELETE FROM _call_def` | `replace_sqlite_call_def` (@798) | internal |
| storage/call.rs:358 | `DELETE FROM {table}` | `apply_sqlite_call_owner_delta_inner` (@277) | internal |
| storage/call.rs:115 | `DROP TABLE IF EXISTS _call_def` | `ensure_sqlite_call_def_shape` (@108) | schema migration |

Keyed but not repo-aware (id/tick/state/rev/site_id/stage_id): effect.rs:1092,
cold_stage.rs:496, declare.rs:347,363, derive.rs:207,212,305,311,1712,1749,1789,1824,
extract/mod.rs:649,657-662,679,687-703,992,1016,1071,1082,1102, extract/node.rs:101-120,
meta.rs:164,201-203,501,515,526,1097,1228,1290,1443,1471, pipeline/source_stage.rs:161-175,297-311,
rpc.rs:284,393, source_rows.rs:33,59, staged_delta/sql.rs:183, term_extract.rs:102,
storage.rs:113, storage/call.rs:382,385,528,771-782.

### 1C. `DROP TABLE` outside the above

| file:line | SQL | enclosing fn | note |
|---|---|---|---|
| declare.rs:344 | `DROP TABLE IF EXISTS {tbl(&d.name)}` | `declare` (@212), drift branch | **drops the WHOLE physical table for every repo that rel serves, on any column/PK drift** |
| declare.rs:1102 | `DROP TABLE IF EXISTS {v}` | `declare_closure` (@1079) | closure view |
| derive.rs:1676 | `DROP TABLE IF EXISTS {prefix}{rel}` | `rebuild_derived_seminaive` (@1609) | delta scratch |

## 2. Where physical table names are computed

```
src/lower.rs:6        pub fn tbl(name: &str) -> String { format!("rel_{name}") }
src/lower.rs:7        pub fn txt_tbl(name: &str) -> String { format!("rel_{name}_txt") }
src/engine/mod.rs:103 fn scc_node_tbl(edge) -> format!("scc_node_{edge}")
src/engine/mod.rs:106 fn scc_edge_tbl(edge) -> format!("scc_edge_{edge}")
src/engine/mod.rs:112 fn carry_tbl(rel)    -> format!("_carry_{rel}")
```

Mangling rules:

- `tbl(name)` = `rel_<name>`, literal prefix, no escaping. `name` is a validated rel
  identifier at every call site found.
- `txt_tbl(name)` = VIEW `rel_<name>_txt`, `CREATE VIEW {view} AS SELECT {select} FROM
  {tbl(rel)}` (`declare.rs:121,134`). Decodes interned `sym` INTEGER columns to text.
  Every `txt_tbl` caller is a READ, never a write.
- **`_rev` twin is NOT a helper-applied suffix.** It is baked into the declared rel
  NAME in `src/engine/decls.rs` (`"module_edge_rev"` :471, `"call_def_rev"` :571,
  `"call_edge_rev"` :582). `tbl("module_edge_rev")` mangles like any other name. The
  pairing (a `_rev` rel carries a `rev` column and every revision's rows; the
  unsuffixed rel is a union/dedup over it) is a hand-written convention, not code.
- `scc_node_tbl` / `scc_edge_tbl` carry NO `rel_` prefix and are unreachable through
  `tbl()`.
- `SRC_SUFFIX` / `DRV_SUFFIX` (`src/engine/desugar.rs:32,34`) = `"__src"` / `"__drv"`,
  appended to a rel's NAME before it reaches `tbl()`, for the mixed source+derived
  desugar. A different mechanism from the `_rev` convention: a per-tick synthesized
  `Program` rewrite.

**275 call sites** of `tbl()`/`txt_tbl()` across 24 files
(`git grep -n "\btbl(\|\btxt_tbl(" HEAD -- src/`). Top files: derive.rs 27,
extract/mod.rs 26, lens.rs 25, symbols.rs 24, declare.rs 22, anchor.rs 21, rpc.rs 13,
storage/call.rs 10, lower.rs 7, git.rs 6, analysis.rs 6, meta.rs 5.

Name interpolated into SQL from a rel name beyond the writes in section 1: reads at
derive.rs:163,1084,1428,1461,1916,1978,2031,2124; DDL at declare.rs:867,1060,1090;
meta.rs:853,875,883; query.rs:55; tick.rs:1081; and the lowerer itself at
lower.rs:284,417,452,738,783,830,867,907 (every lowered rule body and every
`?`-query FROM clause).

## 3. How `repo` is populated today

`self.self_slug()` (`src/engine/mod.rs:796`) returns the `--root` basename.
`self.repo_roots()` (`:806`) builds `slug -> PathBuf` from `self_slug()` plus every
`RepoCfg` in `self.repos`. Extraction sites stamp rows by reading `self_slug()`
directly: extract/mod.rs:541, meta.rs:491, path_reconcile.rs:70, lens.rs:373,
git.rs:298, propose.rs:102, scip.rs:248.

The `repo` column is declared `Type::Text` everywhere, never `Type::Repo` (which
exists at `src/ast.rs:5` for DSL-surface typing and is never used for a `"repo"`
column). Physical storage is INTEGER because `Col::interned()` (`ast.rs:60`) is
`ty.textish() && !raw` and `Col::sql()` (`ast.rs:61`) returns INTEGER for any
interned column.

### THE HEADLINE: `repo` is NOT on every rel

Of 122 raw `RelDecl {` hits, 116 are literal declarations, 4 of those are test
fixtures, leaving **112 real built-in rels**:

| | with `repo` | without `repo` | total |
|---|---|---|---|
| all literal `RelDecl` | 33 | 83 | 116 |
| excluding 4 test fixtures | **32** | **80** | **112** |

WITH `repo` (32): decls.rs repo, rev, file, scip_want, rev_cmp_want, checkout,
type_edge, type_edge_rev, type_entity, type_entity_rev, doc_comment, doc_tag,
call_def, call_def_rev, call_site, df_node_repo, df_node_repo_rev, const_value,
const_value_rev, doc_node, doc_ref, head, rev_advanced; scip.rs scip_def, scip_ref,
scip_edge, scip_occurrence, scip_binding; git.rs git_ref, rev_behind;
catalog.rs op_catalog; filelines.rs file_lines.

WITHOUT `repo` (80), including: the ENTIRE module-graph family (module_import,
module_edge, module_edge_rev, module_unresolved, module_unresolved_rev, crate_edge,
module_binding, module_binding_rev, module_binding_resolved,
module_binding_resolved_rev), plus call_edge, call_edge_rev, call_name, call_kind,
type_sig, type_link, type_link_rev, df_node, df_node_rev, df_edge, df_param, df_arg,
df_arg_rev, df_field, df_field_rev, df_lit, df_lit_rev, loop_over, allocates, nest,
comment_node, unresolved, template_parts, node, child, string, ref, program, every,
clock, diag, diag_stage, diag_mute, hover_note, graph_node, graph_edge, def_target,
effect_cmd, effect_log, hook_event, checkout_done, checkout_plan, type_decl_row,
scip_name, scip_fn_edge, scip_callee_type, scip_local, scip_impl, agent_edit,
agent_touch, skill_loaded, dl_diag, type_shape, type_lgg, changed, changed_line,
created, rel_catalog, rel_col, fn_catalog, verb_catalog, propose_extract,
propose_clone, rel_count, stmt_ms, query_log, env, similar.

Caveat on the count: parsed by regex-splitting on `RelDecl {` boundaries and matching
`name:\s*"([^"]+)"` per chunk. Self-consistent per file, not cross-checked against a
second parser.

Sibling evidence that derived rules do not carry `repo` through: `type_entity`
(has repo) and `type_link` (no repo) are populated by the SAME family
(`extract/type_rels.rs`). Same split for `call_def`/`call_def_rev` (repo) versus
`call_edge`/`call_name`/`call_kind` (no repo), and `df_node_repo`/`df_node_repo_rev`
(repo) versus the whole dataflow body (no repo).

## 4. Blast radius of `root_db_path` / `root_db_dir` / `roots.json` / `key_of`

Definitions in `src/daemon/home.rs`: `daemon_home()` :66, `roots_json_path()` :67,
`root_db_dir(key)` :72, `root_db_path(root)` :84, `key_of(canon)` :91 (blake3-16hex).
`read_roots_json` / `write_roots_json` at `src/daemon/root.rs:760,774`.

| file:line | symbol | enclosing fn |
|---|---|---|
| cli/health.rs:42 | `read_roots_json` | `run` (@37) |
| cli/mod.rs:424 | `root_db_path` | `dispatch_mode` (@385) |
| daemon/client.rs:288-292 | `key_of`, `read_roots_json`, `write_roots_json`, `root_db_dir` | `drop_root` (@284) |
| daemon/mod.rs:253 | `key_of` | `resolve` (@247) |
| daemon/mod.rs:274,303,311,314 | `key_of`, `root_db_dir`, roots.json rw | `add_root` (@272) |
| daemon/mod.rs:329,335,337,340 | `key_of`, roots.json rw, `root_db_dir` | `drop_root` (@327) |
| daemon/mod.rs:598,615 | roots.json rw | `run_daemon` (@411), boot replay |
| daemon/mod.rs:622,624,630,633 | `key_of`, `root_db_dir`, roots.json rw | `run_daemon` (@411), stale eviction |
| daemon/mod.rs:872,873 | `key_of` x2 | `#[test] key_of_is_stable_and_short` (@870) |
| daemon/root.rs:664 | `root_db_dir` | `open` (@639) |
| hook.rs:468,473 | `root_db_path`, `read_roots_json` | `refuse_worktree_cold_check` (@461) |
| lsp.rs:91 | `root_db_path` | `run_lsp` (@45) |

3 files own the helpers, 7 call-site files consume them across 10 functions.

## 5. Tests asserting per-root paths, orphans, or multi-root isolation

| file:line | test | assertion |
|---|---|---|
| tests/it/daemon.rs:137 | `singleton_serves_two_roots_isolated` | a query against root A never sees root B's rows |
| tests/it/daemon.rs:171-174 | same | `dbs.len() >= 2` under `home/sprefa/roots/` |
| tests/it/daemon.rs:183,198 | `add_root_is_idempotent` | same key, `root_count == 1` |
| tests/it/daemon.rs:207 | `nested_root_registration_refused` | nested root refused |
| tests/it/daemon.rs:228,250 | `drop_purge_removes_root_db` | `roots/<key>/db.sqlite` exists before, gone after |
| tests/it/daemon.rs:262,324 | `boot_replay_evicts_root_with_deleted_directory` | deleted root evicted from roots.json |
| tests/it/check_daemon.rs:94-102,167,285 | `root_dbs` helper, `discovery_check_cold_fallback_is_loud`, `forced_inprocess_check_shares_the_daemon_db` | cold run lands in `roots/<key>/db.sqlite`, count == 1 |
| tests/it/discover.rs:26-31,130,149 | `root_dbs` helper, `discovery_defaults_db_to_shared_root_db`, `old_cache_db_world_is_left_untouched` | count == 1 |
| tests/it/worktree_cold_check.rs:60-70,82 | `roots_db_bytes`, `check_in_cold_unregistered_worktree_skips_instead_of_building` | cold refusal writes ZERO bytes under `roots/` |
| tests/it/setup_manifest.rs:313 | `uninstall_removes_journal_and_wiring_but_leaves_unowned_content` | roots.json gone after uninstall |
| src/daemon/mod.rs:870-874 | `key_of_is_stable_and_short` | deterministic, 16 hex |

**Empty search recorded**: no `tests/it/health.rs` exists and
`git grep -n "report_roots_overview" HEAD -- tests/` returns nothing. The orphan-roots
reporting path (`src/cli/health.rs:69`), which `dl daemon health` help text advertises,
has NO integration coverage.

## 6. busy_timeout, WAL, transaction helpers in src/db.rs

| file:line | setting |
|---|---|
| db.rs:210 | `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;` on every open |
| db.rs:220-223 | `cache_size=-{kib}`, `mmap_size`, `temp_store=FILE` |
| db.rs:235 | `PRAGMA busy_timeout=50` throwaway probe, restored immediately |
| db.rs:312 | `busy_timeout(1000ms)` in `open_read_only` |
| db.rs:328-368 | `install_busy_verdict_handler`: custom `busy_handler` REPLACING the pragma. 20ms sleep per retry (:366), **gives up at 5000ms** (:364), logs a `[sqlite] busy retry` verdict once past `SQLITE_BUSY_WARN_MS` |
| verdict.rs:21 | `SQLITE_BUSY_WARN_MS = 100` |
| db.rs:1418 | `busy_timeout(1000ms)`, second read-only path |
| db.rs:1495 | `PRAGMA wal_checkpoint(TRUNCATE)` in a Drop impl |
| db.rs:1201-1212 | `begin()`, `begin_immediate()`, `commit()` |
| db.rs:1042,1063 | `owns_tx` pattern in `upsert_rows` (@1022): BEGIN/COMMIT only when `rows.len() > chunk_rows && conn.is_autocommit()`, rollback on chunk error |
| db.rs:1292,1317 | same `owns_tx` pattern in `insert_rows_keyed` (@1276) |
| engine/tick.rs:9-55 | `BulkRebuildIo` RAII: on enter `synchronous=OFF; wal_autocheckpoint=0`; on Drop restores and runs `wal_checkpoint(TRUNCATE)`. Its doc (:18-22) says this is safe "ONLY because the derived tables live in the CACHE db" |

## 7. Contradictions with the framing

1. **`repo` is NOT on every rel.** 32 of 112 real built-in rels carry it; 80 do not.
   The module-graph family has ZERO repo-carrying members despite being populated per
   call from `self_slug()`. This invalidates "no schema work needed."
2. **Some deletes are ALREADY repo-scoped**: `source_rows.rs:46,53`
   (`WHERE (repo, path) IN ...`) and `meta.rs:1293` (`WHERE (repo, path, rev) IN ...`).
   The exception, not the rule, but they should not be re-scoped from zero.
3. **`_repo`, the repo registry table, is wiped table-wide on every save**
   (`repo.rs:393`, `save_repos_meta` deletes then reinserts the engine's entire
   in-memory list). Harmless per-root today. In a shared db two engines calling this
   would each wipe the other's rows.
4. **No unconditional `UPDATE` exists.** All 63 carry a WHERE, verified individually.
5. **`INSERT OR REPLACE INTO` returns ZERO hits.** Upserts go through
   `db.rs::upsert_rows` (`ON CONFLICT ... DO UPDATE`) or `INSERT OR IGNORE`.
6. **`Type::Repo` is not what the `repo` column uses.** Every `"repo"` column is
   `c("repo", Type::Text)`. Grepping `Type::Repo` as a proxy finds nothing relevant.
7. **A repo-via-companion-table precedent already exists**: `df_node_repo` /
   `df_node_repo_rev` (`decls.rs:635,645`) hold only `(id, repo[, rev])` and join
   against the repo-less `df_node`. Shipped for exactly one rel family.
8. **`invocation` and `Jobs` live in SEPARATE database files** (`invocations.db`,
   `jobs.sqlite`) and are out of scope for a per-repo single-db migration. Their
   table-wide-looking deletes (`invlog.rs:89`, `jobq/mod.rs:468`) must not be
   miscounted as part of the blocker surface.
9. **`pending_effect` has no `repo` column** (raw `CREATE TABLE` at `meta.rs:392-398`,
   hand-written, never routed through `RelDecl`/`tbl()`) yet lives in the per-root db.
   In scope for the migration with no repo axis at all, not even a companion table.
10. **Test fixtures inflate a naive grep**: raw count 122, literal 116, 4 of those are
    test fixtures (`typed_plan.rs:701`, `pipeline/full_sources_tests.rs:10,20`,
    `declare.rs:1193`).
