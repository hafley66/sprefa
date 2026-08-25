---
created: 2026-08-24
updated: 2026-08-24
type: improvement
reporter: hafley66
status: done
priority: normal
related: ['@braced-nested-relations']
labels:
- area:dl6
- component:parser
- lang-design
- blocked-by:temporal-v2
- size:med
- model:medium
epic: userland-type-graph
lane: dot-path
lane_seq: 10
collision: [parser-paths, generic-type-core]
size: M
blocked_by: ['@temporal-v2-salvage']
assignee: terra-high
commits:
- hash: a697649e06e6f4ed34281d56282e4178a4f8e1c0
  summary: brace nesting as a name path
- hash: c7b958d44
  summary: seed dotted relation facts before lowering
- hash: fe5b9ff6b
  summary: lower zero-arity relations through SQLite
closed: 2026-08-24
closed_by: codex
---

# Dot and brace nesting is a name path only: no implicit parent column, no key shift

## Description

User decision 2026-08-24 (Chris, in the room): `rel a(...) { rel b(...). }` and `rel a.b(...)` are the same spelling of a NAME PATH. The dot is a separator that flattens to `a__b` and nothing else: no implicit leading `parent: a` column, no key(N) shifting, no refCount tie to the parent row. Today `0_dot_expand.pl:360-461` injects `col_type(Ref, parent, ParentName)` and shifts keys; `4_braced_nested_relations.test.pl` pins that (tests `brace_children_reuse_parent_capture_and_key_shifting`, `a_zero_column_brace_child_has_one_parent_column`). Those tests flip. Radius: 21 programs declare dotted rels (grep `^rel [a-z_]*\.[a-z_]*(` over conformance fixtures, text-door, v6/dl); per-file parent-column use recorded in the first comment. Receipt: conformance PASS set unchanged except fixtures that assert the parent column, each listed and re-decided; ghcache config block rewritten as `rel config { rel global(...). rel org(...). rel repo(...). rel exclude(...). }` compiling byte-identical to today's flat decls modulo names.

## Comments

### 2026-08-24T20:52:43Z · @sprefa-coordinator

Radius, 21 programs with dotted rel decls. 10 module_path_* fixtures use the dot as a name path only and already match the decision. 7 nested_* fixtures encode the parent-column semantics and flip or get re-decided: nested_child_carries_the_parent_reference, nested_two_parent_rows_partition_the_child, nested_zero_column_child_is_one_row_per_parent, nested_three_levels_chain_the_references, nested_parent_with_no_rows_yields_an_empty_child, nested_body_atom_reads_every_partition, nested_child_and_an_option_column_coexist. 4 real programs (crosswalk, feature-reach, ghcacher_checkout_golden, receiver-rail) declare dotted rels; whether any reads the injected parent column is unchecked (zero literal 'parent' mentions, but the column is positional). Injection site: 0_dot_expand.pl:360-461.

### 2026-08-24T21:02:01Z · @sprefa-coordinator

CORRECTION with receipt. rel config() { rel global(poll_period: int) key(1). rel exclude(owner: text, name: text). } compiles on main and emits config__global: [poll_period], config__exclude: [owner, name]: NO parent column. 0_dot_expand.pl parent_specs requires ParentArity >= 1, so a zero-column parent is already a pure namespace; the capture only fires for a parent with columns (the 08-04 decision). The user's concrete ask works today. Still open: (1) a dotted FACT under a namespace stops with level_rule_no_positive_body (config.exclude("a","b"). treated as a bodiless rule); (2) whether a parent WITH columns should stop capturing (reverses 08-04) is the only design question left, user's word.

### 2026-08-24T21:15:28Z · @sprefa-coordinator

