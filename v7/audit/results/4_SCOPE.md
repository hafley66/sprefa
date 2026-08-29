# Slice 4: scope, modules, imports, nesting, and projection

DL6-to-DL7 audit. Scope: owner/scope/local-name/qualified-name identity and dot
projection, as carried by `use_resolve.pl`, `0_dot_expand.pl`,
`0_annotation_expand.pl`, `executor_modules.pl`, and the related fixtures/tests.

```mermaid
flowchart TD
    A[use_resolve.pl: collect_all/8<br>file walk + use splicing] --> B[decl seeds]
    B --> C[0_dot_expand.pl: declared_path/3<br>flat name = __ join]
    C --> D[decl_scope_tree/2<br>node(file,...) trie]
    D --> E[resolve_path/3<br>innermost scope first]
    E --> F[rewrite_rel_paths/3<br>rel_path - flat atom]
    A --> G[mount_decl/4 + module_edge_decl/4<br>module identity = short_hash stem]
    G --> C
    H[0_dot_expand.pl: expand_dot_rule/3<br>dot_get - decode/2 brace payload] --> I[lower/emit: flat names only]
    J[executor_modules.pl: bind_executor_modules/3<br>use soopy = rename to canonical atom] --> I
    K[0_annotation_expand.pl<br>annotation_steps + implicit Target] --> I
```

One-line map: the compiler's entire scope model is `declared_path/3` (a
Segments-Name multiset over the concatenated decls) plus `decl_scope_tree/2`
(a trie over it); module identity is a path-relative stem hashed by
`short_hash/2`; braces, slashes, and dots are all parse-time spellings that
normalize to flat `__`-joined atoms before any later phase runs.

## Report blocks

### v6/prolog/0_dot_expand.pl

