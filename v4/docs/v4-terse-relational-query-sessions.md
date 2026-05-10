# V4 Terse Relational Query Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make common rule joins, filters, ordering, limiting, grouping, and render-hole relation queries terse without making raw SQL the main user surface.

**Architecture:** Keep the host language centered on pipes, cursors, terms, rules, and nested DSL holes. Lower terse relational pipe segments to SQLite-shaped execution when they cross a batch relation boundary, while preserving raw `sql`` as the explicit escape hatch. Store rule/table/column/input metadata in compiler/LSP context so SQL and terse ops can autocomplete valid moves.

**Tech Stack:** Rust, tree-sitter-sprefa, effect_runtime v2, `FactStore<Cursor>`, SQLite via `rusqlite`, V4 LSP DSL providers.

---

## Current Executable Baseline

- Rule query syntax already covers equality join + projection:

```sprf
api_ops(PATH?, METHOD?, OP?)
  > api_responses(PATH, METHOD, STATUS?, DESC?)
```

- `sql`` already supports batch-local execution, `${TERM}` interpolation to `input.TERM`, mounted query output diffing, table dirty wakes, SQLite-backed queue revive, and `ORDER BY`/`GROUP BY`/`LIMIT` through raw SQLite.
- Nested render holes now support arbitrary nested pipe expressions.
- SQL LSP exists at DSL level but is schema-light: keywords, `input.__cursor_idx`, `input.value`, host-hole hover.
- Missing: a relational plan layer for terse ops, schema-aware SQL/LSP completions, tiny query-shaping ops, and numeric coercion rules.

## Target Surface

### Terse Equality Join

```sprf
api_ops(PATH?, METHOD?, OP?, SUMMARY?)
  > api_responses(PATH, METHOD, STATUS?, DESC?)
  > order_num(STATUS)
  > render_markdown`- ${STATUS}: ${DESC}`
```

Lowered relational shape:

```sql
SELECT input.__cursor_idx, r.STATUS, r.DESC
FROM input
JOIN api_responses AS r
  ON r.PATH = input.PATH
 AND r.METHOD = input.METHOD
ORDER BY CAST(r.STATUS AS INTEGER)
```

### SQL Escape Hatch With Schema-Aware Interpolation

```sprf
api_ops(PATH?, METHOD?, OP?)
  > sql`
      SELECT input.__cursor_idx, r.STATUS, r.DESC
      FROM api_responses AS r
      WHERE r.PATH = ${PATH}
        AND r.METHOD = ${METHOD}
      ORDER BY CAST(r.STATUS AS INTEGER)
    `
```

`${TERM}` remains value interpolation only. Table and column names are resolved by compiler metadata, never by identifier interpolation.

### Render-Hole Relation Query

```sprf
render_markdown`
${ api_ops(PATH?, METHOD?, OP?)
  > render_markdown`### ${OP}
${ api_responses(PATH, METHOD, STATUS?, DESC?)
  > order_num(STATUS)
  > render_markdown`- ${STATUS}: ${DESC}
`
}
`
}
`
```

Each render hole implicitly collects emitted cursor values for the current generation.

---

## File Map

| Path | Responsibility |
| --- | --- |
| `v4/src/rel.rs` | New sprf relational plan types and lowering helpers for terse relational ops. |
| `v4/src/sql.rs` | Existing SQLite batch executor. Add reusable SQL table/schema metadata and generated-SQL constructor entry points. |
| `v4/src/rule.rs` | Existing rule write/query runtime. Keep rule call semantics here; expose table declaration metadata through `FactStore`. |
| `v4/src/compile/lower/op_def.rs` | Add optional operator metadata for relation-shaping ops if needed. |
| `v4/src/compile/lower/ops.rs` | Register `where_*`, `order_*`, `limit`, `count`, `group` defs. |
| `v4/src/compile/lower/ctx.rs` | Carry per-program rule schema and current input schema snapshots for LSP and lower diagnostics. |
| `v4/src/cst/dsls/sql/mod.rs` | Expand SQL completions/hover/token context with rule table and column metadata. |
| `v4/src/app.rs` | Thread schema metadata into LSP open/hover/completion responses. |
| `v4/tests/sql_rule_query_smoke.rs` | Add terse rule join/order/filter tests. |
| `v4/tests/lsp_hover_smoke.rs` | Add schema-aware SQL hover/completion smoke tests. |
| `v4/tests/v3_parity_target.rs` | Add end-to-end examples using render holes and terse relational ops. |
| `v4/examples/openapi-cardinality-markdown.sprf` | Convert from inner raw SQL to terse relational ops once implemented. |

