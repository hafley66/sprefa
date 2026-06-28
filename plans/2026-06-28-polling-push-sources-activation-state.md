# Off-disk sources: polling-as-push + interruptible/storable activation state

Idea captured 2026-06-28 (not scheduled). The motivating case: a spec that is a
BUILD ARTIFACT, not a committed file — e.g. FastAPI serves its OpenAPI at
`GET /openapi.json` from the live app; the schema only exists by making a network
call (or running a generator command) your machine CAN make, but it is never on
disk in git. sprefa needs to ingest such a source, treat repeated polling as a
PUSH into the reactive tick, and store/interrupt WHEN a source is active.

## Two capabilities, one seam

### C1 — a command/network source that materializes a virtual file
Today sources read disk (`scan` WORK) or a git blob (`scan` rev); `cmd` shells out
PER already-matched file (engine.rs:6052), and `repo.url` lazy-clones (1169). None
of these produce content from a one-shot command/URL into the file space.
- New source: `pull("<virtual/path>", "<command>")` (or a `:url` variant) that
  runs the command, captures stdout, and interns it as the content of
  `<virtual/path>` in `_files` (content-addressed via the existing blake3 path).
  Downstream `json`/`jsonp`/`match`/the SpecLang extractor then treat it exactly
  like a scanned file — zero new extraction code.
- FastAPI shape: `pull("build/openapi.json", "curl -s localhost:8000/openapi.json")`
  or `pull("build/openapi.json", "python -m app.export_openapi")`.
- rev semantics: the pulled content gets a synthetic rev (e.g. content hash or a
  monotonic pull counter) so `_file (repo, path, rev)` stays well-formed and
  history-aware; WORK-vs-pulled diffs work like any rev pair.

### C2 — polling-as-push with stored, interruptible activation state
Re-running the command on an interval is POLL; the engine converts it to PUSH by
content-diffing: same blake3 -> no-op tick (free, content-addressed); changed
bytes -> the normal owner-subscribe wake re-fires only the dependent rules. This
is the push/pull "dam" boundary (see theory:push-pull-dam): the poll loop is the
puller, the tick graph is the pushed side, the content-hash gate is the dam.
- Activation state is STORED and INTERRUPTIBLE: a source can be active (polling),
  paused, or one-shot, and that state survives daemon restarts.
- "Interruptible" = pause/resume without losing the last-pulled snapshot; a paused
  source keeps serving its last content to the graph (deterministic), it just
  stops re-pulling.

## Type / storage sketch

```rust
// A new source body item (ast.rs), sibling of Cmd/Scan.
//   pull(path: Term, command: String)  ->  binds nothing new; populates _files
//   for `path` at a synthetic rev. Optional `:url` form fetches over HTTP.
BodyItem::Pull { path: Term, command: String, mode: PullMode /* Shell | Url */ }

// Stored activation state (db table, persists across restarts).
//   source_id = stable hash of (path, command)
struct SourceState {
    source_id: String,
    active: bool,            // polling on/off (interruptible)
    interval_ms: Option<u64>,// None = one-shot / manual trigger only
    last_rev: String,        // synthetic rev of the last pulled content
    last_pull_unix: i64,     // for the scheduler (pass time in; no Date::now in engine)
}
```

```
-- storage
CREATE TABLE _source_state (
  source_id TEXT PRIMARY KEY,
  active    INTEGER NOT NULL,
  interval_ms INTEGER,        -- nullable
  last_rev  TEXT NOT NULL,
  last_pull INTEGER NOT NULL
);
```

Read/write sequence:
1. First tick referencing a `pull(...)` source: run command, intern stdout into
   `_files` (blake3), upsert `_source_state` (active per the rule/flag, last_rev =
   content hash). Downstream extraction runs.
2. Daemon poll loop: for each active source whose `interval_ms` elapsed, re-run;
   if content hash == last_rev, skip (push-dam no-op); else update `_files` +
   last_rev and wake dependents (existing reactive path).
3. Pause: set `active = 0`; the loop skips it; last content stays in `_files`.
   Resume: `active = 1`; next loop pull re-syncs.

Uniqueness / correctness:
- `source_id` = hash(path, command) so the same pull declared twice coalesces.
- Content-addressed dedup is the dam: identical pulls never re-tick.
- A failing command must NOT wipe the last good content (serve stale, surface a
  diag) — same stance as gen's "don't clobber on no-op".
- No `Date::now` in the engine: the poll scheduler lives in the daemon and passes
  the clock in (matches the workflow/resume constraint elsewhere).

## Control surface (CLI / LSP / dl)
- CLI: `--pull-interval`, or a `source` subcommand to `pause`/`resume`/`pull-now`.
- dl-driven (preferred, matches the convention-rel pattern): a program declares
  the source and its desired activation; a `source_control(source_id, active,
  interval)` convention rel the daemon reads, so activation is editable from a
  page like `diag`/`def_target`/`hover_section`.
- LSP: a command/code-lens to toggle a source active, and a diag when a pull
  fails or drifts.

## Open questions
- HTTP directly (a `:url` pull) vs always-shell-out (`curl`/generator command)?
  Shell-out is simpler and reuses `cmd` plumbing; native HTTP avoids a curl dep.
- Synthetic rev scheme: content-hash (dedup-friendly, but unordered) vs monotonic
  pull counter (ordered history, but re-pulls of identical content bump it). Maybe
  both: rev = counter, dedup gate = hash.
- Security: a pull command runs arbitrary shell on tick. Gate behind an explicit
  flag / allowlist (same posture as the dual-use stance).
- Relationship to the existing reactive owner-subscribe + incremental tick: a
  pulled source is just another source table; confirm the wake path treats it
  identically to a file edit.

## Why this matters for the spec/flow work
Makes the OpenAPI SpecLang (`2026-06-28-openapi-speclang-flows.md`) work against a
LIVE spec (FastAPI/codegen build artifact), not just a committed file — the
cross-lang goto/flow graph stays correct as the running service's contract
changes, with the poll loop pushing updates into the same tick that drives the
LSP overlays.
