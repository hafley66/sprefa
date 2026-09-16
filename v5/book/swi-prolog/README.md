# SWI-Prolog for a Self-Describing Type Language

Research date: 2026-07-20

This sub-book starts from Datalog knowledge and builds toward the `schema.soup` compiler and a Prolog-hosted language server. Chapters are short so the application can collapse them independently.

## Reading order

0. [Orientation](0_orientation.md)
0a. [From SQL and Datalog to Prolog](0a_sql-to-prolog.md)
1. [Runtime toolbox](1_runtime-toolbox.md)
2. [Logic and constraint toolbox](2_logic_toolbox.md)
3. [Server, storage, and deployment toolbox](3_server-storage-deployment.md)
4. [Rust implementations](4_rust-prolog.md)
5. [A self-LSP for Soup](5_self-lsp.md)
6. [Exercises against the lab](6_exercises.md)
7. [Diagram appendix](7_diagrams.md)
8. [Research notes and sources](8_research-notes.md)
9. [How this started and where it landed](9_session-arc.md)

## Current software snapshot

| Target | Version observed | Role |
|---|---:|---|
| SWI-Prolog | 10.0.2 installed locally | Full Prolog runtime used by the lab |
| SWI stable manual | 10.0.1, February 2026 | Current official stable reference found during research |
| Scryer Prolog | 0.10.0 | Prolog implementation written mostly in Rust |
| LSP | 3.17 | Editor protocol implemented over JSON-RPC |

## Local executable lab

The companion implementation is [`labs/swi-typespec-lab`](../../labs/swi-typespec-lab/README.md).

```sh
cd labs/swi-typespec-lab
swipl -q -s 4_demo.pl
```

It currently parses `schema.soup`, checks nested JSON-shaped values, parses and evaluates typed patterns, enumerates structural paths, and generates Rust and JavaScript.
