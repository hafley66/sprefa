# Brief: lowering census — IR nodes per rule, by cause

User (2026-10-05): "are macro/comptime and kernel prelude truly this fat". Hello world installs
1,057 + 1,069 + 2,137 IR nodes for 66 macro rules and 116 prelude rules (16-18 nodes per rule);
dd builds about 7 operators per node. Read-only census, then a ranked list of lowering changes.

## Do

1. Attribute every IR node of the three hello-world installs (`DL8_IR_DUMP=<dir> dl8 compile
   <hello.dl7>`; hello = `(: Hello (* (: msg str)))` + `(Hello "hello world")`) and of the registry
   comptime install to (a) the source rule (file:line) or relation it serves and (b) one cause:
   body goal read, join between goals, constructor-term decode, head Mint, constant filter,
   kernel op, projection after join, seed source per relation key, union per relation,
   threshold per relation, negation internals, aggregate internals, prewarm/term map, other
   (name it). Temporary instrumentation in `_9a_ir_lower.rs` in a scratch worktree is fine.
2. Tables in `plans/v8/2026-10-05-lowering-census.findings.md`: nodes by cause per install; top 20
   rules by node count with their source text and node breakdown; nodes that compute nothing for
   hello world (empty relations at settle) by cause; dd operators per IR node kind.
3. Ranked list of lowering changes that would remove nodes (e.g. no seed source for keys with no
   seeds, shared constructor decodes, fused project-after-join, threshold only where duplicates
   can arise), each with the node count it removes, per install, and the semantic condition that
   makes it safe. No code changes in this lane.

## Laws

- Read-only on main; scratch worktree for instrumentation, removed after; commit only the
  findings file on branch `census/lowering`.
- dd default engine. Counts only, no wall clock. No caching.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
branch + sha:
nodes by cause (hello world, 3 installs):
top 5 rules by nodes:
ranked changes: <change: nodes removed per install, safety condition>
```
