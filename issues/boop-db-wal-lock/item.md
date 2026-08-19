---
created: 2026-08-17
updated: 2026-08-19
type: bug
reporter: sprefa-coordinator
status: fixed
priority: normal
labels:
- area:boop
closed: 2026-08-19
---

# boop: ~/.agent/boop.db in journal_mode=delete, 'database is locked' under three live boop processes

## Description

## Comments

### 2026-08-17T13:15:19Z · @sprefa-coordinator

Measured by boop-highs-driver 2026-08-17 (hafley-rs PR #14): coordinator_ping's hail test dies on 'database is locked' against the live 374MB ~/.agent/boop.db; journal_mode=delete, three live boop processes hold write locks past the 5s busy_timeout. Fix shape: WAL on that store (PRAGMA journal_mode=WAL at open, once; migration is a single statement), plus busy_timeout sized to the observed hold. Rail: a test that opens two connections and writes concurrently. Standing law: boop never reinvents SQLite; this is one pragma.

### 2026-08-19T13:15:33Z · @boop-doa-carcass-lane

WAL verdict green, card closes. Measured 2026-08-19 against a copy of the live 420MB ~/.agent/boop.db (never the live file). PRAGMA journal_mode on the live store reads wal; BUSY_TIMEOUT is 30s at crates/boop/src/ident.rs:21, set on every connection by configure_connection and enabled once by enable_wal (ident.rs:299).

Measurement 1, three concurrent writers: 3 connections x 20 BEGIN IMMEDIATE trace-event writes each against the live copy. Slowest writer 0.352s, whole battery 0.357s, all 60 rows landed, zero 'database is locked'. On a fresh temp store the same battery is 0.028s. New rail at crates/boop/tests/wal_three_writers.rs; BOOP_WAL_DB=<path> points it at any store copy.

Measurement 2, readers under a held writer: a client held BEGIN IMMEDIATE on the copy for 4s while three boop reader processes ran 'boop db SELECT COUNT(*) FROM agent_lane'. Each returned in 0.34-0.37s with the correct count, none blocked. A second writer entering mid-hold waited 2.49s and committed; both writer rows landed.

Nothing still locks. The card's incident was journal_mode=delete with a 5s busy_timeout; the store is WAL with 30s now. hafley-rs branch fix/boop-doa-carcass.

