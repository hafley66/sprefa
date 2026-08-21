---
created: 2026-08-19
updated: 2026-08-19
type: feature
status: open
priority: high
epic: openapi-clap-uds-lab
related: ['@engine-rs-serve-uds', '@boop-hosted-in-dl6']
labels:
- pkg:engine-rs
- area:http
---

# Reusable typed Axum UDS route runtime

## Description

Extract the completed Axum Unix-socket serving seam into one reusable typed route runtime. Programs supply RouteSpec rows and a RouteProgram operation dispatcher; the library owns Axum router construction, UnixListener serving, request decoding, response encoding, shutdown, and errors. OpenAPI and clap consume the same RouteSpec rows. Generated programs do not emit bespoke Axum router implementations. Plan: plans/2026-08-17-openapi-clap-uds.PLAN.md section 6.1.\n\n## Acceptance Criteria\n- [ ] RouteSpec carries stable operation id, method, path, input TypeId, and output TypeId.\n- [ ] RouteProgram provides typed operation dispatch independent of Axum.\n- [ ] Duplicate method/path pairs receive a typed construction error.\n- [ ] One router/serve_uds implementation serves both static Rust route tables and ProgramJson-loaded route tables.\n- [ ] Socket paths remain runtime deployment state outside schema identity.\n- [ ] Library UDS test executes request decode, engine dispatch, response encode, and shutdown.\n- [ ] PokeAPI emitted-program CI uses the shared runtime.\n- [ ] Boop hosting can implement the same dispatcher without another HTTP server.\n\n## Tests Run\n\n## Implementation Notes
