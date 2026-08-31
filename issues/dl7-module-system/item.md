---
created: 2026-08-31
updated: 2026-08-31
type: epic
owner: chris
status: done
priority: high
labels: [compiler]
commits:
- hash: 8aa7aa9e1
  summary: Compile DL7 filesystem modules through colon edges
closed: 2026-08-31
closed_by: codex
---

# DL7 module system parity

## Description

Compile V7 source files as separately owned modules inside a filesystem-derived
product graph. Module traversal uses the canonical `:/4` edge relation and
ordinary logic variables.

## Acceptance Criteria

- [x] Separate files preserve declaring-module type identities.
- [x] Project roots, directories, and files are product nodes.
- [x] Filesystem containment is represented by deterministic `:/4` edges.
- [x] Numeric author-order prefixes are removed from semantic path labels.
- [x] Cross-module rules traverse modules with ordinary colon goals.
- [x] Positive recursion and stratification use the existing Datalog checker.
- [x] The unused host path, import visibility, and import-cycle resolver is removed.

## Tests Run

- [x] Nested filesystem graph fixture passes.
- [x] Two-file colon traversal fixture passes.
- [x] V7 SWI suite passes 32/32.
- [x] V7 Tree-sitter corpus passes 1/1.

## Decisions

Dot projection, `::`, import/export declarations, punning, keyword arguments,
first-class edge identity, and generated-edge ordinal allocation are deferred.
They are not module-system completion criteria.

## Agent Runs

### 2026-08-31T06:20:41Z · @codex

Internal module substrate reached the earlier separate-unit and ownership
milestones before the module surface was reconciled.

### 2026-08-31 · @codex

PR 618 replaced the parallel host resolver with filesystem products and
ordinary colon traversal. Local V7 gates passed before merge.

## Resolution

### 2026-08-31T22:19:26Z · @codex

The V7 filesystem module graph and colon traversal model merged in PR 618 with V7 gates passing.
