# herdr vs boop: candidate analysis

Date: 2026-08-10. Scope: whether sprefa's `v6/boop` should fork, compose with,
or be replaced by `herdr`, and what session/pty interface boop actually needs.
No code changed. The user rules; this doc is the cited input.

## Table of contents

1. [Verdict in one table](#1-verdict-in-one-table)
2. [Identifying herdr](#2-identifying-herdr)
3. [herdr facts](#3-herdr-facts)
4. [boop's tmux seam, measured](#4-boops-tmux-seam-measured)
5. [The interface boop needs (step 3)](#5-the-interface-boop-needs-step-3)
6. [Overlap matrix: boop verb x herdr capability](#6-overlap-matrix-boop-verb-x-herdr-capability)
7. [Library candidates before any fork](#7-library-candidates-before-any-fork)
8. [Candidate a: FORK+COMPOSE](#8-candidate-a-forkcompose)
9. [Candidate b: REPLACE](#9-candidate-b-replace)
10. [Candidate c: KEEP+COPY](#10-candidate-c-keepcopy)
11. [Recommendation](#11-recommendation)
12. [What the earlier codex session already found](#12-what-the-earlier-codex-session-already-found)
13. [Open questions for the user](#13-open-questions-for-the-user)

---

## 1. Verdict in one table

| Candidate | Blocking fact | Cost | Call |
|---|---|---|---|
| a. FORK+COMPOSE (herdr as a crate dep) | herdr ships no library target: no `src/lib.rs`, no `[lib]` in `Cargo.toml`, pty types are `pub(crate)` (`ext/herdr/src/pty/backend/unix.rs:7`) | Add `lib.rs` + widen visibility across 245 files / 221,963 lines, then carry that fork against a repo pushed 2026-08-11 with 75 contributors | No |
| b. REPLACE (herdr owns lane ops) | herdr never execs a command in a new pane; it types the command into a shell (`ext/herdr/src/cli/pane.rs:1054-1059`, `ext/herdr/src/app/agents.rs:193-197`) | Rewrite 4 harness adapters + 29 non-test call sites; take a hard dependency on a running herdr server | Not yet |
| c. KEEP+COPY (trait first, tmux as impl #1, herdr adapter second) | Nothing blocks it | One relocation slice behind 9 functions, zero new dependencies | **Yes** |

Second call: the herdr adapter, when it lands, shells `herdr api` / `herdr pane`
and parses JSON, exactly the way boop shells `tmux` today. No fork.

---

## 2. Identifying herdr

`gh search repos herdr` returns one upstream and an ecosystem of satellites:

| Repo | Stars | Language | What it is |
|---|---|---|---|
| `herdrdev/herdr` | 27,118 | Rust | "the runtime your coding agents live on". THE match. |
| `persiyanov/herdr-reviewr` | 383 | Rust | code-review sidebar plugin for herdr |
| `smarzban/herdr-file-viewer` | 376 | Rust | file viewer plugin for herdr |
| `AltanS/collie` | 339 | TypeScript | PWA client for herdr |
| `ogulcancelik/herdr-browser` | 289 | TypeScript | Chromium-in-a-pane plugin |
| `cloudmanic/herdr-plus` | 223 | Go | herdr plugin bundle |
| `dcolinmorgan/herdr-remote` | 218 | Python | menubar/Telegram driver |
| `thinkerisme/herdr` | (fork) | Rust | fork of the upstream |

`ogulcancelik/herdr` redirects to `herdrdev/herdr` (same author, Ogulcan Celik).
Picked `herdrdev/herdr` because it is the only one that is a Rust app spawning
and driving agent sessions, which is the description the user gave ("rust app
for same-ish functions from its code that boop does"). Every satellite is a
plugin or client OF that app.

Cloned to `/Users/chrishafley/projects/ext/herdr` (shallow, 200 commits).

## 3. herdr facts

| Fact | Value | Source |
|---|---|---|
| License | Apache-2.0 | `ext/herdr/Cargo.toml:9`, `ext/herdr/LICENSE:1` |
| Version | 0.8.0 (released 2026-08-03) | `ext/herdr/Cargo.toml:3`; `gh api .../releases` |
| Last commit | `ddffb6e1` 2026-08-11 03:53 +0300 | `git log -1` |
| Repo age | created 2026-03-27 | `gh api repos/herdrdev/herdr` |
| Stars / forks / watchers | 27,118 / 1,903 / 89 | `gh api repos/herdrdev/herdr` |
| Contributors | 75 (GitHub); 37 distinct authors in the last 200 commits | `gh api .../contributors` |
| Open issues | 140 | `gh api repos/herdrdev/herdr` |
| Rust edition / toolchain | 2021 / pinned 1.96.1 | `ext/herdr/Cargo.toml:4`, `ext/herdr/rust-toolchain.toml:2` |
| Size | 245 `.rs` files, 221,963 lines under `src/` | `find src -name '*.rs' \| wc -l` |
| Library target | NONE. No `src/lib.rs`, no `[lib]` stanza | `ls src/lib.rs`; `grep '\[lib\]' Cargo.toml` |

### Dependency spine

| Crate | Version | Role | Source |
|---|---|---|---|
| `portable-pty` | `=0.9.0`, patched to `vendor/portable-pty` | the pty; the patch bundles modern ConPTY on Windows only | `Cargo.toml:35`, `Cargo.toml:[patch.crates-io]`, `vendor/portable-pty/` |
| `interprocess` | 2.4.2 | the unix-socket JSON API transport | `Cargo.toml:29`, `src/ipc.rs` |
| `tokio` | 1.x, `rt-multi-thread macros sync time process io-util` | server runtime | `Cargo.toml` |
| `ratatui` + `crossterm` | 0.30 / 0.29 | the TUI it renders | `Cargo.toml` |
| `schemars` | 1.2.1 | generates `docs/next/api/herdr-api.schema.json` | `Cargo.toml` |
| `tracing` / `tracing-subscriber` | 0.1.44 / 0.3.23 | logging | `Cargo.toml` |
| ghostty bindings | vendored | terminal emulation | `src/ghostty/bindings.rs` |

No `tmux_interface`, no zellij internals. herdr owns its own pty:
`ext/herdr/src/pty/backend/unix.rs:12-42` is a 30-line
`spawn_with_portable_pty(rows, cols, CommandBuilder)` that dups the master fd
CLOEXEC and hands back `SpawnedPty { master_fd, child }`.

### The socket API

herdr's control surface is newline-delimited JSON over a unix socket, protocol
version 20, schema version 1 (`ext/herdr/docs/next/api/herdr-api.schema.json`).
The client is 208 lines (`ext/herdr/src/api/client.rs`): connect, write one JSON
line, read one JSON line. Socket path resolution honors `HERDR_SOCKET_PATH`
(`ext/herdr/src/server/socket_paths.rs:14-32`), mode `0o600`.

The server is a real daemon. `herdr server` with no subcommand falls through the
CLI dispatcher (`ext/herdr/src/cli/server.rs:4-6` returns `Ok(None)`) into
`server::headless::run_server()` (`ext/herdr/src/main.rs:570-571`). The
interactive path spawns that daemon setsid'd with stdio to `/dev/null`
(`ext/herdr/src/server/autodetect.rs:181-217`, called from
`autodetect.rs:290-303`).

Caveat that matters for scripting: no CLI verb autostarts the server. A `herdr
pane split` against a dead socket returns error code `server_not_running` and
tells you to run `herdr` (`ext/herdr/src/cli/server_not_running.rs:28-40`).
`tmux new-session -d` starts its own server; herdr requires the caller to start
one first.

## 4. boop's tmux seam, measured

boop already keeps tmux in one file. `v6/boop/src/main.rs:4` states the law:
"no direct `Command::new("tmux")` beyond the layer-1 helpers."

Measured today:

| Metric | Count |
|---|---|
| `tmux::` references outside `src/tmux.rs` | 35 |
| of those, in non-test code | 29 |
| raw `Command::new("tmux")` outside `src/tmux.rs` | 7 (all inside `#[cfg(test)]` helper blocks in `harness/{claude,codex,opencode}.rs`) |
| distinct `tmux::` functions consumed | 9 |

The 9 functions are the entire interface surface, all in `v6/boop/src/tmux.rs`:

| Function | Line | tmux command | Consumers |
|---|---|---|---|
| `new_detached_session(socket, name, cwd, command)` | `tmux.rs:433` | `new-session -d -s -c <cwd> <cmd>` via `tmux_interface` | `harness/opencode.rs:114`, `harness/codex.rs:49`, `harness/claude.rs:67`, `harness/kimi.rs` |
| `send_keys_literal(socket, pane, body)` | `tmux.rs:410` | `send-keys -t -l -- <body>` then a separate `Enter` | `harness/opencode.rs:138`, `harness/codex.rs:75`, `harness/claude.rs:94` |
| `capture_pane(socket, target, lines)` | `tmux.rs:280` | `capture-pane -p -t [-S -N]` | `main.rs:3078` (`beep lane pane`) |
| `has_session(socket, session)` | `tmux.rs:338` | `has-session -t =<name>` via `tmux_interface` | `main.rs:1708`, `main.rs:3048`, harness `stop()` |
| `target_alive(socket, target)` | `tmux.rs:392` | `list-panes -t =<target>` | `main.rs:1743` |
| `live_sessions(socket)` | `tmux.rs:306` | `list-sessions -F '#{session_name}'` | `main.rs:931`, `main.rs:1732`, `main.rs:2980`, `main.rs:3021`, `main.rs:3632` |
| `pane_pid(socket, target)` | `tmux.rs:369` | `list-panes -t -F '#{pane_pid}'` | `main.rs:826`, `main.rs:1019`, `main.rs:3184`, `main.rs:3368` |
| `kill_session(socket, session)` | `tmux.rs:353` | `kill-session -t =<name>` via `tmux_interface` | `main.rs:3049`, harness `stop()` |
| `session_of_pane(socket, pane)` | `tmux.rs:258` | `display-message -p -t '#{session_name}'` | `identity.rs:98` |

Two design facts inside that file are worth preserving under any host:

- `live_sessions` returns `Option<LiveSessions>` where `None` means the host
  itself is unreachable, which is a different answer from an empty set
  (`tmux.rs:304-305`). Three states, deliberately.
- `exact_target(name) -> "=name"` exists because `-t boop` prefix-matches
  `boop-shell-v2` (`tmux.rs:272-276`).

`ControlClient` (`tmux.rs:94-179`) is a long-lived `tmux -C` control-mode client
with `%begin/%end/%error` block pairing. It is currently unused by the verbs
above (the file carries `#![allow(dead_code)]` at `tmux.rs:6`).

### What does NOT come from the session layer

Exit rc. `SpawnSpec::with_on_exit` wraps the harness command as
`{command}; __rc=$?; {epilogue}; exit $__rc` (`v6/boop/src/harness.rs:164-169`).
The epilogue hails a `kind=result` row carrying `rc=N`; `run_lane_wait`
(`main.rs:3084-3095`) polls the mailbox and `parse_result_rc` (`main.rs:3140`)
reads it. So the rc travels through the shell and the mailbox, never through
tmux. That is what makes a host swap tractable.

Also outside the session layer:

| Concern | boop seam |
|---|---|
| worktree creation | `worktree.rs:16-46` (`git worktree add -b <branch> <dir> <sha>` then `merge --ff-only`) |
| mailbox | `bus.rs:118-233` (ndjson append, CAS update at `bus.rs:235`) |
| registry routes | `bus.rs:49-111` (`registry.json`) |
| lane identity derivation | `lane.rs:56-124` (branch is the identity) |
| rss/cpu per lane | `proc.rs:88-99` (`sysinfo`), joined to `pane_pid` |
| SQLite store | `ident.rs`, `query.rs`, `usage.rs`; `boop db` passthrough at `main.rs:3576-3608` |

## 5. The interface boop needs (step 3)

Derived from the 9 call sites above. herdr's shape had no vote here, and the
names avoid the banned identifier list.

```rust
/// The session/pty host a lane runs on. tmux is impl #1; a herdr adapter is
/// impl #2. Every method maps to at least one existing boop call site.
pub trait TerminalHost {
    /// Stable short id printed in `beep lane get` and stored on the route.
    fn id(&self) -> &'static str;

    /// Create a detached session running `open.command` in `open.cwd`.
    /// Replaces tmux.rs:433 `new_detached_session`.
    /// Returns the handle every later call targets.
    fn open(&self, open: &OpenSession) -> anyhow::Result<TargetRef>;

    /// Deliver one line of text plus Enter into the target.
    /// Replaces tmux.rs:410 `send_keys_literal`. Carries hail delivery.
    fn send_line(&self, target: &TargetRef, body: &str) -> anyhow::Result<SendOutcome>;

    /// The target's visible region, or the last `lines` rows of scrollback.
    /// Replaces tmux.rs:280 `capture_pane`; backs `beep lane pane`.
    fn capture(&self, target: &TargetRef, lines: Option<u32>) -> anyhow::Result<String>;

    /// Per-target liveness. Three states on purpose: tmux.rs:304 records that
    /// "host unreachable" and "no sessions" are different answers.
    /// Replaces tmux.rs:338 `has_session` + tmux.rs:392 `target_alive`.
    fn alive(&self, target: &TargetRef) -> Liveness;

    /// Every live session name, or `None` when the host itself is unreachable.
    /// Replaces tmux.rs:306 `live_sessions`; backs `beep lane list` state.
    fn list(&self) -> Option<std::collections::BTreeSet<String>>;

    /// The pid of the process at the root of the target, for the `sysinfo`
    /// tree walk in proc.rs:54. Replaces tmux.rs:369 `pane_pid`.
    fn root_pid(&self, target: &TargetRef) -> Option<u32>;

    /// Tear the target down. Replaces tmux.rs:353 `kill_session`.
    fn close(&self, target: &TargetRef) -> anyhow::Result<()>;

    /// The session that owns a pane-shaped target, for identity.rs:98.
    /// Replaces tmux.rs:258 `session_of_pane`.
    fn owner(&self, target: &TargetRef) -> Option<String>;

    /// What this host can do beyond the required set. `true` only where a
    /// test exercises it, mirroring harness.rs:41-44.
    fn extras(&self) -> HostExtras { HostExtras::default() }
}

/// What `open` should create. `command` arrives already wrapped by
/// SpawnSpec::with_on_exit (harness.rs:164), so the rc epilogue is inside it
/// and the host never needs to report an exit status.
pub struct OpenSession {
    pub name: String,
    pub cwd: std::path::PathBuf,
    pub command: String,
    pub env: std::collections::BTreeMap<String, String>,
    /// tmux `-L <socket>`; herdr `--session <name>` / HERDR_SOCKET_PATH.
    /// `None` is the host's default endpoint.
    pub endpoint: Option<String>,
}

/// A host-opaque handle. tmux spells it `feature-schema-emit` or `sess:0.0`;
/// herdr spells it `pane_id`.
#[derive(Clone, Debug)]
pub struct TargetRef {
    pub host: &'static str,
    pub handle: String,
    pub endpoint: Option<String>,
}

pub enum Liveness { Live, Dead, HostUnreachable }

pub enum SendOutcome { Injected, QueuedForNextSpawn, Unsupported }

/// Capabilities the required set does not cover. Every one of these is a
/// thing herdr has and tmux does not.
#[derive(Default, Clone, Copy)]
pub struct HostExtras {
    /// Host classifies the occupant as idle/working/blocked/done.
    pub agent_state: bool,
    /// Host can block until output matches a pattern.
    pub wait_for_output: bool,
    /// Host pushes an event stream instead of being polled.
    pub event_stream: bool,
    /// Host creates git worktrees itself.
    pub worktree: bool,
}
```

Note what is absent by design: no `exit_rc()`. boop gets the rc from the shell
epilogue and the mailbox, so no host is asked for something tmux and herdr both
decline to give.

`SessionRef.tmux` / `SessionRef.tmux_socket` (`harness.rs:90-94`) become
`SessionRef.target: Option<TargetRef>`, and `Route.tmux` (`bus.rs:81`) grows a
sibling `host` field. Those two renames are the visible blast radius.

## 6. Overlap matrix: boop verb x herdr capability

| boop verb | boop seam (file:line) | herdr capability | herdr seam (file:line) | Fidelity |
|---|---|---|---|---|
| `beep lane create` (worktree half) | `worktree.rs:16-46`; `merge --ff-only` gate at `worktree.rs:41,49` | `worktree.create` (`branch`, `base`, `path`, `label`, `focus`) | `src/api/schema/worktrees.rs:11-28`; CLI `src/cli/worktree.rs:14` | Partial. herdr creates the worktree AND opens a workspace around it. No fast-forward gate, no ordered setup steps (`SpawnSpec.setup`, `harness.rs:136`). |
| `beep lane create` (spawn half) | `tmux.rs:433` exec's the command as the session's shell-command | `pane.split` takes `cwd` + `env` but NO command (`PaneSplitParams`); the command is typed in afterwards | `src/api/schema/panes.rs:26-43`; `pane run` = `PaneSendInput` at `src/cli/pane.rs:1054-1059`; `agent.start` builds an argv and types it into an idle shell at `src/app/agents.rs:193-197` | **Weaker.** tmux exec's; herdr types. A typed command is subject to shell prompt state. |
| `beep hail` (delivery half) | `tmux.rs:410` `send-keys -l --` + Enter; called from `main.rs:1355` through the harness `send()` facet | `pane.send_text`, `pane.send_keys`, `pane.send_input` | `src/api/schema/panes.rs:253-272`; CLI `src/cli/pane.rs:36-37` | Equal, plus `agent.prompt` with a `wait { until: [status], timeout_ms }` option that boop has no equivalent of (`src/api/schema/agents.rs:175-181`). |
| `beep hail` (mailbox half) | `bus.rs:196-233` + `main.rs:1324` append to `~/.agent/mail/bus.ndjson`; edge recorded at `main.rs:1368-1382` | none | | **herdr has nothing here.** No mailbox, no durable message log. |
| `beep lane wait` | `main.rs:3084-3095` polls; rc parsed at `main.rs:3140`; rc produced by the shell epilogue `harness.rs:164-169` | `pane.exited` event carries `pane_id` + `workspace_id` and NO exit code | `src/api/schema/events.rs:526-529` | **Not provided.** Exit codes exist in herdr only for its plugin runtime (`src/app/api/plugins/runtime.rs:146`). boop's epilogue trick keeps working under either host, unchanged. |
| `beep lane list` (state column) | `main.rs:2973-3013`, `lane_state` at `main.rs:3007`, over `tmux::live_sessions` | `pane.list` / `agent.list` returning `PaneInfo` with `agent_status` in {idle, working, blocked, done, unknown} | `src/api/schema/panes.rs:446-480`; states parsed at `src/cli.rs:881-885` | **Stronger.** boop's states are alive/dead; herdr classifies the occupant. |
| `beep lane get` / `route` | `main.rs:3015-3038`, `main.rs:1229` over `registry.json` | `pane.get`, `workspace.get`, `tab.get` | schema verbs in `docs/next/api/herdr-api.schema.json` | Equal for topology. herdr has no notion of boop's route fields (`goal`, `parent`, `model`, `session_id`). |
| `beep lane pane` | `main.rs:3064-3080` -> `tmux.rs:280` | `pane.read` with `source` in {visible, recent, recent-unwrapped, detection}, `format` in {text, ansi}, `strip_ansi` | `src/api/schema/panes.rs:274-286`; sources at `src/cli.rs:859-872` | **Stronger.** boop has visible-or-N-lines; herdr adds ansi passthrough and a detection view. |
| (boop has none) | | `pane.wait_for_output` blocking on substring or regex with a timeout | `src/cli/pane.rs:1077-1110`; matcher enum `OutputMatch::{Substring,Regex}` | **herdr-only.** This is the capability boop most visibly lacks. |
| (boop has none) | | `events.subscribe` / `events.wait` push stream | `src/api/schema/events.rs:12-115`; `src/api/subscriptions.rs` (853 lines) | **herdr-only.** boop polls at 1s (`main.rs:3091`). |
| `beep lane delete` | `main.rs:3040-3062` -> `tmux.rs:353` | `pane.close`, `tab.close`, `workspace.close` | schema verbs; CLI `src/cli/pane.rs:35` | Equal. |
| `beep ps` | `main.rs:3170-3355`: `tmux::pane_pid` joined to `proc.rs:88` sysinfo tree | `pane.process_info` -> `PaneProcessInfo { shell_pid, foreground_process_group_id, tty, foreground_processes[{pid,name,argv0,argv,cmdline,cwd}] }` | `src/api/schema/panes.rs:489-514`; CLI `src/cli/pane.rs:23` | **Stronger for identity, weaker for cost.** herdr gives argv and tty directly; it gives no rss or cpu, so boop keeps `sysinfo` either way. |
| `boop db` | `main.rs:3486-3608` SQLite at `~/.agent/boop.db`; ingest via `harness.rs:29` `ingest()` per adapter | none | | **herdr has nothing here.** Its persistence is layout snapshot/restore (`src/persist/snapshot.rs` 1,267 lines, `src/persist/restore.rs` 1,722 lines). Transcripts, tokens, and cost have no counterpart anywhere in it. |
| identity resolution | `identity.rs:98` -> `tmux::session_of_pane` | `pane.get` returns `workspace_id`, `tab_id`, `terminal_id` | `src/api/schema/panes.rs:446-451` | Equal. |
| harness registry | `registry.rs:17-35`, 4 adapters (claude, codex, kimi, opencode) | `detect` module recognizes 20+ agent kinds by manifest | `src/detect/mod.rs:183-203`; remote manifest updates at `src/detect/manifest_update.rs` | **Stronger for breadth, different in kind.** herdr detects what is running in a pane; boop reads transcripts off disk. |

Summary of the matrix: herdr covers boop's session layer well and beats it on
reading and on state classification. herdr covers zero of boop's mailbox,
registry, transcript, token, and cost layers. The one place herdr is genuinely
weaker is command execution, where it types rather than exec's.

## 7. Library candidates before any fork

The build-vs-buy law requires this survey before any bespoke or fork work.

| Crate | Version | What it gives | Why it does or does not fit |
|---|---|---|---|
| `portable-pty` | 0.9.0 | `openpty` + `spawn_command`, cross-platform | This is what herdr itself uses (`ext/herdr/Cargo.toml:35`), and its unix path is 30 lines (`ext/herdr/src/pty/backend/unix.rs:12-42`). The herdr patch is Windows ConPTY only (`vendor/portable-pty`, commit `8afd52a`), so on macOS the upstream crate is exactly what herdr runs. Fits IF boop grows a daemon. See the trap below. |
| `tmux_interface` | 0.4.0 | typed argv builder for tmux commands | Already a boop dependency (`v6/boop/Cargo.toml:31`) and used for `has_session`, `kill_session`, `new_detached_session`. Its measured limits are recorded at `v6/boop/src/tmux.rs:1-5`: CLI-only, no `-C` control-mode parsing, no literal send-keys mode. Keep it; it covers what it covers. |
| `pty-process` | 0.5.3 | spawn a command attached to a pty, unix-focused | Smaller than `portable-pty` and adequate for a unix-only host, but it inherits the same daemon trap. |
| `expectrl` | 0.9.0 | expect-style automation over a pty: send, expect pattern, timeout | The closest library answer to herdr's `pane.wait_for_output`. Worth a look on its own merits regardless of the herdr decision. |
| `vt100` | 0.16.2 | parse terminal output into a screen model | You need this (or `wezterm-term`) the moment you own a pty, because `capture` has to answer "what is on screen" and raw pty bytes do not. |
| `zellij` | n/a | multiplexer with a plugin API | Same shape as herdr: an app with no library target. Same fork problem, smaller agent story. |
| `herdr` (crates.io) | 0.1.0 published, repo at 0.8.0 | the binary | Published as a binary crate. `cargo add herdr` gets you no importable API. |

**The daemon trap.** boop today has no long-running process: every verb is a
one-shot `main.rs` invocation that asks a server someone else keeps alive.
Adopting `portable-pty` directly means boop must also own (1) a vt100 screen
model to answer `capture`, (2) a persistent process so panes survive the CLI
exiting, and (3) reattach and restore. That is rebuilding tmux. The standing law
"Infra is bought, never built" (CLAUDE.md) points the other way. So the library
route is real only under a decision to give boop a daemon, which is a much
larger question than this doc.

## 8. Candidate a: FORK+COMPOSE

Shape: boop keeps `bus.ndjson`, `registry.json`, `worktree.rs`, and
`~/.agent/boop.db`. A forked herdr becomes the `TerminalHost` impl behind the
step-5 trait, consumed as a path dependency.

### Blocker, cited

herdr cannot be a Cargo dependency as it stands:

| Blocker | Evidence |
|---|---|
| No library target | `ls ext/herdr/src/lib.rs` -> absent; `grep '\[lib\]' ext/herdr/Cargo.toml` -> no match |
| Modules rooted in `main.rs` | `ext/herdr/src/main.rs` declares the module tree |
| pty types are crate-private | `pub(crate) struct SpawnedPty`, `pub(crate) fn spawn_with_portable_pty` (`ext/herdr/src/pty/backend/unix.rs:7,12`) |
| Protocol types coupled to app state | `src/api/schema/panes.rs:284` carries `pub(crate) intent: super::common::ReadIntent` inside a wire struct |
| Vendored dependency patch | `[patch.crates-io] portable-pty = { path = "vendor/portable-pty" }` propagates to any consumer's lockfile |
| Embedded terminal bindings | `src/ghostty/bindings.rs` |

### Pros

- One process instead of two, and typed calls instead of JSON round trips.
- Access to internals boop cannot reach through the socket API: the raw pty fd,
  the vt100 screen model, the detection manifests.

### Cons

- 245 files, 221,963 lines. Adding `lib.rs` plus widening visibility touches the
  module graph broadly, and every upstream commit thereafter conflicts with it.
- Upstream ships fast: 200 commits in the shallow window, 37 distinct authors in
  it, last push 2026-08-11, 140 open issues. A private fork of a repo moving that
  fast is a standing tax.
- The `[patch.crates-io]` entry leaks into sprefa's lockfile.
- Pulling `ratatui`, `crossterm`, `tokio`, `png`, and ghostty bindings into a
  CLI that today builds on 10 dependencies.

### Migration cost

High and open-ended. The fork work itself is mechanical; the rebase work is not.

### The narrow variant, which is different

Fork ONLY the wire types and the client: `src/api/schema/*` (7,804 lines,
mostly derive-heavy structs) plus `src/api/client.rs` (208 lines). That yields
`herdr-protocol` + `herdr-client` with no terminal ownership, no ratatui, no
ghostty, no persistence. The client is 208 lines of "write one JSON line to a
unix socket, read one back". This is worth an upstream issue asking herdr to
publish those two crates, since the whole ecosystem in section 2 needs them.

## 9. Candidate b: REPLACE

Shape: herdr owns lane operations wholesale. boop shrinks to mailbox + registry
+ db + worktree, and calls `herdr` for everything session-shaped.

### Pros

| Gain | Evidence |
|---|---|
| Agent state classification (idle / working / blocked / done / unknown) | `src/api/schema/panes.rs:471` `agent_status`; parsing at `src/cli.rs:881-885` |
| Block until output matches, with timeout | `src/cli/pane.rs:1077-1110`, `OutputMatch::{Substring,Regex}` |
| Push event stream instead of 1s polling | `src/api/schema/events.rs:12-115`; `src/api/subscriptions.rs` |
| Worktree creation with workspace binding | `src/api/schema/worktrees.rs:11-28` |
| Richer process info per pane (argv, tty, foreground group) | `src/api/schema/panes.rs:489-514` |
| Reads with ansi preserved | `src/api/schema/panes.rs:274-286` |
| Remote sessions over ssh, and reattach | `src/remote/`, `src/session.rs` |
| 27k stars of maintenance you do not pay for | |

### Cons

| Loss or risk | Evidence |
|---|---|
| Commands arrive as typed keystrokes | `pane.run` is `PaneSendInput` (`src/cli/pane.rs:1054-1059`); `agent.start` builds an argv and types it into an idle shell (`src/app/agents.rs:193-197`). boop's `tmux new-session -d '<cmd>'` exec's it as the session's command (`tmux.rs:445-449`). Typed commands depend on prompt state. |
| No exit code from the session layer | `pane.exited` carries only `pane_id` + `workspace_id` (`src/api/schema/events.rs:526-529`). Survivable, because boop's rc comes from the shell epilogue (`harness.rs:164-169`), but it means herdr adds nothing here. |
| No CLI autostart | `src/cli/server_not_running.rs:28-40`. boop would have to `setsid herdr server` itself (`src/main.rs:570-571` makes that a valid headless start) and own that lifecycle, which is a new failure class. |
| Naming collision risk | herdr agent names must match `[a-z][a-z0-9_-]{0,31}` and be unique among live agents (`skills/herdr/SKILL.md`). boop's law is "the branch is the identity" (agent-bus SKILL.md), and `feature/some-longer-branch-name` maps to a lane id that can exceed 32 chars. |
| Lanes become visible in the user's interactive UI | Every boop lane lands in a herdr workspace/tab. Feature or noise; the user decides. |
| Hard runtime dependency | A herdr version bump can break lane spawning. tmux is on the machine already and moves slowly. |
| `pane.split` cannot set the session name | boop keys everything on the session name it chose (`bus.rs:81` `Route.tmux`). herdr mints `pane_id` and offers `pane.rename` / `agent.rename` after the fact. |

### Migration cost

Rewrite 4 harness adapters (`harness/{claude,codex,kimi,opencode}.rs`, 2,947
lines total) plus 29 non-test call sites, plus a route schema change
(`Route.tmux` -> host + handle), plus a server lifecycle boop does not have
today. Every `beep` output snapshot changes.

## 10. Candidate c: KEEP+COPY

Shape: write the step-5 `TerminalHost` trait, move `tmux.rs` behind it
unchanged, ship with tmux as the only impl. Steal herdr's crate choices and
naming as reference, and leave its code where it is. Add a `HerdrHost` impl later that shells
`herdr api` / `herdr pane` / `herdr agent` and parses the JSON, exactly the way
`tmux.rs` shells `tmux` today.

### Pros

- Answers the user's stated want on day one: tmux behind an interface, and the
  interface is written down from real call sites rather than guessed.
- Zero new dependencies. Zero fork. Zero new runtime requirement.
- The relocation is bounded: 9 functions, 35 references, 29 of them non-test.
- Every `beep` output snapshot can be held identical, so the slice is provable.
- The second impl becomes a single new file rather than an architecture change,
  and it can be added or dropped without touching the trait's consumers.
- herdr's schema is a free design review: `ReadSource::{visible, recent,
  detection}` (`src/cli.rs:859-862`) and `AgentStatus` (`src/cli.rs:881-885`)
  are better vocabulary than boop's alive/dead, and cost nothing to adopt.

### Cons

- No new capability on the day it lands. It is a relocation.
- The trait can be shaped wrong if only one impl ever exercises it. Mitigation:
  write the `HerdrHost` shell-out impl second and soon, since it is the thing
  that proves the shape.
- `HostExtras` risks becoming a grab bag. Mitigation: the existing
  `Capabilities` precedent at `harness.rs:113-119` sets the rule already, "true
  only where a test exercises it".

### Migration cost

One slice. Rename `SessionRef.tmux` / `.tmux_socket` to `.target: Option<TargetRef>`
(`harness.rs:90-94`), add a `host` field beside `Route.tmux` (`bus.rs:81`),
move the 9 functions behind the trait, keep `tmux.rs` as `TmuxHost`. The 7 raw
`Command::new("tmux")` calls in test helpers stay as-is; they are test scaffolding
for a throwaway tmux server, well outside the production seams.

## 11. Recommendation

**Candidate c, then a shell-out herdr adapter. Do not fork.**

Reasoning, in priority order:

1. **The fork blocker is mechanical and belongs upstream.** herdr has no library
   target and crate-private pty types (`src/pty/backend/unix.rs:7,12`). That is a
   packaging gap the author has simply not closed yet, and the herdr plugin
   ecosystem in section 2 has the same need. File an upstream issue asking for
   `herdr-protocol` + `herdr-client`; do not carry a private fork of a 222k-line
   repo that pushed today.

2. **boop has no daemon, so the raw-library route is a trap.** Taking
   `portable-pty` directly obliges boop to own a vt100 screen model, a persistent
   process, and reattach. "Infra is bought, never built" points at using someone
   else's multiplexer, which is what boop already does.

3. **The interface is the deliverable the user asked for, and it is independent
   of herdr.** "i want tmux behind interface so we know what we need from our
   session/pty handlers" is satisfied by section 5 with tmux as the only impl.
   herdr's fate does not change that work.

4. **The shell-out adapter costs one file.** boop already
   shells `tmux` in `tmux.rs`. Shelling `herdr api --json` is the same move, and
   herdr's API is machine-shaped by construction: newline-delimited JSON with a
   published schema (`docs/next/api/herdr-api.schema.json`, protocol 20).

5. **The wins that motivate herdr survive the shell-out.** Agent state,
   `wait_for_output`, and the event stream all come through the socket API. None
   of them needs linked code.

6. **The one place herdr is weaker matters for lanes specifically.** boop lanes
   are non-interactive `opencode run` invocations (agent-bus SKILL.md: "opencode
   run takes its prompt from ARGV"). tmux exec's that command as the session's
   command; herdr types it into a shell prompt. For a fire-and-forget lane, exec
   is the more reliable of the two.

### Sequence

| Step | Work | Gate |
|---|---|---|
| 1 | Land the `TerminalHost` trait with `TmuxHost` as the only impl. No behavior change. | Every `beep` output snapshot identical; boop test suite green |
| 2 | Open an upstream herdr issue requesting `herdr-protocol` + `herdr-client` crates | Issue posted |
| 3 | Add `HerdrHost` shelling `herdr api`, gated behind an explicit `--host herdr` | One lane spawned, hailed, captured, and waited end to end |
| 4 | Consider adopting herdr's read vocabulary (`visible` / `recent` / `detection`) and `AgentStatus` into `beep lane list`, whichever host is active | User's call |

Step 3 is where the real decision gets made with evidence instead of reading.
If `HerdrHost` proves out, candidate b becomes a flag flip rather than a rewrite.

## 12. What the earlier codex session already found

The user probed this on 2026-08-09 in codex session
`019fe6f0-e567-7823-8c1f-78bc2e0646d0` (`~/.codex/sessions/2026/08/09/`). The
user's own framing from `~/.codex/history.jsonl`:

- "herdr real quick is it boop?"
- "herdr eclipse boop not the other way around u dingus"
- "herdr i would want, bc i like my api better (boop beep is giving me life)"
- "herdr could be an impl with shellouts or some shit, zellij etc."

That session reached the same structural conclusion this doc reaches
independently: herdr has no `lib.rs`, modules rooted in `main.rs`, `pub(crate)`
boundaries, cross-module state access, a vendored `portable-pty` patch, and
embedded ghostty bindings; and "the smallest upstream extraction would be
`herdr-protocol` plus `herdr-client`". It also sketched a `TmuxMultiplexer`
trait and a type list (`RuntimeIdentity`, `LaneRuntimeBinding`, `TerminalRead`,
`Viewport`, `SendReceipt`, `ForegroundProcess`, `MultiplexerCapabilities`) and
proposed new verbs `boop beep viewport <lane>` and `boop beep screen <lane>` for
an Instant transcript minimap.

Nothing was implemented. No herdr checkout existed before this task; no boop
code references herdr.

The one place this doc differs from that session: it proposed adding capability
before proving the abstraction with a second impl. Section 11 reverses that
order, because a trait with one impl is a guess.

## 13. Open questions for the user

| # | Question | Why it needs you |
|---|---|---|
| 1 | Should boop lanes appear in your interactive herdr UI, or stay invisible on a separate herdr session name? | A product call rather than a technical one. herdr supports `--session <name>` isolation (`src/server/socket_paths.rs:14-32`). |
| 2 | Do you want the trait's second impl to be herdr, or zellij, or both? | Your codex note said "zellij etc." Two impls prove the shape better than one; three is a tax. |
| 3 | Does boop ever get a daemon? | This is the gate on the `portable-pty` route ever being viable, and it is a bigger question than the herdr one. |
| 4 | File the upstream `herdr-protocol` issue under your name? | It is your ecosystem ask, and the plugin authors in section 2 share it. |
| 5 | Adopt herdr's `AgentStatus` vocabulary in `beep lane list` even on the tmux host? | It changes `beep` output, which the agent-bus skill documents. |
