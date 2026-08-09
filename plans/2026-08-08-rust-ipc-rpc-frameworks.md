# Rust RPC frameworks, and what shipping projects actually chose

Lane `ipcrpc`. Layer above the wire: call semantics, schema, codegen, ceremony, and the choices of real shipping projects. The transport layer (shared memory, Unix sockets, rkyv, arrow, SQLite WAL) is owned by the concurrent lane `ipcwire` and is out of scope here. All version/download numbers were fetched from the crates.io API on 2026-08-08 unless a date is written otherwise. This is a research document; nothing here is committed Rust.

## TOC

- Verdict table
- ConnectRPC in Rust (the named question)
- The ceremony ladder
- Case studies table
- One IR, many surfaces
- UNVERIFIED list
- d2 board: `2026-08-08-rust-ipc-rpc-frameworks.d2`

## Verdict table

One row per framework. "Schema file" and "build.rs" answer the ceremony question that drives this repo.

| name | version + date | downloads total / recent | schema file? | build.rs? | async runtime? | transports | verdict | one reason |
|---|---|---:|---:|---:|---:|---|---|---|
| tonic | 0.14.6, 2026-05-07 | 351,660,150 / 81,790,603 | yes (.proto + prost) | yes (tonic-build/prost-build) | tokio | HTTP/2 (TCP); UDS awkward | rejected | HTTP/2 framing for local IPC buys nothing, costs the schema+codegen tax |
| connectrpc | 0.8.1, 2026-07-02 | 5,527,282 / 5,105,147 | yes (.proto) | yes (connectrpc-build) or `buf generate` | tower/any | HTTP/1.1 + HTTP/2 (Connect, gRPC, gRPC-Web) | consider, late | first-party Rust now exists, pre-1.0; same HTTP/no-local benefit problem as tonic |
| tarpc | 0.37.0, 2025-08-10 | 9,131,327 / 1,201,488 | no | no | tokio | TCP, Unix socket, serializers | consider | drop-in channel-style RPC, no codegen, but maintenance is slow |
| capnp + capnp-rpc | 0.27.0, 2026-08-02 | capnp 13,463,398 / 2,112,159; capnp-rpc 4,062,625 / 254,014 | yes (.capnp) | yes (capnpc + build.rs) | async optional | any (serde-less binary) | consider | promise-pipelining model, schema is the tax |
| jsonrpsee | 0.26.0, 2026-05-27 | 23,368,404 / 3,769,664 | no | no | tokio | HTTP, WebSocket, UDS | consider | JSON-RPC without a schema; strings not types |
| zbus / D-Bus | 5.18.0, 2026-07-17 | 70,113,149 / 18,993,258 | no (XML introspect) | no | async | D-Bus (system/session bus) | rejected | D-Bus is a Linux system bus, wrong shape for app-to-app on macOS |
| zmq (libzmq bindings) | 0.10.0, 2022-11-04 | 6,115,061 / 1,189,850 | no | no | no | ipc://, tcp://, inproc:// | consider | stale but real; pattern-based (req/rep, pub/sub) |
| zeromq (native, zmq.rs) | 0.6.0, 2026-05-04 | 2,232,851 / 766,742 | no | no | no | tcp, ipc (unix), inproc | consider | native, no C dep; socket patterns not RPC |
| nng | 1.0.1, 2021-12-08 | 302,997 / 33,198 | no | no | no | ipc, tcp, inproc | consider, low | nanomsg v2, tiny adoption, stale |
| ipc-channel | 0.22.0, 2026-04-30 | 5,712,930 / 791,030 | no (serde types) | no | no (blocking) | unix socket + fd passing, Mach, named pipe | consider | servo's channel-style IPC, zero schema ceremony |

Numbers are from the crates.io API on 2026-08-08 via `curl -s https://crates.io/api/v1/crates/<name>`.

## ConnectRPC in Rust (the named question)