---

## Session 1: Lock Relational Segment IR

**Files:**
- Create: `v4/src/rel.rs`
- Modify: `v4/src/lib.rs`
- Test: `v4/tests/sql_rule_query_smoke.rs`

- [ ] **Step 1: Add failing tests for relation segment SQL generation**

Add tests that do not run the whole runtime yet. They should prove a plan can represent:

```text
input schema: PATH, METHOD
join table: api_responses(PATH, METHOD, STATUS, DESC)
project: STATUS?, DESC?
order: STATUS numeric asc
```

Expected generated SQL:

```sql
SELECT input.__cursor_idx, r0."STATUS", r0."DESC"
FROM input
JOIN "api_responses" AS r0
  ON r0."PATH" = input."PATH"
 AND r0."METHOD" = input."METHOD"
ORDER BY CAST(r0."STATUS" AS INTEGER) ASC
```

Run:

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml rel_plan -- --nocapture
```

Expected: fail because `v4::rel` does not exist.

- [ ] **Step 2: Create `v4/src/rel.rs` with explicit data types**

Add:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelValueType {
    Text,
    Number,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelOrderDir {
    Asc,
    Desc,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelRuleSchema {
    pub table: String,
    pub cols: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelArg {
    BoundTerm { col: String, term: String },
    Project { col: String, term: String },
    Literal { col: String, value: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelJoin {
    pub table: String,
    pub alias: String,
    pub args: Vec<RelArg>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelOrder {
    pub term: String,
    pub value_type: RelValueType,
    pub dir: RelOrderDir,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelPlan {
    pub input_terms: Vec<String>,
    pub joins: Vec<RelJoin>,
    pub order: Vec<RelOrder>,
    pub limit: Option<u64>,
}
```

- [ ] **Step 3: Add SQL renderer with quoted identifiers**

Add functions:

```rust
pub fn quote_ident(s: &str) -> String;
pub fn render_rel_plan_sql(plan: &RelPlan) -> Result<String, String>;
```

Rules:

- Every plan selects `input.__cursor_idx`.
- Project args add `alias."COL" AS "TERM"`.
- Bound args add `alias."COL" = input."TERM"`.
- Literal args add `alias."COL" = 'literal'` using SQLite string quoting.
- `order_num(X)` renders `ORDER BY CAST("<source>" AS INTEGER)`.
- Missing source for order term returns an error.

- [ ] **Step 4: Run relation plan tests**

Run:

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml rel_plan -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add v4/src/rel.rs v4/src/lib.rs v4/tests/sql_rule_query_smoke.rs
git commit -m "feat: add relational query plan"
```

---

## Session 2: Lower Rule Query + `order_*` Into Generated SQL

**Files:**
- Modify: `v4/src/sql.rs`
- Modify: `v4/src/rule.rs`
- Modify: `v4/src/compile/lower/ops.rs`
- Test: `v4/tests/sql_rule_query_smoke.rs`

- [ ] **Step 1: Add failing runtime test**

Add a test:

```sprf
rule(:api_ops, PATH?, METHOD?, OP?);
rule(:api_responses, PATH?, METHOD?, STATUS?, DESC?);
rule(:rows, OP?, STATUS?, DESC?);

