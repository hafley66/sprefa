# DL7 relational expression-flow review

Date: 2026-08-30

Scope: plan 94a921d95; V7 at 0da6fb89e. Production code and tests were unchanged.

## Current call and bind path

| Stage | Predicate and line | Current data flow |
| --- | --- | --- |
| Reader | v7/src/0_reader/0_parser.pl:36-80, :82-116 | read_term/7 and read_form_items/8 produce every parenthesized expression as node(NodeId, form(Items)). |
| Unit | v7/src/0_reader/2_embedder.pl:18-32 | dl7_text_unit/5 reads and expands into dl7_unit(..., Forms, ...). |
| Entry | v7/src/2_comptime/2_compiler.pl:24-34, :58-72 | compile_dl7/4 prepends prelude/0_types.dl7; compile_unit/3 runs lower, check, evaluate. |
| Declaration pass | v7/src/2_comptime/0_lowerer.pl:14-45, :47-74 | lower_datalog/4 collects nodes, pending_edge/4, relation/3, origins, and reservations before executable forms. |
| Bind | 0_lowerer.pl:76-102, :371-373 | lower_bind/5 accepts only (: Atom Target), then emits static pending_edge(Owner, Name, Target, Index) and reservation/4. |
| Bind target | 0_lowerer.pl:104-123 | lower_target/4 accepts *, +, atom references, and literals. A form, including (Partial User), is unsupported_bind_target. |
| Relation declaration | 0_lowerer.pl:125-146 | A product becomes relation(Owner, Arity, []). User declarations install no key sets. |
| Executables | 0_lowerer.pl:169-213 | lower_executables/7 skips binds, lowers <- through lower_rule/8, and other forms through lower_seed/6. |
| Calls | 0_lowerer.pl:226-329 | Heads, body goals, and seeds use lower_call_mode/6. It requires an atom operator, product reservation or kernel relation, and full arity. |
| Arguments | 0_lowerer.pl:333-367 | Variables, literals, and atoms become var/1, const/1, and name/2. A form is nested_call_argument. Rule-head (count X) is the sole special case. |
| Resolve and check | v7/src/2_comptime/1_checker.pl:251-285, :396-464, :602-650 | Static pending edges become :/4; calls and arguments resolve before evaluation. Checked rules contain checked_goal(Polarity, call(ref(Relation), Arguments)). |
| Ordering and safety | 1_checker.pl:466-581, :652-679 | Goal modes fold left to right. cons/3 and intern/3 have inputs; head variables need a positive body occurrence. |
| Rounds | 2_compiler.pl:74-105, :135-240, :244-292 | Colon rows and intern requests freeze as edge_snapshot/4 and intern_snapshot/3 for the next round. |
| Generated program | v7/src/2_comptime/1a_generated_program_assembler.pl:14-34, :88-345 | def/head/head_arg/body/body_arg rows become relations and checked rules. Positions are dense; generated variables are var(generated(RuleId, Name)). |

The prelude declares Partial/2 at v7/prelude/0_types.dl7:2-4 and derives it from partial_request/1 at :103-113. The fixture supplies that request at v7/test/fixtures/2_partial.dl7:16-19. The current oracle expects application(Constructor, Arguments) identities at v7/test/1_entrypoints.test.pl:606-657.

## Plan against current machinery

| Subject | Current evidence | Result |
| --- | --- | --- |
| Full reverse query | Full calls require all columns at 0_lowerer.pl:284-313. Partial/2 has intern/3 and intern_snapshot/3 clauses at 0_types.dl7:103-113. Ordinary positive calls are relational through proves/2 at 0_evaluator.pl:427-445. | A full (Partial ?Source KnownResult) can query source from result. Expression lowering must add an omitted-return path without rewriting full calls. |
| Body ordering | lower_goals/7 preserves source order at 0_lowerer.pl:240-255; check_goal_sequence_failures/7 checks order at 1_checker.pl:466-504. The cons/intern ordering test is 1_entrypoints.test.pl:164-196. | Nested goals must be inner-first, then outer call, then surrounding authored goal. |
| Functional keys | IR supports relation(Relation, Arity, KeySets); stable closure validation is 0_evaluator.pl:188-240. User products set [] at 0_lowerer.pl:141-143. Kernel keys are 1_checker.pl:302-317. | The checked declaration can carry key data. There is no authored-key path, mode representation, or supplied-position-to-return proof. |
| Compiler rounds | continue_compiler_rounds/13 freezes rows at 2_compiler.pl:171-190. HistoryV1 emits generated carriers at 0_types.dl7:267-303; their checked rule is asserted at 1_entrypoints.test.pl:700-752. | A lowerer-authored :/4 :- Partial/2 rule can use compiler closure. Its colon row is edge_snapshot/4 input only next round. |
| First-order Datalog and SQL | Checked and generated calls require concrete ref(Relation) at 1_checker.pl:602-623 and 1a_generated_program_assembler.pl:104-133, :225-249. | Complete expressions are fixed-relation goals. Dynamic application cannot survive checked Datalog. Compile-known partial application must erase before checking. |

