# Slice 2: AST and relation normalization

Read-only audit of the DL6 expansion/normalization half: surface-sugar expansion
(`1_expansion.pl`), its per-phase expanders in this slice (`0_ast_expand.pl`,
`0_seq_expand.pl`, `0_relation_edge_expand.pl`, `0_relation_pattern.pl`), the
shared body traversal (`0_body_walk.pl`), and the rel record accessor module
(`0_rel_record.pl`). Protocol: `v7/1_AUDIT/0_SHARED.md`.

## Contents

1. Earliest stable IR
2. Report blocks
3. Counts by class
4. Canonical term shapes in and out
5. Hidden state
6. Smallest extraction boundary
7. First forced adaptation
8. Unresolved V7 rulings

---

## 1. Earliest stable IR

The pipeline entering this slice:

```
surface prog(Decls, Rules)                      (parser output, Prolog terms)
  -> expand_program/3        1_expansion.pl     ordered phase fold
       5 option / 10 enum / 40 match / 42 seq / 44 dot / 45 coalesce
       46 ast / 47 negated_guard / 50 relation_edge
  -> prog(FrozenDecls, OptionRules)             leaving slice
```

The earliest stable IR inside the slice is the post-fold
`prog(FrozenDecls, OptionRules)` at `1_expansion.pl:127-131`: all surface sugar
(`seq`, `ast`, match arms, dot chains, coalesce, negated guards, relation-edge
membership) has been lowered to ordinary `prog/2` declarations and rules over
flat atoms, with type rows frozen. Every consumer after `plan`
(`0_program_check`, `analyze`, `compile`/`lower`, engine, TS/Rust emitters)
reads that shape.

Two normalization passes are separate from the fold:

- `0_relation_pattern.pl:expand_relation_values/2` runs AFTER `expand_program`
  (reference engine `conformance/engine.pl:631`), rewriting relation-shaped
  head/body terms into canonical `obj([...])` objects. It is the LAST
  normalization before storage/unification.
- `0_ast_expand.pl` is both a phase (46) and a standalone entry
  (`expand_ast_program_with_bindings/3`) used by the compiler
  (`compile/test/plunit_tests.pl` line 5035 region) with the compiler's
  bindings door.

Predicates whose ONLY purpose is DL6 syntax recovery (surface sugar a DL7
cons-tree source would spell differently or not at all): the seq four-rule
lowering, the AST host-sh demand/response minting, the negated-guard inversion
hook, coalesce splitting, dot chains, match arms, enum/option/row merge, and
the `<+`/`<-`/`:=` operator declarations. These are marked below; a DL7
frontend may express them directly.

Predicates preserving semantics (body, occurrence, edge, relation, rel record):
`walk_body` and its projections, `expand_relation_values`, the edge-membership
append, the rel/5 accessors. These survive as laws.

---

## 2. Report blocks

### 1_expansion.pl

```prolog
% File: v6/prolog/1_expansion.pl:35
% Existing comment: expansion_phase(+Order, +Name, +Expander) declared-order
%   table; comments above lines 41-68 justify each ordering edge (dot after
%   match before coalesce; coalesce before relation_edge; negated_guard after
%   coalesce).
% Signature: expansion_phase(Order, Name, Expander)
% Called by: expand_program_run/5 (msort into the fold);
%   compile/test/plunit_tests.pl:5017 pins the order
% Calls: none (data)
% Tests: v6/prolog/compile/test/plunit_tests.pl (~line 5017)
% V7 class: adapt
% Parser coupling: surface-policy (the phase order IS DL6 surface recovery)
% Preserved law: phases run in declared Order, each prog->prog, deterministic
%   and order-pinned.
% DL7 seam: DL7 may collapse or reorder phases if the cons-tree source is
%   already in canonical shape; keep the phase-name trace step
%   (expansion:Name in run_compile_step).
```

