# Extract binding contract checkpoint

## Context

Main 8aba008df contains reconciled extract, TypeSpec SQLite writers, DL7
query/IVM emitters and existing Rust host consumers. The user wants extract
and SQLite usable during Prolog compilation and in generated applications.
The direct Prolog subprocess wrapper is parked. No generated Prolog binding
implementation was completed. See plans/2026-09-09-main-reconciliation.md.

## Decisions

This dispatch is a bounded source-read and design checkpoint. No source edits,
kernel changes, builds, installs, new dependencies, commits, pushes or agents.
Write only plans/2026-09-09-extract-binding-contract.report.md in your worktree,
at most 120 lines. Work in the spawned worktree, never the primary checkout.
Do not use Codex workers or Claude through OpenCode. The user explicitly chose
Claude Opus 5, medium effort, in the Claude harness through Boop.

Keep three consumers distinct: Prolog subprocess, in-process Prolog binding,
and emitted Rust/library consumers. Do not choose or implement transport yet.
TypeSpec remains current storage authority; eventual DL7 schema bootstrap
requires explicit parity. Preserve caller transactions, batching, integer
ordinals and source identity. No replacement schema or hand-copied table maps.

## Discrete work

1. Read existing contracts by symbol, avoiding entire generated-file dumps:
   schema/README.md, schema/1a_fact_emit.mjs, generated writer public signatures;
   extract src/lib.rs, src/dispatch.rs and src/bin/extract/0_sqlite.rs;
   engine src/source_bind/_1_runtime.rs; DL7 0c_extract_loader.pl and the parked
   5_extract_sqlite_query_mainer.pl. Read applicable AGENTS instructions.
2. Map authored declaration -> generated artifact -> current consumers.
   Inventory loader tsi_relation/2, foreign_record/1, argument ordering and
   Rust TSI registry. Identify which authority actually covers each today.
3. Show exact existing Rust/Prolog signatures and minimal proposed signatures,
   with one concrete source input -> facts -> SQLite/query result example.
   Distinguish syntax extraction from semantic resolution. Show transaction
   lifetime and who owns schema creation, ordinal allocation and publication.
4. Propose the smallest first implementation task reusable by all three
   consumers, explicit file scope and tests. Identify transport/kernel choices
   requiring user review. Do not execute that task.

## Verification

Evidence is local paths, symbol names and signatures. No benchmark or suite
reruns. Every claim must distinguish implemented, proposed and unresolved.
Final condition: a bounded report with one ASCII architecture panel, exact
binding/storage contracts and one implementation brief ready for review.

## Staffing

Claude Opus 5 at medium effort through the Claude harness; one Boop lane.
Codex coordinator limits itself to dispatch, acceptance review and integration.
Send a short initial hail, a checkpoint after inventory, and a final hail to
parent using boop beep parent --no-wait. No silent expansion or subdelegation.
If blocked, report the concrete missing contract and stop. At completion stop;
implementation requires another explicit assignment.
