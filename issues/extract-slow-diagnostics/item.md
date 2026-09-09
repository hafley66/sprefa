---
created: 2026-09-08
updated: 2026-09-08
type: bug
reporter: codex
status: untriaged
priority: normal
provenance: codex
source_ref: falcon-lab-extract-slow-2026-09-08
---

# extract slow loses actionable rust-analyzer failure diagnostics

## Observed failure

While testing semantic reads before a Rust game-lab refactor, `extract slow` returned exit status 0 but produced only a `scip_skip`. No semantic index or facts were produced. The failure detail retained only a rust-analyzer stack-frame tail, which does not identify the triggering error.

## Reproduction

Repository: `hafley-rs`, working directory at the repository root. Target: `games/blender-godot-sqlite-proof/falcon-lab`.

```sh
probe_dir=$(mktemp -d)
CARGO_BUILD_JOBS=2 extract slow --scip-timeout 45 \
  --scip-cache "$probe_dir/scip-cache" \
  games/blender-godot-sqlite-proof/falcon-lab \
  > "$probe_dir/falcon-extract-slow.jsonl" \
  2> "$probe_dir/falcon-extract-slow.log"
```

Observed stdout, in full:

```json
{"record":"scip_skip","lang":"rust","bin":"rust-analyzer","reason":"failed","detail":"scip indexer failed: 8: __pthread_joiner_wake"}
```

Exit status: 0. Runtime approximately 0.4 seconds. Stderr file empty. The requested temporary SCIP cache directory did not exist afterward. The reproduction above substitutes a generated temporary directory for the original machine-specific output paths. The invocation ran inside Codex's workspace-write sandbox on macOS; sandbox causality has not been established.

## Environment

- Apple M2 Pro, macOS, POSIX `sh`.
- `extract`: Cargo-installed executable on PATH, version 0.1.0, build git `5fa36e78046e`, build datetime `2026-09-06T02:49:11Z`.
- `rust-analyzer`: Cargo-installed executable on PATH, `1.100.0-nightly (17fd5b8a3 2026-08-28)`.
- Target is an isolated Cargo workspace with local path dependencies, optional Godot extension, and Rust simulation/physics/renderer code.
- `extract fast` on six source files succeeded with 746 JSONL facts.
- `rust-analyzer scip --help` succeeded. Direct SCIP indexing and a reduced-thread retry have not yet been tested.

## Expected behavior and investigation

Successful indexing should produce semantic facts. On an indexer failure, preserve an actionable diagnostic: invoked command, exit code or signal, and the leading error/panic plus a bounded stderr tail or saved full log path. A final stack frame alone prevents distinguishing workspace setup, resource limits, sandbox restrictions, and an indexer defect.

Exit 0 with an explicit `scip_skip` is observed behavior, not independently asserted to violate the CLI contract. Consumers must inspect records rather than treating exit 0 as indexing success.

## Acceptance Criteria

- [ ] Reproduce or characterize the rust-analyzer failure using the supplied workspace and installed versions.
- [ ] Preserve the root indexer diagnostic in emitted failure evidence, with bounded output or a durable full-log reference.
- [ ] Add a deterministic failing-indexer test whose stderr contains an initial cause followed by stack frames; verify the cause remains visible.
- [ ] Document the exit-status and `scip_skip` contract for semantic-read callers.

## Tests Run

- [x] `extract fast`: 746 facts from the six-file probe.
- [x] `extract slow`: failure record captured verbatim above.
- [x] `rust-analyzer scip --help`: command available.

## Implementation Notes

Report only. No extract, rust-analyzer, or simulation code changed. Root cause remains unconfirmed.

## Comments

### 2026-09-08T13:17:25Z · @codex

#### Falcon refactor follow-up: fast resolution precision and provider contracts

User direction: ratchet down false positives through small syntax/resolution rules without rebuilding compilers. Record these findings alongside the slow-indexing failure. No extract fixes have been implemented by this reporting task.

##### Reproduction and confirmed evidence

Repository `hafley-rs`, commit `4496f1e`. Paths below are relative to `games/blender-godot-sqlite-proof/falcon-lab/`.

Run `extract fast` with these six files: `1b_boundary.rs`, `27_geometry.rs`, `28_extension.rs`, `35_runtime.rs`, `74_process_peer.rs`, `1c_live_rows.rs`. Inspect `resolved_edge` records with caller_path ending in `74_process_peer.rs`.

Confirmed false positives:

| Source site | Emitted target / provenance | Actual call |
|---|---|---|
| `74_process_peer.rs:29`, `self.0.send_to(message, address)` | enclosing `RelaySocket::send_to`, `same_file`; false recursion | method on wrapped `UdpNonBlockingSocket` |
| `74_process_peer.rs:33`, `self.0.receive_all_messages()` | enclosing `RelaySocket::receive_all_messages`, `same_file`; false recursion | method on wrapped `UdpNonBlockingSocket` |
| `74_process_peer.rs:35`, iterator `.filter(...)` | `1b_boundary.rs:78` SQLite cursor `filter`, `corpus_unique` | `Iterator::filter` |

Confirmed omission: `74_process_peer.rs:135`, `fixture::handle(...)` did not resolve to `35_runtime.rs:145`, although that file was in the supplied universe. Module declarations relevant to alias resolution also live in the crate entry file, which was outside this six-file probe. Do not infer whole-project omission coverage from this reduced universe.

Positive control: `stamp_presented` resolved all five actual call sites across three caller functions: `Runtime::advance`, UDP `run`, and the metadata-preservation test (three calls). This does not establish overall precision or recall.

##### Proposed small precision ratchets, not implemented