```prolog
% File: v6/prolog/1_expansion.pl:73
% Existing comment: expand_program/3 + with_bindings run the fold under the
%   oracle/door distinction; expand_program_run comment block explains the
%   prepass (qualified types, annotation context, enum context) before phase 5.
% Signature: expand_program(+SurfaceProgram, -ExpandedProgram, -ExpansionContext)
%   / expand_program_with_bindings/4 / expand_program_run/5
% Called by: conformance/engine.pl:624, tools/self_map_facts.pl:213,
%   compile/test/plunit_tests.pl (dozens), sweep.pl, compile.pl chain
% Calls: run_compile_step/4, resolve_qualified_types, annotation_context_decls,
%   enum_context, expansion_phase/3, foldl(run_phase), refresh_relation_type_decls,
%   erase_type_path_aliases, drop_minted_keyed_on_derived, merge_enum_type_rows,
%   merge_option_type_rows, freeze_type_rows
% Tests: v6/prolog/compile/test/plunit_tests.pl (expansion order, idempotence,
%   permutation goldens at 5753-5836), dl6c.test.pl
% V7 class: adapt
% Parser coupling: surface-policy (annotation_context_type strips
%   annotated_type wrappers; direct_compiler_type_call/3 is DL6 decl-shape)
% Preserved law: surface program -> fully expanded program where every later
%   phase is invariant to source spelling of the sugar.
% DL7 seam: prog(Decls, Rules) of DL7 cons trees in; same shape out with
%   sugar-free rules; expansion context carrying enum rows and bindings.
```

```prolog
% File: v6/prolog/1_expansion.pl:143
% Existing comment: none directly above annotation_context_decls
% Signature: annotation_context_decls(Decls0, Decls) and
%   annotation_context_type/3 + direct_compiler_type_call/2
% Called by: expand_program_run/5 only
% Calls: itself recursively
% Tests: indirectly via compile/test/annotation_surface.test.pl
% V7 class: adapt
% Parser coupling: term-shape (annotated_type/2 wrapper is DL6 surface)
% Preserved law: compiler-annotation type calls contribute their input type to
%   the enum context, unwrapping nested annotations.
% DL7 seam: type expr -> input type projection; keep as pure function.
```

```prolog
% File: v6/prolog/1_expansion.pl:176
% Existing comment: entering line runs unconditionally so a wedged phase names
%   itself; comment at 192-197 explains the ast cut.
% Signature: run_phase/4, run_phase_step/4, run_phase_call/4
% Called by: expand_program_run/5 (foldl)
% Calls: dl6_debug/3, dl6_debugging/1, dl6_program_sizes/3, run_compile_step/4
% Tests: indirectly all expand_program tests
% V7 class: extract
% Parser coupling: none
% Preserved law: phases run in Order; unwired is identity; ast/option get the
%   bindings-carrying context; the cut on ast/option/coalesce clauses is part
%   of the contract (prevents wrong-argument re-call).
% DL7 seam: phase list + context -> folded program; logging can move out.
```

### 0_ast_expand.pl

```prolog
% File: v6/prolog/0_ast_expand.pl:19
% Existing comment: none (module header only)
% Signature: expand_ast_program_with_bindings(+Program, +Bindings, -Expanded)
% Called by: expand_ast_program/2, expand_ast_in_context/3 (phase 46),
%   compile/test/plunit_tests.pl:3832-5035
% Calls: cst_unsupported, normalize_cst_program, ast_unsupported, rewrite_rules
% Tests: v6/prolog/compile/test/plunit_tests.pl (ast phase tests)
% V7 class: adapt
% Parser coupling: term-shape (cst/4-cst/5 body nodes, serialized-tree-Query)
% Preserved law: an ast/4 body goal is replaced by a host call plus demand/
%   witness/response atom chain, deduplicated per (Language, Query) mapping.
% DL7 seam: in: program with cst/5 nodes; out: prog with sh_decl + minted
%   __host_demand_/__host_response_ relations + witness binds. DL7 would keep
%   the host contract but restate cst(Path,Digest,Lang,Query) as a cons-tree
%   node.
```

