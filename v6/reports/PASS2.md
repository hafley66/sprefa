# LANE boop — PASS 2. The scope was too narrow. `boop` is 1-1 with `bus`.

Pass 1 built the transcript tailer. Keep it; it becomes layer 2 of four. This
document is the rest of the design and the research behind every dependency
choice, so you make zero of those calls yourself.

## 0. FIRST: finish pass 1. It ended mid-turn and never committed.

Your previous turn stopped while applying an edit to
`v6/boop/src/harness/claude.rs`. Verified by the coordinator on the worktree:

- `cargo build` SUCCEEDS. The binary is fine.
- `cargo test` FAILS to compile with 2 errors, both the same cause, both in the
  test module of `src/harness/claude.rs` at lines 307 and 324:

```
error[E0599]: no method named `read_from` found for struct `Claude` in the current scope
help: trait `Harness` which provides `read_from` is implemented but not in scope
     use crate::harness::Harness;
```

Add that `use` to the test module. That is the whole fix.

- There is NO commit. `git log` is still at `9b2f8b0f` and `v6/boop/` is
  untracked.
- There is NO `REPORT.md`.
- `v6/boop/target/` exists and must never be committed. Add
  `v6/boop/.gitignore` containing `/target`.

Do all of that, get `cargo test` and `cargo clippy -- -D warnings` green, and
COMMIT before starting any section below. A pass that ends with nothing
committed is a lost pass.

**If reality deviates from THIS document, STOP and report.**
**Add no dependency outside the table in section 3. If you think you need one,
STOP and report the reason.**

## 1. What changed

`boop` is the Rust replacement for `bus`, one verb for one verb, plus the two
things `bus` cannot do: read what an agent actually did, and measure what its
processes cost. `bus` today is `~/projects/instant/scripts/bus.ts`, 532 lines
of node, and it shells out to `tmux` in 37 places with hand-built argv arrays.

tmux control is the mechanism, not an implementation detail to hide. Do not
invent a process-spawning abstraction that could be swapped for something else
later. tmux IS the transport.

## 2. The four layers. Keep them as four modules; do not merge them.

| layer | module | owns | knows about |
|---|---|---|---|
| 0 process | `src/proc.rs` | pid liveness, ppid tree, per-process cpu and rss, cwd | the OS only |
| 1 control | `src/tmux.rs` | tmux sessions, panes, send-keys, control-mode stream | tmux only |
| 2 transcript | `src/harness/` | the `Harness` trait, session discovery, byte-offset tailer, `AgentEvent` | files and DBs only |
| 3 identity | `src/ident.rs` | the id maps in section 5, the spawn tree, the message DAG | layers 0-2 |

The CLI in `src/main.rs` routes to layers. It contains no `match` on harness
id and no direct `Command::new("tmux")`.

Layer 0 must not know tmux exists. Layer 1 must not know what a transcript is.
A lane is alive at layer 1 (its tmux session exists) OR at layer 0 (its pid is
alive) and those are different questions with different answers; keep both and
report both.

## 3. Build-vs-buy. Researched, decided. Use exactly these.

Numbers pulled from the crates.io API on 2026-08-08.

