# The store

## What

`--db <file>` on `dl8 eval` or `dl8 run` opens one SQLite file, loads the arena and the rows already there, evaluates, and writes one transaction per tick (`src/bin/dl8.rs:188-205`, `:280-299`, `:369-382`).
The table prefix is the compile JSON's file stem (`dl8.rs:174-180`). A declared relation's table is `"<program>.<Name>_a<arity>"`, an undeclared one `"<program>.rel<id>_a<arity>"`; `sym`, `term`, `term_arg`, `relation` and `kernel` are reserved objects (`src/_9_runtime/_1_sqlite.rs:1-2`, `:58-70`).
A product table has `"__id" INTEGER PRIMARY KEY` and one typed column per field; a text or compound cell stores its arena id (`_1_sqlite.rs:116-120`, `src/_9_runtime/_0_store.rs:72-80`). Kernel rows, `effect` among them, go to the `kernel` table (`_1_sqlite.rs:625-626`).
INSERTs are chunked by SQLite's variable limit, so the statement count depends on chunk size, never on rows (`_1_sqlite.rs:73-106`). A continued run writes only the rows the new seeds add.
On a reloaded db, a `Once` application with a matching data row is answered; an error row is not, so a restart retries it. A reloaded timer numbers past its stored ticks (`v8/README.md:94-100`).

| store plan decision (`plans/v8/2026-09-14-v8-store.PLAN.md:899-907`) | code |
|---|---|
| 1: ints in every key; term table for compound and kernel terms | `term`, `term_arg`, `sym` tables (`_1_sqlite.rs:15-20`) |
| 2: one table per product, typed columns | `<Name>_a<arity>` with typed columns |
| 3: one transaction per tick | `persist` (`dl8.rs:188-205`) |
| 4: `"fetch_json.pending"` and `"fetch_json.settled"` tables, `aborted_at` | not in code: effect rows sit in `<program>.kernel`, no pending table, no abort; the code wins |
| 5: dictionary release by refCount and sweep | not built |

## Why

Plan section 15 records Chris's five answers of 2026-09-14 (`store.PLAN.md:899-907`); rows 1 to 3 are what the code does.
`dl8.rs:188`: one transaction per tick so the arena and the rows never disagree after a kill.

## When to use

Use it when:

- a closure must survive the process: `fixtures/store/0_round_trip.dl7`
- new seeds extend an old closure: `fixtures/store/1_continue.dl7` then `1_continue_more.dl7`
- a run must not repeat an answered request after restart: `tests/_17_reconcile.rs:270`

Do not use it when:

- two programs share a JSON stem: they share tables, `dl8.rs:174-180`
- a seed was removed from the source: its row stays, nothing retracts, [Not built yet](16_not_built.md)
- a derived relation must be a live SQL view: [The SQLite emitter](13_sqlite.md)

## Example

Continue a closure with one more edge, in the same file path:

```dl7
; fixture: v8/fixtures/store/1_continue_more.dl7
(: Edge (* (: from int) (: to int)))

(: Path (* (: from int) (: to int)))

(<- (Path ?From ?To)
    (Edge ?From ?To))

(<- (Path ?From ?To)
    (Edge ?From ?Middle)
    (Path ?Middle ?To))

(Edge 1 2)

(Edge 2 3)

(Edge 3 4)
```

```console
$ d=$(mktemp -d) && cp fixtures/store/1_continue.dl7 $d/program.dl7 && bash book/show.sh eval $d/program.dl7 --db $d/store.db && cp fixtures/store/1_continue_more.dl7 $d/program.dl7 && bash book/show.sh eval $d/program.dl7 --db $d/store.db
(Edge 1 2)
(Edge 2 3)
(Path 1 2)
(Path 1 3)
(Path 2 3)
exit 0
(Edge 1 2)
(Edge 2 3)
(Edge 3 4)
(Path 1 2)
(Path 1 3)
(Path 1 4)
(Path 2 3)
(Path 2 4)
(Path 3 4)
exit 0
```

A timer run twice against one db numbers past the stored ticks:

```console
$ d=$(mktemp -d) && bash book/show.sh run fixtures/reconcile/0_timer.dl7 --serve timer --max-ticks 2 --db $d/run.db | grep '^(timer \|^ticks' && bash book/show.sh run fixtures/reconcile/0_timer.dl7 --serve timer --max-ticks 2 --db $d/run.db | grep '^(timer \|^ticks'
(timer 1 1)
(timer 1 2)
ticks 2
(timer 1 1)
(timer 1 2)
(timer 1 3)
(timer 1 4)
ticks 2
```

## What proves it

| claim | path | command |
|---|---|---|
| a reopened db leaves the closure identical and writes no row | `tests/_13_store.rs:209-235` | `cargo test --test _13_store reopening_the_db_leaves_the_closure_identical` |
| a continued run executes at most 4 INSERTs for 1 Edge and 3 Path rows | `tests/_13_store.rs:237-276` | `cargo test --test _13_store continuing_a_program_writes_only_the_new_rows` |
| declared relations name their tables | `tests/_13_store.rs:278-322` | `cargo test --test _13_store a_declared_relation_names_its_own_table` |
| the arena round-trips every id | `tests/_13_store.rs:168-207` | `cargo test --test _13_store arena_round_trip_preserves_every_id` |
| a second fetch run answers nothing, inserts nothing, and sends 1 request in total | `tests/_17_reconcile.rs:269-289` | `cargo test --test _17_reconcile a_second_run_against_the_db_answers_nothing_and_inserts_nothing` |
| timer numbering continues past the db | `tests/_17_reconcile.rs:183-201` | `cargo test --test _17_reconcile timer_numbering_continues_past_the_ticks_in_the_db` |