`/pets` > PATH? > `get` > METHOD? > `listPets` > OP? > api_ops(PATH, METHOD, OP);
`/pets` > PATH? > `get` > METHOD? > `200` > STATUS? > `ok` > DESC? > api_responses(PATH, METHOD, STATUS, DESC);
`/pets` > PATH? > `get` > METHOD? > `401` > STATUS? > `auth` > DESC? > api_responses(PATH, METHOD, STATUS, DESC);

api_ops?(PATH?, METHOD?, OP?)
  > api_responses?(PATH, METHOD, STATUS?, DESC?)
  > order_num(STATUS)
  > rows(OP, STATUS, DESC);
```

Expected rows:

```text
rows[0].STATUS = 200
rows[1].STATUS = 401
```

Run:

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml terse_rule_join_orders_numeric_status -- --nocapture
```

Expected: fail because `order_num` does not exist and rule query executes immediately as cursor operation, not as a fused relational segment.

- [ ] **Step 2: Add `OrderNumDef`**

Register `order_num` in `default_registry`.

Temporary implementation can be a marker component whose `describe()` exposes:

```rust
pub struct RelOrderMarker {
    pub term: Arc<str>,
    pub value_type: RelValueType,
    pub dir: RelOrderDir,
}
```

This marker should not emit rows by itself once fused. If it runs unfused, emit a runtime diagnostic:

```text
rel/unfused-order: order_num requires a preceding relation query segment
```

- [ ] **Step 3: Add relation marker descriptors**

Rule query lowering should be able to expose a descriptor for:

```rust
pub struct RelRuleReadMarker {
    pub table: Arc<str>,
    pub args: Vec<CallArg>,
}
```

Do this without changing rule apply/write semantics. Bare rule call applies/writes/runs. Question-suffixed rule call queries/reads.

- [ ] **Step 4: Add a pipe fusion pass before final lowering or as a lower-time chain rewrite**

Find a contiguous segment:

```text
rule read -> rule read -> order_num/order/order_desc/limit
```

Convert it into one `SqlQueryComponent` using `rel::render_rel_plan_sql`.

Minimum first fuse:

```text
api_ops(...) > api_responses(...) > order_num(...)
```

Do not fuse across:

- `render_markdown`
- `write_file`
- `write_cursor`
- `sh` / `sh!`
- `next` / `next?`
- any component without a relation descriptor

- [ ] **Step 5: Run targeted tests**

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml terse_rule_join_orders_numeric_status -- --nocapture
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml sql_rule_query_smoke -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add v4/src/sql.rs v4/src/rule.rs v4/src/compile/lower/ops.rs v4/tests/sql_rule_query_smoke.rs
git commit -m "feat: fuse terse rule joins to sql"
```

---

## Session 3: Tiny Query-Shaping Ops

**Files:**
- Modify: `v4/src/rel.rs`
- Modify: `v4/src/compile/lower/ops.rs`
- Test: `v4/tests/sql_rule_query_smoke.rs`

Implement only the low-decision ops first:

| Op | Meaning |
| --- | --- |
| `order(TERM)` | `ORDER BY TERM ASC` as text |
| `order_desc(TERM)` | `ORDER BY TERM DESC` as text |
| `order_num(TERM)` | `ORDER BY CAST(TERM AS INTEGER) ASC` |
| `order_num_desc(TERM)` | `ORDER BY CAST(TERM AS INTEGER) DESC` |
| `limit(N)` | `LIMIT N`, integer literal only |
| `where_eq(TERM, VALUE)` | text equality |
| `where_ne(TERM, VALUE)` | text inequality |
| `where_num_ge(TERM, VALUE)` | numeric greater-or-equal |
| `where_num_gt(TERM, VALUE)` | numeric greater-than |
| `where_num_le(TERM, VALUE)` | numeric less-or-equal |
| `where_num_lt(TERM, VALUE)` | numeric less-than |

- [ ] **Step 1: Add tests for text filter + limit**

Example:

```sprf
api_ops?(PATH?, METHOD?, OP?)
  > api_responses?(PATH, METHOD, STATUS?, DESC?)
  > where_eq(STATUS, `200`)
  > limit(1)
  > rows(OP, STATUS, DESC);
