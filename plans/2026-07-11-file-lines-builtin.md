# file_lines builtin + dl-native file-size rail (handoff brief: codex)

Base: main at or after `29e95e7`. Stopped Claude agent had NOT started — clean slate.

## Hard rules

- FILE-SIZE LAW: no new file over 500 lines (target 300). Over 500 = stop and
  present 3 split ideas or 1 justification. `scripts/filesize-rail.sh` enforces
  in verify.sh (exit 2 on a new offender).
- N+1 law: every table write is one batched `Db::insert_rows` per refresh,
  never per-row loops.
- Hermetic dl runs: `SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1` + scratch
  `--db`. Never touch `~/.local/state/sprefa` or a running daemon.
- Suite budget: max 2 full `cargo test` runs. `./scripts/verify.sh` green, then
  `git commit -n` (pre-commit hook is un-hermetic).
- NO hook wiring anywhere in this task — hooks stay disabled until the perf
  arc lands (2026-07-11 outage: an error-severity rail exit-2'd the PostToolUse
  hook and blocked every write repo-wide). The .dl rail ships warning-severity
  only; the bash rail in verify.sh is the enforcement prong.
- dl style: descriptive variable names (never single-letter), one rel = one
  rule kind (source vs derived — split and union), never hand-edit inside
  BEGIN:/END: generated marker zones (fix the generator).

## Task

### 1. Line-count capture in the corpus walk

`enumerate_with_hash` (src/engine/mod.rs ~8089): when the WORK arm reads+hashes
a file, also count lines from the same bytes (`bytes.split(|b| *b == b'\n')`
count, no lossy String; empty file = 0; no trailing newline still counts the
last line). On the mtime+size fast path, reuse the STORED count.

Storage: `_file` gains `lines INTEGER DEFAULT -1` (ALTER on open beside the
existing size migration, src/engine/mod.rs ~4212). `FileMeta` +
`load_file_meta`/`save_file_meta` (~4817-4866) carry it. `-1` = unknown (old
rows; git revs). A `-1` hit on the fast path forces one read+count, then
persists. Git-rev arm (`ls-tree`): leave `-1` — counting blobs would spawn
reads; the rail only needs WORK.

### 2. Builtin rel

`file_lines(repo, path, rev, line_count)` — follow the registered-builtin
checklist end to end: `RelDecl` with group + doc string (doc must state
"line_count = -1 when unknown (git revs)"), reserved-name guard, catalog row,
refresh fills from `_file` WHERE lines >= 0 (batched), README/reference regen
via the generator (examples/builtin-rels.dl / gen-reference.dl — regenerate,
don't hand-edit).

### 3. The rail: .dl/file-size.dl

- Own bare `scan("WORK", "src/**/*.rs", source_file, _)` so it never rides
  another program's corpus.
- soft budget 300: warning `file-over-soft-budget` ("needs a written reason to
  stay this size, or a split").
- hard budget 500, grandfathered (mirror scripts/filesize-allow.txt contents as
  `big_file_ok(path, reason)` facts; header comment: the script's list is
  canonical, this mirrors it, SHRINK ONLY): warning
  `file-over-budget-grandfathered` with the reason.
- hard budget 500, NOT grandfathered: warning `file-over-budget` with the STOP
  protocol text ("STOP: propose 3 ways to split this file, or 1 reason ...").
  WARNING severity, not error — advisory in live checks by design.
- Plus ONE aggregate row (Chris's spec): a diag at path ".dl/file-size.dl"
  line 1: `you still have ${offender_count} unacceptable files (>500 lines)`
  via a count aggregate over the >500 set.

### 4. Tests

- Unit: line counter edges (empty, no trailing newline).
- e2e: fixture repo → `file_lines` returns real counts.
- Rail e2e: a 501-line fixture file yields `file-over-budget` AND the
  aggregate count row.
- Fast-path e2e: second tick, unchanged mtime → stored count reused (no
  re-read; existing enumerate fast-path test shows the pattern). KNOWN FLAKE
  HAZARD: an equal-length edit inside one fs timestamp tick defeats the
  mtime+size fast path check — use a length-varying edit in tests (ledgered
  engine gap).

### Verification

Full suite green (2-run budget), verify.sh green (runs filesize + magic-rel +
recompute rails), rail demonstrated on this repo (should show the grandfathered
warnings + the aggregate count; exit 0).

<!-- todo(feature): file_lines for git revs via the cat-file batch reader, if a rail ever needs history -->
<!-- todo(feature): re-enable PostToolUse hook (timeout-wrapped, advisory) once the perf arc lands — NOT in this task -->