```prolog
% File: v6/prolog/0_ast_expand.pl:35
% Existing comment: none
% Signature: ast_unsupported/1, cst_unsupported/1, *_unsupported_term/3
% Called by: expand_ast_program_with_bindings/3
% Calls: 0_program_check:first_violation/3
% Tests: compile/test/plunit_tests.pl ast refusal tests (3832-3844)
% V7 class: drop
% Parser coupling: token/CST (cst_* violations name DL6 tree-node spelling;
%   ast_query_single_quote names DL6 single-quote query policy)
% Preserved law: unsupported tree shapes throw named unsupported_construct
%   reasons instead of silently compiling.
% DL7 seam: keep the violation names as an oracle contract; the cst_ family
%   dies with the DL6 tree-node spelling.
```

```prolog
% File: v6/prolog/0_ast_expand.pl:63
% Existing comment: none
% Signature: normalize_cst_program/2, normalize_cst_rule/2, normalize_cst_body/2
% Called by: expand_ast_program_with_bindings/3
% Calls: 0_cst_query:serialize_ts_query/2
% Tests: compile/test/plunit_tests.pl ast tests
% V7 class: drop
% Parser coupling: term-shape (cst/5, cst/4 -> ast/4 rewrite)
% Preserved law: a tree-sitter query node serializes to its text form once,
%   before unsupported checks and rewriting.
% DL7 seam: none; the cons-tree source carries the query text directly.
```

```prolog
% File: v6/prolog/0_ast_expand.pl:117
% Existing comment: none
% Signature: rewrite_rule_body/11, rewrite_goals/9
% Called by: rewrite_rules/3 (from expand_ast_program_with_bindings)
% Calls: body_goals/2, goals_body/2, build_rule/4, ast_host/9, host_atoms/5
% Tests: compile/test/plunit_tests.pl ast tests
% V7 class: adapt
% Parser coupling: term-shape (ast/4 goal functor)
% Preserved law: every ast body goal becomes one demand rule + a witness bind
%   + a response atom in the enclosing rule, in left-to-right order; the
%   enclosing rule is appended AFTER the demand rules.
% DL7 seam: body goal list -> (demand rule, rewritten body) pairs.
```

```prolog
% File: v6/prolog/0_ast_expand.pl:153
% Existing comment: none
% Signature: ast_host/9, ast_host_decl/5, output_columns/2, column_type_decls/3,
%   host_relation_refs/3, host_atoms/5, digest_expr/5, output_variables/3
% Called by: rewrite_goals/9
% Calls: 0_program_check:ast_capture_names/2
% Tests: compile/test/plunit_tests.pl:5035 region
% V7 class: adapt
% Parser coupling: surface-policy (host command template string,
%   `$DL_EXTRACT_BIN` shell contract, __ast_q/__host_demand_/__host_response_
%   name minting, identity/witness digest concatenation format)
% Preserved law: identical (Language, Query) pairs share one host relation and
%   one sh_decl; demand/response arity = 2+inputs (+outputs); the witness bind
%   `WitnessValue := Witness` connects demand to response.
% DL7 seam: capture-name extraction in; decl terms + demand/response pair out.
%   The digest concat format is a cross-language contract (TS/Rust emitters
%   mirror it) and must survive as-is.
```

```prolog
% File: v6/prolog/0_ast_expand.pl:243
% Existing comment: none
% Signature: body_goals/2, goals_body/2, build_rule/4
% Called by: rewrite_rule_body/11, rewrite_goals/9
% Calls: none
% Tests: indirect
% V7 class: extract
% Parser coupling: term-shape (','/2 spine, true atom)
% Preserved law: conjunction flattening/rebuilding is shape-based and exact.
% DL7 seam: keep only if DL7 bodies are Prolog conjunctions; under cons trees
%   the equivalent is list flatten.
```

### 0_body_walk.pl

