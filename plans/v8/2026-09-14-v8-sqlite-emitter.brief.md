# Brief: sqlite emitter targeting sqlite_ivm

## 0. Contents
1. Job
2. Base and first action
3. Ownership
4. What exists, with lines
5. Design (decided)
6. Deliverables in order
7. Validation
8. Style laws
9. Reporting

## 1. Job
`dl8 emit sqlite <compile.json>` lowers every derived relation of a program to one `CREATE VIRTUAL TABLE <name> USING sqlite_ivm('<SELECT>')` over the row tables the store already creates (`#756` names them). Positive strata, including recursion, lower to SQL that `sqlite_ivm` 0.3.0 accepts; a stratum with negation or an aggregate lowers as a consumer stratum (NOT EXISTS, GROUP BY); anything outside the accepted grammar is a named diagnostic naming the rule. The receipt: for every fixture inside the boundary, the closure `dl8 eval` prints equals `SELECT * FROM <view>` after the seeds are inserted, and after one seed is deleted.

## 2. Base and first action
- Base sha: `BASE_SHA` (`origin/main`). Branch `feat/v8-sqlite-emitter-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only BASE_SHA`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `v8/`.

## 3. Ownership
You own: `v8/src/_5_reify/_7_sqlite.rs` (new) and one arm in `_4_emit.rs` (`Emitter::Sqlite`), `v8/src/bin/dl8.rs` (the `Emit` subcommand only), `v8/Cargo.toml` (dev-dependency on `rusqlite` with `load_extension` only), `v8/tests/_18_sqlite_emit.rs` (new), `v8/fixtures/sqlite_emit/**` (new).
Forbidden: `v8/src/_6_eval/**`, `v8/src/_9_runtime/**` (the reconciler lane owns it; read `_1_sqlite.rs` for table names, change nothing), `sqlite_ivm/**` (report defects by throw site), `v8/oracle/**`, `v7/`, `v6/`. Never spawn subagents. Never `--no-verify`.

## 4. What exists
| thing | where |
|---|---|
| v7 sqlite emitter: layout rows, boundary list (positive rules, projection, equijoins, constant filters, nonrecursive union), artifact shape | `v7/src/3_emit/1c_sqlite_query_emitter.pl:1-120` |
| its tests: deterministic keyed inner joins, native ivm lifecycle, integer comparisons to SQL, rejected shapes | `v7/test/14_sqlite_query_emitter.test.pl:17,38,118,135,203` |
| v8 emitter seam: `compiler_view`, `emit_compiled`, `Emitter`, `Emitted` | `v8/src/_5_reify/_4_emit.rs:21-70`, `_0_api.rs` |
| logical program rows (rules as rows) | `v8/src/_5_reify/_1_rows.rs` |
| store DDL: `sym`, `term`, `term_arg`, `relation` dictionary, one row table per relation named by declared name, columns `a0..a{n-1}` INTEGER (term ids) | `v8/src/_9_runtime/_1_sqlite.rs:430-470`, `:53`, `:94-113` |
| `sqlite_ivm` 0.3.0 accepted grammar, recursion row | `sqlite_ivm/README.md:38-52`, `:70-90` |
| `sqlite_ivm` rejects: outer join in a recursive step, UNION ALL recursion, aggregate or NOT EXISTS over own relation in the step, ORDER BY or LIMIT in the CTE | `sqlite_ivm/README.md:75-85`, `sqlite_ivm/src/0b_relational.rs` `fn recursive` |
| loading the extension in rusqlite | `sqlite_ivm/README.md:12-16` (`.load`, `PRAGMA recursive_triggers=ON; PRAGMA trusted_schema=ON`), `sqlite_ivm/tests/4_features.rs:20-30` |
| `no coercions` decision | `CLAUDE.md` User decisions; `lower.pl:2319` |
| kernel comparisons `int_lt` etc. and `term_lt` | `v8/src/_3_check/_5_kernel.rs` `KERNEL_RELATIONS` |

