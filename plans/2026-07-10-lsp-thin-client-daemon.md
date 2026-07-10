# dl --lsp as a daemon thin client (the Gradle model)

## Context

Gradle's CLI is a thin client: every invocation ships its args, cwd, env, and
attached stdio to a long-lived daemon over a local socket; a fingerprint check
(version + JVM + daemon-relevant env) respawns incompatible daemons; IDEs use
the Tooling API, a persistent subscription to the same daemon rather than a
differently-moded process. Flags are request payload, never daemon config, so
the client is too dumb to be stale.

dl is already halfway there: one-shots auto-attach (`ensure_daemon`,
daemon.rs:1654) with a build_id fingerprint (version + exe mtime,
daemon.rs:1044) that auto-respawns a stale daemon (daemon.rs:1670-1673);
--mcp and --hook are daemon-first pumps (`Pump{Local,Daemon}`, mcp.rs:42);
--check and one-shot queries route via the `diag`/`query` RPCs (lib.rs:161,
325). The holdout is `--lsp`: run_lsp opens its OWN full engine against the
shared `<root>/.dl/cache.db` (lsp.rs:93-104, WAL sharing per cli/mod.rs:273),
ticks in-process on didSave, and only uses the daemon as a diag_changed
doorbell (spawn_daemon_subscriber, lsp.rs:252) after which it re-reads the
shared db instead of asking the daemon. Consequences: two engines contending
on one db, a whole second corpus warmup per editor window, and the
served-copy divergence class of bugs (a stale resident LSP engine that no
fingerprint ever checks).

This plan makes `--lsp` a stdio<->socket adapter over the daemon RPC, with
the in-process engine demoted to the `--no-daemon` fallback arm, and states
the general law: a mode flag picks a client-side transport binding; engine
work happens in the daemon.

## Existing assets (verified)

- Wire: JSON-RPC 2.0, Content-Length framed, over a unix socket (src/rpc.rs;
  `write_frame`/`read_frame` rpc.rs:21,29; `write_notification` rpc.rs:155).
  Socket at `<root>/.dl/daemon.sock` with the /tmp/dl-sock relocation for
  long roots (socket_path daemon.rs:93).
- Dispatch: `handle_request` (daemon.rs:1176) serves 18 methods including
  `ping` (returns build_id), `query_sql`, `query_rel`, `diag`, `definition`,
  `hover`, `subscribe`, `mcp_request`/`mcp_retire`, `hook_event`.
- Push: `Daemon.subscribers` + `broadcast_diag_changed` (daemon.rs:220, 435)
  sends `{tick, paths}` to every subscriber after each tick; the LSP already
  consumes this (lsp.rs:136).
- Fingerprint: `build_id()` = `{version}:{exe_mtime}`; compared only in
  `ensure_daemon`; mismatch = stop + respawn.
- Pump precedent: `enum Pump { Local(Box<Engine>), Daemon { stream, next_id } }`
  with per-method match arms; daemon-side drift guard validates the request
  against the DAEMON's program (daemon.rs:1366-1372).

## RPC gap analysis

Already served: `query_sql` (dl/query incl. paging runs SQL client-composed),
`diag`, `definition`, `hover`, `query_rel`.

Missing for the LSP handler set (engine sigs at src/engine/mod.rs):

| LSP need | engine call | new RPC |
|---|---|---|
| references + dl/refs | `refs_lens(path, byte) -> Result<Option<RefLens>>` (2607) | `refs {path, byte}` -> RefLens JSON |
| cursor->span for def/hover | `span_at(path, byte)` (2320) | fold into `definition`/`hover` v2: accept `{path, byte}` instead of pre-resolved text |
| multi-repo URI mapping | `repo_roots()` | `repo_roots {}` -> `{slug: abs_root}` (fetch once at init, refresh on graphChanged) |
| didSave immediacy | `tick_paths(prog, [abs], quiet)` (tick.rs:624) | `saved {paths}` -> `{tick_count}` (idempotent: digest skips make a watcher-raced second tick ~free) |
| mute triad | `toggle_diag_mute` (4300) / `muted_codes` (4321) / `diag_code_states` (4336) | `diag_mute {toggle?: code}` -> `{codes: [(code, muted)]}` (one RPC, toggle optional) |
| publish extras | `extraction_drops()` (2923) | fold into `diag` response as an `extra` array |

Design rule for the new RPCs: match the LSP REQUEST shape, not the engine
method (Gradle ships "run this build", not JVM internals). So `definition`
and `hover` grow `{path, byte}` params and do their own span_at daemon-side;
the adapter never needs span_at itself. Position->byte conversion stays
client-side (it reads the saved file from disk, same as today's
resolve_path_byte, lsp.rs:626).

## Type signatures

```rust
// src/lsp.rs
enum LspPump {
    Local(Box<Engine>),                       // --no-daemon / attach-fail arm
    Daemon {
        stream: UnixStream,                   // request/response channel
        next_id: u64,
        root: PathBuf,                        // for reconnect + respawn
    },
}

impl LspPump {
    // rpc(method, params) -> Result<serde_json::Value>
    //   Daemon arm: write_frame(Request{next_id++, ..}); read_frame; on io
    //   error -> reconnect(): ensure_daemon(root) [fingerprint respawn lives
    //   here for free] + connect + retry ONCE; second failure bubbles.
    // refs(path, byte) / definition(path, byte) / hover(path, byte)
    // saved(abs_paths) / diags(only) / query_sql(sql, params, page)
    // diag_mute(toggle: Option<&str>)
}
```