```prolog
% File: v6/prolog/0_dot_expand.pl:82
% Existing comment: none (module header comment above the export list describes
%   the dot phase erasing dot_get before checks/typing/lowering)
% Signature: expand_dot_in_context(+EnumContext, +prog(Decls0,Rules0), -prog(Decls,Rules))
% Called by: compile pipeline (phase 44); tests (4_braced_nested_relations.test.pl:286)
% Calls: resolve_qualified_type_paths/3, resolve_enum_arm_term/3, decl_scope_tree/2,
%   resolve_rel_path_rule/3, expand_dot_rule/3
% Tests: compile/test/4_braced_nested_relations.test.pl (deep_rule_head_body_match...,
%   deep_negated..., deep_relation_value..., deep_query_target...)
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: after expansion no dot_get or rel_path survives; the program is
%   term-identical to its brace/flat spelling.
% DL7 seam: in: cons-tree rule list + enum context; out: rules whose atoms carry
%   flat resolved names and synthesized decode(...) goals.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:94
% Existing comment: none
% Signature: resolve_qualified_types(+Program0, -Program)
% Called by: 1_expansion.pl (ahead of the fold); export list
% Calls: resolve_qualified_type_paths/3
% Tests: 4_braced_nested_relations.test.pl:342,350,370,376
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: qualified type paths in decls resolve to flat names before any
%   expansion phase sees a type_path/1.
% DL7 seam: program in, program with resolved type terms out.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:102
% Existing comment: "Qualified types retain their path until mount_decl/4
%   supplies scope here. Resolution produces the same flat relation identity as
%   a qualified call."
% Signature: resolve_qualified_type_paths(+Decls0, -Decls)
% Called by: resolve_qualified_types/2, expand_dot_in_context/3
% Calls: decl_scope_tree/2, anonymous_sum_path_aliases/3, qualified_type_names/3,
%   resolve_qualified_type_decl/3, ensure_type_decl/3
% Tests: 4_braced_nested_relations.test.pl:342 (deep_bare_wrapped...), :370,
%   :440 (mount path through use)
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: type identity after resolution equals the identity a qualified
%   call mints (one flat name).
% DL7 seam: decl list in, decl list out; mounts must pre-exist as decls.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:111
% Existing comment: none
% Signature: anonymous_sum_path_aliases(+Decls, +Root, -AliasDecls)
% Called by: resolve_qualified_type_paths/3
% Calls: resolve_qualified_type/3, declared_path/3,
%   0_anonymous_expand:materialized_sum_path_decls/6
% Tests: none direct (covered via anonymous enum fixtures)
% V7 class: extract
% Parser coupling: none
% Preserved law: an anonymous sum variant's path aliases resolve like authored
%   aliases.
% DL7 seam: decl list -> synthesized alias decl list.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:123
% Existing comment: "refresh_relation_type_decls ... Re-run that same mirror
%   rule after all declaration-producing phases, when stored columns and
%   wrapper rewrites are final."
% Signature: refresh_relation_type_decls(+Program0, -Program)
% Called by: 1_expansion.pl late pass; export list
% Calls: column_element_type_name/2, ensure_type_decl/3
% Tests: 7_module_path_element.pl (artifact minting order)
% V7 class: extract
% Parser coupling: none
% Preserved law: a relation-valued column always gains a mirror type_decl even
%   when the relation was materialized after the column was parsed.
% DL7 seam: program -> program.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:139
% Existing comment: none
% Signature: erase_type_path_aliases(+Program0, -Program)
% Called by: 1_expansion.pl (final cleanup)
% Calls: is_type_path_alias/1
% Tests: plunit_tests.pl (path alias erasure)
% V7 class: extract
% Parser coupling: none
% Preserved law: type_path_alias decls are compile-time only; no phase after
%   erasure can see one.
% DL7 seam: program -> program.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:174
% Existing comment: none
% Signature: resolve_qualified_type(+Scopes, +Type0, -Type)
% Called by: resolve_qualified_type_decl/3, anonymous_sum_path_aliases/3,
%   qualified_type_names/3
% Calls: resolve_path/3, relation_id_path/3
% Tests: 4_braced_nested_relations.test.pl:342,370,431
% V7 class: adapt
% Parser coupling: term-shape (type_path/1, type_path_application/2)
% Preserved law: type_path(Segments) resolves to the flat rel name; a terminal
%   .id over a resolved relation becomes id(Name); resolution failure throws
%   unresolvable_type_path(Segments) with every segment.
% DL7 seam: in: type term carrying path nodes + scope chain; out: type term
%   over flat names only.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:201
% Existing comment: "A terminal `.id` has relation-identity meaning only when
%   its prefix already resolves to a declared relation. A mounted `source.span`
%   remains an ordinary module path unless `source` itself names a declared
%   relation."
% Signature: relation_id_path(+Scopes, +Segments, -Name)
% Called by: resolve_qualified_type/3
% Calls: resolve_path/3
% Tests: 4_braced_nested_relations.test.pl:342
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: `.id` is relation identity only over a resolvable relation
%   prefix; otherwise it is a module path segment.
% DL7 seam: scope chain + segment list -> rel name or fail.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:245
% Existing comment: "`Enum.variant(...)` is the arm's own spelling. The
%   generated ref comes from enum_context, so this cannot drift from what
%   expansion actually minted."
% Signature: resolve_enum_arm_term(+EnumContext, +Term0, -Term)
% Called by: expand_dot_in_context/3 (maplist)
% Calls: enum_arm_ref/4
% Tests: enum arm fixtures (0_enum_variants.pl, 25_parameterized_enum.pl paths)
% V7 class: adapt
% Parser coupling: term-shape (rel_path/2 with two segments)
% Preserved law: an `Enum.variant(args)` arm resolves to the exact rel the
%   expansion minted, never re-derives it.
% DL7 seam: context assoc + term -> resolved term.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:265
% Existing comment: "Either door: the text door parses rel_path/2, and SWI
%   reads `a.b(X)` as '.'(a, b(X)), which would otherwise become a rel literally
%   named '.'."
% Signature: resolve_rel_path_rule(+Scopes, +Rule0, -Rule)
% Called by: resolve_relation_paths/3, expand_dot_in_context/3
% Calls: contains_rel_path/2, rewrite_rel_paths/3
% Tests: 7_module_path_wrapper.pl (whole file), 4_braced_nested_relations.test.pl:276
% V7 class: adapt
% Parser coupling: term-shape (SWI '.'/2 dict expansion trap)
% Preserved law: a functor-position dotted path resolves against the decl tree
%   in every wrapper and position; failure throws unresolvable_path(Segments).
% DL7 seam: scope chain + rule -> rule with flat atoms.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:271
% Existing comment: (export list) "Query declarations are compiled before the
%   ordinary expansion fold, so host preparation resolves their path carriers
%   through the same tree as rule atoms."
% Signature: resolve_relation_paths(+Decls, +Terms0, -Terms)
% Called by: 1_host_expand.pl (query preparation), 0_generic_expand.pl
% Calls: decl_scope_tree/2, resolve_rel_path_rule/3
% Tests: 4_braced_nested_relations.test.pl:317 (deep_query_target...)
% V7 class: extract
% Parser coupling: term-shape
% Preserved law: query and rule path carriers resolve against one scope tree.
% DL7 seam: decls + terms -> resolved terms.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:296
% Existing comment: "A literal '.'(A, B) in a clause head would itself be
%   dict-expanded by SWI, which is the trap this predicate exists to catch, so
%   the shape is inspected."
% Signature: rel_path_parts(+Term, -Segments, -Args)
% Called by: contains_rel_path/2, rewrite_rel_paths/3
% Calls: is_list/1, rel_path_parts/2 (recursive)
% Tests: 7_module_path_wrapper.pl (whole file; term-door receipt)
% V7 class: adapt
% Parser coupling: token/CST (SWI '.'/2 compound spelling)
% Preserved law: both the rel_path/2 parsed form and SWI's `a.b(X)` '.'/2
%   spelling decompose into segments + args.
% DL7 seam: drop '.'/2 shape once DL7 parses Lisp-shaped terms; keep rel_path
%   equivalent if paths survive as a term shape.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:318
% Existing comment: "Scopes run innermost first, so a nearer room's name binds
%   before an outer same-name; one file with no block surface = the file room
%   alone."
% Signature: decl_scope_tree(+Decls, -Root)
% Called by: expand_dot_in_context/3, resolve_relation_paths/3,
%   resolve_qualified_type_paths/3, 0_generic_expand/5b_type_graph.pl
% Calls: declared_path/3, check_path_collisions/1, insert_path/3
% Tests: 7_module_path.pl (whole file), 4_braced_nested_relations.test.pl
% V7 class: adapt
% Parser coupling: none (decl-level)
% Preserved law: innermost scope wins; one path spelling two rels is refused
%   (mount_path_collision).
% DL7 seam: in: decl list; out: owner/name tree. V7: represent as
%   owner/name/ordinal edges per 0_SHARED assumptions.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:336
% Existing comment: "The mount graft. The mounted module's rel keeps its own
%   flat NAME, so a reference through the alias resolves by identity and mints
%   no new rel."
% Signature: declared_path(+Decls, ?Segments, ?Name)
% Called by: decl_scope_tree/2, use_resolve.pl (check_use_local_name_collision/3,
%   source_relation_name/2, subtree_paths/2), 5b_type_graph.pl, executor
%   collision check
% Calls: member/2, atomic_list_concat/3
% Tests: use/mount tests (plunit_tests.pl:9523,11529), 7_module_path*.pl
% V7 class: adapt
% Parser coupling: term-shape (six decl functors: rel_path_decl, type_path_alias,
%   rel_template, rel_template_enum, mount_decl, plus six flat-name decl kinds)
% Preserved law: a mounted rel keeps the declaring module's flat name; the path
%   [Alias|Segments] resolves to that same identity, minting nothing.
% DL7 seam: the central relation-identity oracle. V7: one owner/name/target
%   edge predicate replaces the six clause shapes.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:375
% Existing comment: (insert_path/insert_segments) path insertion into the
%   scope tree
% Signature: insert_path(+Segments-Name, +Node0, -Node) / insert_segments/4
% Called by: decl_scope_tree/2
% Calls: selectchk/3
% Tests: 7_module_path.pl (three-segment room minting)
% V7 class: drop (replaced by owner/name edges)
% Parser coupling: term-shape (node(Local, Resolved, Children) trie)
% Preserved law: interior rooms are minted from the path even when no decl
%   names them (module_path_three_segments...).
% DL7 seam: V7 scopes are edge rows; the in-memory trie is compiler-local.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:393
% Existing comment: none
% Signature: resolve_path(+Scopes, +Segments, -Name) / descend/3
% Called by: resolve_qualified_type/3, rewrite_rel_paths/3, relation_id_path/3
% Calls: descend/3
% Tests: 7_module_path.pl (local-name-before-dotted fixture)
% V7 class: adapt
% Parser coupling: none
% Preserved law: innermost-first scope chain; local name binds before a dotted
%   path ending in the same segment (two independent rels).
% DL7 seam: scope chain = owner edges; keep the innermost-first search law.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:406
% Existing comment: "A head dot chain is ruled IN ... The receiver still has to
%   be bound by the BODY" and the module header's placement rules (decode AFTER
%   a plain relation atom, BEFORE other goals; head decodes appended after body)
% Signature: expand_dot_rule/3, desugar_dot_rule/3, desugar_head_and_body/5,
%   rewrite_head/6, rewrite_goal/5, replace_dot_gets/6
% Called by: expand_dot_in_context/3
% Calls: bound_body_vars/2, check_dot_receiver/3, dot_fields_pattern/6,
%   fields_pattern/3, dot_chain_parts/3
% Tests: plunit_tests.pl:6965 (head_dot_expands_to_the_brace_body...),
%   7_module_path.pl, decode fixtures
% V7 class: adapt
% Parser coupling: term-shape (dot_get/2 nest, '{}'/1 brace payload)
% Preserved law: the desugared program is term-identical to the brace decode
%   spelling; decode placement after a plain atom / before readers; head decodes
%   appended last.
% DL7 seam: in: rule with nested dot access; out: flat head + decode/2 goals.
%   The '{}'/1 payload shape is the decode pattern, a VALUE-plane brace, not a
%   scope brace; it stays.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:495
% Existing comment: "`FileRec.revision.id` reads File's stored endpoint once.
%   The synthesized decode joins File's dictionary but never follows Revision."
% Signature: dot_fields_pattern/6, relation_id_member_path/6,
%   receiver_relation_type/4
% Called by: rewrite_goal/5, replace_dot_gets/6
% Calls: type_definitions/2, type_definition/4, relation_columns_and_types/5,
%   declared_type_name/2
% Tests: occurrence_identity.pl, text_identity_literal.pl (id-member receipts)
% V7 class: adapt
% Parser coupling: none
% Preserved law: a terminal `.id` on a typed reference reads the stored endpoint
%   once and never follows the joined row.
% DL7 seam: needs the column type map at rewrite time; keep the typing oracle.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:574
% Existing comment: "Vars come from the dot-stripped goals: in `f(X.a)` the
%   receiver X is READ through the dot, never bound by f."
% Signature: bound_body_vars/2, goal_bound_vars/3, binding_positions/2,
%   strip_dot_gets/2
% Called by: desugar_head_and_body/5
% Calls: surface/5 (registry), term_variables/2
% Tests: unresolvable_member fixtures (7_module_path.pl refusal, dot fixtures)
% V7 class: extract
% Parser coupling: term-shape (hard-coded binding goal list: :=, is, decode,
%   latest, pre, finalize, next, coalesce, now, probe, combine)
% Preserved law: the chain's root must be a variable the body binds, else
%   unresolvable_member with an atom-root whole-path / variable-root fields-only
%   payload convention.
% DL7 seam: V7 must restate binding positions over the cons-tree body forms.
%   This list is the slice's biggest term-shape coupling.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:542
% Existing comment: (payload convention) "an ATOM root spells the whole path
%   (`foo.bar`), a variable root spells the fields alone. The parse keeps
%   variable IDENTITY, not surface names..."
% Signature: check_dot_receiver/3, dot_path_atom/2, fields_path_atom/2
% Called by: rewrite_goal/5, replace_dot_gets/6
% Calls: memberchk_eq/2, dot_chain_parts/3
% Tests: 7_module_path.pl (unresolvable_path refusal), dot fixtures
% V7 class: oracle
% Parser coupling: none
% Preserved law: unsupported_construct payloads are ground and pinned by ==/2:
%   atom roots report the whole path, variable roots report fields alone, and
%   an unnameable path reports '?'.
% DL7 seam: error payload terms are the oracle; keep ==/2-grade fixtures.
```

