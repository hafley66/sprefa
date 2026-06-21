# Daemon mode + menu bar (cross-platform)

Date: 2026-06-21. Status: PLAN.

One binary (`dl`), one long-lived daemon per workspace, file watchers
running inside it, one menu-bar/status-tray item per platform. Subsequent
`dl` invocations attach to the warm daemon instead of cold-ticking. The
gradle shape, in one Rust binary, with a Mac-first tray UX that also
runs on Windows and Linux.

Explicitly NOT the v4 shape: v4 split `sprefa-daemon` (HTTP/axum),
`sprefa-lsp` (tower-lsp + SprfClient), `sprefa-run` (CLI) into separate
bins. v5 stays one bin and dispatches by mode flag, the way it does today.

## Vision

The end goal is one UI that is the **ultimate code-interaction helper**
for both a human reading the codebase and an AI agent acting in it.
The daemon is the warm state; the UI is every lens onto that state.
The dashboard panels below (active files, request log, db explorer,
resolved paths) are the *operational* layer: "what is the daemon doing,
what does it know, what has it done." The same UI also hosts the
*analytical* layer: hotspots (churn x complexity), blast radius
(transitive reach from a fn/type/module), call/type/module graphs,
missing-repo coverage, doc-coverage gaps, AI-line attribution. Those
analytical views already exist as one-shot `.dl` programs (`glean.dl`,
`typegraph.dl`, `lint-docs.dl`, `examples/rails.dl`); the UI is where
they become live, scrubable, side-by-side instead of TSV prints.

Both human and AI drive the same surface. The AI's `--check` rail
failures, the human's hover/def_target jumps, and the daemon's tick
history are three views of one underlying interaction log. The UI is
the place to read all three.

The host for this UI is **anim** (today: `v5/anim/` for the explainer
deck, plus a sibling `~/projects/anim/` with the atlas/workbench graph
shells). anim is the React/Vite/d2 stack already in the tree; the
dashboard and analytical views become anim modes, not a separate web
app. The dl binary opens the dashboard by spawning anim's dev server
(in dev) or loading a bundled build (in release); anim talks to the
daemon over the same local socket the CLI uses.

## The commitment

1. **One binary.** `dl` plus mode flags. No `dl-daemon`, `dl-lsp`, `dl-tray`
   binaries. The same `target/release/dl` runs in foreground, daemon, or
   tray form depending on flags and context.
2. **Daemon is the source of truth.** Warm db, parse cache, file watcher,
   tick loop all live in one process per workspace root. CLI/LSP/check
   are clients of that process. No client opens the db directly.
3. **Menu bar / status tray, not Dock or cmd-tab.** On macOS the daemon
   runs as `LSUIElement = true` (accessory): menu bar icon top-right, no
   Dock icon, no app-switcher entry. Windows and Linux get their native
   tray equivalent. The tray is the always-present UI surface.
4. **Cold start once, then warm.** First invocation per workspace spawns
   the daemon (sub-second delay). Every invocation after attaches via
   socket and returns in tens of ms.
5. **A dashboard window behind the tray icon.** Click the status item
   (or run `dl --dashboard`) to open a window with three panels:
   active files, request log, db explorer. The tray is the persistent
   handle; the dashboard is the read surface when you want to see what
   the daemon is doing. Same daemon, same socket, no second process.

## Current state (v5, as of 2026-06-21)

| piece | state | where |
|---|---|---|
| single binary, mode dispatch | landed | `src/main.rs:100-117` |
| `--watch` foreground watcher | landed, in-process only | `src/lib.rs:194-263` (`notify` crate, 150ms debounce, recursive on root + config + git dir) |
| `--lsp` stdio server | landed, separate process | `src/lib.rs:190` → `src/lsp.rs` |
| `--changed` incremental tick | landed | `src/lib.rs:177`, `engine.rs:1418` (`tick_paths`) |
| file watcher speaks to LSP | no | `--lsp` does saves only; watcher runs in `--watch` only |
| daemon process holding warm cache | no | every invocation is cold |
| IPC transport (socket / pipe) | no | `Cargo.toml` has no tokio / axum / hyper / `interprocess` / `tray-icon` |
| tray / status item | no | |
| daemon discovery (PID + socket file) | no | |
| `.app` bundle for macOS accessory mode | no | |

