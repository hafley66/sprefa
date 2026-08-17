---
created: 2026-08-17
updated: 2026-08-17
type: feature
status: open
priority: normal
epic: openapi-clap-uds-lab
related: ['@boop-db-wal-lock']
labels: [lab]
---

# boop store + verbs hosted in dl6 (concatmap loop as rules)

## Description

## Use case
boop's store (`~/.agent/boop.db`, plain SQLite) and boop's verbs become dl6
hosts, so a loop like `boop concatmap` (poll new (assistant,user) turn pairs
from the store, map each through a one-shot model pass, to fixed point) is
writable as ordinary dl6 rules, the same way `sh` hosts spell from->to today.

## Shape (rel decls only; no new construct is asserted here)
- `bind` on the store: rows arriving from `turns` (poll or sqlite update hook) -> `rel turn(session, turn, ts, role, text)`.
- pure rule: `pair(session, turn, ai_text, user_text)` from two `turn` rows.
- host `boop_oneshot(model: text, template: text, input: text) -> (output: text)` = the one-shot pass, retry policy at the host.
- host `boop_db(sql: text) -> (row: json)` = `boop db "<sql>"`, read side.
- write side: staged mutations back to the store (compare `source-mutations.dl6`).

The rx lowering, per the standing law: `interval(poll).pipe(concatMap(readNewPairs), concatMap(pair => oneshot(pair)), scan(fixedPoint))`.

## Depends on / relates
- @openapi-clap-uds-lab: if a spec (OpenAPI or boop's own clap tree) can generate a CLI and an API, the same spec generates the dl6 host declarations (from->to columns) for boop verbs, so hosting boop needs no hand-written host per verb.
- boop review audit (hafley-rs `audit/boop-review`): which verbs already have a clean input->rows shape.
- `boop-db-wal-lock`: a dl6 poller adds a reader; WAL first.

## Not decided (Chris in the room)
Which boop verbs are hosts vs binds; whether the store bind is poll or hook; how the one-shot host reports a dropped pair (option vs a status column).