```prolog
% File: v6/prolog/0_body_walk.pl:59
% Existing comment: file header (lines 1-42) is the full contract: event/4
%   list, left-to-right, polarity absorbing, wrapper always emitted,
%   conjunction always flattened by shape.
% Signature: walk_body(+Body, +WalkPolicy, -Events) ; walk_node/6 ;
%   node_surface/2 ; walk_children/7 ; walk_arguments/6
% Called by: 0_program_check.pl:859, analyze.pl:114/1767/1884,
%   0_relation_edge_expand.pl:74, conformance/body.pl:598,
%   conformance/engine.pl:332, and all projections below
% Calls: compile/registry:body_surface_for_term/6
% Tests: v6/prolog/compile/test/plunit_tests.pl body_walk_characterization
%   (4085-4490), conformance/body.pl
% V7 class: extract
% Parser coupling: term-shape (not/1, next/1, combine, ','/2 shapes come from
%   the registry, not this file; the walk itself is registry-driven)
% Preserved law: one left-to-right event list, wrapper always emitted,
%   polarity absorbing through not/1, conjunction flattened by shape, variable
%   sharing preserved (never findall).
% DL7 seam: in: DL7 body cons tree + policy; out: event(Path, Polarity,
%   Surface, Term) list. Registry rows (Surface) replace
%   body_surface_for_term/6 with the DL7 construct table.
```

```prolog
% File: v6/prolog/0_body_walk.pl:114
% Existing comment: comment above states splice_bare semantics and the
%   NEVER-findall rule (copying breaks variable sharing).
% Signature: body_conjunction_goals/3, events_goals/3, spliced_goal/3
% Called by: analyze.pl (conjunction_goals consumers), 0_program_check.pl
% Tests: plunit_tests.pl body_walk_characterization
% V7 class: extract
% Parser coupling: none
% Preserved law: goals keep sharing the body's own variables; splicing drops
%   the wrapper and stands in its arguments.
% DL7 seam: body -> ordered goal list with variable sharing intact.
```

```prolog
% File: v6/prolog/0_body_walk.pl:152
% Existing comment: the wrapper-family comment (lines 134-151): the family
%   latest/pre/finalize is STATED, not derived, with the B11 rationale and the
%   registry pin test.
% Signature: relation_atom_wrapper/1, event_relation_atom/2,
%   body_relation_atoms/4, body_reserved_word/4, body_wrapper_refs/4,
%   wrapper_arity/2
% Called by: 0_program_check.pl:145-158/549/865, analyze.pl:1448-1586,
%   conformance/engine.pl:367-377, 0_relation_pattern.pl:79,
%   compile/test/plunit_tests.pl:4466 (family-vs-registry pin)
% Calls: walk_body/3
% Tests: compile/test/plunit_tests.pl relation_atom_wrapper_family_matches_
%   the_registry, body_walk_characterization
% V7 class: oracle
% Parser coupling: none (the family is the drift-stopper; the registry pin
%   test is the contract)
% Preserved law: exactly latest/1, pre/1, pre/2, finalize/1 hold a relation
%   atom as their single argument, in one place, pinned to the registry.
% DL7 seam: keep the stated list as data; consumers rebind to DL7 wrappers.
```

### 0_relation_pattern.pl

```prolog
% File: v6/prolog/0_relation_pattern.pl:32
% Existing comment: file header (lines 1-16) — rel-term -> canonical obj
%   rewriting, malformed shapes rejected by shared checks.
% Signature: expand_relation_values/2, expand_rule/4, expand_goal/4,
%   expand_surface_goal/5, expand_atom/3, expand_argument/4
% Called by: conformance/engine.pl:631 (the reference engine's own pass, after
%   expand_program)
% Calls: 0_type_plane:type_definitions, declared_type_name,
%   relation_columns_and_types, relation_value_object; registry:
%   body_surface_for_term; 0_body_walk:relation_atom_wrapper
% Tests: compile/test/anonymous_product_values.test.pl:118,
%   compiler_relations/0_value_domains.test.pl, conformance/engine.pl
% V7 class: adapt
% Parser coupling: term-shape (obj/1 canonical object; Prolog =.. rebuild)
% Preserved law: a relation-shaped term in a rule head or body is rewritten to
%   the canonical nested object the rest of the system speaks, before
%   storage/unification; recursion descends the not arm, splice families, and
%   the wrapper family only.
% DL7 seam: in: prog with authored rel-value terms + type table; out: prog
%   with obj([...]) pairs. Under DL7 cons trees this becomes a term rewriter
%   keyed on declared column types; the type resolution half
%   (0_type_plane:relation_value_object) is the real dependency.
```

