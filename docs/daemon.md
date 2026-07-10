# Daemon mode + the full CLI

One binary, `dl`, dispatches by mode flag. There is no separate `dl-daemon` /
`dl-lsp` / `dl-tray` binary; the same `target/release/dl` runs as a one-shot,
an LSP server, a long-lived daemon, or a menu-bar tray depending on flags and
context. This doc is the reference for the daemon lifecycle and for every CLI
flag, including the ones the README CLI table summarizes.

**The daemon is "everything on."** ONE daemon process lives at a constant home
(`$XDG_STATE_HOME/sprefa`, else `~/.local/state/sprefa`) and serves EVERY
`.dl`-owning root over ONE Unix socket. Each root gets its own warm `Engine` +
SQLite db + file watcher inside that process; cwd only picks WHICH root a query
addresses. Every consumer — an LSP editor (`dl --lsp`), an MCP client
(`dl --mcp`), a CI check (`dl --check`), an agent hook (`dl --hook`), a query —
is a thin adapter that ATTACHES to the singleton over the socket and names its
root in the RPC; they are not modes you choose between. You turn the daemon on
(or a one-shot auto-attaches one) and control it with the `dl daemon <verb>`
subcommand:

| verb | does |
|---|---|
| `dl daemon load prog.dl` | serve a program reactively (starts the daemon if down, registers the cwd root, hot-reloads on edit) |
| `dl daemon status` | is it up? build_id + every registered root with its tick count |
| `dl daemon rows REL` | print a relation's live rows for the cwd root |
| `dl daemon restart` | stop + respawn with the current binary (after `cargo install`) |
| `dl daemon stop` | shut the whole singleton down (every root) |
| `dl daemon drop <root> [--purge]` | deregister one root (`--purge` deletes its db) |
| `dl daemon start [prog]` | detach a background singleton; `--foreground` runs it here (debug) |
| `dl daemon await-settle [--ms N]` | block until the cwd root is quiescent |

Source: [src/cli/](../src/cli/) (flag parsing + dispatch),
[src/daemon.rs](../src/daemon.rs) (singleton + registry + spawn-if-missing
client), [src/rpc.rs](../src/rpc.rs) (the wire codec). Design plan:
[plans/2026-07-10-singleton-daemon-registered-roots.md](../plans/2026-07-10-singleton-daemon-registered-roots.md).

## Why a daemon

A cold `dl` invocation parses the program, scans the tree, and ticks the
fixpoint from an empty db. The singleton keeps a warm `Engine` + SQLite db + file
watcher resident per served root, so a second `dl` (a `--check` hook, an LSP
request, a query) attaches over the socket and reuses the warm tables instead of
cold-ticking. The gradle shape, in one binary — with a registry, so it is ONE
process, not one-per-repo.

## One daemon, registered roots

There is ONE daemon and ONE socket. A root is an addressing KEY carried in each
RPC's `root` field, not a socket selector:

- **Registered root** — any `.dl`-owning directory. Its engine + db live under
  `<home>/roots/<key>/` (`key` = blake3-16hex of the canonical root path). It is
  registered lazily: the first RPC that names it (a one-shot, an LSP attach, `dl
  daemon load`) auto-registers it — **attach IS registration** — cold-ticking its
  `<root>/.dl/*.dl` program inside the daemon while the caller blocks on the reply.
  `roots.json` persists the set; a daemon restart replays it (warm from each db).
- **Config view** — the `root`-absent engine (the org / "folders in view" model).
  It scans nothing and draws its facts from the configured repos.

There is **no `--root` flag**. The addressed root is the cwd's nearest `.dl/`
ancestor (a one-shot resolves it from where it runs); a spawned helper carries it
in the internal `DL_DAEMON_ROOT` env; run a `dl daemon <verb>` where no `.dl/`
ancestor exists to address the config view. Registering a root nested inside — or
containing — an already-registered root is refused loudly (naming both paths), so
one process never double-serves overlapping trees.

Home layout (`$XDG_STATE_HOME/sprefa`, `src/daemon.rs`):

