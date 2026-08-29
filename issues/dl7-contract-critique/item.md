---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: opus
status: done
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-opus5]
size: M
lane: dl7-ruling
lane_seq: 0
collision: [v7-design]
closed: 2026-08-28
closed_by: codex-v7
commits:
- hash: 4018330a1
  summary: v7 contract critique report
---

# Critique DL7 kernel contract and delete unnecessary machinery

## Description

Critique the blocked Sol kernel contract for internal contradictions,
unnecessary machinery, and the unresolved declared-node identity. Read the
plan, Sol report, and only the donor reports needed to verify disputed claims.

The report must keep the semantic choice open for the user. Compare both
identity forms using exact term shapes, required inputs, collision behavior,
rename behavior, artifact portability, and downstream signatures.

Audit these observed contract risks:

- compiler ownership inferred from a `primitive(type)` return instead of an
  explicit stage declaration;
- lowering that inserts `intern/3` only for type-returning callables;
- module-unqualified `Name/Arity` relation references;
- duplicate identity carried by `application/2`, `construction_request/3`,
  `specialization/3`, and `argument/3`;
- machinery that can be deleted before the first reader and evaluator proof.

## Acceptance Criteria

- [x] Identify any duplicate representation of bind, edge, call, return, or
      specialization.
- [x] Identify any evaluator branch that makes compile time and runtime
      mechanically different.
- [x] Identify any feature outside the overnight ceiling.
- [x] Propose deletions before additions.
- [x] Compare both module identity forms without selecting one.
- [x] Rule on the stage-ownership, inserted-interning, and relation-reference
      risks with donor file and line receipts.
- [x] Write `v7/3_TASKS/results/1_CONTRACT_CRITIQUE.md`.
- [x] Modify no implementation file.

## Tests Run

Run no suite.

## Agent Runs

### 2026-08-28T04:41:35Z · @codex-v7

2026-08-28: spawned Boop lane chore-dl7-contract-critique from base 7073ffa20 with Opus 5/high. Expected report v7/3_TASKS/results/1_CONTRACT_CRITIQUE.md and at least one documentation commit. Identity choice remains reserved for the user.

## Resolution

### 2026-08-28T04:50:53Z · @codex-v7

Reviewed Opus report and commit 0ee0597c1, cherry-picked as 4018330a1. The report modifies one documentation file, runs no suite, keeps the module identity choice open, and provides source receipts for the blocking Partial demand gap, inserted interning, stage ownership, relation qualification, evaluator request branch, duplicate carriers, and scope deletions.
