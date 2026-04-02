# Schema Migration + Extraction Coverage

## Context

sprefa indexes all string references across codebases into a normalized SQLite DB. Two problems with the current schema:

1. **ref_kind is a hardcoded Rust enum** -- adding a new extraction pattern (deploy values, OpenAPI operationIds) means editing Rust code and assigning u8 slots. Kinds should be user-definable strings from config.

2. **No provenance on refs** -- when two rules extract the same string at the same byte offset, there's no way to record both interpretations. The `matches` table exists but is only used by the (dormant) standing rules system.

### Design decision: physical vs semantic separation

A ref is physical: "string X at byte Y in file Z." All semantic interpretation (kind, rule name, labels) lives on the match. One ref can have multiple matches from different rules.

```
refs (physical)                    matches (semantic)              match_labels (metadata)
┌──────────────────────┐          ┌─────────────────────────┐     ┌──────────────────────┐
│ id                   │◄────────┐│ id                      │◄───┐│ match_id             │
│ string_id -> strings │         ││ ref_id -> refs          │    ││ key TEXT              │
│ file_id -> files     │         ││ rule_name TEXT           │    ││ value TEXT            │
│ span_start           │         ││ kind TEXT                │    │└──────────────────────┘
│ span_end             │         │└─────────────────────────┘    │
│ is_path              │         │  UNIQUE(ref_id, rule_name)    │ UNIQUE(match_id, key)
│ confidence           │         │                               │
│ target_file_id       │         │  "js" / "dep_name"            │  "env" / "prod"
│ parent_key_string_id │         │  "rs" / "rs_use"              │  "codegen" / "true"
│ node_path            │         │  "deploy-values" / "deploy_value" │
└──────────────────────┘         │  "pkg-json-deps" / "dep_name" │
  UNIQUE(file_id,                │                               │
         string_id,              └───────────────────────────────┘
         span_start)
```

Language extractors write `rule_name = "js"` or `"rs"`. User rules write their config-defined name. Kind is a free-text string -- no enum, no u8 mapping. Queryable with `WHERE kind = 'whatever'`.

### What's done (prior sessions)
- +wt tracking (sessions 1-4): schema, flush, watcher, query scope, ghcache, pause
- JS/TS extractor: imports, exports, aliases, re-exports, require
- Rust extractor: use paths, declarations, modules, extern crates
- Rules engine: JSON/YAML/TOML tree walk, git/file selectors, regex value split
- Watcher pipeline: debounce, classify, diff, plan rewrites, apply
- Query crate: dormant (Expr tree, evaluator, standing rules -- untested, not wired)

## Session 1: Schema migration -- ref_kind moves to matches

**Goal**: Migrate from `ref_kind INTEGER` on refs to `kind TEXT` on matches. All insert paths updated.

### Schema changes (`crates/schema/src/migrations.rs`)

Drop `ref_kind` from refs unique constraint:
```sql
-- refs: remove ref_kind from table and unique constraint
-- New unique: UNIQUE(file_id, string_id, span_start)

-- matches: replace current (rule_id, ref_id) with richer schema
CREATE TABLE IF NOT EXISTS matches_v2 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ref_id INTEGER NOT NULL REFERENCES refs(id),
    rule_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    UNIQUE(ref_id, rule_name)
)
CREATE INDEX IF NOT EXISTS idx_matches_v2_ref_id ON matches_v2(ref_id)
CREATE INDEX IF NOT EXISTS idx_matches_v2_kind ON matches_v2(kind)
CREATE INDEX IF NOT EXISTS idx_matches_v2_rule_name ON matches_v2(rule_name)

-- match_labels
CREATE TABLE IF NOT EXISTS match_labels (
    match_id INTEGER NOT NULL REFERENCES matches_v2(id),
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    UNIQUE(match_id, key)
)
CREATE INDEX IF NOT EXISTS idx_match_labels_match_id ON match_labels(match_id)
```

Migration strategy: idempotent ALTER TABLE / CREATE TABLE IF NOT EXISTS, same pattern as existing migrations. Old `matches` table renamed or kept for compatibility during transition.

### RawRef changes (`crates/extract/src/lib.rs`)

`RawRef.kind` changes from `RefKind` enum to `String`. Add `rule_name: String` field.

```rust
pub struct RawRef {
    pub value: String,
    pub span_start: u32,
    pub span_end: u32,
    pub kind: String,           // was RefKind enum
    pub rule_name: String,      // "js", "rs", "cargo-deps", etc.
    pub is_path: bool,
    pub parent_key: Option<String>,
    pub node_path: Option<String>,
}
```

### Downstream changes

- `crates/js/src/lib.rs` -- all `RefKind::ImportPath` becomes `kind: "import_path".into(), rule_name: "js".into()`
- `crates/rs/src/lib.rs` -- all `RefKind::RsUse` becomes `kind: "rs_use".into(), rule_name: "rs".into()`
- `crates/rules/src/emit.rs` -- `ActionKind::to_ref_kind()` becomes `ActionKind::to_kind_str()`, rule_name comes from `Rule.name`
- `crates/rules/src/types.rs` -- `ActionKind` enum stays as config schema validation, but maps to strings
- `crates/cache/src/flush.rs` -- insert into matches_v2 after inserting refs, using rule_name + kind from RawRef
- `crates/schema/src/types.rs` -- `RefKind` enum kept for backwards compat / display, but not authoritative. Add `kind_from_str()` for known kinds.
- `crates/schema/src/queries.rs` -- `search_refs` JOINs through matches_v2 for kind filtering
- `crates/watch/src/diff.rs`, `plan.rs`, `queries.rs` -- update ref_kind references to JOIN through matches
- `crates/query/src/` -- dormant, update types but don't test yet

