# LANE boop — STANDING AMENDMENT. Read this before PASS2.md and PASS2-ADDENDUM.md.

## The dependency rule is REVOKED

PASS2.md section 3 said "add no dependency outside the table; if you think you
need one, STOP and report." That rule was wrong and it cost a round trip. It is
dead.

**Pick your own dependencies. Do not stop to ask about one. Ever.**

`tokio` is fine. `sqlx` is fine. `rusqlite` is fine. `time`, `chrono`,
`crossbeam`, `notify`, `rayon` are all fine. This binary is standing in for a
532-line bash-and-node script. It does not need a procurement process.

The research already in PASS2.md and PASS2-ADDENDUM.md is FYI, not a gate. If
`sqlx` + `tokio` reads better to you than `rusqlite`, take it and move.

The only dependency judgment that still matters: prefer something maintained
over something you hand-roll. That is the whole rule now.

## What is still a real law

These are measured, not stylistic. They stay.

- **Surrogate keys.** Stored rels key on INTEGER ids. Natural or composite TEXT
  keys live ONCE in a dictionary table with UNIQUE on the natural key. A
  composite TEXT PRIMARY KEY is a defect. Measured on this machine: TEXT keys
  run 1.7-2.0x slower on identical tables because every index copies the full
  key.
- **WAL mode on `boop.db`.** `PRAGMA journal_mode=WAL`. Two processes reading
  and writing one SQLite file in WAL mode is the IPC layer for anything else
  that wants boop's data. No HTTP server, no socket protocol. Set it at open
  and say so in the report.
- **`session_edge.relation` keeps the harness disagreement as data.** claude
  says a subagent is not a session; opencode and codex say it is. Do not pick a
  winner.
- No `unwrap()`/`expect()` outside tests. A corrupt `boop.db` prints its path
  and exits non-zero. It never panics, never silently recreates itself.
- No em dashes. Banned identifiers and prose: `provenance`, `substrate`,
  `load-bearing`, `regime`.
- Comments state only constraints the code cannot show.

## What "done" means for this pass

The four layers from PASS2.md section 2, the 1-1 verb map from section 6, and
`boop list` agreeing with `bus list`. Commit on top of `b3428e68`. If something
in the brief blocks you, make the call yourself, note it in REPORT.md under a
heading "calls I made", and keep going.

Stop only for something that would destroy data or that contradicts a measured
law above.