### 0_relation_edge_expand.pl

```prolog
% File: v6/prolog/0_relation_edge_expand.pl:29
% Existing comment: file header (lines 1-16) — head relation values require
%   target membership via ordinary body relations; edge rules get latest(...)
%   samples; no-op when the body already reads the identical pattern.
% Signature: expand_relation_edges_in_context/3, expand_rule_relation_edges/4,
%   missing_head_target_atoms/5, head_target_atoms/4, relation_value_atom/3,
%   target_absent_from_body/2, body_reads_identical_target/2,
%   unwrap_membership_sample/2, wrap_latest/2, append_body_goals/3,
%   append_one_goal/3
% Called by: 1_expansion.pl expansion_phase(50, relation_edge, ...)
% Calls: 0_type_plane:type_definitions/type_definition/declared_type_name,
%   0_body_walk:walk_body/3
% Tests: compile/test/plunit_tests.pl:6344 (comment names this module and the
%   hand-built no-op case), type_relation_ir.test.pl
% V7 class: adapt
% Parser coupling: term-shape (rel-value head args as compound terms functor-
%   equal to the type name; latest/1 wrapper)
% Preserved law: every head relation-value target is made visible to
%   stratification by an appended membership atom (latest-wrapped for edge
%   rules), skipped iff the body already reads the identical pattern.
% DL7 seam: in: expanded prog + type defs; out: prog with 0..n appended body
%   atoms per clause. Phase position (after coalesce, before checks) is the
%   law; keep it phase-pinned in DL7.
```

### 0_seq_expand.pl

```prolog
% File: v6/prolog/0_seq_expand.pl:18
% Existing comment: file header — seq sugar expands to four ordinary rules;
%   cursor is a visible keyed relation (tick-log contract).
% Signature: expand_seq_in_context/3, expand_rules/5, add_cursor_decls/3,
%   four_rules/8, seq_rule_parts/7, seq_in_level_rule/2, cursor_ref/3,
%   refuse_author_collision/2, original_refs/3, declaration_ref/2, term_ref/2,
%   infer_partition_type/4, declared_columns/3, conjunction_goals/2,
%   goals_conjunction/2, replace_nth1/4
% Called by: 1_expansion.pl expansion_phase(42, seq, ...)
% Calls: none outside (self-contained)
% Tests: compile/test/plunit_tests.pl:5103 (four-rule cursor block),
%   5127 (level-rule refusal), 5144-5185 (expansion shapes + collisions)
% V7 class: adapt
% Parser coupling: surface-policy (the `Ordinal := seq(Partition)` bind
%   spelling, seq_in_level_rule refusal, cursor name minting
%   `seq_<Head>_<Pos>`, keyed([1]) minting)
% Preserved law: one seq bind expands to the shared four-rule cursor block;
%   the cursor relation's name and rows are tick-log contract; a surface decl
%   with the minted name is an author collision; seq in a level rule is
%   refused.
% DL7 seam: in: edge rule with a seq bind; out: four edge rules + col_type/
%   keyed cursor decls. DL7 keeps the four-rule shape (both engine and
%   compiler consume the result) even if the bind spelling changes.
```

```prolog
% File: v6/prolog/0_seq_expand.pl:160
% Existing comment: none
% Signature: infer_partition_type/4
% Called by: expand_rules/5
% Calls: conjunction_goals, declared_columns
% Tests: plunit_tests.pl seq tests
% V7 class: adapt
% Parser coupling: term-shape (bool_lit/1, literal typing, col_type lookup)
% Preserved law: partition type inferred from literal or a matching declared
%   column; unknown partition type throws seq_partition_type_unknown.
% DL7 seam: literal-or-column type inference is a semantic law; keep as a
%   function over the type plane.
```

### 0_rel_record.pl

