# v8 lane: SQLite row store for dl8 (implementation of the store plan)

## TOC
1. Goal
2. Base and first action
3. Read first
4. Files you own, files you never touch
5. Design (decided; the plan plus Chris's five decisions)
6. Steps with receipts
7. Validation commands
8. Style laws, comment budget
9. Finish protocol

## 1. Goal
`dl8 eval --db <file> <program.json>` persists the closure into one SQLite file and, on a second run against the same file, loads the arena and rows back, treats every loaded row as old (semi-naive frontier), and derives only what new seeds add. Two real-binary tests. Every existing test (24 suites green on `origin/main`, byte-identical oracle fixtures) stays green.

## 2. Base and first action
- Base sha `cf6326e741499c0a7204f1ec65272af75c408cda` (`origin/main`).
- Branch `feature/v8-store-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only cf6326e741499c0a7204f1ec65272af75c408cda`. Failure = stop, `boop beep` the coordinator.
- Crate `v8/`, package `dl8`. Every command runs from `v8/`.

## 3. Read first
| path | why |
|---|---|
| `/Users/chrishafley/projects/sprefa/.boop-worktrees/plan/v8-store-20260914/plans/v8/2026-09-14-v8-store.PLAN.md` | the plan (PR #740, unmerged): sections 3.2 signatures, 3.3 cursor, 4 pseudo-code, 5 lifetimes, 7 tick sequence, 8 uniqueness, 13 measurements, and section 15, Chris's decisions, which OVERRIDE section 6 |
| same dir, `...PLAN.visual.human.unga.md` | the short form |
| `v8/src/_6_eval/_0_term.rs` | `Universe { syms: IndexSet<String>, terms: IndexSet<Term> }`, Term kinds Int, Float(OrderedFloat), Bool, Atom(Sym), Str(Sym), Compound |
| `v8/src/_6_eval/_3_table.rs`, `_5_evaluate.rs:39-60, 615-630` | `Store`, `Table { rows: IndexSet<Box<[TermId]>>, frontier, index }`, `mark_all` |
| `v8/src/_6_eval/_6_json.rs:190` | `closure_to_json`, the shape the tests compare |
| `v8/src/bin/dl8.rs:57-63, 106-115` | the `Eval` door you extend |
| `v6/sprefa-engine-rs/src/sql.rs:64-300` | `SqliteSeam` begin/commit/rollback and the statement budget, the shape to copy |
| `.claude/skills/sql-relational-design/SKILL.md`, `.claude/skills/sqlite-costs/SKILL.md` | mandatory |

## 4. Files you own
| file | change |
|---|---|
| `v8/src/_9_runtime/mod.rs`, `_0_store.rs`, `_1_sqlite.rs` | new: `IRowStore`, `Watermark`, `StoreError`, `SqliteRowStore` |
| `v8/src/lib.rs` | one `pub mod _9_runtime;` line (mirror how `_8_driver` is declared) |
| `v8/src/bin/dl8.rs` | `Eval { program, trace, db: Option<PathBuf> }` |
| `v8/Cargo.toml` | `rusqlite = { version = "0.40", features = ["bundled"] }` |
| `v8/tests/_13_store.rs` | new, real binary |
| `v8/fixtures/store/*.dl7`, `*.expected.json` | new |
Never touch: `v8/src/_6_eval/**` (everything you need is already `pub`), `_0_read` through `_5_reify`, `v8/oracle/**`, `v7/`, `v6/`. If a field you need is private, STOP and `boop beep` the coordinator naming it.

## 5. Design (decided)
Signatures: section 3.2 of the plan verbatim (`IRowStore` with `open`, `load_arena`, `load_rows`, `begin_tick`, `commit_arena`, `commit_rows`, `commit_tick`, `rollback_tick`; `Watermark { syms, terms }`; `StoreError { NestedBegin, ArenaMismatch, Sql }`). The durability cursor is the store's own per-relation count, never `Table.frontier` (plan 3.3).

Layout, section 15 overriding section 6:
| table | columns | law |
|---|---|---|
| `"<p>.sym"` | `id INTEGER PRIMARY KEY, text TEXT UNIQUE` | the one TEXT column, once |
| `"<p>.term"` | `id INTEGER PRIMARY KEY, kind INTEGER, ival INTEGER, rval REAL, sym INTEGER REFERENCES sym` | compound and atom terms only; Int/Float/Bool/Str cells never need a term row |
| `"<p>.term_arg"` | `term INTEGER, position INTEGER, child INTEGER, PRIMARY KEY (term, position)) WITHOUT ROWID`, `CHECK (child < term)` | plan 3.3 finding: args always precede the compound |
| `"<p>.<relation>"` | one table per user product, typed columns in edge order: Int → INTEGER, Float → REAL, Bool → INTEGER 0/1, Str → TEXT value, Atom → INTEGER sym id, Compound → INTEGER term id; `UNIQUE` over all columns | table per product, typed, no denormalising |
| `"<p>.kernel"` | `rel INTEGER, arity INTEGER, a0..a7 INTEGER` term ids, UNIQUE over used columns | kernel and prelude rows (nil, cons, intern_snapshot, edge_snapshot, ...) whose cells are compounds |
`<p>` is the program name; names are double-quoted with a dot, `"fetch_json.pending"` style, no `__txt_` prefix. Ids are never reused. No SQL index beyond the UNIQUE and primary keys (plan 13.4). Booleans are 0/1. No NULL cell, absence is no row.

Load: `open` runs idempotent DDL; `load_arena` re-interns `sym` then `term` in `ORDER BY id` (children first by the CHECK); `load_rows` re-interns each typed cell into a `TermId` and inserts into `Store`, then `Table.index` rebuilds as it does today. After load, `mark_all()` once so every loaded row is old.
Commit: one transaction per tick (`begin_tick` ... `commit_tick`); `commit_arena` appends past the watermark; `commit_rows` appends rows past the store's per-relation cursor with one multi-row `INSERT` per relation (never per row), 8 rows per statement batch is fine, one prepared statement per relation cached.
The relation-to-table mapping comes from the program JSON's relation names; a relation whose rel term is `ref(kernel(...))` or `ref(prelude ...)` goes to `"<p>.kernel"`.

## 6. Steps with receipts
1. `_9_runtime` module with the trait, error, watermark, and `SqliteRowStore::open`. Receipt: `cargo build` clean, 24 tests green.
2. `load_arena` / `commit_arena` round trip. Receipt: unit test in `tests/_13_store.rs` on a temp file: intern 5 terms of every kind, commit, fresh `Universe`, load, ids equal.
3. `commit_rows` / `load_rows` and the `--db` flag on `dl8 eval`. Receipt: fixture 0 below.
4. Restart continuation. Receipt: fixture 1 below, plus the COUNT receipt.

Fixtures, `.dl7` compiled with `dl8 compile` then run with `dl8 eval --db` (same test shape as `tests/_10_literals.rs`, compile then eval):
```lisp
; 0_round_trip.dl7
(: Reading (* (: name text) (: value float) (: ok bool) (: count int)))
(Reading "a" 1.5 true 3)
(Reading "b" -0.25 false 4)
(: Copied (* (: name text) (: value float)))
(<- (Copied ?Name ?Value) (Reading ?Name ?Value ?_ ?_))
```
Test: run once with `--db t.db`, closure equals `0_round_trip.expected.json`; run again with the same db, closure equal again; `sqlite3 t.db 'SELECT count(*) FROM "0_round_trip.Reading"'` prints 2 (through rusqlite in the test, no shell).

```lisp
; 1_continue.dl7   first program
(: Edge (* (: from int) (: to int)))
(Edge 1 2) (Edge 2 3)
(: Path (* (: from int) (: to int)))
(<- (Path ?From ?To) (Edge ?From ?To))
(<- (Path ?From ?To) (Edge ?From ?Middle) (Path ?Middle ?To))
; 1_continue_more.dl7  same declarations plus (Edge 3 4)
```
Test: run `1_continue` with `--db t.db`, then `1_continue_more` with the same db. Closure of the second run equals `1_continue_more.expected.json` (Path 1-2, 1-3, 1-4, 2-3, 2-4, 3-4). COUNT receipt: with `RUST_LOG=dl8=info` and the existing `--trace`, the second run's stderr shows rounds that insert only the 3 new Path rows; assert the number of `INSERT` statements executed in the second run through a rusqlite trace/profile hook in the test is `<= 4` (one per relation touched plus arena), and state the number you measured in the PR body. A run that rewrites the 2 existing Edge rows or the 3 existing Path rows fails the test.

## 7. Validation commands
```bash
cd v8 && cargo test 2>&1 | grep -E "^test result|FAILED|panicked"
cd v8 && cargo clippy --all-targets 2>&1 | grep -c "^warning\|^error"   # 0
cd v8 && cargo fmt --check
git status --short v8/oracle          # empty
git diff --stat cf6326e741499c0a7204f1ec65272af75c408cda...HEAD   # owned files only
```
Every test runs through the real `dl8` binary with a temp file db. No unit fakes; the arena round trip in step 2 is pure computation and may be a plain test.

## 8. Style laws, comment budget
- COMMENT BUDGET IS HARD: a comment states only a constraint the code cannot show, one line, no narrative, no plan references, no "this mirrors v6", no restating the next line. Doc comments on the trait methods only. Expected comment lines added across the lane: under 15. The coordinator counts them.
- Tests only under `v8/tests/`, files `_<n>_name.rs`, everything `pub`, interfaces carry the `I` prefix, no `eprintln!` (`tracing` only), no em dashes, banned words (provenance, substrate, load-bearing, regime, honest, ground as a verb, refusal, "ground truth"), no shortnames (`connection` not `conn`, `statement` not `stmt`), no function over 70 lines, no per-row write, no composite TEXT key, no NULL cell.
- Commit message ends with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## 9. Finish protocol
Commit, push, PR to `main` titled `feat(v8): SQLite row store, dl8 eval --db`, body with: the two fixtures, test counts before and after, clippy count, comment lines added (`git diff cf6326e7...HEAD -- v8/src | grep -c '^+\s*//'`), the INSERT count measured in fixture 1, `git status --short v8/oracle` output. End with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`. Then `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "store: PR #<n>, tests <before>-><after>, clippy 0, inserts <n>"`. Never merge, never spawn subagents, never touch `_6_eval`.
