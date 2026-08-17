---
created: 2026-08-17
updated: 2026-08-17
type: epic
owner: hafley66
status: open
priority: normal
labels: [lab, research]
---

# Lab: OpenAPI spec -> dl6 -> clap CLI + HTTP-over-UDS server, CLI mimics the API

## Description

## Use case
One OpenAPI YAML spec is the source. From it, generate (via dl6 rules over the
spec plus popular Rust crates, never a bespoke generator):
1. a clap CLI whose verbs/args mirror the HTTP surface (one verb per operation),
2. an HTTP server bound to a Unix domain socket file (never a TCP port),
3. a client where the CLI mimics the HTTP API byte-for-byte (same JSON bodies,
   same status mapping to exit codes).
The engine's own `5_emit_openapi.pl` already emits OpenAPI from dl6 programs;
this epic reads the other direction as well (spec in) and closes the loop.

## Research questions (build-vs-buy law: candidate-by-candidate, no one-line dismissals)
- OpenAPI parsing in Rust: `openapiv3`, `oas3`, `utoipa` (emit side), `progenitor`, `openapi-generator` (java), `paperclip`; which model the spec faithfully and which round-trip.
- clap generation from a spec: `clap` derive vs builder at runtime from parsed spec, `clap_complete`, `clap-serde`; whether `progenitor`'s CLI output fits.
- HTTP over UDS: `axum` + `hyper-util` `UnixListener`, `hyperlocal`, `tower`; client side `reqwest` uds feature vs `hyper` client over `UnixStream`.
- Where dl6 sits: the spec as EDB rows (`sh`/`json_each` host over the yaml->json), operations/params/schemas as rels, dl6 rules derive the clap tree and the router table; emitters (`7_emit_ts_types.pl`, `8_emit_rust_types.pl`) render. Name every place a new construct would be needed and STOP there (lang design is Chris in the room).
- Prior art in tree: `docs/bootstrap-typegen-lab-vs-typespec.md`, `plans/2026-08-16-typespec-parity-typegen.PLAN.md`, `dl/fixtures/pokeapi_shape.dl6`, `sprefa-lanes/pokeapi.openapi.yml`, `docs/daemon.md:232` (axum router), `docs/effect-inventory.md` (uds.rs sites).

## Deliverable of the first lane (research only, no code)
`plans/2026-08-17-openapi-clap-uds.PLAN.md` + `.visual.human.unga.md`: TOC,
candidate table per question (crate / version / what it gives / what it lacks / verdict),
the dl6 rel schema for the spec (rel decls only), the pipeline mermaid,
a minimal end-to-end lab plan (pokeapi spec -> CLI verb `pokemon get <name>` over UDS),
open forks for Chris.