1. Receiver-aware fallback gate: for `receiver.method()`, do not turn a same-file/corpus-unique spelling into a resolved method edge when receiver compatibility is unknown. Retain a candidate or unresolved fact with the reason. This trades recall for fewer asserted false edges.
2. Treat `self.field` and `self.0` as distinct receivers from `self`. A containing impl's method cannot be selected solely because its spelling matches a delegated call. Resolve a field's declared type where locally available; otherwise abstain. Do not ban real recursion on `self.method()`.
3. Before name fallback, eliminate candidates whose known impl owner conflicts with the known receiver type. A SQLite cursor method must not win an iterator call merely because it is the only local `filter`. Avoid blacklisting names such as filter/new/publish, which are valid user methods too.
4. Qualified-call handling: use available `mod`, `#[path]`, and `use ... as ...` declarations to follow explicit paths. When module context lies outside the supplied universe, emit an unresolved/scope reason rather than an empty answer.
5. Separate unresolved external receivers, ambiguous internal targets, and missing module context. Name uniqueness within supplied files is not uniqueness in the program.
6. Regression fixtures should assert these false edges are absent AND the five verified metadata edges remain. Include actual `self.method()` recursion, delegated tuple/named fields, duplicate method names, iterator chains, and explicit module aliases. Track precision and recall separately over the labeled cases.

##### Tool and integration observations

- `sem 0.24.0` found the new definition but reported zero callers/impact for `stamp_presented`. Source has five calls. Both sem and extract found the local `handle` caller and missed the UDP qualified call in the probes.
- `sem entities --text ... --json` emitted terminal text. A probe scoped to `35_runtime.rs` also returned a hit in `74_process_peer.rs`. A previous Display test-impact lexical fallback matched an unrelated soopy test.
- `sem context` returned the requested method in a bounded response (92 reported content tokens within a 500-token budget). Compact entity-addressed retrieval remains useful independently of graph completeness.
- `extract --resolve --family call --witness` emitted protocol/run identity, content-digest scope, and partial coverage. Fast-alias witness output on the small fixture did not contain this envelope; invocation parity needs checking before relying on it.
- Git history and semantic diff are outside extract's responsibility. User places temporal/source coordination in soopy and may integrate sem as another extraction provider beside ast-grep/SCIP/rust-analyzer. No sem dependency or integration was implemented.
- Desired common contract: scoped source identity/digest, symbol identity, byte spans, provider and resolution evidence, explicit partial/unresolved results, consistent JSON, and downstream bounded lookup/context projections. Keep lexical candidates distinct from resolved edges. No compiler reconstruction is required to suppress unsupported claims.

##### Measurements and slow-mode update

Three sequential CLI invocations in the existing hafley-rs workspace, without controlled cold-cache isolation: sem batched find 731-782 ms / 992 stdout bytes; sem method callers 263-269 ms / 267 bytes; sem context 293-318 ms / 865 bytes; extract fast six-file stream 109-148 ms / 158423 bytes. These are different operations, not a speed ranking. Facts can remain internal while only a bounded query result reaches an agent.

Falcon slow retry with a 15-second indexer budget failed in approximately 534 ms with the same 134-byte scip_skip and empty stderr. A small dependency-free Rust fixture indexed successfully in approximately 2.8 seconds in the same environment. Therefore slow mode is not universally broken here; Falcon-specific root cause remains unconfirmed.

##### Verification scope

This is a source-inspected sample, not a full-corpus precision benchmark. The Falcon metadata refactor passed all 17 gdext library tests before this follow-up. No tool fixes, semantic policy changes, or compiler changes are authorized by this report alone.

### 2026-09-08T13:51:03Z · @codex

#### Requested capability: structural clone detection over CST statement sequences

User request: detect similar lines/statement sequences in the CST, such as the three Godot mesh-upload/acknowledgment blocks consolidated during the Falcon refactor. User recalls a distance/measure function in older versions. Its existence, location, and suitability have not been verified. Search repository history and prior implementations before introducing a new distance metric or clone detector; record recovered symbols/commits and behavior.

Concrete source reference: `hafley-rs`, parent commit `4496f1e`, `games/blender-godot-sqlite-proof/falcon-lab/godot/2_stage.gd`. Refactor commit `88767b1` consolidates the repeated blocks in `_process`, `_process_external`, and `_process_scheduled` into `_upload_and_acknowledge`.

Proposed extraction facts, not implemented:

- A clone group carries source coordinates/digests, occurrence byte spans, matched statement count, normalization policy, and differing CST nodes.
- First tier: contiguous statement windows with exact structural equality after explicit normalization of formatting and local bindings. Preserve binding relationships when normalizing identifiers. Preserve member names, operators, literal values, and call order by default.
- Second tier: separately labeled near matches with the metric, threshold, and structural differences exposed. The Falcon example includes a temporary `uploaded` binding versus an inlined `mesh.surface_get_arrays(0)` expression; alpha-normalization alone does not make those CSTs equal. Report that difference rather than silently asserting equivalence.
- Detection supplies refactor candidates, not proof that consolidation preserves evaluation order, side effects, scope, or behavior.
- Check parser/grammar availability for GDScript in the installed extract before claiming this exact example is supported.

Suggested bounded follow-up:

- [ ] Locate the historical distance/measure implementation and evaluate reuse against the three real Falcon blocks.
- [ ] Specify clone-group facts and exact versus approximate match provenance.
- [ ] Add positive and negative fixtures: identifier-renamed copies, literal/operator/member changes, statement reordering, and temporary-versus-inline expressions.
- [ ] Bound window sizes, overlap reporting, output volume, and runtime. Avoid claiming corpus-wide precision from the example.

Keep this inside extract's source-fact scope. Git history is used to retrieve the old implementation, not proposed as part of extract's runtime responsibilities. No clone detector or tool code was changed by this report.
