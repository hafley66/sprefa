# Plan: file-scoped rule/fact tables in v4

## 0. Status

| piece | exists? | where |
|---|---|---|
| `Rule { name, sink_table, sink_cols, ... }` | yes | `v4/src/rule.rs:34-49` |
| `Rule::new(name, store, sink_table, cols, body)` declares `sink_table` | yes | `v4/src/rule.rs:52-70` |
| `RuleDef::lower` calls `store.declare(name, cols)` for decl-only rules | yes | `v4/src/compile/lower/ops.rs:235` |
| `RuleDef::lower` calls `Rule::new(name, store, name, cols, body)` (table = name) | yes | `v4/src/compile/lower/ops.rs:249` |
| `LowerCtx.rules: Arc<Mutex<HashMap<Arc<str>, Rule>>>` (per-file lookup) | yes | `v4/src/compile/lower/ctx.rs:35` |
| `LowerCtx.sprf_dir: PathBuf` (script's folder, not URI) | yes | `v4/src/compile/lower/ctx.rs:29` |
| `LowerCtx.sprf_uri` | **no** | this plan |
| `FactStore` per-file isolation | **no** | one global store on `SprfState.facts` (`v4/src/app.rs:601`) |
| Rule-call dispatch (`r?(args)`) uses `LowerCtx.rules.get(name)` | yes | already per-file |
| SQL-DSL bare table refs (`SELECT * FROM rule_name`) | unscoped today | `v4/src/cst/dsls/sql/` |
| Cross-file federation (explicit "share this table") | **no** | this plan defers it; v3 had no equivalent either |
| v3 had this; v4 rewrite dropped it | yes | `9948a884` — "Per-rule tables now prefixed by .sprf filename stem" |

The bug surface today: the LSP daemon's `lsp_pre_warm` (`v4/src/app.rs:1665-1860`) ingests every `.sprf` under the workspace root into ONE shared `SprfStore`. Two files declaring `rule(:hits, ...)` with different column shapes panic `fact_store.rs::declare` (`v3/crates/effect_runtime/src/v2/fact_store.rs:940`). CLI mode (one `.sprf` per run) never hit this.

## 1. Goal

Same logical rule name in two different `.sprf` files lowers to two distinct backing tables. Same-file rule calls (`r?(args)`) and SQL refs continue to resolve transparently. No new author syntax; no opt-in flag. CLI mode is unchanged because CLI always sets the URI to one file.

Non-goal: cross-file federation surface. Deferred; v3 didn't have it. If/when it lands, it's an explicit op (e.g., `extern_rule(:other_file__name)` or `import "other.sprf" { :rule }`), NOT the default.

## 2. Type signatures

### 2.1 `LowerCtx` — `v4/src/compile/lower/ctx.rs:30`

```rust
pub struct LowerCtx {
    // ... existing fields ...
    pub sprf_dir: PathBuf,
    /// Identity of the .sprf source being lowered. Some(uri) in the LSP
    /// ingest path; None in CLI/test paths. When None, rule sink_tables
    /// fall back to the bare rule name (today's behavior). When Some,
    /// they prefix per §2.2.
    pub sprf_uri: Option<Arc<str>>,
}

impl LowerCtx {
    pub fn with_sprf_uri(mut self, uri: impl Into<Arc<str>>) -> Self;
    // body: self.sprf_uri = Some(uri.into()); self
}
```

### 2.2 Prefix derivation — new helper in `v4/src/compile/lower/ops.rs`

```rust
/// Derive the backing fact-table name for a rule whose user-facing
/// atom is `name`, given the source URI being lowered.
///
/// Scheme: `{stem}_{8-hex}__{name}` where
///   - stem = sanitized basename (alnum + underscore) of the URI's
///     final path segment, with the `.sprf` extension dropped. Empty
///     stem (e.g. "file:///") yields "anon".
///   - 8-hex = lowercase hex of the first 4 bytes of blake3(uri) so
///     two `infra.sprf` files in different directories don't collide.
///   - separator `__` so a stem ending in `_` is unambiguous.
///
/// Returns the bare `name` when `sprf_uri` is None (CLI/test).
fn rule_sink_table(sprf_uri: Option<&str>, name: &str) -> String;
// body:
//   match sprf_uri {
//     None => name.to_string(),
//     Some(uri) => {
//       let stem = sanitize_stem_from_uri(uri);    // "" => "anon"
//       let h = blake3::hash(uri.as_bytes());
//       let prefix = hex_first_n(h, 4);             // 8 hex chars
//       format!("{stem}_{prefix}__{name}")
//     }
//   }
```

### 2.3 `RuleDef::lower` rewrite — `v4/src/compile/lower/ops.rs:185-252`

```rust
fn lower_with_chain(
    &self,
    ctx: &LowerCtx,
    _flow: Option<Value>,
    args: &[Value],
    block: Option<Pipe<Cursor>>,
    _dsl: Option<&DslBody>,
    chain_pos: usize,
) -> Result<Pipe<Cursor>, LowerError> {
    let name = atom_arg(args, 0);
    let table = rule_sink_table(ctx.sprf_uri.as_deref(), &name);
    // sink position, no body: FactWrite into the file-scoped table.
    if chain_pos >= 1 && block.is_none() {
        return Ok(Pipe::new().step(Arc::new(FactWrite::new(ctx.store.clone(), table))));
    }
    // head-of-pipe: collect cols, declare(table, cols), optional Rule::new.
    let cols = collect_cols(&args[1..])?;
    if block.is_none() {
        ctx.store.declare(&table, &cols);                       // file-scoped declare
        return Ok(Pipe::new());
    }
    let body = block.unwrap();
    let rule = Rule::new(name.clone(), ctx.store.clone(), table, &cols, body);  // user name, table prefixed
    ctx.register_rule(rule);                                     // ctx.rules keyed by USER name
    Ok(Pipe::new())
}
```

`ctx.register_rule(rule)` continues to key the in-ctx lookup HashMap by the USER-FACING atom (`name`), NOT the prefixed table. Rule calls and `r?(name)` queries still resolve by atom; the prefix is an internal sink-table detail.

### 2.4 `RuleQueryDef` — `v4/src/compile/lower/ops.rs:1898+` (rule-query lowering)

Already routes through `LowerCtx.rules.get(name)` (per-file). The retrieved `Rule.sink_table` is already prefixed. No change. (Verify in step 3.3.)

### 2.5 `SqlBody` rewrite — `v4/src/cst/dsls/sql/mod.rs`

```rust
/// Rewrite bare table refs in a SQL DSL body to the file-scoped
/// physical table name. Bare refs are tokens that match a registered
/// rule name in `ctx.rules`; everything else (SQLite builtins, the
/// `input` keyword, the `with` CTE syntax, `${X}` holes) is left
/// untouched.
fn rewrite_table_refs_in_sql(
    body: &str,
    rule_name_to_table: &HashMap<&str, &str>,
) -> Cow<str>;
```

The map `rule_name_to_table` is built at lower time from `ctx.rules` snapshot. Implementation: tokenize the SQL body, replace identifiers that key into the map, leave everything else verbatim.

### 2.6 `ingest()` wiring — `v4/src/app.rs:847+`

```rust
let mut ctx = LowerCtx::new(self.facts.clone(), self.root.clone())
    .with_probe(probe_sink.clone() as Arc<dyn ProbeSink<Cursor>>)
    .with_sprf_store(self.sprf_store.clone())
    .with_sprf_uri(uri.clone());   // <- NEW
```

The CLI entry path (`sprefa-run`) does NOT call `with_sprf_uri`, so it stays on the unprefixed path.

## 3. Instance lifetimes

| value | lives in | reset when |
|---|---|---|
| `LowerCtx.sprf_uri` | per-`ingest()` LowerCtx | dropped after `lower_program` returns; never shared |
| `rule_sink_table(uri, name)` output | recomputed per call site | not cached; cost ~50ns (one blake3 of a short string) |
| `FactStore` table entry | one per (uri, rule-name) pair | drops with `SprfStore` (process lifetime) |
| `Rule.sink_table` Arc<str> | one per registered Rule | dropped with `LowerCtx.rules` at end of compile |
| `LowerCtx.rules` HashMap | per-file (today) | dropped at end of compile (today) |

## 4. Storage layout / reads / writes / uniqueness

### 4.1 `FactStore` schema map (in `SqliteFactStore.schemas`)
- key: `String` table name. With this plan: `{stem}_{8hex}__{rule_name}`.
- value: `Vec<String>` column names (declared at `Rule::new` time).
- uniqueness: PK = table name (post-prefix). Two files declare `:hits` → two distinct keys.

### 4.2 `LowerCtx.rules`
- key: `Arc<str>` — the USER-FACING rule atom (unchanged).
- value: `Rule` (with `sink_table` carrying the prefixed name internally).
- uniqueness: per-file; today's behavior preserved.

### 4.3 `MEMO_DEPS` table
- key: `(owner_op_id, in_key, source_id)`. Owner_op_id is the seam digest (`re_owner_hex`). Independent of rule names. No change.

## 5. Backward compatibility

| caller | behavior |
|---|---|
| `sprefa-run` CLI | `LowerCtx::new` → `sprf_uri = None` → bare table names → identical to today |
| `SprfState::run` (HTTP/RPC test path) | same as CLI unless caller threads URI |
| `SprfState::ingest` (LSP path) | URI threaded → file-scoped tables |
| Existing tests using `LowerCtx::new` directly | no URI → bare names → all tests pass unchanged |
| Existing `.sprf` examples / bench files | only the LSP daemon namespaces; CLI unchanged |

## 6. Test plan

### 6.1 Unit
- `rule_sink_table_bare_when_no_uri`: `rule_sink_table(None, "hits") == "hits"`
- `rule_sink_table_prefix_when_uri_set`: `rule_sink_table(Some("file:///x/y/infra.sprf"), "hits") == "infra_<8hex>__hits"`
- `rule_sink_table_collision_safe`: same stem in different dirs yields different 8hex chunks
- `rule_sink_table_sanitizes_stem`: `"hello-world.sprf"` → stem `hello_world`

### 6.2 Integration: pre-warm collision
- Tempdir with two `.sprf` files:
  - `a.sprf`: `rule(:hits, FS?, LO?) { ... }`
  - `b.sprf`: `rule(:hits, FS?, LO?, HI?) { ... }`
- Construct `SprfState`, call `lsp_pre_warm`. Assert no panic.
- Assert both rules' tables exist with distinct column sets.

### 6.3 Integration: same-file rule call still works
- `c.sprf`: `rule(:counter, N?) { str\`1\n2\n3\` > split(N?)\`\n\` }; counter?(N?) > lsp_warn(:x)\`${N}\``
- `lsp_open` + `get_diags`. Assert 3 diags (one per N).
- Same example via `lsp_pre_warm`. Assert same result.

### 6.4 Integration: cross-file isolation
- Workspace with `a.sprf` (`rule(:t, X?){ ... }`) and `b.sprf` (`rule(:t, X?, Y?){ ... }`).
- `b.sprf` references `t?(X?, Y?)` in its own body. Assert it resolves to `b.sprf`'s table, not `a.sprf`'s.
- Cross-file query (`b.sprf` references `t` from `a.sprf`) is NOT supported in this plan; document as a `LowerError::Unknown` if attempted.

### 6.5 Regression: full workspace test suite
- `cargo test --workspace --no-fail-fast` continues to pass at 537 (current count after MVP-6a).

## 7. Phasing

### Phase A — basic prefix (steps 2.1, 2.2, 2.3, 2.6)
- Adds `sprf_uri` field + the prefix function + the lower-site change.
- Tests 6.1, 6.2, 6.3 must pass.
- CLI + workspace tests must stay green.
- Ships the pre-warm crash fix as a side effect.

### Phase B — SQL bare-table-ref rewrite (step 2.5)
- Without this, SQL bodies that mention bare rule names (`SELECT * FROM hits`) silently break because the physical table is now prefixed.
- Test 6.4 covers it.
- Implementation: walk the existing SQL DSL lower path; before sending the SQL string to SQLite, substitute each identifier-tokens that matches a registered rule name with the prefixed name.

### Phase C — cross-file federation (DEFERRED)
- Out of this plan's scope.
- Would add: `extern_rule(:atom, "../other.sprf")` op OR a `:other_file__atom` atom-qualifier syntax.
- Without C, any cross-file query is a compile error (good — surfaces the dep explicitly).

## 8. Self-critique: edge cases

### 8.1 `fact(:tbl, ...)` op
`fact()` is a sibling of `rule()` declaring an externally-loadable table. The user-visible memory says it's deprecated (`memory/feedback_no_tag_fact_use_rule.md`). Skip namespacing for `fact()` in Phase A — it's slated for deletion. If a user has an outstanding `fact()` call, the bare name still works.

### 8.2 Same `.sprf` file ingested twice with different versions
LSP `did_change`: `ingest()` is called repeatedly for the same URI as the user edits. Each call re-runs `Rule::new` → re-runs `store.declare(prefixed_table, cols)`. The declare's idempotence-on-equal-cols path (`fact_store.rs:938-945`) handles this: same URI + same name + same cols = same prefix = no panic. If the user edits the rule's column list between ingests, the second declare DOES panic. That's a pre-existing bug in the FactStore — orthogonal to this plan but should be addressed (mark schema migration permissible, or rename the table when cols change). Adding a test for it but not fixing it in this PR.

### 8.3 Anonymous .sprf (no URI scheme/path)
`stem == ""` happens for URIs like `untitled:foo` from VS Code's in-memory buffers. Fallback to `"anon"`. Multiple anonymous buffers still collide on `anon_<same-hex>__name` because the URI is different per buffer; the hex disambiguates them.

### 8.4 Stem collisions across separate workspace roots opened in two LSP daemons
Not relevant — each daemon owns its own SprfState, no cross-process sharing.

### 8.5 The `9948a884` v3 approach used pure stem (no hash)
That's the duct-tape variant. Two `infra.sprf` in different dirs would still collide. The hash chunk makes it safe at marginal readability cost. The trade-off: error messages now show `infra_a1b2c3d4__rule` instead of `infra__rule`. Acceptable; the user-visible rule name in source is still just `:rule`.

### 8.6 Walker test fixtures and `assert!(table_name == ...)` style checks
A `grep` for `"::hits"` / `__rule_name` / hardcoded table-name assertions in the test suite is REQUIRED before landing Phase A. Any test that hardcoded a bare table name + ALSO threaded a URI will break. Plan: pre-flight grep, list affected tests, decide per-test whether to thread a URI in the test setup or assert on the rule's logical name instead.

## 9. Critical files

- `v4/src/compile/lower/ctx.rs` — Phase A: add `sprf_uri` field + builder.
- `v4/src/compile/lower/ops.rs` — Phase A: add `rule_sink_table`, rewire `RuleDef::lower` at `:185-252`. Verify `RuleQueryDef` at `:1898+` already resolves via `ctx.rules` (no change needed).
- `v4/src/rule.rs:52-70` — Phase A: no signature change. `Rule::new` already takes a separate `sink_table` arg.
- `v4/src/app.rs:847+` — Phase A: thread `uri` into LowerCtx via `with_sprf_uri`.
- `v4/src/cst/dsls/sql/mod.rs` — Phase B: SQL DSL rewriter for bare table refs.
- `v4/tests/` — Phase A: new tests at §6.1-6.3; sweep grep for hardcoded table-name assertions.
- `v4/plans/file-scoped-rule-tables.md` — this file.

## 10. Out-of-band

- Once this lands, the `bench/` skip in `lsp_pre_warm` becomes unnecessary. Remove it as a follow-up commit (or leave it as defense-in-depth, the user picks).
- A future plan should reconsider `FactStore.declare()` panicking on schema mismatch — at minimum, it should return a `Result` instead of `assert_eq!`. Out of scope here.
