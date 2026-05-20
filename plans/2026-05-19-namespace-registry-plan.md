# Namespace + duplicate-registry plan — 2026-05-19

Targets items 1, 17 from `plans/2026-05-19-v4-worst-audit.md`. Plus three adjacent symmetry breaks: parallel `Op` trait, two recursion detectors, three SQL tokenizers, idempotent registry.

## 0. Shapes

```rust
// (A) reserved-namespace gate on term names
pub struct TermName(Arc<str>);
pub enum TermNameError { Reserved(Arc<str>), Empty, BadChar(char) }
impl TermName {
    pub fn user(s: impl Into<Arc<str>>) -> Result<Self, TermNameError>;
    pub(crate) fn reserved(s: &'static str) -> Self;
    pub fn as_arc(&self) -> &Arc<str>;
    pub fn as_str(&self) -> &str;
}

// (B) registry duplicate detection
pub struct AlreadyRegistered { pub name: &'static str }
impl Registry {
    pub fn register(&mut self, def: Arc<dyn OperatorDef>) -> Result<(), AlreadyRegistered>;
    pub fn register_or_panic(&mut self, def: Arc<dyn OperatorDef>);
}

// (C) one Op surface; Liftable on OperatorDef
pub trait OperatorDef: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn cursor_binds(&self) -> &'static [&'static str] { &[] }
    fn classify(&self, call: &OpCall) -> Liftable { Liftable::Opaque }
    // ...
}

// (D) one recursion oracle
pub struct StratifyResult {
    pub strata: Vec<Stratum>,
    pub recursive_rules: BTreeSet<Arc<str>>,
}

// (E) one SQL lexer
pub mod cst::dsls::sql::lexer {
    pub struct Token { pub range: Range<usize>, pub kind: TokenKind }
    pub enum TokenKind { Keyword(SqlKw), Ident, Number, String, Punct(u8), HostHole, Comment, Whitespace }
    pub fn scan(body: &str) -> Vec<Token>;
    pub fn from_clause_idents(body: &str) -> Vec<(Range<usize>, &str)>;
}
```

Storage: `TermName` one `Arc<str>` clone; reserved variants point into `&'static str`. Registry stays `HashMap<&'static str, Arc<dyn OperatorDef>>`; only `register` return type changes. `StratifyResult` is a struct return. SQL lexer is one function reused across three call sites.

## 1. Reserved-term audit

Three known holes plus a likely fourth:

- `v4/src/mounted_query.rs:20` — `SUPPORT_CURSOR_ID = "__support_cursor_id"`.
- `v3/.../v2/effect_dispatch.rs:70` — `MUTATION_KEY_COL = ":mutation_key"`.
- `v4/src/chan.rs:28` — `mark_key = format!(":nextq:{}", chan)`.
- Likely: `v4/src/sql.rs` cursor column colon-prefix sigils.

Grep patterns:

```
rg -n 'set(_arc)?\(\s*"[A-Za-z_:][^"]*"' v4/src v3/crates/effect_runtime/src
rg -n 'set(_arc)?\(\s*&format!' v4/src v3/crates/effect_runtime/src
rg -n 'CursorTerm::new\(' v4/src v3/crates/effect_runtime/src
rg -n 'Term::new\(' v4/src v3/crates/effect_runtime/src
rg -n '"__[a-z_]+"' v4/src v3/crates/effect_runtime/src
rg -n '":[a-z_]+"' v4/src v3/crates/effect_runtime/src
rg -n 'declare\(\s*"[a-z_]+"\s*,\s*&\[' v3/crates/effect_runtime/src
```

Audit complete when:
1. Every hit is either user-text in a fixture or a `TermName::reserved(...)`.
2. `cargo check -p sprefa` rejects `cursor.set(&str, ...)` not wrapped in `TermName`.
3. CI test `reserved_term_collision_smoke` writes each reserved name from user syntax (`@bind <name>`) and asserts `term/reserved` diag.

## 2. Encoding decision

**Option A — encode.** `\x00`-prefix reserved names in storage. Collision-impossible. Cost: every external decoder sees a control byte.

**Option B — validate.** `TermName::user` rejects names starting with `_`, `:`, or `__`. Reserved minted only via `TermName::reserved`.

```
rg -n 'set(_arc)?\(\s*"_[a-zA-Z]' v4/tests v4/examples v4/src
```

**Pick B.** The conventions are already informal sigils; codifying costs less. Reserved set:

- Prefix `__`
- Prefix `:`
- Exact `&`, `&.value`
- `:nextq:.*` family

## 3. Op-vs-OperatorDef collision

