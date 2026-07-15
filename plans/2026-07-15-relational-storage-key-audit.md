# Relational storage-key audit

## Context

The 10-file release probe confirms that almost every public identity column is
already a SQLite INTEGER backed by `_strings.id`. The remaining storage cost is
not one missing global interner. It is duplicated B-trees from ordinary rowid
tables with full-row primary keys, a universal `__src TEXT`, entity relations
that ignore their existing integer identity column, and internal metadata that
bypasses the interning layer.

A read-only schema audit found 119 `rel_*` tables in the self-map database. Of
those, 93 multi-column/all-INTEGER tables carry both a rowid B-tree and a
composite-primary-key B-tree. Raw TEXT is mostly correctly limited to payloads.

## Decisions

1. Reusable entities and occurrences use a narrow INTEGER identity.
2. Pure set/junction relations use composite integer keys and `WITHOUT ROWID`;
   they do not get redundant surrogate IDs.
3. Existing `id`/`sym` columns become the key only where uniqueness is a real
   semantic contract.
4. Repeated repo/rev/path/name/kind/relation identifiers use `_strings.id`.
5. Raw source, JSON, markdown, diagnostic detail, and literal bodies stay TEXT.
6. Every migration is justified by `dbstat` bytes/row plus read/write timings;
   a narrower-looking schema is not accepted on shape alone.
7. Work lands in small family slices with versioned migration and rollback,
   never as one database-wide rewrite.

<!-- todo(perf): measure and remove or compact universal rel __src storage, preserving source provenance and public set semantics -->

<!-- todo(perf): emit WITHOUT ROWID for measured pure composite-integer junction relations and retain a schema escape hatch -->

<!-- todo(perf): normalize remaining identity storage in SCIP occurrences, entity keys, internal metadata, checkout, and embeddings using measured family migrations -->

## Priority map

| priority | relations | change |
| --- | --- | --- |
| P0 | every canonical `rel_*` | remove/compact universal `__src TEXT`; ownership already has `_prov` |
| P0 | `child`, module/type/call/dataflow/SCIP/graph edges | measured `WITHOUT ROWID` composite integer keys |
| P0 | `scip_occurrence`, `scip_binding` | deterministic compact occurrence ID, then bind by ID |
| P0 | `node`, `df_node[_rev]`, `graph_node`, `call_def[_rev]`, `type_entity[_rev]` | key on existing integer `id`/`sym` only where contractually unique |
| P1 | `_file`, `_prov`, `_files`, `_where_bytes`, `_node_path`, digest/meta tables | intern coordinates and use fixed-width IDs/digests |
| P1 | checkout relations | intern repo/branch/action; keep detail raw |
| P1 | `_embeddings.sid`, `similar(a,b)` | store canonical INTEGER StringIds, not decimal TEXT handles |
| P2 | query/effect event metadata | integer time/IDs and compact categorical values; keep bodies raw |

SCIP is the first measured large target: the audited artifact showed
`scip_occurrence` at roughly 4.13 MB plus a 4.86 MB PK autoindex, and
`scip_binding` at roughly 1.63 MB plus a 1.96 MB PK autoindex.

## Implementation sequence

1. Add a schema classifier that labels each relation entity, occurrence,
   junction/set, or payload-bearing and records its intended key shape.
2. Add current `dbstat` baselines and query-plan snapshots for selected P0
   tables.
3. Prototype `WITHOUT ROWID` on representative two-, four-, and wide-column
   junctions; retain it only where total bytes and timings improve.
4. Separate/compact `__src` without weakening source retraction or derived set
   semantics.
5. Migrate one identity-bearing family at a time, starting with SCIP, behind a
   schema version and rebuild path.
6. Normalize internal metadata and the checkout/embedding exceptions only
   after public relation savings are attributed.

## Verification

- `PRAGMA table_info/index_list` matches the declared key class.
- `dbstat` reports table bytes, index bytes, and bytes/row before and after.
- Cold ingest, unchanged tick, one-file edit, and representative point/range
  queries are measured with two workers.
- Public relation contents match a clean rebuild.
- Duplicate facts retain set semantics and multi-owner facts retract only after
  their final owner disappears.
- Raw payload values remain byte-exact and do not enter `_strings`.
- Old-schema databases rebuild or migrate loudly; rollback reopens them.

Tests are regression rails. Storage and wall-time measurements are the
evidence; a passing suite is not proof of improvement.

## Staffing

- Root owns key semantics, migration order, measurements, and stopping-point
  walkthroughs.
- A bounded worker may implement only the relation classifier/schema rail.
- A bounded worker may prototype only the representative `WITHOUT ROWID`
  experiment.
- A harder reviewer audits uniqueness, provenance, and rollback before any
  entity key is narrowed.
- No production corpus, daemon, or whole-workspace heavy query is used.
- Build/test concurrency and Rayon remain capped at two; formatting runs only
  immediately before commit.

## Final interface destination

```text
extract / resolve / reactive tick
        │ logical typed batches and owner deltas only
        ▼
family store traits
  CallStore, TypeStore, ModuleStore, SourceStore
        │ one atomic semantic operation per family/scope
        ▼
backend implementation
  SQLite today; another backend later
        │ DBAPI execute/query/transaction + physical schema policy
        ▼
SQLite connection, SQL, TEMP tables, indexes, interning, migrations
```

The practical `Storage` trait is the DBAPI mechanism, not the engine-facing
semantic contract. Backend implementations may use SQL. Extractors and
reactivity code call family traits such as `CallStore`; they never receive a
connection, cursor, SQLite row, physical table name, DDL, TEMP-table protocol,
intern ID, or transaction handle.

Migration is mechanical and vertical: move one family operation under
`storage/`, stitch that extractor to one logical call, measure, then repeat.
When the final production caller is migrated, `Db::conn()` becomes private to
the SQLite backend. Replacing SQLite then means implementing the DBAPI and the
family store traits, not rewriting extraction, resolution, or tick scheduling.

The first completed stitch is the call-family wholesale persistence boundary:
`engine/extract/call.rs` emits five logical batches and invokes one
`persist_call_family`; `storage/call.rs` alone owns SQLite replacement scope,
legacy projections, transaction ownership, and physical SQL. Owner-scoped
delta persistence remains the next call-family operation, not a new interface.