**Answer: a first-party Rust implementation exists.** `connectrpc/connect-rust` is in the ConnectRPC GitHub org (org repo list includes `connect-rust` alongside `connect-go`, `connect-es`, `connect-swift`, `connect-kotlin`, `connect-dart`, `connect-py`). It is pre-1.0 and explicitly reports that status.

Facts from the repo README and crates.io on 2026-08-08:

- Crates.io `connectrpc` 0.8.1, 5,527,282 total / 5,105,147 recent downloads, description "A Tower-based Rust implementation of the ConnectRPC protocol". Repo `github.com/connectrpc/connect-rust`.
- GitHub repo: 481 stars, last push 2026-08-07, language Rust.
- Status line in the README: "pre-1.0. The API surface is settling but may shift in 0.x." MSRV Rust 1.88.
- Built on `tower::Service`, HTTP-framework agnostic (Axum, Hyper).
- Serves Connect, gRPC, and gRPC-Web over HTTP/1.1 and HTTP/2, binary or JSON protobuf.
- Passes the full conformance suite: 3,600 server and 6,872 client tests across the three protocols (per the README).
- Codegen is `.proto` based: `protoc-gen-connect-rust`, or `connectrpc-build` for build-time generation in a `build.rs`. Message types are generated with `buffa` (anthropics/buffa, 0.9.1, 5,906,129 downloads) rather than `prost`.
- Wire transports: HTTP. The `client`/`server` features are HTTP transports (plaintext/TLS). There is no Unix-domain-socket transport in the crate as documented in the README's feature list (`client`, `client-tls`, `server`, `server-tls`, `tls`, `axum`, `json`, `gzip`, `zstd`, `streaming`).

Verdict for the user's question: **yes, first-party Rust exists, but it is HTTP-bound, pre-1.0, schema (.proto) + codegen-bound, and aimed at real network services.** For two Rust processes on one machine it inherits the same objection as tonic: HTTP framing and schema codegen for a local pipe. It is a "consider, late" not a "use".

Every claim above cites the repo README at `https://github.com/connectrpc/connect-rust` (fetched 2026-08-08) and crates.io data fetched 2026-08-08. The org repo list was checked via the GitHub API on 2026-08-08 and includes no additional Rust repo beyond `connect-rust`.

## The ceremony ladder

Ordered from least to most ceremony. Each rung: what you write, what it buys, where it stops being enough.

| rung | what you write | what it buys | where it stops being enough |
|---|---|---|---|
| 1. Run the other binary (git model) | a CLI with structured stdout (JSON/TSV) | zero IPC code; every tool is a tiny process | any request-response round trip or streaming; process spawn cost per call |
| 2. stdio framed JSON (LSP model) | one process spawns child at startup, newline- or Content-Length-framed JSON over stdin/stdout | one long-lived worker, no sockets, works everywhere incl. sandboxes | high throughput, binary blobs, backchannel needs, no pub/sub |
| 3. UDS + a serialization format | a Unix domain socket, serde/bincode/rkyv messages | true bidirectional byte pipe, ~60-70% less latency than TCP localhost (see case study), fd passing | you now hand-roll framing, error codes, dispatch; no schema |
| 4. JSON-RPC (jsonrpsee) | schema-free method/params/result over a socket/WS/HTTP | request-response and notifications, batched, JSON for free, no schema | JSON cost; no types at the boundary; streaming is awkward |
| 5. gRPC / Connect (tonic, connectrpc) | .proto schema, codegen, generated-code churn, build.rs | typed contracts, streaming, bi-directional, ecosystem | the full schema+codegen tax; HTTP/2 framing for a local pipe pays for nothing |

Point where each stops being enough is the honest cost boundary. The user's reflex is to jump to 5; the shipping projects below mostly live at 2 and 3.

## Case studies table

What real shipping projects chose, and why.