```prolog
% File: v6/prolog/0_rel_record.pl:39
% Existing comment: file header (lines 1-37) — the rel/5 field contract, the
%   single documentation site; declared vs inferred origins; relplan_ prefix
%   names the plan list.
% Signature: rel_cols/4, inferred_cols/3, relplan_parts/6,
%   relplan_storage_name/2,3, relplan_origins/2, relplan_declared/2,
%   relplan_of/3, relplan_shape/6, relplan_kind/3, relplan_columns/3,
%   relplan_column_types/3, relplan_key/3, relplan_declared_types/3,
%   relplan_reference_target/2, relplan_reference_targets/2
% Called by: compile.pl:62 (rel_cols/4), analyze.pl:41, sweep.pl:44,
%   lower.pl:183 (whole module), print_dl.pl, emit_ts.pl (many),
%   emit_rust.pl, compile/0_storage_projection.pl,
%   compile/test/plunit_tests.pl (rel_record tests at 376, 2574)
% Calls: none outside (pure term accessors)
% Tests: compile/test/plunit_tests.pl (rel_record tests line 376,
%   surface_spelling_in_the_rel_record line 2574)
% V7 class: extract
% Parser coupling: term-shape (rel/5 and rel/4 record terms; col/3)
% Preserved law: rel(Ref, StorageName, Kind, Cols, KeyOrNone) is the one
%   record per relation read by every post-plan phase; rel/4 remains accepted
%   for hand-built and minted plans; Declared vs Inferred both survive
%   (arrival gate reads declared only, SQL storage reads Storage).
% DL7 seam: this is the compiler-side IR seam V7 may revise; keep
%   Ref/Kind/Columns/Key and the declared-vs-inherited origin split, merge
%   rel/4 into rel/5.
```

```prolog
% File: v6/prolog/0_rel_record.pl:135
% Existing comment: comment above lines 131-134 — ref(TypeName) targets,
%   one per distinct occurrence, sorted set variant.
% Signature: relplan_reference_target/2, relplan_reference_targets/2
% Called by: lower.pl, compile.pl materialize_reference_target_rels
% Tests: compile/test/type_relation_ir.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: the set of type names a plan's rels point at via
%   ref(TypeName) storage, deduplicated and sorted.
% DL7 seam: plan rel list -> sorted target-name list.
```

---

## 3. Predicate counts by class

| class | count | predicates |
|---|---|---|
| extract | 1 | walk_body walk (plus its projections: body_conjunction_goals, events_goals, spliced_goal, node_surface, walk_children, walk_arguments, body_relation_atoms, body_reserved_word, body_wrapper_refs, event_relation_atom) — 15 total in 0_body_walk |
| adapt | 24 | expansion fold + phases (4), ast_expand host minting (6), relation_pattern (6), relation_edge_expand (8), seq_expand (6), rel_record accessors (2 explicit blocks cover 15 exported names) |
| oracle | 1 | walk_body + the wrapper-family projection block (pinned by body_walk_characterization golden and the registry-family pin; re-implementable in DL7, contract preserved) |
| drop | 4 | cst_unsupported/1-3 + ast_unsupported_term families tied to DL6 cst/5, single-quote, and tree-node spelling; normalize_cst_* |

Counts group report blocks above; the full per-predicate enumeration of
0_body_walk covers walk_body, walk_node, node_surface, walk_children,
walk_arguments, body_conjunction_goals, events_goals, spliced_goal,
relation_atom_wrapper, event_relation_atom, body_relation_atoms,
body_reserved_word, body_wrapper_refs, wrapper_arity (14).

## 3. Canonical term shapes entering and leaving

Entering the slice (parser/door output):

```
prog(Decls, Rules)          Decls: sh_decl | col_type/3 | keyed/2 | kind/2 | keep/2
Rules: (Head <- Body) | (Head <+ Body) | non-rule term
Body: ','/2 spine of goals; goals include ast/4, cst/4-5, not/1, latest/1,
      pre/1-2, finalize/1, next/1, combine, comparisons, (V := Expr), bare atoms
```

Leaving the slice (post expansion fold):

