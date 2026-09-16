# Brief: sqlite_ivm comma joins take their equality keys from WHERE

## 0. Contents
1. Job
2. Base and first action
3. Ownership
4. Defect, with receipts
5. Design (decided)
6. Deliverables in order
7. Validation
8. Style laws
9. Reporting

## 1. Job
A comma join (`FROM a, b, c WHERE b.k = a.k AND c.k = a.k`) is planned as a cross product with the WHERE applied afterwards. Three tables of 2 x 1431 x 2064 rows persist a 5.9M-row arrangement and the `CREATE VIRTUAL TABLE` runs past 10 s. The same query as `JOIN ... ON` finishes in 0.03 s. Make comma joins (and `JOIN` with no constraint) pull their equality keys from the WHERE conjuncts, one step at a time, so both spellings plan identically. Nothing else changes.

## 2. Base and first action
- Base sha: `baf09ef954021d328667902fda1573594e7bd8f7` (`origin/main`). Branch `fix/sqlite-ivm-comma-join-keys`.
- FIRST command: `git merge --ff-only baf09ef954021d328667902fda1573594e7bd8f7`. Failure or missing tree = stop and report via `boop beep`.
- Every commit ends with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `sqlite_ivm/`.

## 3. Ownership
You own `sqlite_ivm/src/**`, `sqlite_ivm/tests/**`, `sqlite_ivm/README.md`. Forbidden: `v8/**`, `v7/**`, `v6/**`, `plans/**`, `chat_log/**`, `sqlite_ivm/bench/44_pg_ivm_1_15_expected.json`, `sqlite_ivm/bench/44a_pglite_1_13_expected.json`. Never spawn subagents. Never `--no-verify`. Never touch another worktree.

## 4. Defect, with receipts
| thing | where |
|---|---|
| comma and bare JOIN fall to the USING/NATURAL arm; no USING, no NATURAL, so `l`, `r` stay empty and the node is `Kind::Join { left: [], right: [], predicate: None }`, a cross product | `sqlite_ivm/src/0b_relational.rs:789-870`, the `_ => vec![]` arm at `:825` |
| `JOIN ... ON` path extracts key pairs from the ON expression | `0b_relational.rs:754-788` `fn joined`, using `index_pairs` `:471` and `pairs` `:442` |
| WHERE is applied after the whole FROM is planned | `0b_relational.rs:1015` and `:1596` |
| the parser marks comma joins | `0b_relational.rs:1541` `JoinOperator::Comma \| JoinOperator::TypedJoin(None)` |
| join maintenance keys per side | `sqlite_ivm/src/1a_relational.rs:511-560` |

Measured on a `dl8 eval --db` store (tables `p.Input_a1` 2 rows, `p.term` 1431, `p.term_arg` 2064, `p.sym` 89), extension built from this tree, 10 s cap:

| shape | result |
|---|---|
| `FROM "p.Input_a1" t0, "p.term" t1 WHERE t1.id = t0.c0_term` | ok 0.02 s |
| `FROM t0 JOIN t1 ON t1.id = t0.c0_term JOIN "p.term_arg" t2 ON t2.term = t0.c0_term` | ok 0.03 s, 2 rows |
| `FROM t0, t1, t2 WHERE t1.id = t0.c0_term AND t2.term = t0.c0_term` | over 10 s, any table order |
| `FROM t0, t1, t2 WHERE t1.id = t0.c0_term AND t2.term = t1.id` | over 10 s |
| `FROM t1 JOIN t2 ON 1 JOIN t0 ON t1.id = t0.c0_term AND t2.term = t0.c0_term` | over 10 s (expected: the first join is a declared cross product) |

## 5. Design (decided)
```rust
// 0b_relational.rs, inside fn from (or a helper it calls)
/// For a comma or unconstrained join step, the WHERE conjuncts that are an equality between
/// one column of the already-joined left fields and one column of the incoming right table.
/// Those conjuncts become the step's ON; the remaining conjuncts stay in WHERE.
fn where_keys_for_step(where_clause: Option<&Expr>, left_fields: &[Field], right_fields: &[Field]) -> (Vec<Expr> /* taken */, Vec<Expr> /* remaining */);
```
- `from` receives the WHERE (thread it from the two callers at `:1006` and `:1525`); after each comma step it takes the conjuncts `where_keys_for_step` selects, builds one AND expression, and calls the existing `joined(id, right, &on, "inner")`. The remaining conjuncts are what the later WHERE filter sees.
- A conjunct is taken only when it is `col = col`, both sides resolve, one side in the left scope and the other in the right table (either order). Anything else (constants, non-equality, three-way expressions, OR) stays in WHERE.
- Order of tables stays as written. No join reordering in this arc.
- Outer joins with no constraint keep today's behaviour.
- Semantics are unchanged by construction: `a, b WHERE p` and `a JOIN b ON p` are the same relation in SQLite; the fixture tests assert equality against the plain query as `tests/4_features.rs:178` does.

## 6. Deliverables in order
1. `where_keys_for_step` and the `from` change.
2. Test in `tests/0_query.rs`: the three-table comma shape from section 4 binds to `Kind::Join` nodes with non-empty `left`/`right` on both steps (plan inspection, the way `tests/0_query.rs:253` inspects binding).
3. Test in `tests/4_features.rs`: a three-table comma join with a star predicate (two tables keyed off the first) over sources of at least 1000 rows each; `SELECT * FROM vtab` equals the plain query on insert, update and delete; whole test under 2 s.
4. COUNT test (the repo law for formerly-quadratic paths): after `CREATE VIRTUAL TABLE` on the fixture from 3, the join arrangement tables (`__ivm_objects` names them) hold at most `rows(left) + rows(right)` entries per step, never the product. Assert the exact counts.
5. README: the Joins row at `README.md:40-41` says comma joins and `JOIN` without ON take equality keys from WHERE. One sentence.

## 7. Validation
```bash
cd sqlite_ivm
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
grep -rn "eprintln!" src/ | wc -l     # 0
cargo build --release --features extension
```
Every command runs in the background with a 10 s per-test cap. A test over 10 s is a defect to report with its name, never a budget to wait out.

## 8. Style laws
Comment budget: comments state only constraints the code cannot show. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth, refusal, support (say refCount). No em dashes. Follow the file's existing style. Language words: SQL, prolog, rxjs only.

## 9. Reporting
Push, then `gh pr create --base main` with title `fix(sqlite_ivm): comma joins take equality keys from WHERE` and the section 4 table plus your measured numbers in the body. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "ivm comma joins: PR #<n>, cargo test <pass>/<total>, clippy 0, star join 3 tables <ms>, arrangement rows <n>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