Current:
- `OperatorDef` (real surface) at `v4/src/compile/lower/op_def.rs:168`.
- `Op { fn classify }` (test-only) at `v4/src/compile/lower/liftable.rs:128`.
- Implementations on `FsOp/AstOp/ReOp` at `ops.rs:1593+`.
- Fuser at `fuser.rs:147` dispatches by name string.

Collapse:
1. Add default `classify(&self, _: &OpCall) -> Liftable { Liftable::Opaque }` on `OperatorDef`.
2. Move `fuser::classify_op` (`fuser.rs:147-189`) body into per-`*Def::classify` impls.
3. Fuser becomes: `let def = reg.get(&op.name)?; let lift = def.classify(op);`. No name table.
4. Delete `liftable::Op` trait + `FsOp/AstOp/ReOp` wrappers. Update tests to construct `*Def`s.

Cross-branch:
- **callable-value**: land 4-5 before that branch. `apply` calls `registry.lower`; one trait surface. `apply` consumes `def.classify` for fusion-through-apply.
- **cons-calling-unification**: independent. Doesn't touch trait surface.
- **type-ir**: widens `TypeLattice` toward `Value`. Step 6's `cursor_bind_lattice` lift onto OperatorDef must accept widened type.

## 4. Killing `binding_graph::op_cursor_binds`

The blocker — "binding_graph runs before registry knows all ops" — is not real. `analyze_program(program, reg)` at `binding_graph.rs:43` takes `&Registry`. `default_registry()` at `compile/lower/mod.rs:67-87` registers everything synchronously.

```rust
// binding_graph.rs:909
let cursor_binds: Vec<Arc<str>> = reg.get(&op.name)
    .map(|d| d.cursor_binds().iter().map(|s| Arc::<str>::from(*s)).collect())
    .unwrap_or_default();
```

`cursor_bind_lattice` at `binding_graph.rs:1304` — lift onto `OperatorDef::cursor_bind_lattice(&self, capture: &str) -> TypeLattice` with default `TypeLattice::String`. Override on `FsDef`, `AstDef`.

Delete `binding_graph::op_cursor_binds` and `cursor_bind_lattice`. Compile error if re-added.

Side discovery: `AstDef::cursor_binds()` (`ops.rs:856`) returns `["LO", "HI"]` but `binding_graph::op_cursor_binds("ast")` returned `["FILE", "LO", "HI"]`. After collapse, `cursor_binds()` must include `FILE`. Verify by reading `AstNmComponent::render` — if it stamps `FILE`, declaration was wrong; if not, binding_graph was wrong. `cargo test -p sprefa binding_graph` picks the winner.

## 5. One recursion oracle

Current:
- `stratify.rs:199` Tarjan SCC.
- `fuser.rs:693` `joined_tables.iter().any(|t| *t == self_facts.as_str())` (self-edge only).

Fuser is strict subset of stratify.

1. `stratify::stratify` returns `StratifyResult { strata, recursive_rules }`.
2. `recursive_rules` = (a) every rule in SCC ≥ 2, (b) every rule with self-edge.
3. App.rs:1771 stashes the result on `App`.
4. Fuser at `:688-693` consumes `recursive_rules.contains(rule_name)`. `joined_tables` still computed for SQL emission.

Sequencing: move stratify to run before fuser (was inside `if !rec.is_empty()` branch). Extra Tarjan over full graph at compile time; negligible.

## 6. One SQL tokenizer

Three scanners:
- `v4/src/sql.rs:800` `referenced_fact_tables` + `:825` `sql_tokens`.
- `v4/src/app.rs:1076` `source_tables_from_fused_sql` + `:1130` `lower_keyword_at`.
- `v4/src/cst/dsls/sql/mod.rs:410` `scan_sql` (most general).

Promote `scan_sql` to canonical:

1. Pull `scan_sql` out of `cst/dsls/sql/mod.rs` into `cst/dsls/sql/lexer.rs`. Add `TokenKind::Keyword(SqlKw)` const set: `WITH`, `RECURSIVE`, `AS`, `FROM`, `JOIN`, `SELECT`, `WHERE`, `(`, `)`.
2. Add helpers:
   - `from_clause_idents(body, exclude) -> Vec<&str>` — track CTE bindings introduced by `WITH name AS (...)`, skip in result.
   - `referenced_fact_tables_from_tokens(tokens) -> BTreeSet<String>`.
3. Port `sql.rs:800` and `app.rs:1076` to call helpers. Delete `sql_tokens`, `referenced_fact_tables`, `source_tables_from_fused_sql`, `lower_keyword_at`.
4. `cst/dsls/sql_where::scan_predicate` keeps its own (no FROM/JOIN); audit at port time.

CTE tests in `v4/tests/sql_lexer_target.rs`:

