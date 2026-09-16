# LANE boop — scaffold the `boop` cross-harness agent-event CLI (Rust, clap)

You are PASS 1 OF 2. Pass 2 will add the opencode adapter and the filesystem
watcher. Build the smallest thing that compiles, runs, and is honest about
what it does not yet do. Favor plain, obvious code.

**If reality deviates from this brief, STOP and report. Do not improvise.**
**Do not invent scope. Everything not listed in section 4 is out of scope.**

## 0. First action, mandatory

```bash
cd /Users/chrishafley/projects/sprefa-lanes/boop
git merge --ff-only 9b2f8b0fd58e79ce4d2be859f7153d8ab7810da1
```

If that fails, STOP AND REPORT. Do not work around it.

## 1. Files you own

You create and own ONLY new files under:

- `v6/boop/`

Two other lanes are running concurrently and own `v6/prolog/lower.pl`,
`v6/prolog/emit_ts.pl` and `v6/prolog/0_type_plane.pl`. Editing any file
outside `v6/boop/` is a defect. Do NOT touch `v6/justfile`; the coordinator
wires the recipe.

## 2. What this thing is

ETL over agent-harness transcripts. Find the data source, unify it behind one
interface, load and map and save. Nothing more.

The name: the dl6 CLI is `bop`, so this one is `boop`. `boop --help` is the
front door and must be self-describing.

Every agent harness on this machine writes a transcript to disk in its own
format. `boop` reads them, resumes from a byte offset rather than re-reading
whole files, and emits one normalized event stream.

The gap that justifies writing it: the prior art was read in full and NOBODY
TAILS. `agent-session` 0.4.42 (MIT, crates.io, 4,425 lines) re-reads the whole
file with `fs::read_to_string` on every parse (`parser.rs:100`), has no
byte-offset resume (`types.rs:267-280` re-parses in full on mtime change), and
hard-caps at 25 sessions with `limit.clamp(1, 25)` applied TWICE
(`types.rs:233, 263`). Its TYPES are good and worth copying by hand; its
transport is the thing being replaced.

## 3. Build-vs-buy, already decided. Use these crates. Do not relitigate.

| need | chosen | why, versus what |
|---|---|---|
| arg parsing | `clap` 4, `derive` feature | Chosen by the user. `argh`/`bpaf`/`lexopt` are smaller but `clap` derive gives the self-describing `--help`, subcommand help, and shell completions this CLI exists to have. `pico-args` has no help generation at all. |
| json | `serde` + `serde_json` | The transcripts are JSONL. No alternative is credible. |
| errors | `anyhow` for the binary | Application-level, not a library API. Do not add `thiserror` in pass 1. |
| home dir | `dirs` 5 | `directories` adds XDG project-dir machinery this does not need; `home` is cargo-internal-flavored. |
| dir walk | `walkdir` 2 | `glob` cannot skip subtrees; `ignore` drags in gitignore semantics that are wrong for `~/.claude`. |
| file tailing | **std `File` + `seek` + `BufReader`**, nothing else | This is the whole point of the lane. `linemux` and `notify` are for WATCHING (pass 2). Pass 1 does offset-resume reads only. |

Add no dependency that is not in that table. If you believe you need one, STOP
AND REPORT with the reason.

## 4. The exact shape to build

### 4a. Crate

`v6/boop/Cargo.toml`. Follow the pattern of `v6/sprefa-extract/Cargo.toml`:
its own `[workspace]` table (an empty one) so cargo does not walk up into the
v5 root workspace at the repo root.

```toml
[workspace]

[package]
name = "boop"
version = "0.1.0"
edition = "2021"
description = "cross-harness agent transcript reader: tail agent events from every harness on this machine as one stream"
license = "MIT OR Apache-2.0"
publish = false
```

Binary name `boop`.

### 4b. One trait, in `v6/boop/src/harness.rs`

The trait is the whole design. Every harness implements it; the CLI never
knows a harness by name.

