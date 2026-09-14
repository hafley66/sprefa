# Brief: sqlite_ivm arbitrary positive recursive strata

## 0. Contents
1. Job in one sentence
2. Base and first action
3. Ownership
4. What exists, with lines
5. Target contract
6. Design constraints (decided, do not re-litigate)
7. Deliverables in order
8. Validation commands
9. Style laws
10. Reporting

## 1. Job
Widen `sqlite_ivm` recursion from one unary reachability shape to any positive recursive `WITH RECURSIVE` program: n-ary recursive CTEs, multiple recursive CTEs in one WITH (mutual recursion), several recursive references per step, WHERE and DISTINCT and multiple joins in the step, and non-recursive consumers (aggregate, EXISTS, antijoin) stratified after the recursion. Negation and aggregation INSIDE a recursive step stay rejected with a named error.

## 2. Base and first action
- Base sha: `f6aec8c3e5c89628031032465ca5133499b74f3b` (`origin/main`).
- Branch `feat/sqlite-ivm-recursion-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only f6aec8c3e5c89628031032465ca5133499b74f3b`. Failure or missing tree = stop and report via `boop beep`.
- Every commit and PR ends with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## 3. Ownership
You own `sqlite_ivm/**` only. Forbidden: everything under `v6/`, `v7/`, `v8/`, `src/`, `plans/`, `chat_log/`, `sqlite_ivm/bench/44_pg_ivm_1_15_expected.json`, `sqlite_ivm/bench/44a_pglite_1_13_expected.json` (frozen external oracles). Never spawn subagents. Never `--no-verify`.

## 4. What exists
| thing | where |
|---|---|
| plan kinds | `sqlite_ivm/src/0b_relational.rs:27-49` `enum Kind { Input, Map, Join, Set, Group, Reach }` |
| SQL acceptance for recursion | `sqlite_ivm/src/0b_relational.rs:1409-1470` `fn recursive`: one column, one UNION distinct, one inner join, one recursive reference, no WHERE, no ORDER/LIMIT |
| reach maintenance | `sqlite_ivm/src/1a_relational.rs:624-700` `fn reach`: three tables `roots/edges/reached` (`table(name,id,0..2)`), deletion = collect affected closure, retract, re-seed from rooted set, re-expand; insertion = expand from start |
| reach indexes | `sqlite_ivm/src/1a_relational.rs:215-220` |
| dispatch | `sqlite_ivm/src/1a_relational.rs:621` `Kind::Reach => self.reach(...)` |
| existing recursion test | `sqlite_ivm/tests/4_features.rs:169` `recursive_delete_preserves_alternate_null_support_and_collated_roots` |
| DD oracle circuits already written for the target shapes | `sqlite_ivm/bench/49_dd_contracts.rs:173-260`: `binary_transitive_closure`, `mutual_even_odd_recursion`, `recursive_min_distance`, `nested_fixed_points`, `stratified_antijoin_after_recursion` |
| feature acceptance harness | `sqlite_ivm/scripts/12_features.sh`, fixtures `sqlite_ivm/tests/fixtures/1_features.json`, report `bench/46_feature_acceptance.md` |
| README contract table | `sqlite_ivm/README.md:38-52`, row `Recursion` |

## 5. Target contract
Every row below gets a fixture case in `tests/fixtures/1_features.json` with 174 source states like its neighbours, a native test in `tests/4_features.rs`, and a DD comparison. Rows 1 to 5 reuse the circuits already in `bench/49_dd_contracts.rs`.

| # | SQL shape | today | after |
|---|---|---|---|
| 1 | binary closure `path(a,b)`: anchor edge UNION `SELECT p.a, e.b FROM path p JOIN edge e ON p.b=e.a` | rejected `:1425` | maintained |
| 2 | mutual recursion: two recursive CTEs `even(n)`, `odd(n)` each referencing the other | rejected `:1467` | maintained |
| 3 | step with WHERE filter and two joins (`edge` twice, or `edge JOIN allowed`) | rejected `:1437` `:1454` | maintained |
| 4 | recursive min distance: recursion emitting `(node, dist)` then `GROUP BY node` MIN outside the recursive CTE | rejected `:1425` | maintained (recursion is bag/set; aggregate is a stratum after) |
| 5 | antijoin after recursion: `reach ... WHERE NOT EXISTS (blocked)` as consumer | consumer rejected because recursion rejected | maintained |
| 6 | nested fixed points (`bench/49:221`) | rejected | maintained OR rejected with `nested recursion unsupported`; measure and pick, write the reason in README |
| 7 | aggregate or NOT EXISTS referencing the recursive CTE INSIDE its own step | rejected | rejected with error text `recursive step may not aggregate or negate its own relation` |
| 8 | UNION ALL recursive (bag recursion) | rejected | rejected with `recursive UNION ALL unsupported` unless termination is provable; do not build |