```

Expected: only the `200` row emits.

- [ ] **Step 2: Add tests for numeric filter**

Example:

```sprf
api_ops?(PATH?, METHOD?, OP?)
  > api_responses?(PATH, METHOD, STATUS?, DESC?)
  > where_num_ge(STATUS, `400`)
  > order_num(STATUS)
  > rows(OP, STATUS, DESC);
```

Expected: only `400+` rows emit in numeric order.

- [ ] **Step 3: Extend `RelPlan` with filters**

Add:

```rust
pub enum RelFilterOp {
    Eq,
    Ne,
    NumGt,
    NumGe,
    NumLt,
    NumLe,
}

pub struct RelFilter {
    pub term: String,
    pub op: RelFilterOp,
    pub value: String,
}
```

Render text filters as direct string predicates and numeric filters with `CAST(... AS REAL)`.

- [ ] **Step 4: Add operator defs and descriptors**

Each `where_*` op lowers to a marker descriptor. Fusion consumes markers into `RelPlan`.

If a marker runs unfused, emit:

```text
rel/unfused-filter
rel/unfused-limit
rel/unfused-order
```

- [ ] **Step 5: Run tests**

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml sql_rule_query_smoke -- --nocapture
```

- [ ] **Step 6: Commit**

```bash
git add v4/src/rel.rs v4/src/compile/lower/ops.rs v4/tests/sql_rule_query_smoke.rs
git commit -m "feat: add terse relational filters"
```

---

## Session 4: Schema-Aware SQL LSP

**Files:**
- Modify: `v4/src/app.rs`
- Modify: `v4/src/cst/dsls/sql/mod.rs`
- Modify: `v4/src/compile/lower/ctx.rs`
- Test: `v4/tests/lsp_hover_smoke.rs`
- Test: `v4/tests/lsp_locate_dsl_smoke.rs`

- [ ] **Step 1: Add schema metadata type**

Add:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlLspSchema {
    pub input_cols: Vec<String>,
    pub rule_tables: BTreeMap<String, Vec<String>>,
    pub aliases: BTreeMap<String, String>,
}
```

Source:

- `input_cols` from terms bound before the `sql`` op in the containing pipe.
- `rule_tables` from `rule(:name, COL?, ...)` declarations in the current program.
- `aliases` parsed from `FROM table AS alias` and `JOIN table alias`.

- [ ] **Step 2: Add failing completion tests**

Open a document with:

```sprf
rule(:api_responses, PATH?, METHOD?, STATUS?, DESC?);
api_ops(PATH?, METHOD?, OP?)
  > sql`SELECT r. FROM api_responses AS r WHERE r.PATH = ${PATH}`;
```

Completions at `r.` should include:

```text
r.PATH
r.METHOD
r.STATUS
r.DESC
```

Completions inside `${` should include:

```text
PATH
METHOD
OP
&.value
```

- [ ] **Step 3: Add failing hover tests**

Hover on:

- `api_responses`: shows `rule table api_responses(PATH, METHOD, STATUS, DESC)`.
- `r.STATUS`: shows `api_responses.STATUS`.
- `${PATH}`: shows `input term PATH`.

- [ ] **Step 4: Implement schema-aware provider path**

Keep `SqlDsl::new()` for static tests. Add:

```rust
impl SqlDsl {
    pub fn with_schema(schema: SqlLspSchema) -> Self;
}
```