The increment to gradle semantics rides entirely on the watcher that
already exists (`lib.rs:194`). Same loop, longer-lived process, plus
one IPC channel.

## Architecture

### Process model

```
┌────────────────────────────────────────────────────────────────┐
│  dl --daemon (one per workspace root, detached, tray-bearing)   │
│                                                                 │
│   ┌────────────┐   ┌────────────────┐   ┌──────────────────┐   │
│   │ notify     │──▶│ debounced tick │──▶│ Engine + SQLite  │   │
│   │ watcher    │   │ (existing loop │   │ (warm: facts,    │   │
│   │ thread     │   │  in run_watch) │   │  parse cache,    │   │
│   └────────────┘   └────────────────┘   │  derived tables) │   │
│                       │                  └──────────────────┘   │
│                       ▼ events           │  ▲                    │
│   ┌────────────────────────────┐         │  │ reads              │
│   │ event ring buffer (recent) │◀────────┘  │                    │
│   │ - ticks (full / paths)     │            │                    │
│   │ - requests (cli / lsp /    │            │                    │
│   │   watcher / manual)        │            │                    │
│   │ - durations, rows, files   │            │                    │
│   │ - resolved paths           │            │                    │
│   └────────────────────────────┘            │                    │
│   ┌────────────────────────────────────┐    │                    │
│   │ reqlog jsonl spill (persistent)    │    │                    │
│   │ - cross-session interaction history│    │                    │
│   └────────────────────────────────────┘    │                    │
│   ┌─────────────────────┐    ┌──────────────────────────────┐   │
│   │ tray-icon (menu bar)│    │ RPC listener (UDS / pipe)    │   │
│   │ - status: warm/idle │    │ - query / diag / ping / stop │   │
│   │ - program path      │    │ - LSP bridge method          │   │
│   │ - open dashboard    │    │ - push diag to subscribers   │   │
│   │   (spawns anim)     │    │ - paths/resolved, reqlog/*   │   │
│   │ - "Quit"            │    │   rels/list, rels/rows       │   │
│   └─────────────────────┘    └──────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
         ▲                  ▲                   ▲
         │ UDS              │ UDS               │ stdio (LSP) → bridge
         │                  │                   │
   dl queryfoo.dl       dl --check        dl --lsp prog.dl
   (CLI client)         (hook client)     (editor client)
         ▲
         │ UDS (webview's postMessage → native bridge → socket)
         │
   ┌─────────────────────────────────────────────────────────────┐
   │ anim (separate process; spawned by tray "Open dashboard")   │
   │   dashboard mode:                                            │
   │     Active files | Request log | DB explorer | Resolved      │
   │   atlas/workbench mode (existing, now warm-backed):          │
   │     call graph | type graph | hotspots | blast radius | ...  │
   └─────────────────────────────────────────────────────────────┘
```

### Transport

| platform | transport | address |
|---|---|---|
| macOS, Linux | Unix domain socket | `<root>/.dl/daemon.sock` (mode 0600) |
| Windows | named pipe | `\\.\pipe\sprefa-daemon-{blake3(canonical_root)[0..16]}` |

The `interprocess` crate presents both under one local-socket API and
removes the cfg split. Alternative: thin cfg wrapper around
`std::os::unix::net::UnixListener` and `tokio::net::windows::NamedPipeClient`.

TCP-on-127.0.0.1 is rejected: ephemeral port needs a discovery file
anyway, and a bound port prompts the macOS firewall GUI on first run.

