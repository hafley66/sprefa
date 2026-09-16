# boop north star (user word, 2026-08-08): a future dl6 codegen target

"like ghcacher before it, we will get the language to codegen that tool later."

Consequence for every pass from now on: boop's data surface stays DECLARATIVE
so a dl6 program can emit it one day.

- `AgentEvent` / chat records stay flat rows with scalar columns; they must map
  1-1 onto rel declarations (`agent_read(session, ts, path, branch)` shape from
  the agentio design). No nested structs a rel cannot carry.
- The `Harness` trait stays exactly reads-over-facts: `sessions()` is a rel
  scan, `read_from(offset)` is a delta read. Add no method whose semantics a
  datalog rule could not express.
- Verb handlers stay thin routing over the registry; logic lives in the
  interface, never in main.rs match arms.

Do not act on this file mid-pass; it binds the NEXT brief.
