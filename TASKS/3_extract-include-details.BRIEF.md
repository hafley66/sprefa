# Extract include-details: checkpointed Sol lane

User-approved endpoint: `extract --resolve --include-details a.ts b.ts`
emits existing per-input extraction records, each with its source `path`,
alongside the unchanged resolved records. Default output stays byte-identical.
Reuse the already-computed extraction outputs. No second read/parse/extract pass.
This only exposes information. No DL7, new analysis, resolver behavior,
in_loop/cfg enrichment, dependencies, issue edits, or unrelated cleanup.

## Authority and workflow

Work ONLY in the lane worktree created from the parent isolated branch.
The parent is `extract-field-reports-parent`. Use Sol through Boop only.
Read `boop --help` before messaging. No native subagents and no child lanes.
User instructions override historical plan staffing/worktree/formatter rules.
The numbered steps below are checkpoints, not permission to execute ahead.
EVERY checkpoint ends your turn. NEVER self-approve the next step.
No background builds, detached runners, installation, full-suite loops, pushes,
merges, stashes, commits, or formatting until explicitly authorized.
If scope/files grow, a test fails, or a contract needs changing: report and END
THE TURN. Do not repair opportunistically. No compiler/kernel semantics edits.
Keep each checkpoint report under 200 words. Use Boop hail then final answer.

## Current authorization: step 1 ONLY, read-only

Inspect the local instructions and these current implementation seams:
`v6/sprefa-extract/src/project.rs` ResolveRequest, resolve_project,
resolve_project_jsonl, read_inputs_with_modules;
`src/bin/extract.rs` Cli and stream_resolve; serialization helpers in wire.rs;
existing CLI/resolve tests, help and schema. Do not read unrelated history.
Use the existing extract binary if helpful, no build:
`/tmp/sprefa-extract-scip-reliability-target.IuDQPc/debug/extract`.

Report exact proposed function signatures, where existing outputs live and
when they are dropped, smallest file allowlist, ordering/path semantics,
witness interaction, one small output example, and exact focused test targets.
Preserve existing ResolveRequest struct-literal callers if possible.
Plan around current files; no arbitrary size-limit refactors.
No source edits, test runs, or commits. Send the checkpoint to the parent and
END THE TURN. Wait for an explicit step-2 message.

## Later steps, NOT yet authorized

2. Source only: approved CLI validation and optional output path. End turn
   with diff stat and behavior notes. No tests/commit.
3. Tests/docs only: deterministic two-file completeness, unchanged default,
   unchanged resolved subset, paths, invalid use, help/schema; witness if
   supported. Existing snapshots remain frozen. End turn.
4. Run ONLY the parent-approved focused test command ONCE. Report and end.
   Failure is a checkpoint, not authority for a repair loop.
5. Parent reviews; explicitly bounded corrections if needed. Parent owns
   final gate and integration. Format/commit only on explicit instruction.

## Completion condition

Reviewed implementation and regression tests demonstrate the command above,
legacy output compatibility, and one extraction pass. Parent verifies the
required gate and integrates into `work/extract-field-reports-20260908`.
Shared primary checkout stays untouched. No global install or push.