### Wire protocol

JSON-RPC 2.0 framed LSP-style (`Content-Length: N\r\n\r\n{json}`). One
codec for the local socket and the LSP stdio bridge; reuse of the
existing `lsp.rs` framing helpers.

Methods (first cut):

| method | request | reply |
|---|---|---|
| `ping` | `{}` | `{ok: true, root, last_tick_ms, tick_count}` (also resets idle timer) |
| `query` | `{program: "path", query: "rel(...)"}` or implicit single query | `{columns, rows, count}` |
| `diag` | `{program: "path"}` | `{rows: [...]}` |
| `shutdown` | `{}` | `{ok: true}` (retires the daemon, removes socket + PID file) |
| `subscribe` | `{events: ["diag_changed"]}` | server-sent notifications thereafter |

The `? rel(...)` queries in a program identify by 1-indexed position;
`query` takes either a program path + query index, or a literal query
string compiled ad-hoc against the warm tables.

### Discovery and lifecycle

**Discovery files** (both at `<root>/.dl/`):
- `daemon.sock` (unix) or `daemon.pipe` (windows) — the listener address
- `daemon.pid` — pid + start time + bound program path

**Spawn-if-missing** (the gradle pattern):
1. Client reads `daemon.pid`. If absent or `kill(pid, 0)` fails, no daemon.
2. Client `connect()`s the socket. If it fails, spawn `dl --daemon
   --root <root>` detached (posix `daemon(3)` equivalent; windows
   `CreateProcess DETACHED_PROCESS`), then poll-connect with backoff
   (10ms, 20ms, 40ms, ..., capped at 500ms; bail after 5s).
3. Once connected, send `ping`. If the bound program differs from the
   request, the daemon either reconfigures (one program per root) or
   the client bails with "daemon busy with `<other_program>`".

**Lifecycle:**
- `dl --stop` (or tray "Quit") sends `shutdown`.
- Idle timeout: 30 min default (env: `DL_DAEMON_IDLE_SECS`). Reset on
  any RPC + any watcher event. Matches gradle.
- `dl --daemon` in foreground (no detach) for debugging: same loop,
  logs to stderr, ignores idle timeout.
- One daemon per workspace root. Multi-root users get multiple daemons
  (and multiple tray icons, or one tray with a submenu per root).

### DB ownership

SQLite WAL stays as-is. **The daemon owns all writes.** Clients never
open the db file directly; they go through `query`/`diag` RPCs. This
removes the existing "two processes fighting over the cache.db" footgun
in `--watch` + `--lsp` and lets the daemon batch ticks safely.

## Tray / menu bar UX

### Crate choice

`tray-icon` (tauri team, pure Rust, no Gtk dep):
- macOS: `NSStatusItem` + `NSMenu`
- Windows: `Shell_NotifyIconW` + popup menu
- Linux: StatusNotifierItem over D-Bus (`zbus`)

Alternatives: `trayify` (older), `ksni` (Linux only), hand-rolled
platform bindings. `tray-icon` is the only one that covers all three
without a Gtk dependency (which would be a heavy add on macOS).

The tray needs an event loop: `tray-icon` runs on the platform's main
thread (`CFRunLoop` on mac, message pump on windows, GLib loop on
linux). The daemon's tick work stays on its own thread; the tray thread
only fields menu events and updates the icon + status text.

### macOS

- Bundle as `.app` with `Info.plist`:
  ```xml
  <key>LSUIElement</key><true/>
  ```
  Accessory mode: status item top-right, no Dock icon, no cmd-tab
  entry. This is the "menu bar instead of shift-tab entry" requirement.
- Tray icon: a small dl glyph (or a green/yellow/red dot for tick
  health). Click opens the menu.
- Menu items:
  - `sprefa — <root basename>` (disabled header)
  - `Status: warm (last tick 12s ago)`
  - `Program: .dl/*.dl`
  - `Tick #423 · 30 files`
  - `---`
  - `Open <root> in Finder`
  - `Pause watcher` (toggles)
  - `Quit sprefa daemon`
