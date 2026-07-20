# Typed template bootstrap lab

Standalone Rust binary. `Cargo.toml` has no path dependencies, Soufflé
dependencies, or network-required dependencies.

The checked-in `schema.dl` exercises:

- aliases and literal unions;
- records, `Array`, `Map`, and `Optional` types;
- brace slots `{name: Type}` and colon slots `:name`, with source spelling,
  names, positions, spans, and normalized slot types retained;
- HTTP and channel consumer declarations;
- typed bind, match, destructure, composition, slot enumeration, and record
  path enumeration.

Run the lab from the repository root:

```sh
cargo test --manifest-path labs/bootstrap-typegen-lab/Cargo.toml
cargo run --manifest-path labs/bootstrap-typegen-lab/Cargo.toml -- check labs/bootstrap-typegen-lab/schema.dl
cargo run --manifest-path labs/bootstrap-typegen-lab/Cargo.toml -- generate labs/bootstrap-typegen-lab/schema.dl
```

`generate` writes `target/bootstrap-generated/models.rs`, `server.rs`,
`models.mjs`, `client.mjs`, `client-smoke.mjs`, and `facts.txt`. The Rust
server uses the normalized pattern table for route matching. The JavaScript
client uses the same normalized pattern for URL construction. The generated
model Rust source is standalone and can be checked with `rustc`.

```sh
rustc labs/bootstrap-typegen-lab/target/bootstrap-generated/server.rs \
  -o labs/bootstrap-typegen-lab/target/bootstrap-generated/server
node --check labs/bootstrap-typegen-lab/target/bootstrap-generated/client.mjs
```

`bootstrap` emits the stage-zero semantic model Rust types and
`bootstrap-boundary.txt`. Stage-one self-regeneration stops at the exact
parser/emitter boundary: parsing, semantic lowering, and emitters remain
trusted Rust modules. A copied parser or emitter body would not constitute
self-regeneration, so no fake `stage1.rs` artifact is emitted.

The fixed Rust fact vocabulary records type kinds, slots, consumers, and typed
paths. Rule evaluation is embedded in the binary and performs one deterministic
fact saturation pass. User-authored rules, recursive bootstrap generation,
HTTP transport behavior, authentication, retries, source maps, LSP support,
and production package integration remain outside this lab.
