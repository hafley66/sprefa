# Daemon mode + the full CLI

One binary, `dl`, dispatches by mode flag. There is no separate `dl-daemon` /
`dl-lsp` / `dl-tray` binary; the same `target/release/dl` runs as a one-shot,
an LSP server, a long-lived daemon, or a menu-bar tray depending on flags and
context. This doc is the reference for the daemon lifecycle and for every CLI
flag, including the ones the README CLI table summarizes.

Source: [src/main.rs](../src/main.rs) (flag parsing + dispatch),
[src/daemon.rs](../src/daemon.rs) (daemon + spawn-if-missing client),
[src/rpc.rs](../src/rpc.rs) (the wire codec). Design plan:
[plans/2026-06-21-daemon-and-menu-bar.md](../plans/2026-06-21-daemon-and-menu-bar.md).

## Why a daemon

A cold `dl` invocation parses the program, scans the tree, and ticks the
fixpoint from an empty db. The daemon keeps a warm `Engine` + SQLite db + file
watcher resident in one process per workspace, so a second `dl` (a `--check`
hook, an LSP request, a query) attaches over a Unix socket and reuses the warm
tables instead of cold-ticking. The gradle shape, in one binary.

## Two kinds of daemon

| kind | home | socket | serves | started by |
|---|---|---|---|---|
| **per-root** | `<root>/.dl/` | `<root>/.dl/daemon.sock` | one workspace root | spawn-if-missing on any one-shot when `<root>/.dl/` exists, or `dl --daemon --root <dir>` |
| **rootless serving** | `$XDG_STATE_HOME/sprefa` (else `~/.local/state/sprefa`) | `<home>/daemon.sock` | the config-folder repos, no project root | `dl --daemon` (no `--root`) |

`--root` is an **option** for the daemon and for `--stop` / `--load`: omit it to
target the singleton rootless serving daemon at the XDG home; pass it to target
that root's per-root daemon. Every other mode resolves a concrete root (explicit
`--root` wins, else nearest `.git` ancestor of the program file, else cwd).

Discovery files written under a daemon home (`src/daemon.rs:8`):

- `daemon.sock` — Unix domain socket, mode `0600`. A deeply-nested root whose
  `<root>/.dl/daemon.sock` path would overrun the OS `sun_path` cap (104 bytes on
  macOS) relocates just the socket to a short hashed path under
  `$TMPDIR/dl-sock/<hash>.sock`; bind and every connect derive it from the same
  root, so they always agree. The pid/log/cache files stay root-local.
- `daemon.pid` — text: `pid\nstart_secs\nprogram_path\n`. A stale socket from a
  `kill -9`'d daemon is reaped on the next bind (connect-probe, then unlink).

## Lifecycle

- **Spawn-if-missing.** A one-shot in a workspace with a `.dl/` dir attaches to
  the per-root daemon, spawning `dl --daemon --root <X>` detached if no live
  socket. A workspace WITHOUT `.dl/` stays in-process — a one-off `dl p.dl` in a
  tempdir never spawns a side process (`enabled_for`, `src/daemon.rs:1091`).
- **Idle timeout.** A spawned daemon exits after 30 min idle. A watcher batch
  resets the clock only if it survives the gate (see below) — pure noise
  (gitignored build output, `.git/objects` churn) no longer keeps the daemon
  awake — as does any RPC. Override with `DL_DAEMON_IDLE_SECS=N`. A foreground
  `dl --daemon` ignores the idle timeout (it stays up for debugging).
- **Watcher gate.** The recursive file watcher mirrors the scan corpus: it ticks
  only for files the engine could actually scan (`.gitignore`-honored, `.git`
  pruned to the narrow `HEAD`/`packed-refs`/`refs/` ref paths, the daemon's own
  bookkeeping dropped). Bursts coalesce through a short quiet-period debounce;
  a dropped/overflowed event forces a loud full-corpus recovery tick.
- **Hot-reload vs respawn.** A `.dl` program edit normally exits the daemon for a
  cold respawn (re-parse from scratch). Tray-driven daemons and the rootless
  serving daemon hot-reload in place instead (no startup program to respawn
  from). Source files (`.rs`/`.kt`/...) editing just re-ticks.
- **Opt out.** `DL_NO_DAEMON=1` (or `--no-daemon`) forces the in-process path —
  no attach, no spawn. Used by tests and when a socket is wedged.
- **Shutdown.** `dl --stop` sends `shutdown` over the socket and waits for the
  daemon to close it. `--root` selects which daemon; omitted = the rootless one.

## Running effectful programs to completion (`--settle`)