Update `app.rs` DSL lookup so `sql`` bodies get a provider with schema when opened through the app/LSP path.

- [ ] **Step 5: Add diagnostics for unknown table/column**

Emit LSP diagnostics for:

```text
sql/unknown-table
sql/unknown-column
sql/unknown-input-term
```

Keep runtime errors from SQLite too. These diagnostics are editor hints.

- [ ] **Step 6: Run tests**

```bash
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml lsp_hover sql_lsp -- --nocapture
```

- [ ] **Step 7: Commit**

```bash
git add v4/src/app.rs v4/src/cst/dsls/sql/mod.rs v4/src/compile/lower/ctx.rs v4/tests/lsp_hover_smoke.rs v4/tests/lsp_locate_dsl_smoke.rs
git commit -m "feat: add schema aware sql lsp"
```

---

## Session 5: Convert OpenAPI Demo Away From Raw SQL

**Files:**
- Modify: `v4/examples/openapi-cardinality-markdown.sprf`
- Modify: `v4/examples/openapi-cardinality.md`
- Test: `v4/tests/sprefa_run_cli_smoke.rs`

- [ ] **Step 1: Add CLI smoke for demo**

Add test that runs:

```bash
RUSTC_WRAPPER= cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/openapi-cardinality-markdown.sprf --root . --show-rows
```

Assert output contains:

```text
api_info: 1 rows
api_ops: 3 rows
api_responses: 6 rows
```

Assert generated markdown contains:

```text
### **get** **/pets**
  - **200**: Pet list
  - **401**: Missing or invalid session
```

- [ ] **Step 2: Replace inner raw SQL with terse ops**

Current:

```sprf
${ sql`
      SELECT input.__cursor_idx, api_responses.STATUS, api_responses.DESC
      FROM input
      JOIN api_responses
        ON api_responses.PATH = ${PATH}
       AND api_responses.METHOD = ${METHOD}
      ORDER BY api_responses.STATUS
    `
  > render_markdown`  - ${ STATUS > `**${&.value}**` }: ${DESC}
`
}
```

Target:

```sprf
${ api_responses(PATH, METHOD, STATUS?, DESC?)
  > order_num(STATUS)
  > render_markdown`  - ${ STATUS > `**${&.value}**` }: ${DESC}
`
}
```

- [ ] **Step 3: Run demo and test**

```bash
RUSTC_WRAPPER= cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/openapi-cardinality-markdown.sprf --root . --show-rows
RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml openapi_cardinality_markdown_demo -- --nocapture
```

- [ ] **Step 4: Commit**

```bash
git add v4/examples/openapi-cardinality-markdown.sprf v4/examples/openapi-cardinality.md v4/tests/sprefa_run_cli_smoke.rs
git commit -m "demo: use terse relational openapi render"
```

---

## Session 6: Lifting Metadata, No Optimizer Yet

**Files:**
- Modify: `v4/src/compile/lower/op_def.rs`
- Modify: `v4/src/sprf_introspect.rs`
- Modify: selected operator defs in `v4/src/compile/lower/ops.rs`
- Test: `v4/tests/v4_v3_parity_ops_smoke.rs`

Add declarative metadata only. Do not rewrite execution yet.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitShape {
    ZeroOrOne,
    ZeroOrMany,
    BatchOne,
    Parked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueAccess {
    None,
    Text,
    Bytes,
    Path,
    RuleTable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpRuntimeShape {
    pub pure: bool,
    pub deterministic: bool,
    pub emit_shape: EmitShape,
    pub value_access: ValueAccess,
}
```

Add:

```rust
fn runtime_shape(&self) -> OpRuntimeShape;
```

Known examples:

| Op | Shape |
| --- | --- |
| static backtick string | pure, deterministic, `ZeroOrOne`, `None` |
| `read` | read/effect boundary, deterministic for file rev, `ZeroOrOne`, `Path -> Bytes/Text` |
| `re` | pure for fixed pattern, deterministic, `ZeroOrMany`, `Text/Bytes` |
| `json` | pure for fixed pattern, deterministic, `ZeroOrMany`, `Text/Bytes` |
| `sql` | read, deterministic over table versions + input, `ZeroOrMany`, `RuleTable` |
| `render_markdown` | batch collect, deterministic, `BatchOne`, `Text` |
| `sh` | impure, not deterministic, `ZeroOrOne`, `Text` |
| `next` | parked, eventful, `Parked`, `None` |