## Checked-IR boundaries

| Milestone | Fit | Boundary |
| --- | --- | --- |
| 1 carrier | Fits as lowerer-local Value + Goals + Origins. | Consume it before check_datalog/4. |
| 2 return position | Declaration owner edges provide ordered return lookup. | Expression-only validation must precede current full-arity call lowering. |
| 3 RHS bind | The rule shape fits rule/2, checked_goal/2, and rounds. | A later same-unit reference does not fit current resolution: resolve_name/5 reads only declaration-pass edges at 1_checker.pl:269-288, before the derived colon edge exists. A deferred binding identity or post-round resolution boundary is required. |
| 4 nesting | Flat inner-first checked-goal lists fit. | Fresh result variables must be ground identities; generated origins need enclosing goal indices. |
| 5 uniform positions | A head result variable plus producer in body fits head safety. | aggregate(count, Expression) is not an ordinary slot. The plan needs aggregate-specific hoisting and grouping semantics. |
| 8 modes and cardinality | KeySets exists in relation/3. | No source key/mode declaration or checker proof exists. |
| 9 partial application | The final direct call fits. | partial_callable(Partial, Base, BoundArguments) cannot be a current checked value with later dynamic invocation. It must stay prechecked and erase. |
| 10 compound edge label | Generic call arguments can hold ground terms. | Declaration edges cannot: bind_form/4 requires an atom label at 0_lowerer.pl:371-373, and edge_seed/2 emits const(Name) at 2_compiler.pl:308-310. A literal-edge carrier or widened edge representation is required. |

## Smallest milestones 1 through 4

### 1. Carrier

~~~prolog
lower_expression(+Node, +Owner, +Environment,
                 -Value, -Goals, -Origins, -Diagnostics) is det.

lower_expression(node(_, variable(Identity, _)), _, _,
                 var(Identity), [], [], []).
lower_expression(node(_, literal(Value)), _, _,
                 const(Value), [], [], []).
lower_expression(node(_, atom(Name)), Owner, _,
                 name(Owner, Name), [], [], []).
lower_expression(node(NodeId, form(_)), _, _,
                 none, [], [],
                 [diagnostic(lower, NodeId, unresolved_expression_form)]).
% Milestone 1 has no call clause.
~~~

This preserves current value forms at 0_lowerer.pl:359-363 and reader variable identities at 0_parser.pl:187-205.

### 2. Return position

~~~prolog
expression_return_position(+Callable, +Environment,
                           -ReturnIndex, -Diagnostics) is det.
callable_declaration_edges(+Callable, +Environment, -Edges) is det.

expression_return_position(Callable, Environment, ReturnIndex, Diagnostics) :-
    callable_declaration_edges(Callable, Environment, Edges),
    findall(Index, member(pending_edge(Callable, return, _, Index), Edges),
            Indices),
    ( Indices = [ReturnIndex] -> Diagnostics = []
    ; Indices = [] -> Diagnostics = [diagnostic(lower, Callable,
                              expression_without_return)]
    ; Diagnostics = [diagnostic(lower, Callable,
                              expression_multiple_returns(Indices))]
    ).
~~~

Only expression lowering calls this predicate. Full lower_call_mode/6 remains unchanged. The diagnostic terms and declaration-source location remain open.

### 3. RHS complete call

~~~prolog
lower_expression_call(+NodeId, +Name, +ArgumentNodes, +Owner, +Environment,
                      -Value, -Goals, -Origins, -Diagnostics) is det.
lower_expression_arguments(+ArgumentNodes, +Owner, +Environment,
                           -Values, -Goals, -Origins, -Diagnostics) is det.
lower_expression_bind(+BindNode, +Owner, +Index, +Environment,
                      -Reservation, -Rule, -Origins, -Diagnostics) is det.