```prolog
% File: v6/prolog/0_dot_expand.pl:617
% Existing comment: (conjunction spine) "same shape as 0_coalesce_expand.pl"
% Signature: plain_relation_goal/1, conjunction_goals/2, goals_conjunction/2,
%   contains_dot_get/2, memberchk_eq/2
% Called by: expand_dot_rule/3, desugar_head_and_body/5, rewrite_goal/5
% Calls: surface/5 (registry), sub_term/2
% Tests: 0_generic_expand.pl fixtures
% V7 class: extract
% Parser coupling: term-shape (registry surface check distinguishes plain rel
%   goals from kernel goals)
% Preserved law: a synthesized decode lands after a plain relation atom and
%   before any non-atom goal.
% DL7 seam: keep placement law; the plain-atom test changes shape with cons
%   trees.
```

### v6/prolog/use_resolve.pl

```prolog
% File: v6/prolog/use_resolve.pl:55
% Existing comment: "Spliced leaf-first; a canonical path already in Loaded0 is
%   never re-parsed."
% Signature: expand_uses(+EntryPath, +OnStack, +Loaded0, -Loaded, -Prog, -ModuleTable)
%   / expand_uses/8
% Called by: compile.pl, diag.pl, tests (scip_namespaces.test.pl:119,
%   4_braced_nested_relations.test.pl:51)
% Calls: collect_all/8, merge_files/4, run_compile_step/4
% Tests: plunit_tests.pl (mount/diamond/cycle tests), 4_braced_nested_relations.test.pl
% V7 class: adapt
% Parser coupling: term-shape (file/7, module/3 table terms)
% Preserved law: leaf-first splice; diamond re-sight is a cache hit; cycle
%   throws use_cycle with the whole stack; entry parses LAST.
% DL7 seam: entry path -> spliced program + module table; keep the cache and
%   last-parse laws.
```

