---
created: 2026-08-17
updated: 2026-08-18
type: task
status: done
priority: normal
epic: openapi-clap-uds-lab
related: ['@boop-hosted-in-dl6']
labels: [lab]
closed: 2026-08-18
---

# engine-rs: typed HTTP-over-UDS serve seam, dl6 program lowers to a socket-file server

## Description

## Arc (user decision 2026-08-17: "okay yes")
dl6 program -> emitted Rust (`emit_rust.pl`) + `.types.rs` + one new library
seam in `v6/sprefa-engine-rs` = a typed HTTP server on a Unix domain socket
file. No clap, no OpenAPI crate, no new emitter, no separate lab binary. yaml/toml/json are one sprefa-extract `data` family, the v5 `src/datapath.rs`
plane ported (extension dispatch, tree-sitter json/yaml/toml-ng, byte spans),
hosted on both doors like every family; `decode/2` is the query. No
conversion step (user 2026-08-17). The
CLI comes later as a generic client of that socket (a rel read for `--help`).

## Steps, in order
1. `.types.rs` all 342 compile under rustc (today 5 fail: `pub where`,
   E0428 duplicate struct, 3x E0392). Source: docs/audits/2026-08-17-userland-typegen.md finding 2. Cost S.
2. `sprefa-engine-rs::serve`: axum 0.8 router on `tokio::net::UnixListener`,
   copy the shape of v5 `src/daemon/shell/http.rs:117-142` (one Router, two
   listeners, graceful shutdown). Routes: `GET /rel/<name>` reads rows typed
   by `.types.rs`; `POST /arrive` posts an arrival batch and folds a tick
   through `driver.rs`. Cost M.
3. Rust golden: emit the pokeapi spec program, boot it on a socket, `curl
   --unix-socket` one rel, byte-diff against the oracle rows. Cost M.

## Rails
- Every `.dl6` snippet carries its rx lowering. Server side:
  `arrivals$.pipe(concatMap(batch => driver.tick(batch)))`, reads are a
  `latest` projection.
- Infra bought: axum + hyper-util + tokio, nothing hand-framed.
- Surrogate keys, no coercions, no eprintln, 10-second law.

## Related
@openapi-clap-uds-lab (this is its first arc), @boop-hosted-in-dl6 (boop
verbs become rows on the same server later), plan
plans/2026-08-17-openapi-clap-uds.PLAN.md fork F0.