```rust
/// One agent harness that writes transcripts to this machine.
pub trait Harness {
    /// Stable short id used in CLI output and as the `--harness` filter value.
    fn id(&self) -> &'static str;

    /// Every session this harness has on disk, newest last. No cap.
    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>>;

    /// Read forward from `offset` bytes. Returns the events decoded and the
    /// new offset to resume from. A partial trailing line is NOT consumed and
    /// NOT counted in the returned offset.
    fn read_from(&self, session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk>;
}
```

`SessionRef` carries at minimum: `harness: &'static str`, `session_id: String`,
`path: PathBuf`, `cwd: Option<String>`, `git_branch: Option<String>`,
`modified_ms: u64`.

`ReadChunk` carries: `events: Vec<AgentEvent>`, `next_offset: u64`.

### 4c. One registry, in `v6/boop/src/registry.rs`

```rust
pub struct Registry {
    harnesses: Vec<Box<dyn Harness>>,
}

impl Registry {
    pub fn discover() -> Self { /* every built-in harness, in id order */ }
    pub fn all(&self) -> &[Box<dyn Harness>];
    pub fn by_id(&self, id: &str) -> Option<&dyn Harness>;
}
```

`Registry::discover` is the ONE place a harness is named. Adding a harness
later must mean adding one line there and one file, nothing else. The CLI
routes to the trait; it must contain zero `match` on harness id.

### 4d. The event type, in `v6/boop/src/event.rs`

Vendor the useful shape from `agent-session`'s `types.rs` by hand. Do not add
it as a dependency. Field set for pass 1, measured from a real 1,277-line
claude transcript:

```rust
pub struct AgentEvent {
    pub harness: &'static str,
    pub session_id: String,
    pub ts_ms: u64,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,   // present on 1,016 of 1,277 records
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub record_type: String,           // assistant | user | system | attachment | ...
    pub tool_name: Option<String>,     // Bash | Read | Edit | Write | Agent | Skill | ...
    pub paths: Vec<ToolPath>,
    pub urls: Vec<String>,
    pub raw_line_offset: u64,
}

pub struct ToolPath {
    pub path: String,
    pub access: Access,               // Read | Write | Create | Delete | Rename
}
```

`parent_uuid` is why this is worth building: the conversation is ALREADY a DAG
on disk and nothing reads it.

Serialize with `serde` so `boop events --format json` emits NDJSON, one event
per line.

### 4e. The claude adapter, in `v6/boop/src/harness/claude.rs`

Sessions live under `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`.
Each line is one JSON record. Fields observed in the corpus, with counts from
one real 1,277-line session:

```
record types: assistant 397, attachment 302, user 261, system 56, mode 55,
              ai-title 55, last-prompt 54, queue-operation 40, pr-link 33,
              file-history-snapshot 24, file-history-delta 10
fields:       sessionId 1253, timestamp 1099,
              cwd / gitBranch / parentUuid / uuid / version 1016,
              toolUseResult 223
tool names:   Bash 180, Read 17 (file_path), Edit 10, Write 6,
              Agent 3 (subagent_type, model), Skill 2 (skill),
              WebFetch 2 (url), ListAgents 1, ToolSearch 1
```

Extract `file_path` from Read/Edit/Write tool inputs into `paths`, and `url`
from WebFetch into `urls`. A record whose shape you do not recognize becomes an
`AgentEvent` with `tool_name: None` and empty `paths`/`urls`; it is NOT an
error and must NOT be dropped.

### 4f. The tailer, in `v6/boop/src/tail.rs`

This is the reason the lane exists. Get it exactly right.

```
step 0  offset=0     seek(0)        read 4096 B  -> 12 complete lines + 37 B partial
                                    next_offset = 0 + (bytes up to last \n)
step 1  offset=4059  seek(4059)     read to EOF  -> 8 complete lines + 0 partial
                                    next_offset = EOF
step 2  offset=EOF   seek(EOF)      read 0 B     -> 0 events, next_offset unchanged
```