- First-run UX: macOS asks for "Accessibility" permission on first
  status-item spawn. Ignore; the API doesn't need it for menu-only.

### Windows

- Build with `#![windows_subsystem = "windows"]` cfg-gated on the
  daemon target so no console window flashes on spawn. The same binary
  keeps the console subsystem for foreground CLI use; subsystem is
  selected at build time per cfg. Cleaner: ship ONE binary in the
  `windows` subsystem and have it `AllocConsole()` only when invoked
  with `--verbose`. Or use the standard two-target trick (`dl.exe` +
  `dlw.exe`); rejected because it's two bins.
- Single binary in `windows` subsystem: CLI output goes to stderr
  (still visible under cmd.exe / PowerShell; editor terminals render
  it). Acceptable trade for one binary.
- Tray icon in notification area; right-click for menu (same items as
  mac, swap "Finder" for "Explorer").

### Linux

- StatusNotifierItem via D-Bus. Works on KDE, Cinnamon, MATE, GNOME
  with AppIndicator extension. Bare WMs (i3/sway without
  `dbus-menu-gtk`) won't render a menu.
- The tray is best-effort on Linux: if no SNI host is detected at
  daemon spawn, log once and continue headless. The daemon is fully
  functional without the tray; the tray is a status surface, not a
  control surface.
- Same menu items as mac/windows.

## Dashboard UI

Three panels behind a click on the tray icon (or `dl --dashboard`).
The tray is the persistent handle; the dashboard is the read surface
when you want to see what the daemon is doing. Same daemon, same
socket, no second process.

### Crate choice

`tao` + `wry` (both tauri-team, the two crates `tauri` itself is built
on):
- `tao` — window + event loop, cross-platform (mac NSWindow, win
  HWND, linux Wayland/X11)
- `wry` — webview embedded in the window (mac WKWebView, win WebView2,
  linux webkitgtk)

Together: a real window with HTML/CSS inside, in ~one extra Cargo
dep tree. Reuses the repo's existing webdev muscle (the `anim/` deck
is JS + d2). Native alternatives (egui, iced, slint) were considered
and rejected: egui's table story is weak for the db explorer; iced is
more code for less; slint adds a DSL we don't need.

The dashboard is a static HTML/CSS/JS bundle shipped inside the
binary (`include_str!` / `include_dir`), talking to the daemon over
the same local socket the CLI uses. No HTTP server inside the daemon
(rejected for the same reason TCP-on-localhost was: an extra
transport, an extra firewall surface, no benefit over the socket
we already have).

Optional later: mirror the dashboard to `http://127.0.0.1:<port>/`
for browser-tab access. Deferred; the webview window covers the same
need without a listening server.

### Panel 1: Active files

What the daemon currently knows about. One row per file under the
program's `scan` footprint; columns:

| column | source |
|---|---|
| path | `file` builtin |
| repo, rev | `(repo, rev, path, content)` row |
| content hash | `content` builtin (blake3 short) |
| last tick | engine's per-file last-parsed timestamp |
| state | scanned / changed / parsed-this-tick / unchanged / untracked |
| size, lines | stat |

Filter by state, sort by last-tick or path. Click a row to open the
file's contribution: which rules touched it this tick, which facts
extracted, which derived rows joined from it.

### Panel 2: Request log