| need | verdict | crate | receipts and the candidates it beat |
|---|---|---|---|
| process tree, cpu, rss, cwd | **BUY** | `sysinfo` 0.39.6 | 180,161,141 downloads total, 41,315,468 recent, updated 2026-07-09. Cross-platform in one API. Beat: `procfs` (Linux only), `libproc` (macOS only), `systemstat` (no process tree), `heim` (abandoned). |
| tmux command construction | **BUY** | `tmux_interface` 0.4.0 | 149,170 downloads, 62,539 recent, updated 2026-03-10, MIT, `AntonGepting/tmux-interface-rs`. A typed command builder over one-shot tmux CLI calls. Beat: `tmux-lib` 0.5.0 at 6,313 downloads total and 312 recent, too thin to depend on; and hand-building argv, which is what `bus.ts` does 37 times today. |
| tmux **control mode** | **BUILD**, and only this | none exists | See section 4. `tmux_interface`'s README describes it as "communication with TMUX via CLI" and documents no `-C`/`-CC` support, no `%begin`/`%end` guard parsing, no `%output` stream. No crate on crates.io sells a control-mode client. This is the one legitimate build and the reason `boop` exists in Rust. |
| arg parsing | BUY | `clap` 4 derive | 1,028,213,686 downloads, updated 2026-08-06. |
| json | BUY | `serde`, `serde_json` | transcripts are JSONL. |
| dir walk | BUY | `walkdir` 2.5 | 553,769,188 downloads. |
| home dir | BUY | `dirs` 5 | |
| errors | BUY | `anyhow` | binary, not a library API. |
| file tailing | BUILD (done in pass 1) | std `File` + `seek` | the prior art re-reads whole files; see pass 1 brief. |

### The in-house receipt for buying `sysinfo`

`~/projects/cate-local` built this by hand and paid for it. It has
`src/runtime/capabilities/procfs.ts`, a hand-written `/proc/<pid>/stat` parser
(note its comment about anchoring on the LAST `)` because comm can contain
parens), plus a separate `ps -axo pid=,ppid=,comm=` fork path in
`process.ts:41` for macOS, plus `lsof -a -d cwd -p <pid> -Fn` for cwd at
`process.ts:84`, plus another `lsof -iTCP -sTCP:LISTEN` for ports at
`process.ts:424`. Its own header records why the Linux path exists:

