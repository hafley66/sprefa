# c2_partial_3 SQLite plugin replay

`0_pre_view.db.gz` is the SQLite source database after table declaration, term
dictionary flush, and seed insertion, immediately before `CREATE VIRTUAL TABLE
"program"`. `2_view.sql` is the emitted view statement (78,218 bytes).
`3_rounds.json` contains the evaluator's ordered nonlinear round statements.
`1_arena.db.gz` holds the term arena after the reference evaluation; the
standalone driver loads it only to register the existing `dl_*` scalar functions.

The driver in `examples/0_c2_sqlite_replay.rs` does not invoke the Sprefa
compiler or evaluator. It copies the captured database, installs the view,
executes rounds until they stop changing rows, and writes sorted view rows.
It samples memory through `hafley-observe` and reads SQLite page counts and
live database/WAL file sizes at named boundaries.

```sh
cd /Users/chrishafley/projects/sprefa-wt/sqlite-perf-main
gzip -dc oracle/eval/replay/c2_partial_3/0_pre_view.db.gz > /private/tmp/c2-replay-source.db
gzip -dc oracle/eval/replay/c2_partial_3/1_arena.db.gz > /private/tmp/c2-replay-arena.db
cargo build --offline --locked --example 0_c2_sqlite_replay
RUST_LOG=off SQLITE_IVM_LIB=/Users/chrishafley/projects/sqlite_ivm/target/extension/release/libsqlite_ivm.dylib \
  target/debug/examples/0_c2_sqlite_replay \
  /private/tmp/c2-replay-source.db /private/tmp/c2-replay-arena.db c2_partial_3_program \
  oracle/eval/replay/c2_partial_3/2_view.sql \
  oracle/eval/replay/c2_partial_3/3_rounds.json \
  /private/tmp/c2-replay-run.db /private/tmp/c2-replay-run.rows
shasum -a 256 /private/tmp/c2-replay-run.rows
```

Reference output: 947 view rows, two nonlinear rounds, SHA-256
`8df5e106237fdbf1034e4904dc777f256329be235816bcd3d0ddb6c3ab95da29`.
The result is the raw plugin view, while `dl8 eval` prints a larger closure
that also includes materialized relations.