Tests should assert the descriptors for these ops. No optimizer behavior should change.

---

## Session 7: Forking Syntax Target Tests

**Files:**
- Modify: `v4/crates/tree-sitter-sprefa/grammar.js`
- Regenerate parser files under `v4/crates/tree-sitter-sprefa/src/`
- Modify: `v4/src/compile/parse.rs`
- Test: `v4/tests/v4_parse_smoke.rs`
- Test: `v4/tests/v4_walk_smoke.rs`

Target syntax is undecided. Add target tests first after spec lock-in.

Candidate A:

```sprf
source
  > fork {
      re`TODO`
      json`{ name: $NAME }`
      ast(:rs)`fn $NAME() {}`
    }
```

Candidate B:

```sprf
source
  > (
      re`TODO`
    | json`{ name: $NAME }`
    | ast(:rs)`fn $NAME() {}`
    )
```

Runtime contract:

- Each branch receives the same input cursor.
- Branch outputs merge into one downstream stream.
- Cursor identity/support metadata must preserve branch identity.
- No branch can mutate shared state before merge unless it is explicitly impure.

Implementation should wait for syntax lock-in.

---

## Session 8: JSON Whole-Subtree Capture

**Files:**
- Modify: `v4/src/cst/dsls/json/walk/brace_parse.rs`
- Modify: `v4/src/cst/dsls/json/walk/compile.rs`
- Modify: `v4/src/cst/dsls/json/walk/walker.rs`
- Modify: `v4/src/v2_ops.rs`
- Test: `v4/tests/cst_dogfood.rs`
- Test: `v4/tests/read_gates_bytes_smoke.rs`

Need spec lock-in first.

Candidate target:

```sprf
json`{ paths: { $PATH: $$NODE } }`
```

Runtime:

- `NODE` value is raw JSON slice for the matched subtree.
- `NODE_LO`, `NODE_HI`, `NODE_FS` point at the subtree byte range.
- If subtree is scalar, value is scalar text as today.
- If subtree is object/array, value is source slice including braces/brackets.

Tests:

```json
{"paths":{"/pets":{"get":{"operationId":"listPets"}}}}
```

Pattern:

```sprf
json`{ paths: { $PATH: $$NODE } }`
```

Expected:

```text
PATH = /pets
NODE = {"get":{"operationId":"listPets"}}
NODE_LO/HI cover the object range
```

---

## Risk Register

| Risk | Guard |
| --- | --- |
| Rebuilding SQL inside too many components | Fuse only declared relation segment markers into one `SqlQueryComponent`. |
| SQL becomes the real language by accident | Keep raw `sql`` as escape hatch; add terse ops only for common clauses. |
| LSP schema gets stale | Build schema from current open document parse + declared rule tables, not global cache first. |
| Numeric semantics drift | Keep numeric ops explicit: `where_num_*`, `order_num*`. |
| Rule query and rule apply blur again | `rule?(...)` reads table. `rule(...)` writes/runs. `rule!(...)` stays reserved for policy. |
| Render holes hide expensive queries | LSP hover should show “hole runs relation query, emits N rows from last analysis”. |
| Forking explodes support counts | Branch id must become part of cursor support identity. |

---

## Spec Questions

These need lock-in before implementation starts.

1. Should terse relation ops lower to SQLite immediately for every fused segment, or should the first version keep existing cursor-by-cursor rule query and only use SQLite once `order`/`group`/`limit` appears?

2. Numeric/query predicate syntax: use explicit boring ops like `where_num_ge(STATUS, \`400\`)`, or accept an expression parser now with `where(STATUS >= \`400\`)`?

3. JSON whole-subtree capture syntax: use `$$NODE`, `${NODE?}`, or another marker for “bind the raw matched object/array slice plus byte range”?