lower_expression_call(NodeId, Name, ArgumentNodes, Owner, Environment,
                      Result, Goals, Origins, Diagnostics) :-
    resolve_callable_declaration(Name, Owner, Environment, Callable, Arity,
                                 CallableDiagnostics),
    lower_expression_arguments(ArgumentNodes, Owner, Environment,
                               Arguments, ArgumentGoals, ArgumentOrigins,
                               ArgumentDiagnostics),
    expression_return_position(Callable, Environment, ReturnIndex,
                               ReturnDiagnostics),
    insert_fresh_at(ReturnIndex, NodeId, Arguments, Result, FullArguments),
    length(FullArguments, Arity),
    append(ArgumentGoals,
           [pending_goal(positive, call(name(Owner, Name), FullArguments))],
           Goals),
    Origins = [origin(expression(NodeId), NodeId) | ArgumentOrigins],
    append([CallableDiagnostics, ArgumentDiagnostics, ReturnDiagnostics],
           Diagnostics).

lower_expression_bind(BindNode, Owner, Index, Environment,
                      Reservation, Rule, Origins, Diagnostics) :-
    bind_form(BindNode, BindNodeId, Name, Rhs),
    lower_expression(Rhs, Owner, Environment,
                     Value, Goals, ExpressionOrigins, Diagnostics),
    Reservation = deferred_reservation(Owner, Name, Value),
    Rule = rule(call(name(Owner, ':'),
                     [ref(Owner), const(Name), Value, const(Index)]), Goals),
    Origins = [origin(rule_expression_bind(BindNodeId), BindNodeId)
              | ExpressionOrigins].
~~~

insert_fresh_at/5 inserts one var(expression(NodeId)) at ReturnIndex, requires supplied arity Arity - 1, and reports an arity diagnostic. The deferred reservation marks the resolver gap. Existing reservation/4 cannot represent it without a resolver case.

### 4. Nested complete applications

~~~prolog
lower_expression_arguments(+ArgumentNodes, +Owner, +Environment,
                           -Values, -Goals, -Origins, -Diagnostics) is det.

lower_expression_arguments([], _, _, [], [], [], []).
lower_expression_arguments([Node | Nodes], Owner, Environment,
                           [Value | Values], Goals, Origins, Diagnostics) :-
    lower_expression(Node, Owner, Environment,
                     Value, OwnGoals, OwnOrigins, OwnDiagnostics),
    lower_expression_arguments(Nodes, Owner, Environment,
                               Values, RestGoals, RestOrigins, RestDiagnostics),
    append(OwnGoals, RestGoals, Goals),
    append(OwnOrigins, RestOrigins, Origins),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).
~~~

For (Option (Partial User)), this produces Partial(User, PartialUser), then Option(PartialUser, MaybePatch).

## Count and generated-program collisions

- lower_argument/4 recognizes (count Expression) only in a rule head at 0_lowerer.pl:345-354; body use is rejected at :355-357; at most one aggregate is accepted at :319-331.
- Aggregate heads retain aggregate(count, Value) and evaluate proofs from a completed lower stratum at 0_evaluator.pl:114-186. The consolidated test expects a non-count nested head form to fail as nested_call_argument at 1_entrypoints.test.pl:405-494.
- Hoisting an expression under count adds a body proof. The plan leaves open whether duplicate expression proofs contribute separate count entries, so milestone 5 needs that aggregate decision.
- HistoryV1 generated carrier rows use its result as rule and relation identity. The assembler rejects generated relation collisions at 1a_generated_program_assembler.pl:75-86 and requires dense carrier positions at :190-345.
- A lowerer-authored bind rule must not also emit def/head/head_arg/body/body_arg rows. It already enters the authored-rule route.
- partial_request is both prelude dependency and fixture construction root. Its removal alters the source of Partial/2 facts; generated-program assembly does not replace it.

## Open choices

1. Reader spelling for atoms, strings, numbers, comments, and quoted names.
2. Scope-creating forms and recursive binding-group forms.
3. Unsaturated application behavior: automatic partial value or explicit callable operation.
4. Whether named and ordinal arguments may mix in one call.
5. Syntax or metadata declaring a relation compiler-owned.
6. First V7 plan schema and ProgramJson mapping.
7. Source declaration and checked-metadata route for functional keys, modes, and selected expression mode.
8. Deferred-bind identity and name-resolution timeline for compiler-derived RHS results.
9. Canonical diagnostics and source locations for return-label and expression-mode failures.
10. Fresh-variable identity namespace and generated-goal origin indexing.
11. Pre-erasure partial-value encoding and compile-known expansion point.
12. Key surface shape, typed options, compound-label representation, and whether options participate in edge identity.
13. Aggregate semantics for a nested relation expression used as a count payload.

No syntax choice is made in this review.