## 6. Design constraints, decided
- One `Kind::Fixpoint { members: Vec<...> }` (or equivalent) replaces `Kind::Reach` as the general node; `Reach` may survive only as a fast path selected when the shape matches the current one-column contract, and then the general path must produce byte-identical results on the existing test at `tests/4_features.rs:169`.
- Maintenance algorithm is the user's call among these two only; pick, measure both on rows 1 and 2 with the 174-state fixtures, write the numbers in the PR:
  - **DRed** (delete and rederive): retract the over-approximated affected set, re-seed from rows still derivable, re-expand. This is what `fn reach` does today at `1a_relational.rs:672-696`; generalise it to n-ary heads and multiple members.
  - **Support counting** with semi-naive re-derivation per member table (a `__copies`-style count column, as `Set` nodes already do at `1a_relational.rs:600`).
- Semi-naive delta evaluation inside the fixpoint: each member keeps `old/delta/all` tables; a round joins delta against all, never all against all. A COUNT test asserts the number of `change()` calls on a single-edge insert into a 1000-node chain is linear in the new closure rows, never quadratic (statement counts, per `CLAUDE.md` "formerly-quadratic paths get COUNT tests").
- Surrogate keys, ints only in keys, no composite TEXT PRIMARY KEY in any DDL you emit (`.claude/skills/sql-relational-design`). Read `.claude/skills/sqlite-costs` before any new index.
- Stratification: a recursive CTE group is one stratum. Consumers with aggregate, DISTINCT, EXISTS, NOT EXISTS over it are later strata and use the existing `Group/Set/Join` maintenance unchanged. Negation or aggregation over a member inside the same group is a parse-time error (row 7).
- Infra is bought: the SQL parser stays `sqlite3-parser =0.17.0`; no hand parser. Any new dependency needs a written candidate list in the PR.
- Transactions: rollback and savepoints must still cover the new member tables. Extend `tests/5_transactions.rs` with one recursive case.
- Nothing seizes the machine. Fixtures run under `timeout 10` per case.

## 7. Deliverables in order
1. `plan.md` in `sqlite_ivm/` (temporary, deleted before PR): the type signatures first, pseudo-code bodies, instance lifetimes of the member tables, storage layout, then read/write sequence per insert and per delete. Post the signatures via `boop beep` before writing code.
2. Parser: `fn recursive` accepts the WITH RECURSIVE forms in section 5 rows 1 to 6 and rejects 7 and 8 with the named errors. Tests in `tests/0_query.rs`.
3. Maintenance: general fixpoint node. Tests in `tests/4_features.rs`, one per row, each asserting `SELECT * FROM vtab` equals the plain query on every source state, exactly as `tests/4_features.rs:178` does.
4. DD comparison: wire rows 1 to 5 into the `12_features.sh` flow so `bench/46_feature_acceptance.md` gains those rows with exact-match counts.
5. COUNT test for the semi-naive law (section 6).
6. README `Recursion` row rewritten to the new contract; the sentence at `README.md:75` (`arbitrary recursive programs ... unsupported`) updated to name only what still is.
7. Version bump `0.2.0` to `0.3.0` in `sqlite_ivm/Cargo.toml`, one CHANGELOG line if a CHANGELOG exists there, none created otherwise.

## 8. Validation
```bash
cd sqlite_ivm
cargo test --locked                                   # scripts/9_verify.sh, whole gate
cargo test --locked --features bench --test 4_features --test 5_transactions   # scripts/14_native_values.sh
bash scripts/12_features.sh                           # DD and pg comparison, regenerates bench/46
cargo clippy --locked --all-targets -- -D warnings
grep -rn "eprintln!" src/ | wc -l                     # must print 0
```
Each command runs in the background with a per-case 10 s cap. Anything over 10 s is a defect to report, never a budget.

## 9. Style laws
- Comments state only constraints the code cannot show. No dates, no arc references, no change-log prose.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal (say "not built" or "rejected").
- No em dashes. No `honest`, `distill`, `ground` as a verb.
- dl and SQL variable names descriptive, never single-letter, in tests and docs.
- N+1: never a per-row write inside a loop when a set insert works; the fixpoint round writes its delta as one statement per member.

## 10. Reporting
When done: PR against `origin/main` titled `feat(sqlite_ivm): arbitrary positive recursive strata`, body with the section 5 table filled with measured results, the DRed vs support-count numbers, and the COUNT test output. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "ivm recursion: PR #<n>, cargo test <pass>/<total>, clippy 0, rows 1-6 <maintained|rejected>"
```
Blocked, or brief wrong: same command, one line, and stop. Never revive yourself; one lane, one task.
