# Brief: one process adapter in tsv2 for extract, soopy, boop; `sh` stays shell; sniffing dies (phase 2)

Plan: `/Users/chrishafley/projects/sprefa/plans/2026-08-18-boop-resident-coroutine.md` (committed on main; read it from your worktree). Read "Decision" and "What exists". Read `CLAUDE.md` standing laws and style laws (rxjs words only; exactly one manual `.subscribe()` per app; async becomes rxjs; interfaces in the header `types.ts` with the `I` prefix; comment budget).

## Base
RESUME: your branch starts at 4fe131ab3 (= 5726d78e5 + ONE unverified WIP commit, also kept as `wip/tsv2-process-adapter-1`) holding ~21 files from an earlier lane that ran out of turn: TS adapter interface + registry, shell-only prolog emission, 8 fixture `.adapters.json` sidecars, a partial Rust adapter-row loader, `docs/hosts-are-arrivals.md`. FIRST action: `git log -2` = 4fe131ab3 on 5726d78e5, tree clean. Read the diff, keep what is right, fix the rest. Its last report: `cargo test --test live_hosts --no-run` passes with unused-executor warnings; TS node:test assertions pass but Vitest says "No test suite found" for node:test files (put the tests in the vitest shape the neighbours use); plunit had 3 NEW failures beyond the 5 known-red, one is `mount_door:source_mutations_fixture_keeps_one_document_boundary_and_exact_approval_join` (soopy `sh source_stage` now `execution: shell`; the Rust door must still route it in-process through the sidecar row, and the fixture needs its `.adapters.json`). Remaining: generated output refresh, Rust boot-time sidecar loading, real extract argv construction, sidecar routing tests, count tests, whole batteries. NEVER `git stash`. Never spawn subagents.

## Chris's decision (2026-08-18)
`sh` = shell, and only shell: the engine runs the backtick line. In tsv2 today `sprefa_extract` already IS `runShellLine` (`v6/tsv2/serve/1_hosts.ts:253-255`) and `soopy_mutation` returns a canned `unsupported` row (`:257-270`); the executor name is picked by sniffing the template in prolog (`v6/prolog/compile/registry.pl:333-388`) and only decides fold-ability plus the Rust door's in-process swap. Chris: "make an adapter we can spam for both soopy and extract and boop from within tsv2 that is able to easily adapt shell stuff." One adapter, one shape, three (then N) instances. Any base rel is insertable from outside; the deltas route landed (#369).

## Deliverables