```
prog(FrozenDecls, Rules)    Decls include minted: col_type(__seq_*), keyed,
                            col_type(__host_*, ...), sh_decl, col_type
Rules: flat rules (Head <- Body) | (Head <+ Body) with
  - seq sugar -> 4 rules + keyed cursor rel
  - ast/4 -> demand/response pair + sh_decl + witness bind
  - head rel-values -> membership atoms appended (latest-wrapped on edge rules)
After expand_relation_values (reference engine, post-expansion):
  relation-shaped terms become obj([Name-obj(...)|...]) objects
```

## 4. Hidden dynamic predicates, flags, cuts, tabling, module state

- No dynamic predicates or tabling in any assigned file.
- Global state: `reset_type_row_memo/0` + `freeze_type_rows/2` memo in
  0_generic_expand, reset per expand_program_run (1_expansion.pl:84) — expansion
  is not reentrant across threads.
- Cuts: `rewrite_rules`/`rewrite_rule`/`normalize_cst_*` use once-cuts to
  commit per rule shape; `run_phase_call` cuts select the context clause (the
  comment at 1_expansion.pl:192-194 states the ast clause would re-run on the
  wrong argument without it); relplan_parts/relplan_storage_name cuts commit
  the rel/5 vs rel/4 clause choice; seq_rule_parts cuts after select/3.
- `op(1150, xfx, <-)` / `<+` / `:=` are re-declared per module — module-local
  operator tables, load-order sensitive.
- `msort` over `expansion_phase/3` means phase order is data; the
  plunit_tests order pin (~5017-5025) is the drift rail.
- Assertion order: none. Module-qualified calls via `run_compile_step/4`
  (compile/0_trace) wrap every phase — a compile-step trace dependency.

## 4b. Smallest self-contained extraction boundary

`0_body_walk.pl` whole-file: pure traversal + projections, one registry
dependency (`body_surface_for_term/6`), no dynamic state, difference-list walk,
pinned by body_walk_characterization + the registry-family pin. It extracts as
a module with only the registry row shape to renegotiate.

The next-smallest is `0_rel_record.pl` (pure term accessors, no use_module of
any sibling) but its callers span lower.pl, emit_ts.pl, emit_rust.pl, print_dl.pl,
sweep.pl, analyze.pl, compile.pl — the module is self-contained while its seam
is not.

## 5. First dependency that forces adaptation instead of extraction

`0_ast_expand.pl:ast_host/9` minting `sh_decl(..., template(Command))` with the
literal `"$DL_EXTRACT_BIN" query --lang ...` shell string and the
`identity|witness` digest concat format (`digest_expr/5`): the shape is a
cross-language contract consumed by the engine and both emitters, so the
predicate cannot move without binding to the DL7 host-decl term, which V7
replaces with owner/name/target/ordinal edges. The mapping dedup table
(`mapping(Language, Query, HostName, OutputNames, ...)`, a threaded
first-occurrence cache through rewrite_goals) is also state-threaded, not a
pure fold.

## 6. Unresolved questions requiring a V7 language ruling

1. Does DL7 keep the `seq` cursor as a visible keyed relation (tick-log
   contract, cursor name `seq_<Head>_<Pos>` minted and collision-refused), or
   does the kernel own ordinals as owner/name/target/ordinal edges?
2. Is the witness/demand digest scheme (`role|host|path:text=...|digest:text=...`)
   an execution-plan field the Rust engine consumes, or compiler-side IR free to
   revise under the "preserve execution-plan fields" assumption?
3. Do relation-value heads keep the appended membership atom as an explicit
   body occurrence (stratification-visible), or does DL7 represent target
   membership as an edge so the append becomes a declarative property?
4. Does the absorb-negation polarity law (double negation reads negative,
   matching analyze and level_eval history) survive into DL7, or does DL7
   want real polarity flipping under nested not/1?
5. Which DL6 sugars survive as DL7 surface forms vs drop to cons-tree macros:
   seq, ast query minting, negated-guard inversion, coalesce, dot chains —
   each is currently an expansion_phase row and its order is behavior.