## 5. Design (decided)
```rust
// v8/src/_5_reify/_7_sqlite.rs
pub struct SqliteArtifact { pub views: Vec<SqliteView>, pub diagnostics: Vec<Diagnostic> }
pub struct SqliteView { pub relation: String, pub ddl: String /* CREATE VIRTUAL TABLE ... */, pub stratum: usize }
/// Lower a compiled unit: one view per derived relation, in stratum order, so a later
/// view's SELECT may name an earlier view.
pub fn emit_sqlite(u: &mut Universe, unit: &Compiled, names: &HashMap<String, TermId>) -> Result<SqliteArtifact, Stop>;
```
- Term columns are INTEGER term ids (the store's shape). A comparison goal `(int_lt ?A ?B)` lowers to a join against the `term` table for both sides with `kind = KIND_INT` and `ival < ival`; `term_lt` lowers to the standard-order tuple compare over `(kind, ival, rval, sym)` as the store encodes it, stated in a comment as the one constraint the SQL cannot show.
- Positive non-recursive rule: `SELECT DISTINCT <head cols> FROM <body tables> WHERE <equijoins> AND <constants>`; several rules for one head: `UNION`.
- Positive recursive SCC: one `WITH RECURSIVE <name>(cols) AS (anchor rules UNION step rules) SELECT * FROM <name>`; mutual recursion within an SCC is spelled as ONE CTE with a discriminator column as `sqlite_ivm/README.md:80-83` requires; the discriminator is a literal integer per member relation and the per-member view is `SELECT cols FROM <scc> WHERE member = k`.
- Negative goal: `NOT EXISTS (SELECT 1 FROM <t> WHERE ...)`, only against an earlier stratum (the checker's strata guarantee it).
- Aggregate head (`count`, `sum`, `min`, `max`, and `fold` whose step is `int_add` with the `linear` fact true): `GROUP BY` plain positions; any other `fold` is `emit_sqlite_unsupported(fold, <step>)`.
- `effect` rows, `intern`, `cons`, `nil`, `edge_snapshot`, `intern_snapshot` goals: `emit_sqlite_unsupported(<kernel>)`, one diagnostic per rule, the rest of the program still emits.
- No new tables, no DDL beyond the virtual tables. Names are quoted identifiers built from the declared relation name.

## 6. Deliverables in order
1. `emit_sqlite` with positive non-recursive rules and union; `dl8 emit sqlite` prints the DDL list as JSON.
2. Recursion through `WITH RECURSIVE`, SCC grouping, discriminator for mutual recursion.
3. Negation and aggregates as consumer strata.
4. Test `_18_sqlite_emit.rs`: for each fixture in `v8/fixtures/{literals,aggregates,term_lt,fold,sqlite_emit}` whose emit has zero diagnostics: open an in-memory rusqlite db, load the `sqlite_ivm` extension built from `../sqlite_ivm` (`cargo build --release --features extension`, path via `SQLITE_IVM_LIB` or the default target dir), create the store schema by running `dl8 eval --db` once, run the DDL, then assert `SELECT * FROM <view>` equals the eval closure row set for every derived relation; then delete one seed row through SQL and assert equality with a fresh `dl8 eval` on the reduced seed set. Fixtures outside the boundary assert their exact diagnostic list.
5. A receipt table in the PR: fixture, derived relations, recursive yes/no, equality after insert, equality after delete.

## 7. Validation
```bash
cd v8 && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings && grep -rn "eprintln!" src/ | wc -l
git diff --stat origin/main -- v8/oracle | tail -1     # empty
```
Batteries in the background, per-case cap 10 s.

## 8. Style laws
Comment budget. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support. No em dashes. Interfaces carry `I`. `.claude/skills/sql-relational-design` and `.claude/skills/sqlite-costs` are mandatory reads before any DDL. Language words: rxjs, prolog, SQL only.

## 9. Reporting
PR title `feat(v8): sqlite emitter over sqlite_ivm`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "sqlite emitter: PR #<n>, fixtures equal <k>/<n> insert, <k>/<n> delete, unsupported <list>, cargo test <pass>/<total>, clippy 0"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