```
<home>/
  daemon.sock          # THE socket, mode 0600
  daemon.pid           # pid\nstart_secs\n
  daemon.log           # one log; lines prefixed [<root basename>]
  roots.json           # [{root, key, added_at}] — registration persistence
  db.sqlite            # the config-view engine db
  roots/<key>/db.sqlite  # one db per registered root
```

- `daemon.sock` — if a deep `$XDG_STATE_HOME` would overrun the OS `sun_path` cap
  (104 bytes on macOS), just the socket relocates to a short hashed path under
  `$TMPDIR/dl-sock/<hash>.sock`; bind and every connect derive it from the same
  home, so they always agree.
- A stale socket from a `kill -9`'d daemon is reaped on the next bind
  (connect-probe, then unlink).
- `<root>/.dl/` keeps only the program (`*.dl`) + content-addressed caches
  (`index.scip`, `perf.jsonl`). It no longer holds a socket, pid, or db — the
  per-root daemon files are retired.

Because the home is `$XDG_STATE_HOME`-rooted, a test that sets `XDG_STATE_HOME`
to a sandbox is hermetic by construction: a leaked test daemon can never bind a
developer's socket (the "disc2" class is structurally impossible).

**Migration from per-root daemons.** An existing `<root>/.dl/db` is NOT imported.
The first singleton attach cold-ticks the root into a fresh
`<home>/roots/<key>/db.sqlite` (the same cold-start any first-register pays), then
stays warm. A stale `<root>/.dl/daemon.sock` / `daemon.pid` left by the old
per-root scheme is inert (nothing binds it anymore); delete it at leisure.

## Lifecycle

- **Spawn-if-missing.** A one-shot in a workspace with a `.dl/` dir attaches to
  the singleton, spawning it detached (`dl daemon serve`, idle timer on) if no
  live socket, then names the workspace root in its RPC (which auto-registers it).
  A workspace WITHOUT `.dl/` stays in-process — a one-off `dl p.dl` in a tempdir
  never spawns a side process (`enabled_for`).
- **Detached by default.** `dl daemon start` backgrounds the singleton and
  returns; `--foreground` runs the daemon body in this process (the debug path,
  idle timer off). Auto-attach uses the same detached spawn.
- **Idle timeout.** The detached daemon exits after every registered root has been
  idle 30 min (per-root eviction — dropping one root's engine while keeping the
  process — is a follow-up; today the roots stay warm until the whole process
  idles out). A watcher batch resets a root's clock only if it survives the gate
  (see below); an RPC touches the root it addressed. Override with
  `DL_DAEMON_IDLE_SECS=N`. `--foreground` ignores the idle timeout.
- **Watcher gate.** The recursive file watcher mirrors the scan corpus: it ticks
  only for files the engine could actually scan (`.gitignore`-honored, `.git`
  pruned to the narrow `HEAD`/`packed-refs`/`refs/` ref paths, the daemon's own
  bookkeeping dropped). Bursts coalesce through a short quiet-period debounce;
  a dropped/overflowed event forces a loud full-corpus recovery tick.
- **Hot-reload.** A `.dl` program edit ALWAYS hot-reloads that root in place
  (re-parse, swap the `Program`, re-tick) — one process exit would kill every
  served root, so the old exit-for-respawn path is gone. Source files
  (`.rs`/`.kt`/...) editing just re-ticks the affected root.
- **Opt out.** `DL_NO_DAEMON=1` (or `--no-daemon`) forces the in-process path —
  no attach, no spawn. Used by tests and when a socket is wedged.