A plain one-shot `dl prog.dl` (no daemon) runs **exactly one tick**. That is
correct for a pure query program, but several kinds of program need more than one
tick and an off-tick effect drain to produce their answer:

| program shape | why one tick is not enough |
|---|---|
| `@async` / `sh` / `sh*` effect | the request lands in `pending_effect` queued; the subprocess runs OFF-tick, and the response only surfaces on a later tick |
| demand hop (`scip_want`, `rev_behind`) | the want derives on tick 1, the index/compare lands tick 2, a consumer reads it tick 3 |
| `repo`-sink pull | the repo is cloned + registered post-fixpoint; it is scannable only on the NEXT tick |
| `@next` carry | a staged row surfaces as the live rel one generation later |

For a long time the effect runtime (`drain_effects` / `drain_streams`) ran **only
inside the daemon's poll loop**, so an effectful program had no non-daemon way to
finish: a bare `dl prog.dl` left its requests stuck `queued`. `--settle` closes
that gap.

- **`dl --settle prog.dl`** drives `tick → drain_effects → drain_streams` in a
  loop, in-process, until the program is **quiescent**, then prints `?` once. It
  is the CI / script / test path for effectful and demand-tier programs — the
  org-scale one-shot that actually completes (`scip_want` fetches, `pin-skew`
  demand chains, `discover-orgs` pulls, `flow-services` spec reads).
- **Quiescent** means, for one tick: no non-timer relation moved, no `@next`
  carry is staged, and no non-stream effect is in-flight. The recurring timers
  (`every`, `clock`, and `@stream` subscriptions) are steady state and are
  **excluded** — so a polling program still settles at a quiet point instead of
  looping forever.
- **Bounded.** `--settle-max N` (default 200) caps the loop; a program that keeps
  changing every tick (a `@next` counter, a poll whose args never stabilize) is
  reported as non-convergent — it bails loudly, naming the relations/effects
  still moving, rather than hanging.
- **`dl --await-settle`** is the same guarantee against a **running daemon**: you
  do not own the loop (the daemon's poll loop drives it), you block until it
  reports quiescent (the `await_quiescent` RPC), then exit 0 (settled) or 3
  (timed out). Use it to wait out a daemon that is mid-cascade (a fresh pull, a
  demand fetch) before you query.

`--settle` runs in-process and never attaches or spawns a daemon; `--await-settle`
only talks to an already-running one. Neither is a watch loop — both return after
one converged state.

## Tray (menu bar)

`dl --tray` implies `--daemon` and adds a macOS status-bar icon (accessory mode:
no Dock icon, no cmd-tab entry; `LSUIElement`). The tray event loop owns the main
thread; the socket accept loop moves off-main. The menu shows the workspace +
tick count and a Quit item. When `--tray` is set with no `--root`, the daemon
walks up from cwd to the nearest `.dl/` dir and uses its parent as the workspace
root. Windows/Linux trays are deferred. Source: [src/tray.rs](../src/tray.rs).

## RPC surface

JSON-RPC 2.0 over `Content-Length`-framed messages (LSP-style framing, same
codec on the local socket and the LSP stdio bridge — see [src/rpc.rs](../src/rpc.rs)).
A client speaking this codec is transport-agnostic. Methods (`handle_request`,
`src/daemon.rs:810`):

| method | params | returns |
|---|---|---|
| `ping` | — | `{ok, root, tick_count, settled, program, program_files}` (`settled` = last full tick left the program quiescent) |
| `await_quiescent` | `{timeout_ms?}` | blocks until the program is quiescent (no non-timer rel moved, no `@next` carry staged, no non-stream effect in-flight) or the timeout elapses; returns `{settled, tick_count}`. The daemon owns the effect runtime, so this is the daemon-side twin of `dl --settle` |
| `status` | — | root, program, tick_count, subscriber count, `_program`/`_repo`/`_ref` rows, last 50 ref advances |
| `query` | — | every `?` query's `{rel, columns, rows}` |
| `query_sql` | `{sql, params[]}` | raw rows against the warm SQLite db |
| `eval` | `{text}` | parse + run an ad-hoc program string; return its `?` results |
| `diag` | `{path?}` | `diag` rows (optionally filtered to one path) |
| `definition` | `{file, text}` | def-target `[file, line]` pairs (LSP go-to-def) |
| `hover` | `{file, text}` | hover markdown |
| `schema` | — | every relation's columns + the backing `_*` source tables |
| `subscribe` | `{events[]}` | registers the open socket for server-sent notifications (e.g. `diag_changed`, one per tick); requires a kept-open connection |
| `load` | `{path, mode}` | `mode="watched"` joins the script to the program (reactive, hot-reloaded); `mode="once"` evals on a throwaway engine and returns `?` results |
| `shutdown` | — | `{ok}`, then the daemon exits |

`--load <script>` / `--load-once <script>` are the CLI front-ends for `load`.
`--load` adds the script to the running daemon's watched set (joins the loaded
program, runs every tick, hot-reloads on edit); `--load-once` evals it once,
prints the `?` results, persists nothing. Both target the rootless serving
daemon when `--root` is omitted.