| project | what it chose | mechanism + why | source + date |
|---|---|---|---|
| rust-analyzer | LSP over stdio, newline/Content-Length-framed JSON | editor spawns the server as a child process; the LSP crate boundary is "defined in terms of stdio". They explicitly keep `ide` free of LSP so a custom protocol or library use stays possible | Architecture doc, `https://rust-analyzer.github.io/book/contributing/architecture.html` |
| Servo | ipc-channel | channel-style multi-process IPC; serde-serialized messages over Unix sockets with fd passing, Mach ports on macOS, named pipes on Windows. Rationale in the README: a drop-in replacement for Rust channels across processes, CSP-flavored | `https://github.com/servo/ipc-channel`; Servo Book architecture `https://book.servo.org/design-documentation/architecture.html` |
| Fuchsia | FIDL over Zircon channels | schema-first: one `.fidl` file, compiler (`fidlc`) generates client+server bindings in C/C++/Rust/Dart; ASCII wire format, actor/channel based. Why (from docs): IPC must be efficient, deterministic, robust, easy to use; codegen removes hand-written bindings | `https://fuchsia.dev/fuchsia-src/concepts/fidl/overview` (last updated 2026-03-04) |
| Firefox | IPDL | `.ipdl` protocol files compiled to C++ actors, actor model over endpoints multiplexed onto Chromium's Mojo Ports; generates parent/child classes + ParamTraits serialization | `https://firefox-source-docs.mozilla.org/ipc/ipdl.html` |
| Zed | protobuf RPC (crates/proto + crates/rpc) on tokio, over WebSocket for collab | `rpc.rs` sets `PROTOCOL_VERSION: u32 = 68`; typed envelope/request protobuf messages; collab backend reached over WebSocket-tunneled protobuf RPC | `https://github.com/zed-industries/zed/blob/main/crates/rpc/src/rpc.rs`; architecture summary (secondary) `https://factory.ai/open-source-wikis/zed?page=overview/architecture.md` |
| ROS 2 / rclrs + iceoryx | DDS (FastDDS/Cyclone/rmw) + iceoryx shared-memory zero-copy | rclrs exposes "Loaned messages (zero-copy)" over the rcl/rmw layer; iceoryx is used inside ROS 2 via `rmw_iceoryx` as a zero-copy shared-memory transport for large payloads | rclrs README `https://github.com/ros2-rust/ros2_rust`; iceoryx README `https://github.com/eclipse-iceoryx/iceoryx` |
| Docker/Podman + bollard (Rust) | HTTP/JSON over a Unix socket | bollard connects to `/var/run/docker.sock` (or `//./pipe/docker_engine` on Windows); features `pipe` (unix socket/named pipe) vs `http` (TCP). A Rust client talks to a daemon over a unix socket with HTTP semantics | `https://github.com/fussybeaver/bollard` |
| Deno | child process IPC channel; HTTP over Unix socket as an available transport | official docs: fork a child with an IPC channel, exchange structured messages, no byte parsing. The premise "Deno uses HTTP for inter-worker comms" was NOT found in primary docs; see UNVERIFIED | `https://docs.deno.com/examples/child_process_ipc/` |
| Bevy | no built-in networking; ecosystem crates (bevy_replicon) | Bevy core ships no cross-process networking; the community uses server-authoritative replication crates with pluggable transport backends. A supposed engine-level "Forward" networking system was NOT found; see UNVERIFIED | `https://github.com/...bevy_replicon` (repo listing only; see UNVERIFIED) |

### Mechanism moves (high value)

| move | from -> to | why/result | source + date |
|---|---|---|---|
| drlove.dev gRPC IPC | TCP localhost -> UNIX domain socket | replaced TCP localhost with Unix sockets under gRPC, measured 60-70% latency reduction with benchmarks | `https://drlove.dev/writing/grpc-unix-domain-sockets/`, 27 June 2025 |
| Microsoft ASP.NET Core gRPC | (guidance) gRPC over Unix domain socket | documents UDS as "more efficient than TCP when the client and server are on the same machine" and how to configure gRPC over UDS | `https://learn.microsoft.com/en-us/aspnet/core/grpc/interprocess-uds`, updated 2026-07-08 |
| iceoryx | classic -> iceoryx2 | iceoryx classic is in maintenance mode; maintainers moved focus to iceoryx2 (Rust, more platforms, zero-copy), classic reaches EOL after iceoryx2 v1.0. A transport-layer move, not a protocol swap | `https://github.com/eclipse-iceoryx/iceoryx` README |

