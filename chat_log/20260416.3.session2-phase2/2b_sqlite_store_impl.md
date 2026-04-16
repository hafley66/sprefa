# 2b — SqliteStore impl

Fill in `impl Store for SqliteStore` with the five real methods
(`register_expr_schema`, `flush_batch`, `query_expr`, `files_scanned`)
plus `effect_status` + `record_effect` for the mutation cache.

## Prereqs

2a (foundation in place, `SqliteStore::open` works).

## Scope

```
v2/src/store/_2_sqlite.rs        impl Store for SqliteStore   (~350 LOC)
```

No other file touched.

## Files

### v2/src/store/_2_sqlite.rs — impl Store

Clone Z3 section `### src/store/_2_sqlite.rs` (lines 693–849 of
design doc). Six methods plus `classify_change` helper plus four
chunked-insert helpers.

| Method | Body source | Notes |
|---|---|---|
| `init` | Z3, `Ok(())` | trivial |
| `register_expr_schema` | Z3 lines 727–761 | classify → DROP / CLEAR / CREATE; upsert `sprf_meta`; cache spec in `self.specs` |
| `flush_batch` | Z3 lines 763–776 | 4-stage chunked insert (strings → files → refs → expr rows); one Tx |
| `query_expr` | Z3 lines 778–783 | uses `build_query_sql(spec, &where)` |
| `files_scanned` | Z3 lines 785–797 | SQL JOIN repos→repo_revs→rev_files→files |
| `effect_status` | Z3 lines 799–819 | Skip/Stale/Emit decision based on `EffectResult` + `content_stable_since` |
| `record_effect` | Z3 lines 821–832 | one INSERT into `mutations` |

### Chunked insert helpers

Z3 declares these as signatures only (lines 845–848). Real bodies clone
from v1:

- `intern_strings(tx, strings) → HashMap<Arc<str>, i64>`:
  chunk 2000 rows, `INSERT OR IGNORE` then `SELECT id FROM strings WHERE
  value IN (?,?,...)`. Reference: `crates/cache/src/sqlite_store.rs:30-78`.
- `upsert_files(tx, files, scanner_hash) → HashMap<(FilePath,
  ContentHash), i64>`: similar, on `(path, content_hash)`. Reference:
  `crates/cache/src/sqlite_store.rs:80-140`.
- `insert_refs(tx, refs) → HashMap<RefKey, i64>`: chunk 1000. Reference:
  `crates/cache/src/sqlite_store.rs:148-200`.
- `insert_expr_rows(tx, spec, rows, &string_ids, &file_ids, &ref_ids)`:
  builds `INSERT INTO {data_table_name} VALUES (?...)` with bound
  params per capture column. New code (v1 has it but shape differs —
  v1 uses `RuleTableDef`, we use `ExprTableSpec`).

### `classify_change`

Z3 lines 834–842. Enum + 4-arm match on `(prior, spec.schema_hash,
spec.extract_hash)` → `New | Unchanged | SchemaChanged | ExtractChanged`.

## Z3 deviations

- `StoreErr::Sql` carries `String`, not `sqlx::Error` (set in Phase 1
  for dep-ordering reasons). Wrap all sqlx error sites with
  `.map_err(|e| StoreErr::Sql(e.to_string()))?`.
- `EffectResult::from_str` in Z3 lines 812 — land as a real impl:
  ```rust
  impl EffectResult {
      pub fn from_str(s: &str) -> Self {
          match s { "Applied" => Self::Applied, "Rejected" => Self::Rejected, _ => Self::Superseded }
      }
      pub fn as_sql_str(self) -> &'static str {
          match self { Self::Applied => "Applied", Self::Rejected => "Rejected", Self::Superseded => "Superseded" }
      }
  }
  ```

## Tests (lands alongside impl)

`v2/tests/store_sqlite.rs` (new):

1. `register_expr_schema_new` — fresh store, register → data table + view + refs_view exist
2. `register_expr_schema_unchanged` — same spec twice → idempotent, no DROP
3. `register_expr_schema_extract_changed` — same schema_hash, new extract_hash → rows cleared, table kept
4. `register_expr_schema_schema_changed` — add capture → DROP + rebuild
5. `flush_batch_and_query_expr` — insert 3 rows via flush_batch, query_expr returns them
6. `effect_status_emit_skip_stale` — record_effect Applied → effect_status returns Skip (or Stale when content_stable_since returns false)
7. `files_scanned_returns_set` — manually insert repo/rev/file rows, files_scanned returns tuples

Each test opens `SqliteStore::open_memory().await`, no shared state.

## Verify

```
cd v2 && cargo test -p v2 --test store_sqlite
```

All seven pass. Other tests still green.

## Exit state

- `impl Store for SqliteStore` complete
- 7 integration tests cover the six trait methods
- G3 (json round-trip through SqliteStore) and G8 (schema evolution) testable,
  though not wired into the stream consumer yet (that is 2c)
