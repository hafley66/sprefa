# v8 flash work review (one brief, delivered in rounds)

## TOC
1. Job
2. Base and first action
3. Ownership
4. What flash work exists, and what is still in flight
5. The rubric: what to check, in order
6. Output: the one file
7. Evidence you must produce
8. Kernel gate
9. Style laws
10. Reporting

## 1. Job
Six lanes on `openrouter/z-ai/glm-5.3-flash` (preset `glm53f`) were spawned 2026-09-14 to continue v8/dl8 work after a Claude session-limit outage. You review their output against their briefs and against the repository's own laws. You write findings, not fixes. Read-only on every source file; the only file you create is the review document in section 6.

The work arrives in rounds. Round 1 is `hafley66/hafley-rs` PR #65, complete now. Rounds 2..n arrive as later hails naming a PR or a lane; append a section per round to the same document. Do not block waiting for them.

## 2. Base and first action
- Base `origin/main` of `/Users/chrishafley/projects/sprefa` at spawn.
- Worktree, branch, and lane id are set by `lane create`; work in `$PWD`, never in the primary checkout.
- FIRST command: `git log -1 --format='%H %s'` and record the base sha in the document header.

## 3. Ownership
You own exactly one file: `plans/v8/2026-09-14-v8-flash-work-review.sol-low.md`. You may run any read-only command (`git show`, `git diff`, `cargo test`, `cargo build`, `gh pr view`, `gh pr diff`, `boop db`) and write only under `/tmp`. You may not edit source, tests, fixtures, or goldens, and you may not merge or open a PR.

## 4. What flash work exists
| lane | brief | output | state |
|---|---|---|---|
| fix-extract-oracle-paths-glm | `plans/v8/2026-09-14-hafley-rs-extract-oracle-paths.brief.md` | hafley-rs PR #65, head `54db5a93`, 591+/583- | round 1, review now |
| feat-v8-reconciler-glm | `plans/v8/2026-09-14-v8-reconciler.brief.md` | none; lane found the work already on main as sprefa PR #760 | round 2 when hailed |
| feat-v8-soopy-extract-hosts-glm | `plans/v8/2026-09-14-v8-soopy-extract-hosts.brief.md` | in flight | round 2 when hailed |
| feat-v8-openapi-load-glm | `plans/v8/2026-09-14-v8-openapi-load.brief.md` | in flight | round 2 when hailed |
| lab-v8-retraction-glm | `plans/v8/2026-09-14-v8-retraction.brief.md` | in flight; touches `v8/src/_6_eval/**` | round 2, kernel gate applies |
| feat-v8-sqlite-emitter-glm | `plans/v8/2026-09-14-v8-sqlite-emitter.brief.md` | none; lane halted on a schema mismatch | round 2 |

Each lane's stored brief body is what it was actually told; read it from the store, not from disk, when they differ:
`boop db "SELECT d.value, m.body FROM agent_lane l JOIN dict_session d ON d.id=l.lane_id JOIN markdown_cache m ON m.markdown_id=l.brief_markdown_id WHERE d.value LIKE '%glm'"`

## 5. The rubric
For each deliverable, in this order:

1. **Claim versus evidence.** Every number in the commit message or PR body (test counts, pass/fail, "0 failed", "empty diff") must be re-produced by you, in the worktree or with `gh pr diff`. Record command and result. A claim you could not reproduce is a finding, not a pass.
2. **Golden and oracle audit.** A golden regenerated from the binary under test proves nothing by itself. For `crates/sprefa-extract/tests/fixtures/kind_vocab/wire_golden.jsonl` and the eight `.v5.jsonl` baselines: `git diff` them and classify every changed line as (a) the mechanical path substitution the commit message names (`v6/sprefa-extract` -> `crates/sprefa-extract`), or (b) anything else. Count both. Any (b) line is a finding with the line quoted. If the diff cannot be explained by the path substitution alone, whether the new golden is correct is open.
3. **Ignored tests.** Five tests gained `#[ignore = "needs rust-analyzer on PATH"]`. Confirm rust-analyzer is genuinely absent from `PATH`, and confirm from the pre-change run that those five failed only for that reason. A test ignored to make a suite green is the defect this check exists for.
4. **Build config.** The commit adds an empty `[workspace]` table to `crates/sprefa-extract/Cargo.toml` so the crate builds standalone. Check whether the crate is still a member of the parent workspace, and whether `cargo test -p sprefa-extract` from the repo root still resolves. The commit message claims a `-p` run and a standalone build; if both cannot hold at once, say which one is real.
5. **Brief compliance.** Does the diff do what the brief's deliverables list requires, nothing more. Ownership violations (files outside the brief's declared set) are findings.
6. **Tests through the real binary.** The repository law is integration and e2e tests through the real binary; fakes are not evidence for IO or process behavior. Where a deliverable's tests mock, say so.

## 6. Output: the one file
`plans/v8/2026-09-14-v8-flash-work-review.sol-low.md`, opened by a TOC. Per round, one section, each with:
- verdict in three lines: ship / ship with fixes / rework, and why;
- the claim-versus-evidence table (claim, command, observed, match);
- findings, ranked, each as: what, file:line, why it matters, one-line fix;
- what you verified clean, in one table, so the clean parts are distinguishable from the unexamined parts.

End the document with a section "unexamined" listing anything you did not check, so the review's boundary is explicit. Never overstate coverage.

## 7. Evidence
Every number in the document comes from a command you ran in this session. No inherited counts. Failing commands stay in, with output. If a build times out or a test cannot run, record that as the result.

## 8. Kernel gate
`AGENTS.md` forbids changes to graph core, type semantics, binding, rules, evaluation, or macrotime/comptime phase boundaries without explicit user approval. If a reviewed diff touches `v8/src/_6_eval/**`, the store DDL, or rule lowering, do not rate it. List it under "requires kernel approval", name the file:line, state the current behavior and the changed behavior, and stop that portion.

## 9. Style laws
No em dashes. No sycophancy, no apology, no hedging. Banned words: provenance, substrate, load-bearing, regime, honest, ground as a verb, refusal. Tables over prose. Numbers only from commands you ran. Quote code and commit text verbatim when a finding rests on it.

## 10. Reporting
Commit the document on your lane branch with a `Boop-Status: wip` trailer after round 1. Append and commit again per later round. Do not open a PR. The lane result row is the report; a following hail names the next round.