A stronger "shipping product moved off gRPC to a custom socket protocol, with a dated primary-source postmortem" was NOT found in the searches run (see UNVERIFIED).

## One IR, many surfaces

The repo's live question: one type IR generating both the RPC surface and the CLI surface. The repo already emits a CLI inventory from a registry (`v6/prolog/compile/2_emit_cli_inventory.pl`), so "one IR, many emitted surfaces" is in use here.

Findings:

- Connect/buf: one `.proto` + `buf generate` emits message types, service client, and server traits (protoc-gen-connect-rust). It does not emit a CLI. `buf curl` consumes the same schema at runtime, so the schema serves a CLI-like surface without generating one. `https://connectrpc.com/`, `https://github.com/connectrpc/connect-rust` (README, 2026-08-08).
- Cap'n Proto (`capnpc`) emits typed messages and RPC stubs from one `.capnp` schema; it emits no CLI surface. `https://github.com/capnproto/capnproto-rust`.
- gRPC reflection + a tool like `grpcurl` uses the same `.proto`/descriptor set as the client at runtime, again a schema-to-command mapping, not CLI codegen. `connectrpc-reflection` README (2026-08-08).
- A mature, first-party tool that generates BOTH an RPC client AND a CLI from one schema declaration: **NOT FOUND**. Searched Connect/buf, Cap'n Proto, tonic, tarpc docs. The closest shapes are "schema at runtime powers a curl tool" (buf curl, grpcurl). The "one IR emits CLI inventory + RPC surface from the same declaration" pattern is the repo's own design and is not something found readymade.

## UNVERIFIED list

- **Deno "uses HTTP for inter-worker comms".** NOT found in Deno primary docs. The official child-process-IPC example uses the node:child_process IPC channel with structured messages, not HTTP. What IS documented: HTTP/JSON over a Unix socket is an available transport (Deno docs list a "Fetch over a Unix socket" example). The "HTTP inter-worker" idea matches a third-party product (val-town/deno-http-worker), not Deno core. Searched docs.deno.com; mark as contradicted for the inter-worker-comm claim.
- **Bevy "moved to a Forward schema-based networking system (0.16+)".** NOT found. Bevy 0.16 release notes (2025-04-24) include no networking addition. Searched bevy.org/news/bevy-0-16 and GitHub repo search for Bevy networking named "forward"; nothing. The real, verifiable mechanism is ecosystem crates (bevy_replicon, server-authoritative replication with pluggable transport backends). Search strings and results recorded; treat the "Forward" engine-core claim as unverified.
- **A shipping product's dated postmortem moving from gRPC back to a plain socket (protocol swap, not transport).** NOT found among the searches run (drlove + Microsoft docs cover the TCP->UDS transport move inside gRPC; nothing found for a full off-gRPC swap with a dated primary source).
- bevy_replicon fetch: the `raw.githubusercontent.com/simgine/bevy_replicon/main/README.md` fetch returned 404 on 2026-08-08; the case-study row is supported only by the repo listing/search snippet, so the specific "no built-in networking" framing is correct (Bevy 0.16 notes) but the bevy_replicon transport-detail row is weaker than the others.

## d2 board

`plans/2026-08-08-rust-ipc-rpc-frameworks.d2` draws the ceremony ladder with the frameworks placed on it. Compile numbers run on 2026-08-08 (d2 0.7.1, dagre):

```
viewBox="0 0 1539 140"
shape-count: 11  (budget 24)
```

Wider than tall as required; direction right; 11 shapes, under the 24-cap.