```prolog
% File: v6/prolog/use_resolve.pl:102
% Existing comment: "The dependency edge is minted for every use; an alias ADDS
%   the mount edge beside it (ruling mount_alias_additive) rather than replacing
%   it."
% Signature: use_spec_parts/4, edge_decls_for/7, edge_kind/2
% Called by: collect_children/8, collect_all/8
% Calls: module_name/2, module_hash/3
% Tests: plunit_tests.pl:9523 (mount_emits_a_mount_decl...), 11529
% V7 class: adapt
% Parser coupling: term-shape (use/1,2, pub_use/1,2 use_item parse)
% Preserved law: a use edge is minted for every import; an alias adds a mount
%   edge beside the use edge (never replaces it).
% DL7 seam: owner/name/kind edge rows per 0_SHARED.
```

```prolog
% File: v6/prolog/use_resolve.pl:128
% Existing comment: "One module's whole subtree. The ENTRY parses LAST:
%   parse_dl_source/5 retracts its statement table per call, and the diag
%   channel reads the entry's."
% Signature: collect_all/8, collect_children/8
% Called by: expand_uses/8
% Calls: resolve_use_path/3, strip_entry/4, split_use_specs/3, include_roots/2,
%   bind_executor_modules/3, edge_decls_for/7, rel_module_decls/3,
%   semantic_decl_modules/3, entry_module_decls/3
% Tests: diamond/cache tests (plunit_tests.pl:9305,9521)
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: a mounted rel keeps the identity of the module that declared
%   it, never the module that grafted it; generated relations take the entry
%   module's storage.
% DL7 seam: owner/name/ordinal edges replace the append-of-decl-lists wiring.
```

```prolog
% File: v6/prolog/use_resolve.pl:181
% Existing comment: none
% Signature: check_use_local_name_collisions/2, check_use_local_name_collision/3
% Called by: collect_all/8
% Calls: declared_path/3
% Tests: plunit_tests.pl (use local name collision)
% V7 class: extract
% Parser coupling: none
% Preserved law: a use's local name colliding with a decl the file already
%   declares throws unsupported_construct(use_path_collision/1).
% DL7 seam: unchanged law; decl-set input shape follows V7 decl nodes.
```

```prolog
% File: v6/prolog/use_resolve.pl:194
% Existing comment: "Read off the file's OWN decls, mounts excluded: a mounted
%   rel keeps the identity of the module that declared it, never the module that
%   grafted it."
% Signature: rel_module_decls/3, source_relation_name/2,
%   semantic_decl_modules/3, semantic_source_decl/3, entry_module_decls/3
% Called by: collect_all/8; consumed by lower.pl:2281/2357, compile.pl:565,
%   0_storage_projection.pl:43, enum/generic expansion
% Calls: declared_path/3, atomic_list_concat/3
% Tests: type_relation_ir.test.pl:475,644,738; anonymous_type_syntax.test.pl:271
% V7 class: adapt
% Parser coupling: none
% Preserved law: source relations carry their declaring module's hash through
%   import merging; generated relations inherit the source constructor's module;
%   a program with no module decls gets entry_module_decl/1.
% DL7 seam: rel_module_decl/2 + semantic_decl_module/3 are the module-ownership
%   rows to keep; V7 can store them as owner/target edges.
```

