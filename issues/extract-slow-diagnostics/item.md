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