## Full flag reference

| flag | effect |
|---|---|
| `dl prog.dl` | run once; print `?` queries as TSV |
| `dl` (no positional) | discovery: merge every `<root>/.dl/*.dl` (filename order); auto-cache at `.dl/cache.db` |
| `--root <dir>` | source root (default: nearest `.git` ancestor of the program; cwd in discovery). An OPTION for `--daemon`/`--stop`/`--load`: omitted = rootless serving daemon |
| `--db <path>` | persist to SQLite (default in-memory; discovery defaults to `.dl/cache.db`). Derived tables are plain-TEXT `rel_<name>` |
| `--lsp` / `--stdio` | LSP server over stdio; `diag` rows become live squiggles. `--stdio` is a no-op alias clients append |
| `--check` | render `diag` to stderr. Exit 0 clean, 2 on any `error`-severity row (blocking-hook code), 1 broken program |
| `--diag-json` | `--check` with diagnostics as a JSON array on stdout |
| `--query-json` | `?` results as JSON-lines `{query, columns, rows, count}` |
| `--settle` | run in-process, draining `@async`/`sh`/`sh*` effects off-tick, until the program QUIESCES (no non-timer rel moves, no `@next` carry pending, no effect in-flight), then print `?` once. Guarantees every cascade ran ≥1×; bails loudly if it cannot settle. See `plans/2026-07-06-settle-quiescence.md` |
| `--settle-max N` | tick budget for `--settle` (default 200); over budget bails, naming the still-moving rels/effects |
| `--watch` | re-tick on file changes (in-process, pre-daemon path) |
| `--changed <path>` | one incremental tick for changed paths (repeatable) |
| `--move OLD=NEW [--repo slug\|*] [--fix]` | file-move refactor; dry-run unless `--fix`. See the README CLI table for the per-language detail |
| `--verify "<cmd>"` | transactional codemod: apply `gen` edits, run `<cmd>`, keep-if-pass else restore + exit 1 |
| `--profile` (or `DL_PROFILE=1`) | log slow SQL, per-repo×rev scan times, tick phase breakdown, per-tick statement counts |
| `--cmd-budget N` (or `DL_CMD_BUDGET`) | cap `cmd` invocations per tick; over budget errors loudly. Default unlimited |
| `--tick-audit` (or `DL_TICK_AUDIT=1`) | after each tick, print every relation's row count |
| `--daemon` | run the long-lived daemon in the foreground (logs to stderr, ignores idle timeout). Usually spawned internally; explicit is the debug path |
| `--tray` | spawn the menu-bar tray icon (implies `--daemon`; macOS only today) |
| `--stop` | send `shutdown` to the daemon (`--root` selects which; omitted = rootless) and exit |
| `--await-settle [--await-settle-ms N]` | block on the running daemon until the program is quiescent (the `await_quiescent` RPC), print `settled=<bool> tick=<n>`, exit 0 (settled) or 3 (timed out). The daemon-side twin of `--settle` |
| `--no-daemon` (or `DL_NO_DAEMON=1`) | force the in-process path; never attach or spawn |
| `--load <script>` | push a script to the running daemon as a WATCHED program (reactive, hot-reloaded) |
| `--load-once <script>` | eval a script once on a throwaway engine, print `?` results, persist nothing |

## Environment variables

| var | effect |
|---|---|
| `DL_NO_DAEMON=1` | opt out of the daemon (same as `--no-daemon`) |
| `DL_DAEMON_IDLE_SECS=N` | override the 30-min idle timeout |
| `DL_PROFILE=1` | profile mode (same as `--profile`) |
| `DL_PROFILE_SQL_MS=N` | slow-SQL threshold in ms (default 25) |
| `DL_CMD_BUDGET=N` | per-tick `cmd` budget (same as `--cmd-budget`) |
| `DL_TICK_AUDIT=1` | per-tick row-count audit (same as `--tick-audit`) |
| `XDG_STATE_HOME` | base for the rootless daemon home (`$XDG_STATE_HOME/sprefa`) |
| `SPREFA_SCIP_INDEX` | path to an `index.scip` to ingest into `scip_*` relations |
