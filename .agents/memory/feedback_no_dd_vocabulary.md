---
name: feedback-no-dd-vocabulary
description: "Differential-dataflow vocabulary (witness, support, owner, source as derivation terms, FactStore, RuntimeGraph) is dead in this codebase. Do not propose or extend it."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: ad1ddc5a-c1d4-44c0-8d82-0ea3992f8c76
---

The differential-dataflow paper vocabulary is being exorcised from v4 + v3
effect_runtime. Specifically:

- `FactStore` / `SqliteFactStore` / `MemFactStore` / `FactRuntimeGraph`
- `RuntimeGraph` (both v3 and v4 copies)
- `Queue` as a bare type name with no semantic prefix
- `owner` / `owner_id` (as a derivation-writer label)
- `source` / `source_id` (as a derivation-input label, distinct from sprf's `fs`/`read` source-file concept)
- `supports` (verb + noun for derivation refcount)
- `witness` / `derivation_key` / `__witness` column

All of these are direct lifts from the DD paper. The user's framing
2026-05-20: "DD was a nice booty call." Useful design influence; not a
vocabulary the user wants to live with.

**Why:** The names tell you the implementation tradition, not what the
thing does. The user's stated rule is that names must say what the thing
is, in a vocabulary a normal reader can follow (React/Redux, Rx, graph
theory, or plain English). DD jargon fails that test the same way the
banned [[feedback_rule_is_function_not_channel]] words do.

**How to apply:**

1. When proposing names in any new plan or refactor touching the runtime
   stack, draw from React/Redux/Rx/graph/relational vocab. Not DD.
2. When reading existing code that uses these terms, translate them
   mentally and flag the identifier as rename-target if relevant to the
   task. Do not invent new terms in the same register (no `RowProvenance`,
   no `DerivationOwner`, etc.).
3. The naming-pass triage is happening 2026-05-20 via three Plan agents
   (Redux/Rx/graph lenses). When that lands, this memory will get a
   pointer to the chosen replacement set.
4. The user has separately banned `provenance` / `substrate` /
   `load-bearing` / `regime` in identifiers AND prose
   (`/Users/chrishafley/.claude/CLAUDE.md`). DD-replacement names must
   also pass that filter.

Related:
- [[feedback_rule_is_function_not_channel]] — the prior vocab purge
  (no send/sink/chan; rule = function, write = return)
- [[feedback_lowering_vs_runtime_separation]] — different concern but
  same era; do not let renames re-couple Def + Component files