```prolog
% File: v6/prolog/use_resolve.pl:378
% Existing comment: "Identity is the path relative to the ENTRY's directory,
%   extension dropped: equal basenames stay distinct and an entry hashes exactly
%   its module name."
% Signature: module_name/2, module_stem/3, module_hash/3, short_hash/2,
%   canonical_abs/2, include_roots/2, resolve_use_path/3
% Called by: collect_all/8, edge_decls_for/7, compile.pl (schema digests)
% Calls: sha_hash/3, hex_byte/2
% Tests: plunit_tests.pl (module hash tests); 7_module_path.pl receipts
% V7 class: adapt
% Parser coupling: none
% Preserved law: module identity = sha256(path-relative stem) truncated to 16
%   hex chars; equal basenames stay distinct; one hash for module identity,
%   rel h_id, schema and rule digests.
% DL7 seam: keep the hash law; the SPREFA_STD root list is an install-layout
%   coupling to re-rule.
```

```prolog
% File: v6/prolog/use_resolve.pl:259
% Existing comment: "The parse counter is what a re-parsing loader trips;
%   end-state equality on a diamond looks identical whether the shared file was
%   read once or twice."
% Signature: strip_entry/4, strip_use_lines/3, bump_parse_count/2,
%   reset_parse_counts/0, parse_count/2, split_text_lines/2, parts_to_lines/2
% Called by: collect_all/8; reset_parse_counts used by loaders/tests
% Calls: use_item/3 (parser), read_file_to_string/3
% Tests: plunit_tests.pl (diamond re-parse count tests)
% V7 class: extract
% Parser coupling: token/CST (strips use lines, keeps newline so line numbers
%   match disk)
% Preserved law: a stripped use line leaves its newline behind so parse
%   diagnostics still point at real file lines; parse counts are observable.
% DL7 seam: thread_local parse_count_fact/2 must move into the loader state.
```

```prolog
% File: v6/prolog/use_resolve.pl:308
% Existing comment: "col_type/3 dedupes by (Ref, Column): an equal type keeps
%   one, a conflict hard-errors naming both paths. Every other decl and rule
%   keeps load order."
% Signature: merge_files/4, merge_col/3, col_type_seen/6, col_type_indexed/4,
%   strip_paths/2, merged_prog/4, prog_parts/4
% Called by: expand_uses/8
% Calls: assoc library
% Tests: plunit_tests.pl (merge order tests)
% V7 class: adapt
% Parser coupling: term-shape (prog/2 vs program/3 re-pick, sh_decl presence)
% Preserved law: col_type dedupe by (Ref, Column) with hard conflict error;
%   all other decls and rules keep load order.
% DL7 seam: keep the dedupe law; the prog/2|program/3 re-pick is DL6 shape.
```

### v6/prolog/executor_modules.pl

```prolog
% File: v6/prolog/executor_modules.pl:39
% Existing comment: "A roster row with no `__` names no module and exports
%   nothing."
% Signature: executor_family_export(?Family, ?Segments, ?Canonical) is nondet
% Called by: bind_executor_modules/3, check_known_module/2
% Calls: arrival_executor/2 (registry)
% Tests: compile/test/scip_namespaces.test.pl
% V7 class: oracle
% Parser coupling: token/CST (registry roster)
% Preserved law: three spellings (`use soopy.`, `use soopy as s.`, `rel
%   /soopy/files(...)`) reach one canonical `__`-joined atom.
% DL7 seam: registry roster becomes data; the rename law stays.
```

```prolog
% File: v6/prolog/executor_modules.pl:64
% Existing comment: "Term-identical out when no declaration is claimed by an
%   import."
% Signature: bind_executor_modules(+ModuleSpecs, +parts(D,R,Q), -parts(D,R,Q))
% Called by: use_resolve.pl:153 (collect_all)
% Calls: check_known_module/2, executor_family_export/3, claimed_by/3,
%   rename_term/3
% Tests: scip_namespaces.test.pl:57; executor module tests
% V7 class: adapt
% Parser coupling: term-shape (rel_name_argument/2 whitelist: sh_decl/4,
%   arrival_identity/2, probe/4)
% Preserved law: the import is a RENAME over the importing file's own program;
%   a rel name reaches a term in exactly four shapes (decl ref, plain atom
%   functor, rel_name_argument arg 1, rel_path segments) and nowhere else.
% DL7 seam: the four-shape law is the oracle; cons-tree terms must enumerate
%   their rel-name positions equivalently.
```

```prolog
% File: v6/prolog/executor_modules.pl:96
% Existing comment: (local_name) alias prefixes the `__` join
% Signature: local_name/3, claimed_by/3, ambiguous_executor_leaf
% Called by: bind_executor_modules/3
% Calls: atomic_list_concat/3
% Tests: ambiguous_executor_leaf via plunit tests
% V7 class: adapt
% Parser coupling: token/CST (`__` join as identifier)
% Preserved law: two used modules exporting one unaliased leaf stop at
%   ambiguous_executor_leaf rather than picking a winner.
% DL7 seam: keep the ambiguity refusal; the join spelling is DL6 surface.
```

### v6/prolog/0_annotation_expand.pl