Laws:

- A partial trailing line (no terminating `\n`) is never parsed and never
  counted into `next_offset`. The next call re-reads it. A transcript is
  appended to while you read it; a half-written JSON line is normal, not an
  error.
- `next_offset` is a BYTE offset into the file, never a line index.
- If the file is SHORTER than `offset` (truncated or rotated), reset to 0 and
  say so in the returned chunk. Do not panic, do not silently return empty.
- A line that fails to parse as JSON is counted and skipped, not fatal. Report
  the count.

### 4g. The CLI, in `v6/boop/src/main.rs`

```
boop harnesses                      list registered harnesses, one per line
boop sessions [--harness <id>]      id, harness, cwd, branch, modified, size
boop tail <session-id> [--from <offset>] [--format text|json]
boop events [--harness <id>] [--since-ms <n>] [--format text|json]
```

`--format json` emits NDJSON. `--format text` is the default and is for a
human reading a terminal.

Exit codes: 0 success, 2 usage error (clap default), 1 runtime failure.

## 5. Out of scope for pass 1. Do not build these.

- The opencode adapter. Its store is `~/.local/share/opencode/opencode.db`,
  event-sourced, 231,688 rows. It is a SQL tail, not a file tail, and it is
  pass 2.
- The codex and gemini adapters.
- Any filesystem watcher (`notify`, `linemux`).
- Any write path. `boop` is read-only in pass 1.
- Any dl6 emission. The rel shape is designed but not this lane's job.
- Any `v6/justfile` recipe.

## 6. Validation

```bash
cd /Users/chrishafley/projects/sprefa-lanes/boop/v6/boop
cargo build
cargo clippy -- -D warnings
cargo test
cargo run -- --help
cargo run -- harnesses
cargo run -- sessions --harness claude | head -20
```

Write at least these tests, each against a temp file you create in the test,
never against the user's real `~/.claude`:

1. Tail a 3-line file from offset 0. Expect 3 events and `next_offset` == file
   length.
2. Append a 4th COMPLETE line, tail from the previous offset. Expect exactly 1
   event.
3. Append a PARTIAL line (no trailing newline), tail from the previous offset.
   Expect 0 events and an UNCHANGED offset.
4. Complete that partial line, tail again. Expect exactly 1 event.
5. Truncate the file below the stored offset. Expect a reset to 0 and no panic.
6. A line of invalid JSON in the middle. Expect the surrounding lines to parse
   and a reported skip count of 1.

Test 3 and test 5 are the ones that matter. They are the two failures the
prior art has.

The 10-second law applies: any single test over 10s is a defect to
investigate, never a budget to accept.

## 7. Style laws, non-negotiable

- Comments state ONLY constraints the code cannot show. No change-log
  narrative, no dates, no arc references, no restating the next line. The
  partial-line rule in 4f IS such a constraint and deserves a comment; "// loop
  over lines" does not.
- Banned words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
  Also banned: `support` as a noun for a reference count; this repo says
  `refCount`.
- No em dashes.
- Variable names are descriptive, never single letters.
- Type names say what the thing is on first reading.
- No `unwrap()` or `expect()` outside tests.
- Important functions are trait-bound, never bare free functions, per the
  repo's interface law. The `Harness` trait is the point.

## 8. Deliverable

- The crate, committed on branch `lane/boop`.
- `REPORT.md` at the worktree root: the file tree you created with a one-line
  role per file, the literal output of `cargo clippy -- -D warnings`, the
  literal output of `cargo test`, the literal output of `boop --help`, and the
  first 5 lines of `boop sessions --harness claude` run against the real
  machine.

Last action, mandatory (no tmux lane emits a completion event, so this hail IS
the completion signal):

```bash
bus hail --to fable-main --body "boop DONE <pass|fail>: <one line>"
```
