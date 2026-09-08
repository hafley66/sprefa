# Source inventory

Retrieved 2026-09-08 into task-owned directory `/tmp/sprefa-sqlite-extension-research.ZsALrZ`. These clones and their build products are excluded from this repository.

| Reading order | Family | Default branch / pinned commit | License | Local reading path | Immutable source entry points |
|---:|---|---|---|---|---|
| 0 | SQLite, `https://github.com/sqlite/sqlite` | `master` / [`f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9`](https://github.com/sqlite/sqlite/tree/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9) | Public domain | `/tmp/sprefa-sqlite-extension-research.ZsALrZ/sqlite` | [FTS5 callbacks](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/fts5_main.c#L2117-L2164), [savepoint callbacks](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/fts5_main.c#L3166-L3215), [vtab dispatcher](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/src/vtab.c#L996-L1139), [FTS5 savepoint test](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/test/fts5savepoint.test) |
| 1 | pg_ivm, `https://github.com/sraoss/pg_ivm` | `main` / [`dda7470e085822c215411c0b349f4aa4fbb9fdf4`](https://github.com/sraoss/pg_ivm/tree/dda7470e085822c215411c0b349f4aa4fbb9fdf4) | PostgreSQL license | `/tmp/sprefa-sqlite-extension-research.ZsALrZ/pg_ivm` | [transition-table capture](https://github.com/sraoss/pg_ivm/blob/dda7470e085822c215411c0b349f4aa4fbb9fdf4/matview.c#L1021-L1050), [delta rewriting](https://github.com/sraoss/pg_ivm/blob/dda7470e085822c215411c0b349f4aa4fbb9fdf4/matview.c#L1277-L1424), [outer join rewrite](https://github.com/sraoss/pg_ivm/blob/dda7470e085822c215411c0b349f4aa4fbb9fdf4/matview.c#L1396-L1417), [regression SQL](https://github.com/sraoss/pg_ivm/tree/dda7470e085822c215411c0b349f4aa4fbb9fdf4/sql) |
| 2 | CWI prototype, `https://github.com/cwida/ivm-extension` | `main` / [`940d5350c85bbf34acec60c384bf2f89e4dcdd08`](https://github.com/cwida/ivm-extension/tree/940d5350c85bbf34acec60c384bf2f89e4dcdd08) | MIT | `/tmp/sprefa-sqlite-extension-research.ZsALrZ/cwida_ivm` | [README contract](https://github.com/cwida/ivm-extension/blob/940d5350c85bbf34acec60c384bf2f89e4dcdd08/README.md), [extension](https://github.com/cwida/ivm-extension/blob/940d5350c85bbf34acec60c384bf2f89e4dcdd08/ivm_extension.cpp), [tests](https://github.com/cwida/ivm-extension/tree/940d5350c85bbf34acec60c384bf2f89e4dcdd08/tests) |
| 3 | OpenIVM, `https://github.com/ila/openivm` | `main` / [`3b3938f4f8293875b56157f563c6f8cb196a0b41`](https://github.com/ila/openivm/tree/3b3938f4f8293875b56157f563c6f8cb196a0b41) | MIT | `/tmp/sprefa-sqlite-extension-research.ZsALrZ/openivm` | [build inputs](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/CMakeLists.txt), [delta compiler](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/src/delta/delta_compiler.cpp), [operator dispatch](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/src/delta/operators/dispatch.cpp), [limits](https://github.com/ila/openivm/blob/3b3938f4f8293875b56157f563c6f8cb196a0b41/docs/limitations.md) |
| 4 | cr-sqlite, `https://github.com/vlcn-io/cr-sqlite` | `main` / [`43ab94cac537d080aac7cb7628a435c02ee9e268`](https://github.com/vlcn-io/cr-sqlite/tree/43ab94cac537d080aac7cb7628a435c02ee9e268) | MIT | `/tmp/sprefa-sqlite-extension-research.ZsALrZ/cr_sqlite` | [extension init and commit hook](https://github.com/vlcn-io/cr-sqlite/blob/43ab94cac537d080aac7cb7628a435c02ee9e268/core/src/crsqlite.c#L80-L111), [bundled SQLite hook declarations](https://github.com/vlcn-io/cr-sqlite/blob/43ab94cac537d080aac7cb7628a435c02ee9e268/core/src/sqlite/sqlite3.h#L6570-L6731) |
| 5 | Materialite, `https://github.com/vlcn-io/materialite` | `main` / [`e08c1fa176883a51296d7c2c3a86c539c0e72eab`](https://github.com/vlcn-io/materialite/tree/e08c1fa176883a51296d7c2c3a86c539c0e72eab) | Apache-2.0 | `/tmp/sprefa-sqlite-extension-research.ZsALrZ/materialite` | [join delta operator](https://github.com/vlcn-io/materialite/blob/e08c1fa176883a51296d7c2c3a86c539c0e72eab/packages/materialite/src/core/graph/ops/JoinOperator.ts), [reduce operator](https://github.com/vlcn-io/materialite/blob/e08c1fa176883a51296d7c2c3a86c539c0e72eab/packages/materialite/src/core/graph/ops/ReduceOperator.ts) |

`cwida/ivm-extension` and `ila/openivm` are separate repositories: their default branches, commits, source layouts, interfaces, and tests above are distinct. The CWI README describes `delta_<base>` input tables with a boolean multiplicity and limited SELECT/FILTER/GROUP/PROJECTION support. OpenIVM is a later DuckDB extension with a larger, separate source tree.

## Discussion and follow-up trace

[cr-sqlite discussion 309](https://github.com/vlcn-io/cr-sqlite/discussions/309), retrieved 2026-09-08, proposes an observation layer based on `sqlite3_set_authorizer`, `sqlite3_preupdate_hook`, and commit/rollback coordination. Its body names a possible Rust/WASM port of GRDB observation. It does not name Feldera and the pinned `cr-sqlite` and `materialite` trees contain no Feldera integration. `cr-sqlite` does register its own `sqlite3_commit_hook` in `core/src/crsqlite.c`; that is implemented replication behavior, separate from the discussion proposal.

The requested Rindle URL (`https://rindle.sh/docs/how-it-works?path=engine`) returned HTTP 404 on 2026-09-08. Search located `szTheory/rindle`, an unrelated Phoenix/Ecto media package. No public repository for the referenced engine was identified, so no SQL-resident or external-state claim is made.

## Execution receipts

One focused SQLite attempt was made at revision `f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9`:

```sh
./configure --enable-fts5
make -j2 testfixture
./testfixture ext/fts5/test/fts5savepoint.test ext/fts5/test/fts5conflict.test
```

Exit code: `2`. `configure` completed; `make testfixture` stopped because `/usr/bin/tclsh` could not provide `tclConfig.sh` (`TCL_CONFIG_SH must be set`). The test command was therefore not created or run. Logs remain task-local at `/tmp/sprefa-sqlite-*-{configure,testfixture-build}.log`. No OpenIVM or PostgreSQL suite was built because their documented builds require a DuckDB or PostgreSQL source/build environment; no new engine build was introduced.