### Files touched (estimated)
- `crates/extract/src/lib.rs` -- RawRef struct
- `crates/schema/src/migrations.rs` -- new tables
- `crates/schema/src/types.rs` -- RefKind kept but demoted
- `crates/schema/src/queries.rs` -- search_refs, insert_ref
- `crates/cache/src/flush.rs` -- insert matches after refs
- `crates/js/src/lib.rs` -- kind strings
- `crates/rs/src/lib.rs` -- kind strings
- `crates/rules/src/emit.rs` -- kind string mapping
- `crates/rules/src/types.rs` -- ActionKind -> string
- `crates/watch/src/diff.rs` -- ref_kind -> match kind
- `crates/watch/src/plan.rs` -- ref_kind -> match kind
- `crates/watch/src/queries.rs` -- JOINs
- `crates/server/src/lib.rs` -- QueryHit shape
- `crates/cli/src/main.rs` -- display

### Tests
- Existing tests must still pass (ref insertion, extraction, flush, watcher)
- New: verify two rules can match same ref at same span with different kinds
- New: verify match_labels round-trip

## Session 2: Extraction rule coverage -- new rules + user-defined kinds

**Goal**: Write extraction rules that use user-defined kind strings. Prove the schema works for real patterns.

### New rules in `sprefa-rules.yaml`

1. **tsconfig-paths** -- `**/tsconfig.json`, extract `compilerOptions.paths` keys as `kind: "path_alias"`
2. **package-json-exports** -- `**/package.json`, extract `exports` and `main` as `kind: "package_entry"`
3. **deploy-values** -- `**/values.yaml`, extract keys as `kind: "deploy_value"`
4. **k8s-configmap-envs** -- `**/*configmap*.yaml`, extract `data` keys as `kind: "env_var_name"`
5. **docker-compose-services** -- `**/docker-compose*.yaml`, extract service names as `kind: "service_name"`
6. **openapi-operations** -- `**/openapi*.yaml`, extract operationId as `kind: "operation_id"`
7. **cargo-workspace-members** -- `**/Cargo.toml`, extract workspace.members as `kind: "workspace_member"`
8. **pnpm-workspace** -- `**/pnpm-workspace.yaml`, extract packages as `kind: "workspace_member"`

Each rule defines its own kind string. No Rust enum changes needed.

### Config schema update
The `ActionKind` enum in `crates/rules/src/types.rs` needs to support arbitrary strings in addition to the known variants. Options:
- Add a `Custom(String)` variant
- Or change `kind` field on `EmitRef` from `ActionKind` enum to `String` with validation

### Tests
- Fixture file per rule, snapshot extracted RawRefs
- Verify kind strings appear in matches_v2 table after flush

## Session 3: End-to-end extraction test

**Goal**: Scan a multi-repo fixture, query the DB with raw SQL, verify cross-repo string matching.

### Fixture layout
```
test_fixtures/
  backend/      -- Cargo.toml, src/main.rs, openapi.yaml
  frontend/     -- package.json, tsconfig.json, src/app.ts
  infra/        -- deploy/values.yaml, k8s/configmap.yaml, docker-compose.yaml
```

### SQL assertions
- Same string appearing in multiple repos with different kinds
- FTS5 trigram finds expected results
- parent_key chains intact through matches
- match_labels populated where rules specify them
- Cross-repo: `SELECT s.value FROM strings s JOIN refs r ... JOIN matches_v2 m ... GROUP BY s.value HAVING COUNT(DISTINCT f.repo_id) > 1`

### File
- `crates/scan/tests/extraction_e2e.rs`

## Session 4: Watcher pipeline update + integration test

**Goal**: Watcher works with new schema. Full rename pipeline test with branch scoping.

### Watcher changes
- `diff.rs` and `plan.rs` currently read `ref_kind` from refs table
- Update to JOIN through matches_v2 for kind information
- Rewrite logic needs to know "is this an import_path?" -- query matches_v2 for kind

### Integration test (`crates/watch/tests/3_full_pipeline.rs`)
1. Scan fixtures as committed `main` and `main+wt`
2. Start watcher
3. Rename declaration, verify consumers rewritten
4. SQL query committed vs wt branches

## Session 5: DB documentation + query logging

**Goal**: Document the DB for direct SQL querying. Log queries for pattern discovery.

### Query log table
```sql
CREATE TABLE IF NOT EXISTS query_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sql_text TEXT NOT NULL,
    params_json TEXT,
    result_count INTEGER,
    duration_ms INTEGER,
    context TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
)
```

### Documentation
- `docs/db-schema.md` -- tables, columns, JOINs, FTS5 syntax, query cookbook
- Include: how to find cross-repo string overlap, how to trace parent_key chains, how kind filtering works through matches_v2, useful aggregation queries

## Verification

After session 1:
```bash
cargo test                    # all existing tests pass with new schema
cargo test -p sprefa_cache    # flush tests with matches_v2
```

After session 2:
```bash
cargo test -p sprefa_rules    # new rule extraction snapshots
```

After session 3:
```bash
cargo test -p sprefa_scan --test extraction_e2e
```

After session 4:
```bash
cargo test -p sprefa_watch --test 3_full_pipeline
```

After session 5:
```bash
cargo run -- scan
sqlite3 ~/.sprefa/index.db \
  "SELECT s.value, m.kind, m.rule_name FROM strings s \
   JOIN refs r ON r.string_id = s.id \
   JOIN matches_v2 m ON m.ref_id = r.id \
   LIMIT 20"
```
