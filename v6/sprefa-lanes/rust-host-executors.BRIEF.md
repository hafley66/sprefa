# rust-host-executors

Goal: the constant Rust harness (`v6/sprefa-engine-rs`) executes hosts live,
with `sprefa-extract` LINKED in-process as an executor, mirroring the tsv2
seam. Today host answers are scripted arrivals in the schedule and the runtime
never runs a host (receipt: `conformance/fixtures/4_struct_values.pl:437`
carries `+'__host_response_scan_span'(...)` by hand; zero `host` hits in
`sprefa-engine-rs/src/` and in `emit_rust.pl`).

## First action
`git merge --ff-only 2bf605615cbe0d2e6957c97ff80b964376da1358`. Failure = STOP
AND REPORT.

## Files you own
- `v6/prolog/emit_rust.pl`
- `v6/sprefa-engine-rs/src/**` and `v6/sprefa-engine-rs/tests/**`
- `v6/sprefa-engine-rs/Cargo.toml` (adding the path dep on `sprefa-extract`)
- `v6/sprefa-engine-rs/grade.sh` ONLY if a new receipt leg is added; never
  weaken existing checks.

FORBIDDEN: `v6/tsv2/**`, `v6/prolog/emit_ts.pl`, `v6/prolog/1_host_expand.pl`
edits (read it, reuse it, do not change it), `graded.tsv` except via
`RUST_GRADE_WRITE_GRADED=1` with the diff pasted in your report, `CLAUDE.md`.

## Read first (recon receipts, cite line numbers in your report)
1. `v6/tsv2/serve/1_hosts.ts` whole file: `HostExecutor` type (:250), registry
   (:261), the fold/applicative grouping (:274), output decode shapes (:277 on),
   and the HostRunner tick contract (:498 on). Your Rust runner mirrors this
   contract; name every place you diverge and why.
2. `v6/prolog/emit_ts.pl:391-470`: how `host_plans` rows are emitted from
   `compile_host_decl/2` (`1_host_expand.pl`) + `host_execution/3`
   (`compile/registry.pl:339-351`). Reuse BOTH predicates; the Rust emitter
   must not restate them.
3. `v6/sprefa-engine-rs/src/types.rs:283` (`ProgramJson`),
   `src/program.rs:18` (`GenProgram`), `src/bin/emit_rust_harness.rs` (how a
   schedule drives ticks).
4. Fixture `struct_host_output_schedule_answer_interned`
   (`4_struct_values.pl:414-440`): the shape your live test replays.

## Design contract (signatures first; pseudo-code stays a comment under each)

```rust
// types.rs -- field names MUST match emit_ts.pl:446-462's JSON spelling
pub struct HostColumn { pub name: String, pub r#type: String }
pub struct HostPlanData {
    pub name: String,
    pub inputs: Vec<HostColumn>,
    pub outputs: Vec<HostColumn>,
    pub template: String,
    pub executor: String,
    // plus every other field emit_ts emits; enumerate them from the code
}

// hosts.rs (new)
pub trait IHostExecutor {
    // run one command line; each returned String is one stdout line
    fn run(&self, plan: &HostPlanData, command_line: &str,
           env: &BTreeMap<String, String>) -> Result<Vec<String>, HostError>;
}
pub fn host_executors() -> &'static [(&'static str, &'static dyn IHostExecutor)];
// entries: "shell" (std::process::Command),
//          "sprefa_extract" + "sprefa_extract_repo" (IN-PROCESS call into the
//          sprefa-extract crate; subprocess fallback is a defect, the whole
//          point is the linked twin)
```

Lifetimes: executors are stateless statics; the runner owns per-tick demand
state only, dropped at tick end, same as tsv2's runner.

Sequence per tick: demand rows appear -> group per ApplicativeExecutors fold
rule -> run -> decode stdout by the same three shapes tsv2 tries (JSON array of
objects, JSONL, plain lines) -> inject as `__host_response_<name>` arrivals in
the SAME tick semantics tsv2 uses (read HostRunner, state the tick number
contract explicitly in your report).

## Emit side
`emit_rust.pl`: add `host_plans` to `PROGRAM_JSON` via `compile_host_decl/2` +
`host_execution/3`, field spelling identical to `emit_ts.pl:446-462`.
`ProgramJson`/`GenProgram` gain the matching serde field (default empty so
every existing emitted program still parses).

## Harness
`emit_rust_harness` gains `--live-hosts`: schedule rows for
`__host_response_*` are rejected as a defect in this mode; the runtime must
produce them. Without the flag, behavior is byte-identical to today.

## Tests, fail-first, all in `v6/sprefa-engine-rs/tests/`
1. Live-host happy path: the `scan_span` fixture shape with the scripted
   response REMOVED, a shell executor echoing the same JSON payload, assert the
   tick log equals the oracle bytes. Write it BEFORE the runner exists; paste
   the failing output in the test header per repo TEST-header convention.
2. Sabotage: executor emits a wrong span -> tick log differs -> test asserts
   the difference is detected (not silently accepted).
3. Linked-twin receipt: `sprefa_extract` executor invoked in-process on a
   temp file, no child process spawned (assert via /dev/null PATH or a
   process-count check).
4. `--live-hosts` + scripted response row = error naming the row.

## Validation (run all, paste outputs verbatim)
```bash
bash v6/sprefa-engine-rs/grade.sh          # MUST stay graded=392 byte-clean=285 rc=0
cd v6/sprefa-engine-rs && cargo test
cd v6/prolog/conformance && swipl -g go -t halt go.pl | tail -3   # 392 PASS
```

## Style laws (repo, non-negotiable)
- No `eprintln!` in `src/**`; `tracing` only.
- Comments state only constraints the code cannot show; no change-log narrative.
- Banned identifiers and prose: provenance, substrate, load-bearing, regime.
- Descriptive names, never single-letter.
- N+1: collect rows, one insert call.
- Commit in logical units; final commit message lists the gate numbers.

## Report format
Coworker-with-zero-context brief: what exists now, where, one next action.
Cite every claim as path:line. If a step is impossible, STOP, report the
throw site, do not work around it.