A scrolling ring buffer of every event the daemon handled. This is
the structured version of `--watch`'s `[tick] files X/Y parsed, +A -B
source facts, ...` line — same fields, rendered, filterable,
retained.

| column | values |
|---|---|
| timestamp | ms-precision wall clock |
| source | `cli` / `lsp` / `watcher` / `manual` (from `--changed`) / `tray` |
| kind | `full-tick` / `tick-paths` / `query` / `diag` / `ping` / `subscribe` / `shutdown` |
| trigger | what fired it: `didSave editor=x.rs` / `notify paths=[a.rs,b.rs]` / `cli pid=12345` / `--changed` |
| duration | ms (parsed, derived, total) |
| rows | `+A -B source`, `+C -D derived` |
| program | the `.dl` program path (or `.dl/*.dl` for discovery) |

The trigger-vs-full distinction is the explicit ask: "when did
something fire an incremental tick vs a full sweep" is the most
useful debug column. Color rows by source (cli blue, lsp green,
watcher yellow, manual grey). Click a row to expand the underlying
`--profile` breakdown for that tick (per-repo scan times, slow SQL,
slow `cmd` invocations — same fields `engine.rs` already logs under
`DL_PROFILE=1`, captured instead of printed).

Ring buffer size: default 1000 events, configurable. Older events
spill to `<root>/.dl/reqlog.jsonl` if `--keep-reqlog` is set; off by
default to avoid unbounded growth.

### Panel 3: DB explorer

Browse the warm db: relation list on the left, rows on the right.

- Left pane: every table the daemon has (`rel_*` derived + the
  builtins like `file`, `type_entity`, `call_def`, `call_site`,
  `df_node`, ...). Row count beside each. Search/filter the list.
- Right pane: selected relation's rows, paginated (default 500/page).
  Click a column header to sort; type in a per-column filter box to
  WHERE. Pin a literal to filter one column exactly.
- Follow mode: a toggle that re-queries on every tick, so the view
  updates as the daemon ticks. Off by default (clicks > auto-refresh
  for careful inspection).
- Export: a button copies the current view as TSV (same shape as `dl
  --query-json` output, just one click away).

No writes from the explorer. It's a read-only view of `rel_*`
tables; the daemon owns writes. Adding "run a `?` query" later is a
natural extension (text box → daemon RPC → result rows in the same
grid) but is out of scope for the first cut.

### Window lifecycle

- Opened on demand from the tray menu ("Open dashboard") or by
  running `dl --dashboard`. Single window per daemon; opening again
  focuses the existing one.
- Closing the window does NOT quit the daemon. The daemon is a
  background process; the window is a view onto it. Quit is its own
  tray menu item, with a confirmation if there are unsaved LSP
  `didChange` buffers in flight (a future concern; v1 has no
  didChange support, see `lsp.rs`).
- On macOS the window belongs to the accessory app: no Dock
  activation when it opens, no cmd-tab entry appears, the focus
  stays where it was unless the user clicks into the window.

## Phase ordering

**Phase 1 — daemon without tray.** The gradle win.
- `dl --daemon` (foreground, no tray, logs to stderr). Same loop as
  `run_watch` today (`lib.rs:194`), plus a listener thread on the
  socket.
- Wire protocol: `ping`, `query`, `diag`, `shutdown`.
- `dl prog.dl` (no flag) auto-spawns-or-attaches.
- PID + socket files at `<root>/.dl/`.
- Idle timeout.
- `--stop` flag.
- Test: 1st run = cold tick; 2nd run inside idle window = sub-50ms.

**Phase 2 — tray icon.**
- `tray-icon` integration; daemon spawns the tray thread.
- macOS `.app` bundle with `LSUIElement=true`.
- Windows `windows_subsystem` cfg trick (single bin, no console flash).
- Linux SNI best-effort.
- Menu items: status, program path, "Open dashboard" (disabled until
  Phase 3), "Quit".

**Phase 3 — dashboard window.**
- `tao` + `wry` integration; daemon spawns the window thread on
  demand. Static HTML/CSS/JS bundle inside the binary, talking to
  the daemon over the local socket.
- Three panels landed in order: request log (easiest — render the
  ring buffer), active files (one query against `file` + watcher
  state), db explorer (the table browser; most code).
- RPC additions: `reqlog/recent`, `files/list`, `rel/list`,
  `rel/rows`.
- Follow mode + export TSV.

**Phase 4 — LSP bridge.**
- `dl --lsp prog.dl` becomes a stdio-to-socket bridge: editor ↔ LSP
  frames ↔ daemon RPCs. Daemon is the single stateful process; the
  LSP process is stateless glue.
- Removes the duplicate-tick pathology when both `--lsp` and `--watch`
  run on the same root.
- Push path: daemon broadcasts `diag_changed` to subscribed LSP
  clients after each tick, so editor squiggles update from watcher
  events even without an editor save (today `--lsp` is save-driven
  only).
- Dashboard request log now shows lsp-sourced rows interleaved with
  watcher/cli rows.

**Phase 5 — packaging.**
- macOS `.app` + notarization (codesign + Apple Developer ID).
- Windows MSI via `cargo-wix` or `cargo-bundle`.
- Linux `.deb` / `.rpm` / static musl binary.
- `cargo install --path v5 --bin dl` stays as the source-build path.

## Open questions

- **Per-root vs per-program daemon.** Per-root shares warm tables across
  programs that target the same workspace; per-program is simpler
  (no reconfigure-on-program-change) but spawns N daemons for N
  programs. Recommendation: **per-root**, with the daemon re-ticking
  on program-path change (cheap; parse cache survives).
- **LSP bridge or LSP-in-daemon.** Bridging keeps the daemon
  transport-agnostic and lets the LSP process crash without losing
  state. Embedding LSP in the daemon saves one IPC hop but couples
  lifecycle. Recommendation: **bridge**.
- **Tray crate version pin.** `tray-icon` is 0.x. Pinning is fine; the
  API surface we need (icon, menu, click handler) has been stable
  since 0.10.
- **Idle timeout value.** 30 min matches gradle. For a single-repo
  dev session, longer (2h?) is friendlier; for laptops on battery,
  shorter. Make it a config value, not a constant.
- **What happens on dirty shutdown** (daemon killed -9). Socket file
  lingers; PID file has a dead pid. Next client detects stale PID,
  re-spawns. Stale socket file is fine: `connect()` fails, `bind()`
  reclaims.
- **Multi-root tray.** If a user has three workspaces active, do they
  get three tray icons (one per root, simple, mac menu-bar clutter)
  or one icon with a submenu per root (cleaner, more code)?
  Recommendation: **one icon per root** for phase 2 (matches "one
  daemon per root"), revisit if it gets noisy.
- **The `.dl/cache.db` vs daemon-owned db.** Today `cache.db` is the
  shared cache; multiple processes can collide on it. Once the daemon
  owns writes, the cache becomes daemon-private and clients stop
  reading it directly. Migration: keep the path for one release, log
  a deprecation if a second process opens it.
- **Dashboard: webview vs native.** Webview (wry) reuses webdev muscle
  and makes tables/logs easy; native (egui/iced) avoids the webview
  runtime dep and renders faster for very large row sets.
  Recommendation: **webview**; the db explorer paginates anyway, so
  per-page row count stays small enough that webview is not the
  bottleneck.
- **HTTP mirror of the dashboard.** Serving the same UI on
  `127.0.0.1:<port>` would let users open it in a browser tab
  instead of the tray window. Useful for headless SSH dev (no local
  GUI), but reintroduces an HTTP listener. Recommendation: **defer**;
  revisit when someone is actually doing headless dev against the
  daemon.
- **Request log retention.** Ring buffer (in-memory, default 1000) +
  optional jsonl spill (`--keep-reqlog`). Trade-off is disk growth
  vs debug history. Recommendation: **default off**, opt-in for
  long-running debug sessions.
- **DB explorer writes.** v1 is read-only. A later "run ad-hoc `?`
  query" textbox would surface the same `query` RPC the CLI uses,
  no new transport. Recommendation: **defer to v1.1**; first cut is
  browse-only.