### 1. `IProcessAdapter` in `v6/tsv2/serve/1_hosts.ts`, interface in `v6/tsv2/runtime/types.ts`
```ts
interface IProcessAdapter {
  readonly name: string;                       // "shell" | "sprefa_extract" | "soopy" | "boop" | ...
  readonly applicative: boolean;               // equal-input demands fold into one spawn (today: extract only)
  command(demand: IHostDemand): IProcessSpec;   // {argv, env, stdin?} ; NEVER a shell string for non-shell adapters
  decode(stdout: string, plan: IHostPlanData): readonly IRowValue[][];  // default: the existing three-shape decoder
}
```
`runShellLine` becomes the `shell` adapter (template filled, `sh -c`), the only one that takes a template. `sprefa_extract`: argv `[DL_EXTRACT_BIN, ...flags, path]` built from the demand columns, no template. `soopy`: argv `[SOOPY_BIN, "stage"|"commit", ...]` (the Rust in-process executor's CLI twin; if no CLI exists, the adapter returns the same `unsupported` row it does today and says so in a comment with the Rust site `hosts.rs:47`). `boop`: argv `["boop","host","oneshot"]` with the demand as JSON on stdin (shape from sprefa `27b15b2` `BoopExecutor`, unmerged branch `feature/dl6-boop-concatmap-golden`; copy the JSON contract, not the branch). Adapter selection = the plan's `execution` field first, then a lookup by rel name in a `<program>.adapters.json` sidecar (`[{"adapter":"sprefa_extract","demand_rel":"extract_ask","response_rel":"extract"}]`) so a plain base rel with no `sh` can be served by an adapter. `HostExecutors` map and `ApplicativeExecutors` set are replaced by the adapter registry; the fold in `groupInvocations` reads `adapter.applicative`.

### 2. Prolog: `sh` narrowed, sniffing deleted
`registry.pl:333-388` (`host_executor/2`, `host_execution/3`, `host_executor_contract/2` for non-shell) deleted; `host_execution(_, _, shell)` is the only row. `1_host_expand.pl:184-203` keeps template validation. Emitters (`emit_ts.pl:473`, `emit_rust.pl:394`, `1_host_expand.pl:225`) emit `execution: shell` for every `sh`. The comment at `registry.pl:330` is replaced by one line: `sh` is shorthand for a shell-executed host and nothing else. Any fixture whose `sh` template starts with `"$DL_EXTRACT_BIN"` keeps working because the shell adapter runs it; the ONLY behaviour change is fold-ability, which now comes from the adapters sidecar. Add `.adapters.json` for the 8 fixtures that relied on the fold (`0_extraction-clock-golden`, `1_rtkq-extraction-golden`, `crawl_org`, `diag-rail`, `extraction-live`, `flagship-callgraph`, `flagship-flow`, `openapi-data-family`) so their subprocess COUNT stays what it is today (write the count test: statements/spawns per tick before and after, additive).

### 3. Rust door parity
`v6/sprefa-engine-rs/src/hosts.rs:46-49` `executor_for_plan`: same rule, `execution == shell` -> `ShellExecutor`; in-process executors (`SprefaExtractExecutor`, `SoopyMutationExecutor`) selected by the same `.adapters.json` sidecar rows loaded at boot (`types.rs` gets the row type; the loader is one function). One test per door that a sidecar row routes a demand in-process and that no row means shell.

### 4. `bind` stays THIS PR. Out of scope. (Chris said "sure" to dropping it later; a separate PR after this lands. Do not touch `2_binds.ts`, `bind_decl`, `source_bind.rs`.)

## Files owned
`v6/tsv2/serve/1_hosts.ts`, `v6/tsv2/runtime/types.ts`, `v6/tsv2/serve/4_http.ts` (adapter registry construction only), `v6/tsv2/tests/**`, `v6/prolog/compile/registry.pl`, `v6/prolog/1_host_expand.pl`, `v6/prolog/emit_ts.pl`, `v6/prolog/emit_rust.pl`, `v6/prolog/compile/test/plunit_tests.pl`, `v6/sprefa-engine-rs/src/hosts.rs`, `v6/sprefa-engine-rs/src/types.rs`, `v6/sprefa-engine-rs/tests/**`, `v6/dl/fixtures/*.adapters.json` (new files only; do not edit `.dl6` bodies), `v6/prolog/compile/out/**`, `v6/tsv2/gen_emitted/**`, `docs/hosts-are-arrivals.md` (new, short: decision table, adapter interface, sidecar shape, one before/after). Do NOT touch `serve.rs`, `3_clock_check.pl`, `parse_dl_dcg.pl`, `dl_view/**`, `2_binds.ts`, `source_bind.rs`.

## Tests, one at a time while iterating; whole batteries once at the end
`pnpm vitest run <file>` in `v6/tsv2`; single plunit via `swipl -g "run_tests(<name>)"`; `cargo test --test <one>`. Once at the end: `cd v6/tsv2 && bash scripts/sweep.sh` (RUN and MANIFEST_REASON_DIFF lines; every fixture keeps its bucket), `cd v6 && just plunit` (5 known-red at `.github/CI-KNOWN-RED.md:32`), `cd v6/prolog/conformance && swipl -g go -t halt go.pl` (462 PASS today), `bash v6/sprefa-engine-rs/grade.sh` (RATCHET = refresh `graded.tsv`, commit only newly clean rows), `cd v6 && just typecheck`, `bash v6/tsv2/scripts/flagship-flow.sh`, `bash v6/tsv2/scripts/1_extraction-clock-golden.sh` (both exercise the extractor through tsv2; must stay green), `swipl -g go -t halt v6/prolog/ARCH.pl`. Read `.github/CI-KNOWN-RED.md` before calling anything broken.

## Pre-commit
`DL_EXTRACT_BIN=/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract`; `pnpm install --frozen-lockfile` in `v6/tsv2` and `v6/sprefa-store/js` inside the worktree.

## PR
`gh pr create --base main`. Body: 1-3 plain sentences on what a user gets (the `IProcessAdapter` shape and one sidecar row), `## Reading order` (numbered files, why each), `## Tests` (name, input, expectation, what it printed before; one line "full suite unchanged otherwise"). No words gate/leg/receipt/door/probe/refusal, no em dashes, no suite counts, no allowlist refs. Do NOT merge. Report: PR number, head sha, whole-battery lines, exact error text on any failure.