```prolog
% File: v6/prolog/0_annotation_expand.pl:18
% Existing comment: "The first application receives the parsed input type.
%   Each later application receives the result placeholder produced by its
%   predecessor. The placeholders are site-local and ordered..."
% Signature: elaborate_annotation(+InputType, +Applications, -Elaborated)
% Called by: 0_generic_expand/1_annotations.pl
% Calls: add_implicit_target/3
% Tests: anonymous_type_syntax.test.pl, 4_braced_nested_relations.test.pl:383
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: the implicit Target sequence is made explicit with site-local,
%   ordered annotation_result(Ordinal) placeholders; the Target is implicit
%   (annotation_target_is_implicit refusal for explicit).
% DL7 seam: in: (InputType, Applications); out: annotation_steps/2 term.
%   Ordinal threading is the V7 `ordinal edge` candidate.
```

```prolog
% File: v6/prolog/0_annotation_expand.pl:33
% Existing comment: (module header) "This phase only makes the implicit Target
%   sequence explicit."
% Signature: add_implicit_target(+Application, +Target, -ElaboratedApplication)
% Called by: elaborate_steps/4
% Tests: annotation tests in 0_generic_expand/1_annotations.pl
% V7 class: adapt
% Parser coupling: term-shape (named('Target', _) first argument)
% Preserved law: explicit Target supply by the author is refused
%   (annotation_target_is_implicit) in 0_generic_expand/1_annotations.pl:251.
% DL7 seam: named('Target', T) placeholder position preserved or re-ruled.
```

### v6/prolog/compile/test/scip_namespaces.test.pl

```prolog
% File: v6/prolog/compile/test/scip_namespaces.test.pl:1
% Existing comment: "FAIL-FIRST: sh_head//2 read a bare ident, so a
%   multi-segment executor path did not parse at all"
% Signature: (plunit tests, scip_source/1 fixture)
% Called by: plunit
% Calls: parse_dl_dcg_entry/5, expand_uses/8, compile_host_decl/2, registry
% Tests: self; dl/deadcode/receiver-rail.dl6 rail file
% V7 class: oracle
% Parser coupling: token/CST (slash-rooted vs dotted path spellings)
% Preserved law: slash-rooted and dotted spellings reach one `__`-joined atom;
%   emitted demand/response rel names are SQL identifiers (alnum + `_` only).
% DL7 seam: the flat-atom law survives; the slash/dotted surface spellings are
%   DL6 frontend policy.
```

### Fixtures

```prolog
% File: v6/prolog/conformance/fixtures/7_module_path.pl
% Existing comment: "Module paths in FUNCTOR position. A decl reached at a path
%   carries its segment list as rel_path_decl/2; the flat name is the segments
%   joined by `__` and is what every phase past the dot phase reads."
% Signature: fixture/4 rows (module_path_in_head_resolves..., nested_* family)
% Called by: conformance runner
% Calls: -
% Tests: self
% V7 class: oracle
% Parser coupling: term-shape (rel_path/2, rel_path_decl/2)
% Preserved law: path resolution mints NO new rel; the flat `__` name is the
%   identity; local name binds before a dotted path ending in the same segment;
%   a walk off the decl tree refuses with every segment in the payload; nested
%   children keep authored arity and zero-column children stay flat markers.
```

```prolog
% File: v6/prolog/conformance/fixtures/7_module_path_element.pl:42
% Existing comment: generic expansion ran before the dot phase and met an
%   unresolved type_path/1; resolution now runs ahead of the fold
% Signature: fixture/4 (list/option/list-list/json_list element positions)
% Called by: conformance runner
% Tests: self (compile-time fail-first receipts in header)
% V7 class: oracle
% Parser coupling: term-shape
% Preserved law: a dotted path resolves in element position, including nested
%   list levels minting both artifacts off one resolved element name;
%   json_list(rel) keeps its decided stop and names the RESOLVED rel.
```

```prolog
% File: v6/prolog/conformance/fixtures/7_module_path_wrapper.pl:43
% Existing comment: fail-first receipts; the resolution graded is
%   rewrite_rel_paths/3 which walks into any compound
% Signature: fixture(...) coalesce/latest/combine over dotted sources
% Called by: conformance runner
% Tests: self
% V7 class: oracle
% Parser coupling: term-shape
% Preserved law: a dotted atom inside every surface wrapper (latest, finalize,
%   next, pre, coalesce, pre/2, combine) resolves exactly as in a bare
%   conjunction.
```

```prolog
% File: v6/prolog/conformance/fixtures/20_parent_chain.pl
% Existing comment: "A column typed option(<its own rel>) is the parent-chain
%   shape. The companion split rel names the owner endpoint after the rel and
%   the target endpoint after the column."
% Signature: fixture(self_ref_option_chain..., acyclic_*, a_self_loop..., ...)
% Called by: conformance runner
% Tests: self
% V7 class: oracle
% Parser coupling: term-shape (option(node) typing, node__parent companion)
% Preserved law: there is no implicit parent edge; the parent chain is an
%   explicit option(self) column whose companion split rel carries the edge,
%   with cycle detection naming the whole path and retraction freeing the
%   reverse edge.
```

```prolog
% File: v6/prolog/conformance/fixtures/scopes.pl
% Existing comment: "the scopes/switch-flow promotion ... all written in the
%   MINIMAL KERNEL the ruling landed on: zero stored forest rels ... Every
%   scope root is an ordinary program keyed Set rel"
% Signature: fixture/4 (12 scenarios)
% Called by: conformance runner
% Tests: self
% V7 class: oracle
% Parser coupling: term-shape
% Preserved law: a scope is an ordinary keyed rel row; nesting is rule text
%   (the parent join); scope_done is a plain rule; the zombie A2b leak is
%   asserted as today's behavior until scope_cover_check lands.
% DL7 seam: these twelve are the projection/nesting oracle; no engine concept
%   may be reintroduced.
```

