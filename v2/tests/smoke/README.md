# smoke

Bash smoke scripts covering the sprefa-server HTTP surface end-to-end:
start the server, round-trip over the unix socket, shut it down via
`sprefa stop`, assert the output shape.

## Layout

```
_common.sh       helpers (paths, state dir, start/stop)
_0_status.sh     GET /status via `sprefa status`
_1_run.sh        POST /run (SSE) against examples/g1_fns.sprf + ext/sem
_9_all.sh        driver — runs every _N_*.sh
.state/          per-run dir (gitignored): server.json, socket, store, logs
```

Each script is isolated: it resets `.state/`, starts its own server, runs
its checks, and tears the server down via `sprefa stop` (SIGTERM fallback).

## Running

```bash
cd v2
cargo build --bin sprefa-server --bin sprefa --bin sprefa-lsp
bash tests/smoke/_9_all.sh
```

Skips: `_1_run.sh` skips if `../ext/sem` is absent.

## What is not here

LSP-over-WebSocket is covered by the Rust test `v2/tests/lsp_smoke.rs`,
which spawns the stdio `sprefa-v2-lsp` binary and drives it with framed
JSON-RPC. The WS transport is a thin byte-pump; the interesting logic
(DocSession, hover, diagnostics) lives in `server/_4_transport_lsp.rs::Backend`
and is identical across transports. A daemon-backed LSP smoke lands when
D5 adds server auto-spawn — that smoke belongs in Rust, not bash, because
WebSocket + Content-Length reframing is painful to script.