```rust
// returns { "users" } not { "active", "users" }
"WITH active AS (SELECT * FROM users) SELECT * FROM active"

// returns { "users" } not { "u", "users" }
"SELECT * FROM users AS u"
```

## 7. Registry duplicate detection

```rust
pub fn register(&mut self, def: Arc<dyn OperatorDef>) -> Result<(), AlreadyRegistered> {
    let name = def.name();
    if self.map.contains_key(name) { return Err(AlreadyRegistered { name }); }
    self.map.insert(name, def);
    Ok(())
}
```

`default_registry()` panics on dup (`register_or_panic`). User plugins (future) get `Result` and emit `lower/duplicate-op` diag.

## 8. Order of operations

```
step 1  TermName + reserved set                                  ~5 files
step 2  audit greps; magic literals → TermName::reserved         ~12 files
step 3  cursor.set signature: &TermName not &str                 ~30 files
step 4  Registry::register → Result, register_or_panic           ~3 files
step 5  OperatorDef::classify + per-Def overrides; delete Op     ~6 files
step 6  OperatorDef::cursor_binds correctness (FILE on Ast)      ~2 files
step 7  StratifyResult; fuser uses recursive_rules               ~3 files
step 8  cst::dsls::sql::lexer canonical                          ~4 files
step 9  CI gates                                                  ~2 files
```

Step 1 unlocks 2. Step 4 unlocks 5. Step 5 unlocks 6. Step 7 independent. Step 8 independent.

Cross-branch:
- callable-value: 4-5 first.
- cons-calling-unification: independent.
- type-ir: step 6 must accept post-widening type; if type-ir lands first, `Value`; else `TypeLattice` with TODO.

Step 3 is largest mechanical edit. Test-only: `TermName::user_unchecked(&'static str)` `#[cfg(test)]`.

## 9. CI gates

Tests:

1. `v4/tests/reserved_term_smoke.rs` — `@bind <reserved>` → `term/reserved`; `TermName::user(<reserved>).is_err()`.
2. `v4/tests/registry_dup_smoke.rs` — two `OperatorDef`s same name → `Err(AlreadyRegistered)`.
3. `v4/tests/op_classify_parity.rs` — every default_registry name has classify returning `Some` or explicit `Opaque`.

`xtask check-magic-names` greps:

```
rg -n '\.set(_arc)?\(\s*"[_:][a-zA-Z_:]+"' v4/src v3/crates/effect_runtime/src | grep -v 'TermName::'
rg -n 'op_cursor_binds\|cursor_bind_lattice' v4/src
rg -n 'fn .*_tokens|fn scan_sql\|fn lower_keyword_at' v4/src v3/crates/effect_runtime/src
```

In `justfile` under `just check-symmetry`.

## 10. Estimated impact

- Steps 1-3 (reserved names): ~5 new lines in term.rs; ~12 definition-site edits; ~30 call-site signature updates. Mechanical.
- Step 4: 1 sig change, ~20 call-site `?` insertions.
- Step 5: 6 trait methods, 1 trait deletion, 3 wrapper deletions. Fuser loses ~50 lines.
- Step 6: 1 field on AstDef, 2 deletions in binding_graph, ~3 line patch.
- Step 7: stratify return change, 1 fuser swap, ~10 line app.rs patch.
- Step 8: new `lexer.rs` ~200 lines, 2 port sites, ~3 deletions, ~80 lines tests.
- Step 9: 3 new test files ~150 lines, 1 justfile recipe.

Total: ~80 files touched. Net deletion (collapse removes more than adds).

## 11. Conflicts / unknowns

- `Term` type alias at `v4/src/lib.rs:295` aliases `CursorTerm`. Separate `Term` in `v4/src/term.rs` (runtime Component). Two types share a name. Put `TermName` in `term.rs` (or new `term_name.rs`); pick one.
- `effect_dispatch.rs:70` is v3 crate. `TermName` lives in v4. Options: (a) duplicate constant in v3 + audit v3 store sites; (b) move `TermName` upstream into `effect_runtime`. **Pick (b).** Runtime owns FactStore column-name surface.
- `binding_graph::cursor_bind_lattice` returns `TypeLattice`. Widens to `Value` if type-ir lands first.
- Step 7 reordering assumes fuser does not produce additional rule deps. Verify by reading `compile/fuser.rs` end-to-end first; if it does, order becomes fuser → stratify → re-fuse-recursive (what app.rs:1771 implicitly does today).

## 12. Out of scope

- Cursor codec FOCAL re-injection (item 4) — separate.
- `Any`-downcast effect_runtime panic surface (item 6) — separate.
- Generation RMW races (item 12) — separate.
- `lsp_types` two-version glue — separate.