- **Shutdown vs drop.** `dl daemon stop` sends `shutdown` and stops the whole
  singleton (every root). `dl daemon drop <root> [--purge]` deregisters ONE root
  (stops its watcher, closes its engine; `--purge` deletes its db dir) and leaves
  the process serving the rest.

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
- **`dl daemon await-settle`** is the same guarantee against a **running daemon**: you
  do not own the loop (the daemon's poll loop drives it), you block until it
  reports quiescent (the `await_quiescent` RPC), then exit 0 (settled) or 3
  (timed out). Use it to wait out a daemon that is mid-cascade (a fresh pull, a
  demand fetch) before you query.

`--settle` runs in-process and never attaches or spawns a daemon;
`dl daemon await-settle` only talks to an already-running one. Neither is a watch
loop — both return after one converged state.

## Tray (menu bar)

`dl daemon start --tray` runs the singleton with a macOS status-bar icon
(accessory mode: no Dock icon, no cmd-tab entry; `LSUIElement`; `--tray` forces
foreground since it owns the main thread). The socket accept loop moves off-main.
The menu shows the home + registered-root count + the config-view tick count and a
Quit item. Windows/Linux trays are deferred. Source:
[src/tray.rs](../src/tray.rs).

## RPC surface

JSON-RPC 2.0 over `Content-Length`-framed messages (LSP-style framing, same
codec on the local socket and the LSP stdio bridge — see [src/rpc.rs](../src/rpc.rs)).
A client speaking this codec is transport-agnostic.

**The `root` envelope.** Every root-scoped method carries `params.root` = the
absolute root path; the daemon routes it to that root's engine (auto-registering
it on a miss when it owns `.dl/`). `params.root` absent addresses the config view.
`add_root` / `drop_root` / `subscribe` / `shutdown`, and a `ping`/`status` with no
`root`, are process-level. Methods (`handle_request`, `src/daemon.rs`):

| method | params | returns |
|---|---|---|
| `ping` (no root) / `status` | — | process summary: `{build_id, home, config_tick_count, root_count, roots:[{root, key, tick_count, program, settled}], activity}` |
| `ping` (with root) | `{root}` | `{ok, root, key, tick_count, settled, program, program_files, activity}` |
| `add_root` | `{root}` | register + cold-tick a root; `{root, key, tick_count}` (idempotent; nested-root refused) |
| `drop_root` | `{root, purge?}` | deregister a root; `--purge` deletes its db dir |
| `await_quiescent` | `{root, timeout_ms?}` | blocks until that root is quiescent (no non-timer rel moved, no `@next` carry staged, no non-stream effect in-flight) or the timeout elapses; returns `{settled, tick_count}`. The daemon-side twin of `dl --settle` |
| `query` | `{root}` | every `?` query's `{rel, columns, rows}` |
| `query_sql` | `{sql, params[]}` | raw rows against the warm SQLite db |
| `eval` | `{text}` | parse + run an ad-hoc program string; return its `?` results |
| `diag` | `{path?}` | `diag` rows (optionally filtered to one path) |
| `definition` | `{file, text}` | def-target `[file, line]` pairs (LSP go-to-def) |
| `hover` | `{file, text}` | hover markdown |
| `schema` | — | every relation's columns + the backing `_*` source tables |
| `subscribe` | `{events[]}` | registers the open socket for server-sent notifications (e.g. `diag_changed`, one per tick); requires a kept-open connection |
| `load` | `{path, mode}` | `mode="watched"` joins the script to the program (reactive, hot-reloaded); `mode="once"` evals on a throwaway engine and returns `?` results |
| `shutdown` | — | `{ok}`, then the daemon exits |

`dl daemon load <script>` / `dl daemon load-once <script>` are the CLI
front-ends for `load` (both start the singleton first if it is down and register
the cwd root). `load` adds the script to that root's watched set (joins the loaded
program, runs every tick, hot-reloads on edit); `load-once` evals it once, prints
the `?` results, persists nothing. Both target the cwd root (the config view where
no `.dl/` ancestor exists).

## Full flag reference

One-shot / mode flags (there is **no `--root`**; the root is the cwd):