> Forking `ps`/`lsof` ~1.6x/sec from the Electron main process stalls the event
> loop on Linux ... blocks the main thread 50-175ms ... (issue #246).

Two platform implementations, three subprocess spawns, one measured
performance incident. `sysinfo` is that surface, bought. Do not reimplement any
part of it.

## 4. Control mode: what you are building and the exact protocol

`tmux -C` (or `-CC`) attaches a client that speaks a text protocol on stdio.
You spawn ONE long-lived child and keep it, instead of forking a tmux process
per question the way `bus.ts` does.

Protocol, from the tmux wiki:

- You write a normal tmux command line to the child's stdin.
- The reply is wrapped in guard lines: `%begin <ts> <cmd-number> <flags>`, then
  the command's output, then `%end <ts> <cmd-number> <flags>` on success or
  `%error <ts> <cmd-number> <flags>` on failure. Match replies to commands by
  the command number, never by arrival order alone.
- Asynchronous notifications arrive interleaved, each on its own line starting
  with `%`: `%output`, `%extended-output`, `%window-add`, `%window-close`,
  `%window-renamed`, `%window-pane-changed`, `%session-changed`,
  `%session-renamed`, `%sessions-changed`, `%pane-mode-changed`, `%pause`,
  `%continue`, `%subscription-changed`, `%exit`.
- `-C` leaves the terminal in canonical mode with echo on and is for testing.
  `-CC` disables canonical mode, emits the DSC sequence `\033P1000p` on entry
  so a terminal can detect control mode, and sends `%exit` on client shutdown.
  **Use `-C`.** `boop` is not a terminal emulator and must not touch terminal
  attributes.

Build in `src/tmux.rs`:

```rust
pub trait TmuxControl {
    /// Send one tmux command, block for its %begin/%end block, return the body.
    fn command(&mut self, argv: &[&str]) -> anyhow::Result<Vec<String>>;
    /// Notifications received since the last drain, in arrival order.
    fn drain_notifications(&mut self) -> Vec<Notification>;
}
```

Laws:

- A `%error` block is a returned `Err`, never a panic and never a silent empty
  result.
- An unknown `%`-prefixed line is kept as `Notification::Unknown(String)` and
  never dropped. tmux adds notification types across versions.
- If the child dies, say so as an error naming the tmux exit status. Do not
  respawn silently.
- **`tmux` being unreachable is not the same as "no sessions."**
  `bus.ts:113-118` already gets this right and its comment says so; preserve
  the distinction. `boop prune` must refuse to run when tmux is unreachable,
  exactly as `bus.ts:502` does, because it cannot tell live from dead.

Use `tmux_interface` to BUILD the argv for each command. Use your own control
client to SEND it. That split is the whole design.

## 5. The identity model. This is what layer 3 owns.

Every plane issues its own id and none of them join automatically. Owning this
table is the point of `boop`.

| plane | example | issued by | shape | how it joins |
|---|---|---|---|---|
| tmux session name | `catalogdecls` | coordinator, `--tmux` | flat, unique per tmux server | registry row, pane pid |
| lane name | `catalogdecls` | coordinator, `--name` | flat, unique in registry | tmux name + cwd |
| opencode session id | `ses_01c485cb7ffef38Y47EHTtgUcL` | opencode | one per run, resumable via `-s` | `opencode.db`.`session.directory` LIKE the lane cwd |
| claude session id | `82caf8ca-5a1c-42c2-a12e-223283129cb9` | claude | one per session file | `~/.claude/projects/<encoded-cwd>/<id>.jsonl` |
| record uuid | `uuid` per jsonl line | harness | **DAG** via `parent_uuid` | within one session file |
| bus message id | `m-82f57708` | bus | flat, `from`/`to` are lane names | registry |
| pid | 12345 | OS | **tree** via ppid | tmux pane pid, `sysinfo` |

Two of these are graphs and the rest are flat. `parent_uuid` is on 1,016 of
1,277 records in a real claude transcript, so the conversation is already a DAG
on disk and nothing reads it. That is the asset.

`cwd` is the only join key that reaches across all three of lane, harness
session, and process. Treat it as such: normalize it once (canonicalize the
path, resolve symlinks) and key on the normalized form everywhere.

### Storage

One SQLite file at `~/.agent/boop.db`. Apply the repo's surrogate-key law
without exception: **stored rels key on INTEGER ids; every natural or
composite TEXT key lives ONCE in a dictionary table with a UNIQUE constraint on
the natural key.** A composite TEXT PRIMARY KEY is a defect. Read
`.claude/skills/sql-relational-design` and `.claude/skills/sqlite-costs` in
the sprefa repo BEFORE writing any DDL. Measured on this machine: TEXT keys run
1.7-2.0x slower on identical tables because every index copies the full key.

So: a `session` dictionary table mapping the harness's TEXT session id to an
INTEGER `session_id`, and every event, edge, and message row carries the
integer. Same for `path`, for `cwd`, and for `tool_name`.

Minimum tables:

```
session(session_id INTEGER PK, harness_id INTEGER, natural_id TEXT UNIQUE, cwd_id INTEGER, branch_id INTEGER, first_ts_ms, last_ts_ms)
session_edge(parent_session_id INTEGER, child_session_id INTEGER, relation)   -- the spawn tree
message(message_id INTEGER PK, session_id INTEGER, uuid_id INTEGER, ts_ms, record_type_id INTEGER, byte_offset)
message_edge(parent_message_id INTEGER, child_message_id INTEGER)             -- the parentUuid DAG
tool_touch(message_id INTEGER, path_id INTEGER, access)                       -- the "what did the agent read" answer
```

`session_edge` is where the open question lands: **is a subagent a session?**
claude says no, opencode and codex say yes. Do NOT resolve that by picking one.
Model it in `session_edge.relation` with distinct values (`spawned`, `resumed`,
`subagent`) so the harnesses' disagreement is data rather than a lost
distinction. Report which relation values each adapter actually produces.

## 6. The 1-1 verb map. `bus` has 8 verbs; `boop` has the same 8 plus 4.

Read `~/projects/instant/scripts/bus.ts` in full before writing any of these.
It is 532 lines and it is the specification.

| bus verb | bus.ts line | boop | notes |
|---|---|---|---|
| `dispatch` | 128 | `boop dispatch` | `--cmd` mandatory, ALWAYS `tmux new-session`, dies on `duplicate session: <name>` |
| `resolve` | 203 | `boop resolve` | |
| `hail` | 254 | `boop hail` | send-keys injection into a live pane |
| `sweep` | 296 | `boop sweep` | |
| `list` | 376 | `boop list` | the live/dead column comes from `tmux list-sessions` |
| `lane` | 408 | `boop lane` | register AND spawn; the first-contact verb |
| `adopt` | 466 | `boop adopt` | rewrites registry metadata only, never spawns |
| `prune` | 498 | `boop prune` | refuses when tmux is unreachable |
| — | — | `boop sessions` | pass 1, keep |
| — | — | `boop tail` | pass 1, keep |
| — | — | `boop events` | pass 1, keep |
| — | — | `boop measure` | NEW: layer 0. per-lane pid, rss, cpu, uptime, child count |

Registry compatibility is REQUIRED. `bus` stores `registry.json` in
`~/.agent/mail` (`bus.ts:27`, `:102-110`) and mailbox files beside it. `boop`
reads and writes the SAME files in the SAME shape, so both tools can run
against one registry during the changeover. Do not invent a new registry
format. Do not migrate anything. If `registry.json` is invalid JSON, print the
path and exit non-zero, exactly as `bus.ts:108` does; never silently reset it.

## 7. Out of scope for pass 2

- The opencode adapter's SQL tail. Pass 3.
- codex and gemini adapters.
- Any write into a harness's own files. `boop` is read-only toward transcripts.
- Any dl6 emission.
- Any `v6/justfile` recipe. The coordinator wires it.
- Replacing `bus`. Both run side by side until `boop` is proven.

## 8. Validation

```bash
cd /Users/chrishafley/projects/sprefa-lanes/boop/v6/boop
cargo build
cargo clippy -- -D warnings
cargo test
cargo run -- --help
cargo run -- list          # must print the SAME lanes `bus list` prints
cargo run -- measure
```

`boop list` and `bus list` must agree on lane names and live/dead. Run both,
paste both outputs side by side in the report. A disagreement is a defect.

Control-mode tests, each against a tmux session your test creates with a
random name and kills in teardown. Never touch a session you did not create,
and never touch the sessions named `boop`, `catalogdecls`, `typeplanedupe`, or
`fable-main`; those are live agents including the one that wrote this brief.

1. `command(["list-sessions", "-F", "#{session_name}"])` returns the created
   session name inside one `%begin`/`%end` block.
2. A failing command (`list-sessions -t nope`) returns `Err` from `%error`, not
   a panic and not an empty Ok.
3. Two commands issued back to back match their replies by command number.
4. An unknown `%`-prefixed line parses to `Notification::Unknown` and is not
   dropped.
5. tmux unreachable (point at a nonexistent `-L` socket) is distinguishable
   from zero sessions.

The 10-second law applies: any single test over 10s is a defect to
investigate, never a budget to accept.

## 9. Style laws, unchanged from pass 1

- Comments state ONLY constraints the code cannot show. The `%error`-is-an-Err
  rule and the tmux-unreachable-is-not-empty rule are such constraints. "spawn
  tmux" is not.
- Banned in prose AND identifiers: `provenance`, `substrate`, `load-bearing`,
  `regime`. Use source/origin, base layer, critical, mode.
- No em dashes. Descriptive names, never single letters.
- No `unwrap()` or `expect()` outside tests.
- Important functions are trait-bound. `Harness`, `TmuxControl`, and the layer
  0 process reader are all traits.

## 10. Deliverable

Commit on `lane/boop`. `REPORT.md` at the worktree root:

- the module tree with the layer number and one-line role per file
- `cargo clippy -- -D warnings` output, literal
- `cargo test` output, literal
- `boop list` and `bus list` outputs side by side
- `boop measure` output against the real machine
- which `session_edge.relation` values your claude adapter actually produced
- every place `bus.ts` does something you could not reproduce, with its line
  number

Last action, mandatory:

```bash
bus hail --to fable-main --body "boop PASS2 DONE <pass|fail>: <one line>"
```
