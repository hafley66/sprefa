# v6 cascade tuning log

Each entry: hypothesis, change, measurement, verdict (KEEP / RETRACT). Numbers are
from `cargo run --release --example sqlite_reach -- 10 500000` (5,000,002 nodes /
9,666,667 edges / 500,000 killed / depth 3), `DL_MEMCAP_MB=0`, on-disk WAL. Retract
is the MEASURED op; setup is one-time and not the headline.

Resident targets for reference (same graph, 1.5GB budget): dd/dbsp retract ~290ms
but ABORT past the memory wall where sqlite completes.

## E0 — baseline (composite WITHOUT ROWID key)
- Schema: `cx_row(tag,id,weight) PK(tag,id) WITHOUT ROWID`; `cx_dep` 4-tuple PK WITHOUT ROWID.
- 3 runs: retract 2.579 / 2.564 / 2.579 s. Setup ~8.0s. peak_rss ~1.35-1.46 GB.
- 29 stmts, 3 rounds. **This is the number every experiment below is measured against.**