```prolog
% File: v6/prolog/compile/test/4_braced_nested_relations.test.pl:60
% Existing comment: none (block header)
% Signature: brace_equivalent_sources/2 + tests
% Called by: plunit
% Calls: parse_dl/4, print_dl_program/3, expand_program/3, program_plan/3,
%   full target bundle
% Tests: self
% V7 class: oracle
% Parser coupling: token/CST (braces surface)
% Preserved law: DL6 braces normalize to dotted rel_path_decl at parse
%   (=@= program equality); the canonical printer emits dotted declarations;
%   brace and dotted programs emit identical target artifacts; children keep
%   authored columns/keys and mint NO parent column; explicit parent columns
%   use ordinary relation-value typing; a flat-name collision mangles with a
%   digest suffix.
```

```prolog
% File: v6/prolog/conformance/fixtures/0_decl_order.pl:7
% Existing comment: "A generic list declaration must preserve the author's
%   column order in the unrelated struct reference used by tree_label/2."
% Signature: fixture(declaration_order_preserves_struct_refs, ...)
% Called by: conformance runner
% Tests: self
% V7 class: oracle
% Parser coupling: none
% Preserved law: authored column order survives expansion and struct decode.
```

## Places that assume DL6 surface policy (the audit's named list)

| Assumption | Sites |
|---|---|
| DL6 braces normalize to dotted decls at parse | `compile/parse_dl_dcg.pl:52,1742` (braces_term//1, cst_shape braces_term/1, brace_pair); `4_braced_nested_relations.test.pl:76-93,165-171,472` (braces =@= dotted, printer turns braces into dotted decls). V7: drop the brace statement surface; the OTHER brace use, the decode pattern `'{}'(Key:Leaf)`, is value-plane and stays (0_dot_expand.pl:530) |
| Dotted/slash surface names, `__` mangling | `0_dot_expand.pl:336-345` (rel_template `__` join), `executor_modules.pl:96-99,111-113,158-163`, `use_resolve.pl:202-203,224-225`, scip test:57 ("one segment list, two path spellings, both reach one atom"). Flat name = `atomic_list_concat(Segments,'__')` is assumed by every phase past the dot phase and by SQL/Rust/TS identifier emission (7_emit_ts_types.pl:288, 8_emit_rust_types.pl:758 handle the double-underscore empty-part case) |
| Implicit parents | none in decls: `4_braced_nested_relations.test.pl:108` asserts `\+ col_type(orchard__tree/_, parent, _)`; parent references are explicit option(self) columns (20_parent_chain.pl header); `plunit_tests.pl:2083` records name-path nesting removed four implicit parent-reference planes; `plunit_tests.pl:7154` "a_dotted_head_needs_no_implicit_parent_binding". V7 assumption "no implicit declaration" is already DL6 law here |
| Capitalization | parse-time capitalized keyword pun: `compile/parse_dl_dcg.pl:1301-1337` (`Name` -> `name`, `PollPeriod` -> `poll_period`), requires ALL args to pun (`resolve_named_args/4` guard). Reads `b_getval(dl_vars, Vars)` (global backtrack state, parse_dl_dcg.pl:1339). Emitted type names PascalCase via `emit_ts.pl:146`, `compile/7_emit_ts_types.pl:288`, `8_emit_rust_types.pl:742`. `13_initcap.pl` / registry `initcap/1` is a scalar SQL builtin, orthogonal to naming |
| Declaration order | `0_dot_expand.pl:319` sort of Paths0 before tree insert; `check_path_collisions/1` refuses one path spelling two rels; `use_resolve.pl` entry parses LAST (comment at :126), merge keeps load order, diamond fold caches leaf-first; `0_decl_order.pl` pins authored column order through generic expansion |
| Implicit parents / scope cover | `scopes.pl` fixture 12 records the zombie-scope gap: the minimal kernel moved the scope tree into rule text where no check reads it; `scope_cover_check` is unbuilt (ARCH task). `20_parent_chain.pl` is the explicit-parent-chain oracle |

## Closing items

### 1. Predicate counts by class

Counted over the material predicates in the four primary files + executor_modules (helpers counted with their caller):

| Class | Count | Predicates |
|---|---|---|
| extract | 7 | refresh_relation_type_decls/2, erase_type_path_aliases/2 + is_type_path_alias/1, anonymous_sum_path_aliases/3, split_use_specs/3, module_use_spec/2, subtree_paths/2, conjunction_goals/2 |
| adapt | 24 | expand_dot_in_context/3, resolve_qualified_types/2, resolve_qualified_type_paths/3, resolve_qualified_type/3, relation_id_path/3, resolve_enum_arm_term/3, enum_arm_ref/4, resolve_rel_path_rule/3, resolve_relation_paths/3, rewrite_rel_paths/3, decl_scope_tree/2, declared_path/3, decl-flat helpers (3), insert_path/insert_segments/resolve_path/descend, expand_dot_rule/desugar_* /rewrite_*/replace_dot_gets group, dot_fields_pattern group, bound_body_vars group, expand_uses/6,8, collect_children/8, collect_all/8, edge_decls_for/7, rel_module_decls/3, semantic_decl_modules/3, bind_executor_modules/3, merge_files/4, elaborate_annotation/3, elaborate_steps/4, add_implicit_target/3, executor_family_export/3 |
| oracle | 8 | fields_pattern/3 + payload conventions, check_dot_receiver/3, dot_path_atom/2, check_path_collisions/1, scip_namespaces.test.pl, scopes.pl, 7_module_path*.pl, 4_braced_nested_relations.test.pl, 20_parent_chain.pl |
| drop | 3 | rel_path_parts/3 '.'/2 SWI-dict trap clause, braces_term//1 DL6 surface, slash-rooted executor spelling (surface-policy) |

### 2. Canonical term shapes entering and leaving the slice

Entering (post-parse):
- `rel_path_decl(Name/Arity, Segments)` — authored dotted decl, Name already `__`-joined
- `rel_path(Segments, Args)` — path-carrier atom in rules/queries/types
- `type_path(Segments)` / `type_path_application(Segments, Args)` — unresolved surface types
- `dot_get(Receiver, Field)` — one parsed dot hop, nested
- `mount_decl(Alias, MountedName, OwnerName, Paths)` + `module_edge_decl(OwnerHash, ChildHash, Kind, LocalName)`, kinds `use | pub_use | mount`
- `mount_decl` paths: list of `Segments-Name` pairs

Leaving the slice (flat identity, past the dot/scope phases):
- atoms with `__`-joined flat names (`orchard__tree`), `id(RelationName)` for `.id`
- `decode(Receiver, '{}'(Field:Leaf))` pairs (the value-plane brace)
- scope tree `node(Segment, none|some(Name), Children)` rooted at `node(file, none, [])`
- module decls: `module_decl(Name,Hash)`, `module_storage_decl(Hash,Stem)`, `rel_module_decl/2`, `semantic_decl_module/3`, `entry_module_decl/1`, `module_edge_decl(OwnerHash,ChildHash,use|pub_use|mount,LocalName)`

### 3. Hidden state
- `thread_local parse_count_fact/2` (use_resolve.pl:30) — re-parse detection; exported `reset_parse_counts/0`, `parse_count/2`.
- `:- dynamic hex_byte/2` asserted at load (256 clauses).
- `b_getval(dl_vars, Vars)` in `compile/parse_dl_dcg.pl:1339` (`variable_source_name/2`) — parser backtracking state backing the capitalized pun.
- `merge_col` assoc index keyed `Ref-Column`, degrades to unkeyed scan if a key is non-ground.
- `collect_all/8` parses the ENTRY last because `parse_dl_source/5` retracts its statement table per call (diag channel dependency).
- Cuts: `resolve_use_path/3` (once, first root wins — root order is observable), `fields_pattern/3`, `entry_module_decls/3`, `split_use_specs/3` via once/`;`-cut in `strip_use_lines`, `receiver_relation_type/4` (once/0).
- No tabling. One op block (1150 `<-`, `<+`, `:=`) redeclared per file.

### 4. Smallest self-contained extraction boundary
`declared_path/3` + `decl_scope_tree/2` + `resolve_path/3`/`descend/3` +
`check_path_collisions/1` + `insert_path/insert_segments` in `0_dot_expand.pl:318-404`,
fed by `use_resolve.pl`'s `mount_decl/4` + `subtree_paths/2`. This is the
scope/identity kernel: decl list in, scope tree + resolution out. Everything
else (dot desugar, type resolution, executor rename) consumes it.

### 5. First dependency forcing adaptation instead of extraction
`declared_path/3`'s mount clause (0_dot_expand.pl:353) and the flat-name clause
(346) are recursive through the CONCATENATED decl set that `use_resolve.pl:subtree_paths/2`
mints, and `decl_scope_tree/2` is called from three modules on different decl
sets (authored, path-extended, concatenated). The scope tree's input is a
program-wide concatenation, so V7 owner/target edges must arrive already
spliced before this kernel can extract as-is; the `__`-joined flat-name
identity is baked into the Name side of every pair.

### 6. Unresolved questions requiring a V7 language ruling
1. Does DL7 keep `__`-joined flat names as the single runtime identity, or move
   to structured owner/name edges with the `__` atom only at SQL emission? The
   `__` join is a legal-identifier constraint from SQL emission
   (scip_namespaces.test.pl:93) and mangled artifact names
   (`__gen__list_orchard__tree_<hash>`) pin it byte-exactly.
2. Is `.id` relation identity (relation_id_path/3) retained, and does it stay
   prefix-resolved-only, or become a kernel edge form?
3. Does the capitalized keyword pun survive? It is parse-time surface policy
   over variable IDENTITY (`variable_source_name` via `b_getval(dl_vars)`);
   DL7's explicit `?Variable` spelling makes the surface name available, so the
   pun's ground-payload trick may become simpler or obsolete.
4. Are interior rooms (`orchard.north.tree` with no `north` decl) still minted
   from the path, or must every interior room be declared?
5. Local-name-binds-before-dotted-path: keep as resolution law or make it an
   error (the two-rels-never-merge law is load-bearing for fixture
   module_path_local_name_binds_before_the_dotted_one).
6. Scope_cover_check (zombie_scope_negative_case_a2b): does V7 build the static
   scope-key column-flow check, and does this fixture's expectation flip?
7. Annotation `named('Target', _)` implicit-target spelling: keep the
   `named/2` site-local placeholder, or map to kernel owner/target edges per
   0_SHARED?