| flag | effect |
|---|---|
| `dl prog.dl` | run once; print `?` queries as TSV |
| `dl` (no positional) | discovery: merge every `<root>/.dl/*.dl` (filename order); auto-cache at `.dl/cache.db` |
| `--db <path>` | persist to SQLite (default in-memory; discovery defaults to `.dl/cache.db`). Derived tables are plain-TEXT `rel_<name>` |
| `--lsp` | LSP server over stdio; `diag` rows become live squiggles. Accepts `--stdio` as an alias (clients append it) |
| `--mcp` | MCP (JSON-RPC over stdio) server; binds the program's `rpc`-class `@in`/`@out` ports |
| `--check` | render `diag` to stderr. Exit 0 clean, 2 on any `error`-severity row (blocking-hook code), 1 broken program |
| `--hook` | Claude Code hook mode: read the event on stdin, emit `inject`/`block` JSON on stdout |
| `--diag-json` | `--check` with diagnostics as a JSON array on stdout |
| `--query-json` | `?` results as JSON-lines `{query, columns, rows, count}` |
| `--parse-only` | parse + typecheck + op resolution, NO scan, NO db (sub-second fast fail) |
| `--settle` | run in-process, draining `@async`/`sh`/`sh*` effects off-tick, until the program QUIESCES (no non-timer rel moves, no `@next` carry pending, no effect in-flight), then print `?` once. Guarantees every cascade ran ≥1×; bails loudly if it cannot settle. See `plans/2026-07-06-settle-quiescence.md` |
| `--settle-max N` | tick budget for `--settle` (default 200); over budget bails, naming the still-moving rels/effects |
| `--watch` | re-tick on file changes (in-process, pre-daemon path) |
| `--changed <path>` | one incremental tick for changed paths (repeatable) |
| `--move OLD=NEW [--repo slug\|*] [--fix]` | file-move refactor; dry-run unless `--fix`. See the README CLI table for the per-language detail |
| `--verify "<cmd>"` | transactional codemod: apply `gen` edits, run `<cmd>`, keep-if-pass else restore + exit 1 |
| `--profile` (or `DL_PROFILE=1`) | log slow SQL, per-repo×rev scan times, tick phase breakdown, per-tick statement counts |
| `--cmd-budget N` (or `DL_CMD_BUDGET`) | cap `cmd` invocations per tick; over budget errors loudly. Default unlimited |
| `--tick-audit` (or `DL_TICK_AUDIT=1`) | after each tick, print every relation's row count |
| `--no-daemon` (or `DL_NO_DAEMON=1`) | force the in-process path; never attach or spawn |

Daemon control is the `dl daemon <verb>` subcommand (addressed root = cwd's
nearest `.dl/` ancestor, or `DL_DAEMON_ROOT` for a spawned helper):

| verb | effect |
|---|---|
| `dl daemon start [prog] [--foreground] [--tray]` | detach a background singleton and register the cwd root; `--foreground` runs the daemon body here (debug, idle off); `--tray` adds the macOS status-bar icon (forces foreground) |
| `dl daemon status` | is it up? `build_id` + every registered root with its tick count. Exit 0 running, 1 not |
| `dl daemon stop` | send `shutdown` and stop the whole singleton (every root) |
| `dl daemon drop <root> [--purge]` | deregister ONE root; `--purge` deletes its db dir |
| `dl daemon restart` | stop + respawn with the current binary (the post-`cargo install` one-liner) |
| `dl daemon load <script>` | register the cwd root + push a script as a WATCHED program (reactive, hot-reloaded); starts the singleton first if down |
| `dl daemon load-once <script>` | eval a script once on a throwaway engine, print `?` results, persist nothing; starts the singleton first if down |
| `dl daemon rows <rel>` | print a relation's current rows for the cwd root |
| `dl daemon await-settle [--ms N]` | block until the cwd root is quiescent (`await_quiescent` RPC), print `settled=<bool> tick=<n>`, exit 0 (settled) or 3 (timed out) |

## Environment variables

| var | effect |
|---|---|
| `DL_NO_DAEMON=1` | opt out of the daemon (same as `--no-daemon`) |
| `DL_DAEMON_IDLE_SECS=N` | override the 30-min idle timeout |
| `DL_PROFILE=1` | profile mode (same as `--profile`) |
| `DL_PROFILE_SQL_MS=N` | slow-SQL threshold in ms (default 25) |
| `DL_CMD_BUDGET=N` | per-tick `cmd` budget (same as `--cmd-budget`) |
| `DL_TICK_AUDIT=1` | per-tick row-count audit (same as `--tick-audit`) |
| `XDG_STATE_HOME` | base for the singleton daemon home (`$XDG_STATE_HOME/sprefa`); tests point it at a sandbox for hermeticity |
| `DL_DAEMON_ROOT` | the root a spawned helper / `dl daemon <verb>` addresses (overrides the cwd walk) |
| `SPREFA_SCIP_INDEX` | path to an `index.scip` to ingest into `scip_*` relations |
