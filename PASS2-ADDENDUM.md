# LANE boop — PASS 2 ADDENDUM. Your STOP is correct. The brief contradicted itself.

You caught a real defect in PASS2.md. Section 5 required SQLite persistence at
`~/.agent/boop.db` while section 3 listed no SQLite driver and forbade adding
one. That is the coordinator's error, not yours. Stopping was right.

Both precedents you cited are verified on this machine:

```
Cargo.toml:84                    rusqlite = { version = "0.32.1", features = ["bundled", "functions", "trace"] }
v6/sprefa-store/Cargo.toml:16    "sqlx-sqlite"
v6/sprefa-store/Cargo.toml:55    libsqlite3-sys = "0.37"   # pinned to the version sqlx links
```

## Decision: add `rusqlite` with the `bundled` feature. Nothing else.

```toml
rusqlite = { version = "0.40", features = ["bundled"] }
```

Section 3's dependency table is amended to include exactly this one row. The
"add no dependency outside the table" rule still stands for everything else.

## The candidate analysis behind that choice

Numbers from the crates.io API, 2026-08-08.

| candidate | version | downloads | recent | updated | verdict |
|---|---|---|---|---|---|
| `rusqlite` | 0.40.2 | 89,997,956 | 29,283,314 | 2026-08-08 | **CHOSEN** |
| `sqlx` + `sqlx-sqlite` | 0.9.0 | 127,825,432 | 32,651,909 | 2026-05-21 | rejected for boop |
| `libsql` | 0.9.30 | 1,707,493 | 817,025 | 2026-06-02 | rejected |

**Why `rusqlite`.** It is synchronous, and `boop` is a CLI with no other async
need. It is the driver the root `sprefa-dl` crate already uses, so the repo
already carries the knowledge. `bundled` compiles SQLite into the binary, so
there is no system SQLite version to disagree with, which matters because this
repo has already been bitten by exactly that: `v6/prolog/0_type_plane.pl:88`
records that `jsonb` is not portable across the two SQLite builds this project
runs (3.43.2 CLI rejects it, @libsql 3.45.1 accepts). `bundled` removes that
class of problem for `boop` outright.

**Why not `sqlx`.** More total downloads, and it is what `sprefa-store` uses,
but it is async-first. Adopting it means pulling `tokio` into a CLI whose only
other work is spawning tmux and reading files, purely to satisfy the driver.
`sprefa-store` earns that cost because it is a server-side store under a
tracing collector; `sprefa-store/Cargo.toml:51-55` also has to pin
`libsqlite3-sys` to the exact version sqlx links so the raw `*mut sqlite3`
handle type unifies. `boop` should not inherit that pinning constraint.
Compile-time checked queries are sqlx's real advantage and they do not pay for
the async runtime here.

**Why not `libsql`.** 1.7M downloads against rusqlite's 90M. It is the Turso
fork and its value is remote/replicated SQLite, which `boop` does not want: the
whole design reads local files on this machine.

**Version note.** Use `0.40`, not the root crate's `0.32.1`. `v6/boop` has its
own `[workspace]` table and shares no lockfile with the root, so there is no
unification pressure and no reason to adopt an older release. If the workspaces
are ever merged, the two must converge; say so in your REPORT.md so the
coordinator does not lose that.

## What does not change

- Every law in section 5 stands, especially the surrogate-key law. Stored rels
  key on INTEGER ids. Natural and composite TEXT keys live ONCE in a dictionary
  table with a UNIQUE constraint on the natural key. A composite TEXT PRIMARY
  KEY is a defect. Read `.claude/skills/sql-relational-design` and
  `.claude/skills/sqlite-costs` in the sprefa repo before writing the DDL.
  Measured on this machine: TEXT keys run 1.7-2.0x slower on identical tables
  because every index copies the full key.
- `session_edge.relation` keeps the harnesses' disagreement about whether a
  subagent is a session as DATA. Do not pick a winner.
- No `unwrap()` or `expect()` outside tests. A corrupt or unreadable
  `boop.db` prints its path and exits non-zero; it never panics and never
  silently recreates itself.

## Also fix, since you are in there

`cargo test` reported `harness::claude::tests::parses_the_corpus_timestamp_shape`
failing at 7 passed / 1 failed. You hand-rolled an ISO-8601 parser in
`src/harness/claude.rs` (`days_from_civil`, `atoi`). Under the same buy rule
that settled `sysinfo` and `rusqlite`, do not hand-roll date parsing. Add:

```toml
time = { version = "0.3", features = ["parsing", "formatting"] }
```

and parse the transcript timestamps with `time::OffsetDateTime` against
`Rfc3339`. Delete `days_from_civil` and `atoi`. This is the second amendment to
section 3's table and the last one; anything further needs a new STOP.

## Deliverable, unchanged otherwise

Commit on `lane/boop` on top of `b3428e68`. `REPORT.md` per PASS2.md section 10,
plus: the rusqlite version you pinned, the workspace-convergence note, and
confirmation that `days_from_civil`/`atoi` are gone.

Last action, mandatory:

```bash
bus hail --to fable-main --body "boop PASS2 DONE <pass|fail>: <one line>"
```