The subscriber thread (lsp.rs:252) stays as-is: same `subscribe` RPC, same
synthetic `dl/diagChanged` forwarding. One change: on `dl/diagChanged` the
main loop calls `pump.diags(paths)` instead of re-reading a local engine
(lsp.rs:142-148). The subscriber ALSO reconnects through `ensure_daemon` on
stream death, then triggers a full republish (the daemon may have restarted
with fresh state).

## Instance lifetimes

- `LspPump::Daemon.stream`: lives for the LSP session; replaced in-place on
  reconnect. One request in flight at a time (the LSP main loop is serial
  today; keep that).
- Subscriber `UnixStream`: independent connection, owned by its thread,
  reconnect loop with the existing CONNECT_BACKOFF_MS schedule.
- `LspPump::Local(Engine)`: exactly today's engine + cold tick; constructed
  only when `!daemon::enabled_for(root)` or attach fails at startup (mirror
  run_mcp's gate, mcp.rs:216: `enabled_for(root) && db_path.is_none() &&
  programs.len() <= 1`).
- The daemon engine: unchanged; new RPCs take the same engine lock the tick
  holds. Warm ticks are ~35ms post perf-arc, so hover-during-tick stalls are
  bounded; a read-only replica connection is future work, noted not planned.

## Storage

No new tables. Mute state already lives in the engine's diag_mute rel; the
publish-seam filter moves conceptually daemon-side by having `diag` return
pre-filtered rows plus the mute list (client keeps zero mute state). The
`cache.db` WAL-sharing default for --lsp (cli/mod.rs:280-291) stays only for
the Local arm; the Daemon arm opens NO db.

## Stages

- **T1 (S) daemon RPCs**: `refs`, `saved`, `repo_roots`, `diag_mute`;
  `definition`/`hover`/`diag` param widening ({path,byte} + extras array).
  Each handler is a thin `match` arm over existing engine methods. e2e per
  RPC in tests/it/ (mcp_daemon.rs pattern: tick-counter proof).
- **T2 (M) LspPump adapter**: gate + Daemon arm in run_lsp; every handler
  routes through the pump; Local arm compiles the current code paths
  unchanged. Existing lsp e2e (16) run against the Local arm (hermetic,
  --no-daemon); new tests/it/lsp_daemon.rs drives the Daemon arm end to end
  (spawn daemon, dl --lsp, hover/refs answered, daemon tick_count moved,
  LSP process opened no db file).
- **T3 (S) reconnect + respawn**: kill the daemon mid-session -> next request
  reconnects via ensure_daemon (respawn), resubscribes, full republish. Test:
  kill -9 the daemon pid in-test, assert hover still answers and a new pid
  exists. Stale-binary case is ensure_daemon's existing build_id path; test
  by faking exe mtime is brittle, so assert the code path via the
  ensure_daemon unit seam instead.
- **T4 (S) retire divergence debt**: drop the LSP arm of the cache.db
  default gate comment; update docs/lsp.md + the CLAUDE.md served-copy
  ledger item (a stale daemon now self-heals on the next LSP start or
  one-shot; a stale RESIDENT LSP heals on editor reload, which the vsix
  install flow already requires).
- **T5 (docs) the law**: one paragraph in docs/architecture or CLAUDE.md
  style notes: new mode flags bind a transport client-side and MUST route
  engine work through the daemon RPC with a Local fallback arm
  (Pump-shaped); never a second resident engine on a shared db.

Wave-3 item A9 (route dl/query to the daemon socket) is subsumed by T2 and
should be checked off with it.

## Risks / decided trade-offs

- Hover latency gains a socket round trip (tens of microseconds; noise).
- Requests during a long cold tick block on the daemon engine lock; today
  they block on the WAL writer lock instead, so no regression, but note it.
- didSave double-tick (RPC + watcher event): digest skips make the loser a
  no-op; do not add dedup machinery.
- `parse_request` (daemon.rs:1169) drops non-u64 ids: fine, the adapter owns
  its id space; do NOT widen the daemon codec for this plan.
- Multi-window: N editor windows = N thin adapters on one daemon (today: N
  full engines). Subscriber list already handles N.

## Verification

- tests/it/lsp_daemon.rs (T2/T3 above) + per-RPC e2e (T1).
- Existing 16 lsp e2e stay green on the Local arm unchanged.
- Manual: `dl daemon status` tick_count moves when the editor saves; kill
  the daemon while hovering; `cargo install` a new binary, reload the editor
  window, confirm the daemon respawned with the new build_id without any
  manual `dl daemon restart`.

## Critical files

- src/lsp.rs (run_lsp 39, handlers 129-620, subscriber 252)
- src/daemon.rs (handle_request 1176, ensure_daemon 1654, build_id 1044,
  broadcast_diag_changed 435)
- src/mcp.rs (Pump 42, gate 216: the shape to mirror)
- src/rpc.rs (framing; unchanged)
- src/cli/mod.rs (db default gate 280-291, --lsp dispatch 298)
- src/engine/mod.rs (refs_lens 2607, diags 2868, mute triad 4300-4336,
  extraction_drops 2923)
- tests/it/mcp_daemon.rs (test pattern), tests/it/lsp_refs.rs (Local-arm
  coverage that must not move)
