# SQLite IVM lab extension

SQL interface: `sqlite_ivm_create(name, select_sql) -> name`,
`sqlite_ivm_drop(name) -> name`, `sqlite_ivm_version() -> JSON`,
`sqlite_ivm_log(event_limit) -> limit` (connection-local, 0..10000),
`sqlite_ivm_metrics(name, enabled) -> enabled` (transactional view setting).
All functions are DIRECTONLY. No runtime compiler or maintenance host loop.

Build: `cargo build --release --locked --manifest-path 25_sqlite_ivm/Cargo.toml`.
Use the lane's CARGO_TARGET_DIR. Load `release/libsqlite_ivm.dylib` on macOS or
`release/libsqlite_ivm.so` on Linux through `.load` in stock sqlite3.
SQLite >=3.37 is required. Apple /usr/bin/sqlite3 disables `.load`; the existing
Homebrew CLI at /opt/homebrew/opt/sqlite/bin/sqlite3 is used in this lane.

Every writer sets `PRAGMA recursive_triggers=ON; PRAGMA trusted_schema=ON;`.
Persistent SQL guards reject writes without these settings. Writers do not load
an extension. Journal/synchronous settings remain caller-owned. No trace,
update, commit, rollback or preupdate callbacks are installed.

Admitted: two different main-schema ordinary tables, integer equijoin ON or
single USING, GROUP BY that join key, projection key / COUNT(*) / SUM(column or
column*column). Aggregate aliases required. Aliases and quoted names bind to
catalog columns. Source columns declare INTEGER; stored values must be non-NULL
integers within ±1,000,000 after SQLite affinity. Every group has at most
1,000,000 contributions; SUM fits ±10^18. Multiplicity from duplicate rows on
both sides is retained. Rowid, STRICT and WITHOUT ROWID tables are exercised.
Generated columns and non-BINARY column collations are rejected.

Installation validates before schema mutation, then atomically creates an
accumulator, transient delta table, fixed-size metrics row, public read-only
view, source indexes and BEFORE/AFTER triggers. Savepoint rollback removes
partial installation and preserves caller transactions. Initial population
performs one recomputation. AFTER triggers recheck actual stored values, including assigned INTEGER PRIMARY
KEY values. A changed source row joins OLD (-1) and NEW (+1)
against the opposite table. Group counts/sums add the signed contribution;
zero-support groups disappear. The opposite-side join is the delta work, not
an affected-group full refresh. UPDATE retracts then inserts, including key
moves. The delta table is empty at statement boundaries. Multiple views own
separate objects. Drop removes owned objects under a savepoint.

The public result is a read-only VIEW over the physical accumulator, a departure
from the earlier proposed writable ordinary result table. SQL reads are the
same; direct writes to the public result fail. Names prefixed __ivm_ are reserved
internal state. Deliberate edits to internal tables, schema DDL after install,
disabling triggers through C APIs, writable_schema, custom function overrides,
and incremental blob APIs are outside the ordinary DML contract. Pure persistent
SQL cannot prevent a database owner from removing its triggers. Existing
unmanaged source triggers, outgoing foreign keys (column or table constraints),
and TEMP source shadows are rejected at installation. Foreign-key cascades
can interleave changes to logical join inputs before AFTER maintenance.
Applications must drop maintained views before altering source schema. General
schema ownership/authorization and dependency migration remain unimplemented.

Telemetry: `SQLITE_IVM_LOG_LIMIT=0` by default, bounded to 10000 lifecycle events
per loaded connection; `sqlite_ivm_log` resets the allowance. A private tracing
Dispatch writes JSON to stderr at load/install/bind/lower/schema/release/rollback/
drop boundaries, including operation ID, view ID, duration and extended error.
No SQL text or row values are recorded; payload opt-in is not implemented.
The dispatch does not install a process-global subscriber. Logging sink errors
cannot fail successful SQL, tested with closed stderr. Exhausted allowance drops
further events; no growing buffer exists. Install-release means savepoint release,
not an observed outer transaction commit.

Optional metrics update one fixed row per view in the writer transaction:
operations, absolute join contributions and groups touched (OLD/NEW separately).
Counters saturate at 10^15. `SELECT * FROM __ivm_<hex UTF8 view name>_meta` reads
these SQL-owned counters from any connection. Rollback restores them. They do
not provide per-row events, statement durations, or direct commit/rollback
notifications from uninstrumented writers. The benchmark host can observe its
own transaction boundaries and report errors; this is measurement code.

Source reading order: src/0_types.rs, 1_bind.rs, 2_lower.rs, 3_telemetry.rs,
lib.rs (ABI and lifecycle exports). Independent crate, no compiler/kernel edits.

Dependencies pinned in Cargo.toml and Cargo.lock:

- rusqlite 0.40.2, MIT. The packaged LICENSE and upstream
  [loadable example](https://github.com/rusqlite/rusqlite/blob/v0.40.2/examples/loadable_extension.rs)
  were inspected. libsqlite3-sys 0.38.2 supplies ABI bindings through its extension
  feature. No ABI declarations were hand-written.
- sqlite3-parser 0.17.0, [crate source](https://docs.rs/crate/sqlite3-parser/0.17.0/source/).
  Cargo.toml declares Apache-2.0/MIT; packaged LICENSE contains Unlicense.
  This packaging discrepancy is retained explicitly. SQLite-derived parser AST,
  Name, Select and CreateTable definitions were inspected locally. It parses the
  consumer SELECT and catalog DDL; no SQL parser is implemented in this plugin.
- tracing 0.1.44 / tracing-subscriber 0.3.22 (MIT), following sprefa-store's
  tracing event convention with a scoped subscriber rather than global ownership.
- serde_json 1.0.149 (MIT/Apache-2.0), metadata only.

Audited reuse: existing compiler lower.pl avg_accumulator_update_sql supplies the
signed-sum/count accumulator pattern; its avg_delta_rows_sql admits one positive
source and is not a join installer. The lab adds AST-bound two-source OLD/NEW
lowering; the compiler-generated affected-group competitor remains untouched as
an algorithm. sprefa-store SQL frontier/refcount routines require a host-driven
cascade and are not used as an extension maintenance runtime. v6/dd-runner was
not read or used.