Level-set with the temporal-v2 codex session (worktree /private/tmp/sprefa-temporal-v2, branch feature/temporal-relations-v2). No implicit-parent assumption in its type-projection experiment: 5a_type_projection.pl:69-75 derives nested edges from rel_path_decl + declared_path + frozen decl ids; member scan :47-58 ignores parent/key shift/arity. Removal arc must: (1) keep declared_path/3 and resolve_relation_paths/3 exported from 0_dot_expand.pl (temporal-v2's 0_expand.pl imports them); (2) replace tests at 4_braced_nested_relations.test.pl:96-115 and :350-364 (assert parent capture); leave :117-161 alone. Order: land AFTER temporal-v2 merges. Blocked-by: temporal-v2.



## Decisions

### 2026-08-24T20:57:04Z · @sprefa-coordinator

CONFLICT with an existing user ruling. rulings.pl:613 block_lowering_first = flat_rels_catalog_edges_arg_distribution (user 2026-08-04): 'an outer arg the block captures is IMPLICITLY DISTRIBUTED into every child rel as a leading demand-key column'. The parent column 0_dot_expand.pl injects IS that ruling. Manifest: nested_head_without_a_parent_atom_refuses is bucket unsupported nested_parent_unbound, i.e. a child head must bind its parent atom in the body today. Today's word (dot = name path only, no parent column) supersedes 08-04 only if Chris says so explicitly; the rulings.pl row needs a superseding entry before any code moves.

### 2026-08-24T21:16:39Z · @sprefa-coordinator

Lang-design codex session (temporal-v2 holder) confirms the same nesting decision: brace = name prefix only, parent refs explicit typed columns, no contrary word from Chris. Held decisions on its side, for cross-reference: PATHS deep dotted refs resolve in every reference-bearing position, FD (Owner,Name)->Target with distinct-target rejection (5a_type_projection.pl). TYPES member rows have authored/logical vs physical/storage views; universal FD invalid until plane/role explicit. GENERICS functional heads lower inside-out to type_apply/3; structural pattern matching absent. DIRECTION key/unique/constraint/serializable/Partial/extends/impl as dl6 rules over canonical node/edge/member/application/annotation rows. SYNTAX uniform parens/colon/comma/arrow/dot, no 'is', no space-delimited 'log keep' form (ghcache.dl6:669 'log keep(count(16500))' changes spelling when that lands). Ownership: /private/tmp/sprefa-temporal-v2 feature/temporal-relations-v2, dirty: 0_compiler_relations.pl, 0_generic_expand.pl, 0_generic_expand/0_expand.pl, 2_compiler_plane.pl, 5_type_freeze.pl, 0_unsupported_messages.pl, compile/test/4_braced_nested_relations.test.pl, compiler_relations.test.pl, plunit_tests.pl, typegen_golden.sh; new: dl/fixtures/1_temporal-relations-v2.dl6, dl/std/0_temporal.dl6, dl/std/README.md, 0_generic_expand/5a_type_projection.pl, compile/test/compiler_relations/0_temporal_relation_annotations.test.pl.

### 2026-08-24T21:46:57Z · @codex-0

Execution routing: Medium (`M`, `size:med`, `model:medium`). Native Terra-high through Boop communication; completion hail required. Work starts after `@temporal-v2-salvage`. Any semantic choice beyond the held name-prefix-only ruling stops and asks Chris.

## Agent Runs

### 2026-08-24T23:25:43Z · @codex

Native Terra-high implementation started from main fdfb80106. Scope: name-prefix-only brace/dot declarations; remove implicit parent-column injection, key shifting, short-head/body arity accommodation, and nested_parent_unbound; preserve declared_path/3, resolve_relation_paths/3, deep reference resolution, line provenance, and declaration collision behavior; rewrite the seven capture fixtures and focused PLUnit expectations; include dotted facts under namespace/path resolution if current parser represents them as rule/1. Stop for any semantic choice outside this ruling.

### 2026-08-24T23:39:24Z · @codex-0

Terra-high implementation reviewed on main. Focused braced_nested_relations: 28/28. Full PLUnit: 1,076/1,076. Dotted facts and explicit parent relation columns have direct receipts. Full conformance retains two pre-existing Unicode failures: reverse_reverses_characters and length_counts_characters. Luna-max independent audit remains in progress.

### 2026-08-24T23:48:15Z · @codex-0

Luna-max independent audit delivered through Boop. Blocking dotted-fact ordering gap fixed by e51b09e4f. Full PLUnit 1,076/1,076 and focused brace/catalog 33/33. Text-door compared at a697649e0 and e51b09e4f: identical seven-failure set. Six generic byte differences predate e51b09e4f. The remaining dot-specific failure is nested_zero_column_child_is_one_flat_marker at the already-pinned rel/0 SQL lowering boundary.

## Resolution

### 2026-08-25T00:20:59Z · @codex

Brace and dotted nesting now preserve authored arity as a name path. The remaining rel/0 lowering gap is closed across base, delta, frontier, arrival, boot, edge, and fixpoint SQL. Receipts: focused brace and rel/0 PLUnit 39/39; full Prolog PLUnit 1,079/1,079; compiler sweep 348 compiled, 342 replay-identical, zero wrong replays and zero emitted crashes; focused TypeScript rel/0 runtime 3/3. Text-door retains the six pre-existing generic byte differences.
