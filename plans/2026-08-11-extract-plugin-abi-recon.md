# Extract plugin ABI / shared-memory recon

Base: `91c5ea6e`. Read-only recon, no source edits.

**Where we left off: nothing was ever designed.** `sprefa-extract` has exactly
one non-Rust-artifact crossing today: SCIP protobuf index files written by an
external indexer subprocess and read back off disk after the subprocess exits
(`v6/sprefa-extract/src/scip.rs:52-70`, `src/scip_decode.rs:29`). No plugin
ABI, no shared memory, no wasm runtime, and no daemon mode exist in the crate.
The two IPC research docs the user's question descends from
(`plans/2026-08-08-rust-ipc-transports.md`, `plans/2026-08-08-rust-ipc-rpc-frameworks.md`)
are general "two Rust processes on one machine" research, dated the same day,
and say on their own first line they are not scoped to `sprefa-extract` and
commit no Rust.

## Table of contents

1. [The current boundary](#1-the-current-boundary)
2. [What is already written down](#2-what-is-already-written-down)
3. [The four candidate carriers](#3-the-four-candidate-carriers)
4. [The SCIP question](#4-the-scip-question)
5. [The gap statement](#5-the-gap-statement)

## 1. The current boundary

`sprefa-extract` is a single-shot CLI, never a server. Process shape:

- Binary `extract` (`v6/sprefa-extract/Cargo.toml:107-110`, `src/bin/extract.rs`).
  `main()` parses argv with clap, runs exactly one of several modes, prints
  JSONL to stdout, and exits (`src/bin/extract.rs:251-339`, no listen/serve
  branch anywhere in that function).
- `AGENTS.md:3` states the design directly: "No daemon, no database, no
  network, no watchers."

**In**: a file path from argv, read with `std::fs::read(path)?`
(`src/bin/extract.rs:313`). Optionally an external `index.scip` protobuf file,
either supplied by path (`--scip-index`) or built by spawning a third-party
indexer subprocess (`--scip-build`) whose stdout is discarded and whose output
file is read back after the child exits
(`src/scip.rs:52-70` `attempt()`; the child runs in `scip_ensure::run_capped`,
a budgeted `Command`, not a pipe or socket).

**Out**: one `FlatFact` per line, serialized with `serde_json`, printed to
stdout (`src/wire.rs:33-60` `flatten`/`flatten_jsonl`; `src/bin/extract.rs:496-503`
`stream()`). `--schema` prints the JSONL contract and exits
(`src/bin/extract.rs:533-535`).

**Dispatch is static, not pluggable.** `dispatch(path, content, mask)` looks up
the first matching `Source` in a compiled-in roster and calls it in-process;
nothing is loaded at runtime (`src/dispatch.rs:7-16`, roster at
`src/lang/mod.rs:29-49`). The roster's language front-ends (ast-grep-core,
oxc, syn, tree-sitter-go/-kotlin-sg/-prolog/-md/-html) are all ordinary Cargo
dependencies, statically linked at build time
(`v6/sprefa-extract/Cargo.toml:22-102`, full file). There is no `build.rs` in
this crate: the tree-sitter grammars ship as already-published crates, not as
generated-and-compiled-here artifacts.

**`proto/`, real, used, not vestigial, but narrow.** `proto/scip.proto` (965
lines) is the SCIP wire format, decoded by generated `prost` bindings
committed at `src/scip/scip_proto.rs` and consumed only inside
`src/scip_decode.rs` (`crate::scip_decode` owns the protobuf -> flat-types
decode per `src/scip.rs:26-29`). It is a **plugin-output** format: the one
shape a non-Rust tool (`scip-typescript`, `scip-go`, `rust-analyzer`) already
crosses into this Rust process with. It is not a general plugin ABI: there is
no `.proto` or importer for the crate's own `FlatFact`/JSONL vocabulary, so
nothing outside this crate can feed CST/type/call/df facts in through `proto/`.
Cargo.toml's own comment states why `prost` and not v5's `scip`+`protobuf`
crate pairing: a `thiserror` major-version dependency conflict
(`Cargo.toml:86-95`).

## 2. What is already written down

| plan | date | landed / abandoned / never decided | what it actually says |
|---|---|---|---|
| `plans/2026-08-08-rust-ipc-transports.md` | 2026-08-08 | never decided | General same-machine Rust IPC transport survey (shared memory, sockets, payload formats, kernel mechanisms, databases-as-IPC). Not scoped to `sprefa-extract`. Verdict table only, no code, no crate added anywhere in this repo. |
| `plans/2026-08-08-rust-ipc-rpc-frameworks.md` | 2026-08-08 | never decided | RPC-framework half of the same comparison (tonic, connectrpc, tarpc, capnp-rpc, jsonrpsee, zmq, ipc-channel) plus case studies of real projects. Line 3: "This is a research document; nothing here is committed Rust." |
| `plans/2026-07-30-sprefa-extract-spelunk.md` | 2026-07-30 | landed (as an audit, not a boundary decision) | A capability inventory of the extract crate itself: 10 JSONL record shapes, library-vs-binary parity gaps, and a v5-relation crosswalk (31 `E`, 13 `E+L`, 62 `B` rows). It documents today's in-process boundary; it proposes no cross-process/plugin design. |
| `plans/2026-08-06-rust-emitter-modes.md` | 2026-08-06 | parked, "Nothing here is started" (its own line 3) | A **different** subsystem: whether a compiled `.dl6` **program** loads at runtime as `dylib` vs `wasm` inside a host process. Wasm appears only here, as one undecided option for loading compiled dl6 programs (`:23,38,44`), not for extract plugins. |
| `plans/2026-07-29-sqlite-udf-graft-verdict.md` | 2026-07-29 | landed verdict (a different boundary) | Verdict on registering SQLite scalar functions from the `sprefa-store` TS engine. Finding: the current TS driver (`@libsql/client`) cannot register UDFs at all (`:71`); a "Rust sidecar with rusqlite functions" can, and "rows or projected results cross the process boundary" through it (`:75`). This is the closest landed precedent for "a Rust process and another process share state through SQLite," but the two sides are `sprefa-store`'s TS engine and a Rust UDF sidecar, not `sprefa-extract` and a non-Rust extraction plugin. |
| `plans/2026-07-20-single-db-design-b.md` (no `2026-08-08` version exists; `ls plans/` confirmed) | 2026-07-20 | landed design (unrelated subsystem) | v5 daemon single-DB-per-root consolidation. One line, `:170`, dismisses shared memory in passing ("concurrent `Engine`s converge through the table's PK rather than through shared memory") for multi-`Engine` coordination inside the v5 daemon. Not about `sprefa-extract` or plugins. |

## 3. The four candidate carriers

| carrier | what the repo says | citation |
|---|---|---|
| wasm | No mention anywhere in `v6/sprefa-extract/` (Cargo.toml, AGENTS.md, `src/**` all grepped, zero hits). The only wasm mentions in the repo that are even adjacent are in the **rust-emitter-modes** plan (a different subsystem, parked, undecided) and a WASM-mode SQLite driver probed for `sprefa-store` UDFs (also a different subsystem). | `v6/sprefa-extract/Cargo.toml` (full dependency list, no wasm crate); `plans/2026-08-06-rust-emitter-modes.md:23,38,44`; `plans/2026-07-29-sqlite-udf-graft-verdict.md:74` |
| dense packed array over shared memory | No mention in `v6/sprefa-extract/`. General-purpose research exists one level up: the shared-memory candidate table (iceoryx2, `shared_memory`, `raw_sync`, `memmap2`) and the payload-format table naming `rkyv` for zero-copy reads "straight out of the shared buffer." Scoped to "two Rust processes," never applied to `sprefa-extract`, never decided. | `plans/2026-08-08-rust-ipc-transports.md:25-33` (shared memory table), `:49` (rkyv row) |
| SQLite as the shared medium | No mention in `v6/sprefa-extract/`. The `2026-07-29-sqlite-udf-graft-verdict.md` verdict is the nearest landed precedent, but it is for `sprefa-store` (a different crate) and a UDF sidecar, not extract plugins (see table above, row 5). The general IPC survey separately verdicts "SQLite WAL: use, partial" for row-oriented, millisecond-budget IPC, with a documented single-writer / no-cross-process-notification break. | `plans/2026-07-29-sqlite-udf-graft-verdict.md:69-79`; `plans/2026-08-08-rust-ipc-transports.md:68-76` (verdict row), `:77-136` (the finding) |
| the transport already in `proto/` | Live and used, narrowly. `proto/scip.proto` backs the SCIP index decode path (`src/scip.rs`, `src/scip_decode.rs`, `src/scip/scip_proto.rs`), reached through `--scip-build`/`--scip-index`/`--family scip`/`--scip-facts`/`--scip-deps`. It moves data ONE direction (external indexer subprocess -> file -> Rust decode) and covers only the SCIP vocabulary, not the crate's own `FlatFact` family. | `v6/sprefa-extract/src/scip.rs:1-29`; `src/scip_decode.rs:29`; `src/bin/extract.rs:227-249,294-306` |

## 4. The SCIP question

Phase-1 extraction (the default `extract PATH` path: CST/type/call/df) never
touches SCIP. `dispatch()` calls a `Source` from the static roster directly
(`src/dispatch.rs:7-16`); SCIP only enters through the explicit
`--family scip`, `--resolve --scip-build/--scip-index`, `--scip-facts`, or
`--scip-deps` flags, gated in `main()`
(`src/bin/extract.rs:266-306,343-357`). SCIP is a phase-2, optional,
per-language enhancement: it requires a third-party semantic indexer binary
(`scip-typescript`, `scip-go`, `rust-analyzer`), so it exists only for
TS/Go/Rust today, landed at `task(scip_families, done, [])` in
`v6/prolog/ARCH.pl:895`, whose own comment lists "scip-python/java/clang rows
not ported" as a residual, and Kotlin/Prolog/ast-grep-fallback sources have no
SCIP builder at all (`plans/2026-07-30-sprefa-extract-spelunk.md`'s source
table, row "Prolog" / "Kotlin" both show no SCIP indexer column filled).

A tree-sitter-generated grammar for dl6 would enter this crate as an ordinary
phase-1 `Source`, the same shape as the existing tree-sitter-backed sources
(`tree-sitter-go`, `tree-sitter-kotlin-sg`, `tree-sitter-prolog`, all ordinary
Cargo deps per `Cargo.toml:47-64`, dispatched the same static way). Nothing in
the tree-sitter-door lab's own effort estimate proposes a SCIP indexer for
dl6; its "sprefa-extract integration" line estimates 5-8 engineer-days for
"incremental edit tests, byte/point conversion tests, and extraction parity"
only (`v6/labs/tree-sitter-door/REPORT.md:86`), with SCIP absent from every
row of that table (`:82-88`). SCIP is orthogonal to the tree-sitter-generated
dl6 grammar question.

## 5. The gap statement

| a non-Rust plugin can do today | it cannot | file:line enforcing the "cannot" |
|---|---|---|
| Be compiled to a tree-sitter C grammar, published as a crate exporting `LANGUAGE: LanguageFn`, and statically linked in at Rust build time (the existing pattern for Go/Kotlin/Prolog/Markdown/HTML) | Be loaded or swapped at runtime; there is no plugin loader | `v6/sprefa-extract/Cargo.toml` full dependency list (no `libloading`/`dlopen`-family crate); `src/dispatch.rs:14-15` `source_for` resolves a compiled-in match, not a runtime registry (`src/lang/mod.rs:29-49`) |
| Run as a subprocess and hand facts to `sprefa-extract` **only if** those facts are a SCIP protobuf index written to a file | Stream facts live, or use any wire shape other than SCIP protobuf; there is no importer for the crate's own `FlatFact`/JSONL vocabulary | `src/scip.rs:52-70` (`attempt`: subprocess + file, budgeted `Command`, no pipe/socket); `src/scip_decode.rs:29` (`proto::Index::decode(bytes...)`, a full-file decode, not a stream); `src/wire.rs` exports only `flatten`/`flatten_jsonl` (facts OUT), no importer function anywhere in the crate |
| Invoke `extract` once per file/root and read its stdout | Talk to a long-lived `sprefa-extract` process; there is no server/daemon mode | `v6/sprefa-extract/AGENTS.md:3` ("No daemon, no database, no network, no watchers"); `src/bin/extract.rs:251-339` (`main`, one linear run then exit, no listen/serve branch) |
| nothing found | Move bulk data through shared memory | `v6/sprefa-extract/Cargo.toml` full dependency list, no `memmap2`/`shared_memory`/`raw_sync`/`iceoryx2` |
| nothing found | Run inside or alongside a wasm runtime | `v6/sprefa-extract/Cargo.toml` full dependency list, no `wasmtime`/`wasmer`/`extism` |
