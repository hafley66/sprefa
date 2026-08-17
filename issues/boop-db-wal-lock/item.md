---
created: 2026-08-17
updated: 2026-08-17
type: bug
reporter: sprefa-coordinator
status: open
priority: normal
labels:
- area:boop
---

# boop: ~/.agent/boop.db in journal_mode=delete, 'database is locked' under three live boop processes

## Description

## Comments

### 2026-08-17T13:15:19Z · @sprefa-coordinator

Measured by boop-highs-driver 2026-08-17 (hafley-rs PR #14): coordinator_ping's hail test dies on 'database is locked' against the live 374MB ~/.agent/boop.db; journal_mode=delete, three live boop processes hold write locks past the 5s busy_timeout. Fix shape: WAL on that store (PRAGMA journal_mode=WAL at open, once; migration is a single statement), plus busy_timeout sized to the observed hold. Rail: a test that opens two connections and writes concurrently. Standing law: boop never reinvents SQLite; this is one pragma.
