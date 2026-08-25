% Compiler-plane relations are Datalog relations over semantic type IDs.  They
% are evaluated during expansion and never reach the runtime planner.
:- module(compiler_relations,
          [ partition_compiler_relations/3,
            partition_compiler_program/5,
            evaluate_compiler_relations/3,
            compiler_type_apply_requests/3,
            compiler_builtin_path_decls/1
          ]).

:- use_module(library(lists)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).
:- use_module(library(gensym)).
:- use_module('conformance/body',
              [ eval_expr/2, comparison_goal/1, solve_comparison/1 ]).
:- use_module('compile/registry',
              [ body_surface_for_term/6, surface_for_term/6 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700, xfx, :=).

:- table compiler_proves/2.
:- thread_local compiler_eval_seed/2.
:- thread_local compiler_eval_rule/2.
:- thread_local compiler_eval_lower/2.

%! partition_compiler_relations(+Decls, -CompilerDecls, -RuntimeDecls) is det.
%
% A relation is compiler-plane when a declared column has the `type` value
% domain.  Its remaining columns are compile-time values, including scalar and
% declared enum domains.  The returned compiler declaration is deliberately
% small: runtime-only modifiers have no compiler meaning except `keyed/2`,
% whose positions state functional compiler outputs.
partition_compiler_relations(Decls, compiler_relations(Relations, []), RuntimeDecls) :-
    declared_relation_refs(Decls, Refs),
    maplist(classify_relation(Decls), Refs, Classifications),
    include(compiler_classification, Classifications, CompilerClasses),
    pairs_values(CompilerClasses, Relations),
    pairs_keys(CompilerClasses, CompilerRefs),
    compiler_only_enum_domains(Decls, CompilerRefs, CompilerEnumDomains),
    exclude(compiler_runtime_decl(CompilerRefs, CompilerEnumDomains), Decls,
            RuntimeDecls).

declared_relation_refs(Decls, Refs) :-
    findall(Ref, member(col_type(Ref, _, _), Decls), Refs0),
    sort(Refs0, Refs).

classify_relation(Decls, Ref, Ref-compiler_relation(Ref, Arity, Keys)) :-
    Ref = _/Arity,
    findall(Column-Type, member(col_type(Ref, Column, Type), Decls), Columns),
    compiler_relation_columns(Ref, Columns),
    !,
    ( memberchk(keyed(Ref, Keys0), Decls) -> Keys = Keys0 ; Keys = [] ).
classify_relation(_, Ref, Ref-runtime).

% A `type` column marks the phase boundary. Other columns are typed and
% elaborated by the compiler plane before the relation is erased.
compiler_relation_columns(_, Columns) :-
    member(_-type, Columns).

compiler_classification(_-compiler_relation(_, _, _)).

compiler_runtime_decl(CompilerRefs, _, Decl) :-
    declaration_ref(Decl, Ref),
    memberchk(Ref, CompilerRefs).
compiler_runtime_decl(_, CompilerEnumDomains, enum_decl(Name, _)) :-
    memberchk(Name, CompilerEnumDomains).

% An enum used only as a compiler-relation value domain disappears with those
% relations. Its frozen semantic rows remain available to compiler metadata,
% while enum expansion never creates runtime variant tables for it.
compiler_only_enum_domains(Decls, CompilerRefs, Domains) :-
    findall(Name,
            ( member(enum_decl(Name, _), Decls),
              compiler_domain_reachable(Decls, CompilerRefs, Name),
              \+ runtime_domain_reachable(Decls, CompilerRefs, Name) ),
            Domains0),
    sort(Domains0, Domains).

compiler_domain_reachable(Decls, CompilerRefs, Name) :-
    member(col_type(Ref, _, Type), Decls),
    memberchk(Ref, CompilerRefs),
    compiler_domain_reaches_enum(Decls, Type, Name, []).

runtime_domain_reachable(Decls, CompilerRefs, Name) :-
    member(col_type(Ref, _, Type), Decls),
    \+ memberchk(Ref, CompilerRefs),
    compiler_domain_reaches_enum(Decls, Type, Name, []).

compiler_domain_reaches_enum(_, Type, Name, _) :-
    atom(Type),
    Type == Name,
    !.
compiler_domain_reaches_enum(Decls, Type, Name, Seen) :-
    atom(Type),
    \+ memberchk(Type, Seen),
    member(enum_decl(Type, Variants), Decls),
    compiler_enum_field_type(Variants, FieldType),
    compiler_domain_reaches_enum(Decls, FieldType, Name, [Type | Seen]).
compiler_domain_reaches_enum(Decls, Type, Name, Seen) :-
    compound(Type),
    Type =.. [_ | Arguments],
    member(Argument, Arguments),
    compiler_domain_reaches_enum(Decls, Argument, Name, Seen).

compiler_enum_field_type((Left ; Right), Type) :-
    !,
    ( compiler_enum_field_type(Left, Type)
    ; compiler_enum_field_type(Right, Type) ).
compiler_enum_field_type(Variant, Type) :-
    compound(Variant),
    Variant =.. [_ | Fields],
    member(_Name:Type, Fields).

declaration_ref(col_type(Ref, _, _), Ref).
declaration_ref(kind(Ref, _), Ref).
declaration_ref(keyed(Ref, _), Ref).
declaration_ref(keep(Ref, _), Ref).
declaration_ref(rel_path_decl(Ref, _), Ref).
declaration_ref(return_alias(Ref, _), Ref).

%! partition_compiler_program(+Decls, +Rules, -CompilerDecls,
%!                            -RuntimeDecls, -RuntimeRules) is det.
%
% Facts and rules headed by compiler relations become evaluator input.  A rule
% may only use compiler relations in this first slice; crossing the phase
% boundary in either direction has no runtime representation.
partition_compiler_program(Decls, Rules, compiler_relations(Relations, CompilerRules),
                           RuntimeDecls, RuntimeRules) :-
    partition_compiler_relations(Decls,
                                 compiler_relations(DeclaredRelations, _),
                                 RuntimeDecls),
    compiler_builtin_relations(Decls, Rules, BuiltinRelations),
    compiler_builtin_declaration_collisions(Decls, BuiltinRelations),
    append(DeclaredRelations, BuiltinRelations, Relations),
    relation_refs(Relations, CompilerRefs),
    partition_rules(Rules, CompilerRefs, CompilerRules, RuntimeRules).

compiler_builtin_relations(Decls, Rules, Relations) :-
    findall(compiler_relation(Ref, Arity, Keys),
            ( compiler_builtin_ref(Ref),
              Ref = _/Arity,
              compiler_builtin_is_used(Decls, Rules, Ref),
              compiler_builtin_keys(Ref, Keys) ),
            Relations).

compiler_builtin_keys(type__node/3, [1]) :- !.
compiler_builtin_keys(type__edge/6, [1]) :- !.
compiler_builtin_keys(type__named/4, [1]) :- !.
compiler_builtin_keys(type_member/6, [1, 3]) :- !.
compiler_builtin_keys(type__project/3, [1, 2]) :- !.
compiler_builtin_keys(_, []).

compiler_builtin_path_decls(Decls) :-
    findall(rel_path_decl(Ref, Path), compiler_builtin_path(Ref, Path), Decls).

compiler_builtin_path(type__node/3, [type, node]).
compiler_builtin_path(type__edge/6, [type, edge]).
compiler_builtin_path(type__path/2, [type, path]).
compiler_builtin_path(type__project/3, [type, project]).
compiler_builtin_path(type_decl/4, [type, declaration]).
compiler_builtin_path(type_member/5, [type, member]).
compiler_builtin_path(type_member/6, [type, member]).
compiler_builtin_path(type_member_role/3, [type, member_role]).
compiler_builtin_path(type_application/2, [type, application]).
compiler_builtin_path(type_argument/4, [type, argument]).
compiler_builtin_path(type_application_site/4, [type, application_site]).
compiler_builtin_path(type_apply/3, [type, apply]).
compiler_builtin_path(type_requested/3, [type, requested]).
compiler_builtin_path(type_field/5, [type, field]).
compiler_builtin_path(type_field_count/2, [type, field_count]).

compiler_builtin_is_used(_, Rules, Ref) :- rule_contains_ref(Rules, Ref), !.
compiler_builtin_is_used(Decls, Rules, type__node/3) :-
    compiler_rules_contain_functor(Decls, Rules, primitive, 1),
    !.
compiler_builtin_is_used(Decls, Rules, type__named/4) :-
    compiler_rules_contain_functor(Decls, Rules, named, 3),
    !.
compiler_builtin_is_used(Decls, Rules, type_requested/3) :-
    compiler_rules_contain_functor(Decls, Rules, application, 2),
    !.
compiler_builtin_is_used(Decls, Rules, type__edge/6) :-
    ( compiler_rules_contain_functor(Decls, Rules, member, 3)
    ; compiler_rules_contain_functor(Decls, Rules, variant, 3) ),
    !.
compiler_builtin_is_used(Decls, Rules, type_apply/3) :-
    member(Rule, Rules),
    rule_head(Rule, Head),
    atom_ref(Head, Ref),
    findall(Type, member(col_type(Ref, _, Type), Decls), Types),
    Head =.. [_ | Arguments],
    nth1(Position, Types, type),
    nth1(Position, Arguments, Argument),
    compound(Argument),
    !.

compiler_rules_contain_functor(Decls, Rules, Name, Arity) :-
    member(Rule, Rules),
    compiler_pattern_rule(Decls, Rule),
    sub_term(Term, Rule),
    compound(Term),
    functor(Term, Name, Arity),
    !.

compiler_pattern_rule(Decls, Rule) :-
    rule_head_ref(Rule, Ref),
    ( compiler_request_ref(Ref)
    ; memberchk(col_type(Ref, _, type), Decls)
    ).

compiler_builtin_ref(type_decl/4).
compiler_builtin_ref(type_member/5).
compiler_builtin_ref(type_member/6).
compiler_builtin_ref(type_member_role/3).
compiler_builtin_ref(type_application/2).
compiler_builtin_ref(type_argument/4).
compiler_builtin_ref(type_application_site/4).
compiler_builtin_ref(type_apply/3).
compiler_builtin_ref(type_requested/3).
compiler_builtin_ref(type_field/5).
compiler_builtin_ref(type_field_count/2).
compiler_builtin_ref(derived_relation_request/4).
compiler_builtin_ref(derived_member_request/4).
compiler_builtin_ref(derived_member_role_request/4).
compiler_builtin_ref(type__node/3).
compiler_builtin_ref(type__edge/6).
compiler_builtin_ref(type__named/4).
compiler_builtin_ref(type__path/2).
compiler_builtin_ref(type__project/3).

compiler_request_ref(derived_relation_request/4).
compiler_request_ref(derived_member_request/4).
compiler_request_ref(derived_member_role_request/4).

compiler_builtin_declaration_collisions(_, []) :- !.
compiler_builtin_declaration_collisions(Decls,
                                        [compiler_relation(Ref, _, _) | Rest]) :-
    ( member(col_type(Ref, _, _), Decls)
    -> throw(unsupported_construct(compiler_relation_builtin_collision(Ref)))
    ; true
    ),
    compiler_builtin_declaration_collisions(Decls, Rest).

rule_contains_ref(Rules, Ref) :-
    member(Rule, Rules),
    rule_contains_ref_term(Rule, Ref),
    !.

rule_contains_ref_term(Term, Ref) :-
    nonvar(Term),
    ( atom_ref(Term, Ref)
    ; compound(Term),
      compound_name_arguments(Term, _, Arguments),
      member(Argument, Arguments),
      rule_contains_ref_term(Argument, Ref) ).

relation_refs([], []).
relation_refs([compiler_relation(Ref, _, _) | Rest], [Ref | Refs]) :-
    relation_refs(Rest, Refs).

partition_rules([], _, [], []).
partition_rules([Rule | Rest], CompilerRefs, CompilerRules, RuntimeRules) :-
    rule_head_ref(Rule, HeadRef),
    ( compiler_request_ref(HeadRef)
    -> validate_compiler_rule_refs(Rule, CompilerRefs),
       CompilerRules = [Rule | MoreCompiler],
       partition_rules(Rest, CompilerRefs, MoreCompiler, RuntimeRules)
    ; compiler_builtin_ref(HeadRef)
    -> throw(unsupported_construct(compiler_relation_builtin_head(HeadRef)))
    ; memberchk(HeadRef, CompilerRefs)
    -> validate_compiler_rule_refs(Rule, CompilerRefs),
       CompilerRules = [Rule | MoreCompiler],
       partition_rules(Rest, CompilerRefs, MoreCompiler, RuntimeRules)
    ; named_negation_compiler_ref(Rule, CompilerRefs, CompilerRef)
    -> throw(unsupported_construct(compiler_relation_negation_unsupported(CompilerRef)))
    ; rule_contains_compiler_ref(Rule, CompilerRefs, CompilerRef)
    -> throw(unsupported_construct(compiler_relation_mixed_domain(CompilerRef)))
    ; RuntimeRules = [Rule | MoreRuntime],
      partition_rules(Rest, CompilerRefs, CompilerRules, MoreRuntime)
    ).

rule_head_ref((Head <- _), Ref) :- !, atom_ref(Head, Ref).
rule_head_ref((Head <+ _), Ref) :- !, atom_ref(Head, Ref).
rule_head_ref(Head, Ref) :- atom_ref(Head, Ref).

atom_ref(Atom, Name/Arity) :-
    compound(Atom),
    functor(Atom, Name, Arity).
atom_ref(Name, Name/0) :-
    atom(Name).

rule_contains_compiler_ref(Rule, CompilerRefs, Ref) :-
    rule_body(Rule, Body),
    body_compiler_ref(Body, CompilerRefs, Ref).

body_compiler_ref(Body, CompilerRefs, Ref) :-
    nonvar(Body),
    ( atom_ref(Body, Ref), memberchk(Ref, CompilerRefs)
    ; compound(Body),
      compound_name_arguments(Body, _, Arguments),
      member(Argument, Arguments),
      body_compiler_ref(Argument, CompilerRefs, Ref)
    ),
    !.

validate_compiler_rule_refs(Rule, CompilerRefs) :-
    rule_body(Rule, Body),
    body_atoms(Body, Atoms),
    forall(member(Atom, Atoms),
           ( atom_ref(Atom, Ref),
             ( memberchk(Ref, CompilerRefs)
             -> true
             ; throw(unsupported_construct(compiler_relation_mixed_domain(Ref)))
             ) )),
    true.

validate_compiler_rule_plane(Rule, CompilerRefs) :-
    validate_compiler_rule_refs(Rule, CompilerRefs),
    rule_body(Rule, Body),
    compiler_body_goals(Body, Goals),
    validate_compiler_goal_sequence(Goals, [], BoundVariables),
    rule_head(Rule, Head),
    term_variables(Head, HeadVariables),
    rule_head_ref(Rule, HeadRef),
    forall(member(Variable, HeadVariables),
           ( member_variable(Variable, BoundVariables)
           -> true
           ; throw(unsupported_construct(compiler_relation_unsafe_rule(HeadRef)))
           )).

named_negation_compiler_ref(Term, CompilerRefs, Ref) :-
    nonvar(Term),
    ( Term = not(Body),
      body_compiler_ref(Body, CompilerRefs, Ref)
    ; compound(Term),
      compound_name_arguments(Term, _, Arguments),
      member(Argument, Arguments),
      named_negation_compiler_ref(Argument, CompilerRefs, Ref)
    ),
    !.

rule_head((Head <- _), Head) :- !.
rule_head((Head <+ _), Head) :- !.
rule_head(Head, Head).

rule_body((_ <- Body), Body) :- !.
rule_body((_ <+ Body), Body) :- !.
rule_body(_, true).

:- include('0_compiler_relations/0_goals.pl').

:- include('0_compiler_relations/1_aggregates.pl').

%! evaluate_compiler_relations(+CompilerDecls, +SeedRows, -ClosureRows) is det.
%
% Positive safe rules use ordinary Datalog joins. Scalar goals execute in body
% order. Aggregate heads and negated relation goals read completed lower
% strata before their consumers enter another tabled positive closure. Every
% row set is sorted before use.
evaluate_compiler_relations(compiler_relations(Relations, Rules), SeedRows,
                            ClosureRows) :-
    maplist(validate_compiler_seed(Relations), SeedRows),
    maplist(validate_compiler_rule_plane_with_relations(Relations), Rules),
    validate_type_apply_recursive_construction(Rules),
    validate_compiler_aggregate_heads(Rules),
    sort(SeedRows, SeedSet),
    evaluate_compiler_strata(Rules, SeedSet, Closure0),
    validate_functional_rows(Relations, Closure0),
    ClosureRows = Closure0.

validate_type_apply_recursive_construction(Rules) :-
    member(Rule, Rules),
    rule_head_ref(Rule, Ref),
    rule_body(Rule, Body),
    body_contains_type_apply(Body),
    rule_dependency_path(Rules, Ref, Ref, [Ref]),
    !,
    throw(unsupported_construct(type_apply_recursive_construction([Ref]))).
validate_type_apply_recursive_construction(_).

rule_dependency_path(Rules, From, Target, Seen) :-
    rule_dependency(Rules, From, Next),
    ( Next == Target
    ; \+ memberchk(Next, Seen),
      rule_dependency_path(Rules, Next, Target, [Next | Seen])
    ).

rule_dependency(Rules, HeadRef, BodyRef) :-
    member(Rule, Rules),
    rule_head_ref(Rule, HeadRef),
    rule_body(Rule, Body),
    body_atoms(Body, Atoms),
    member(Atom, Atoms),
    atom_ref(Atom, BodyRef).

compiler_type_apply_requests(Rules, Rows, Requests) :-
    findall(type_apply_request(Application),
            ( member(Rule0, Rules),
              copy_term(Rule0, Rule),
              rule_body(Rule, Body),
              body_contains_type_apply(Body),
              satisfy_compiler_body(Rows, Body),
              body_type_apply_application(Body, Application),
              ground(Application) ),
            Requests0),
    sort(Requests0, Requests).

body_contains_type_apply(type_apply(_, _, _)) :- !.
body_contains_type_apply((Left, Right)) :- !,
    ( body_contains_type_apply(Left) ; body_contains_type_apply(Right) ).

body_type_apply_application(type_apply(_, _, Application), Application) :- !.
body_type_apply_application((Left, _), Application) :-
    body_type_apply_application(Left, Application).
body_type_apply_application((_, Right), Application) :-
    body_type_apply_application(Right, Application).

validate_compiler_seed(Relations, Row) :-
    ( atom_ref(Row, Ref)
    -> relation_refs(Relations, Refs),
       ( memberchk(Ref, Refs)
       -> ( ground(Row)
          -> true
          ; throw(unsupported_construct(compiler_relation_non_ground_seed(Row)))
          )
       ; throw(unsupported_construct(compiler_relation_mixed_domain(Ref)))
       )
    ; throw(unsupported_construct(compiler_relation_invalid_seed(Row)))
    ).

validate_compiler_rule_plane_with_relations(Relations, Rule) :-
    relation_refs(Relations, Refs),
    validate_compiler_rule_plane(Rule, Refs).

%! tabled_compiler_closure(+Rules, +LowerRows, +Seeds, -Rows) is det.
%  One unique table namespace belongs to one compiler round.  The rules and
%  seeds are immutable while SLG evaluation closes recursive positive goals.
%  Negated goals consult only LowerRows, which were completed before this
%  stratum began.
tabled_compiler_closure(Rules, LowerRows, Seeds, Rows) :-
    gensym(compiler_eval_, EvalId),
    setup_call_cleanup(
        install_compiler_eval(EvalId, Rules, LowerRows, Seeds),
        ( findall(Row, compiler_proves(EvalId, Row), Rows0),
          sort(Rows0, Rows) ),
        cleanup_compiler_eval(EvalId)).

install_compiler_eval(EvalId, Rules, LowerRows, Seeds) :-
    forall(member(Rule, Rules), assertz(compiler_eval_rule(EvalId, Rule))),
    forall(member(Row, LowerRows), assertz(compiler_eval_lower(EvalId, Row))),
    forall(member(Seed, Seeds), assertz(compiler_eval_seed(EvalId, Seed))).

cleanup_compiler_eval(EvalId) :-
    abolish_table_subgoals(compiler_proves(EvalId, _)),
    retractall(compiler_eval_rule(EvalId, _)),
    retractall(compiler_eval_lower(EvalId, _)),
    retractall(compiler_eval_seed(EvalId, _)).

compiler_proves(EvalId, Row) :-
    compiler_eval_seed(EvalId, Row).
compiler_proves(EvalId, Row) :-
    compiler_eval_rule(EvalId, Rule0),
    copy_term(Rule0, Rule),
    rule_head(Rule, Head),
    rule_body(Rule, Body),
    satisfy_tabled_compiler_body(EvalId, Body),
    ground(Head),
    Row = Head.

satisfy_tabled_compiler_body(_, true) :- !.
satisfy_tabled_compiler_body(EvalId, (Left, Right)) :- !,
    satisfy_tabled_compiler_body(EvalId, Left),
    satisfy_tabled_compiler_body(EvalId, Right).
satisfy_tabled_compiler_body(EvalId, not(Goal)) :-
    !,
    \+ compiler_eval_lower(EvalId, Goal).
satisfy_tabled_compiler_body(_, type_apply(Constructor, Arguments,
                                            Application)) :-
    !,
    ( ground(Constructor), is_list(Arguments), maplist(ground, Arguments)
    -> Application = application(Constructor, Arguments)
    ;  throw(unsupported_construct(type_apply_non_ground_application(
                                       application(Constructor, Arguments))))
    ).
satisfy_tabled_compiler_body(_, Goal) :-
    compiler_bind_goal(Goal, Variable, Expression),
    !,
    eval_ground_expression(Expression, Value),
    Variable = Value.
satisfy_tabled_compiler_body(_, Goal) :-
    comparison_goal(Goal),
    !,
    holds_ground_comparison(Goal).
satisfy_tabled_compiler_body(EvalId, Goal) :-
    compiler_proves(EvalId, Goal).

satisfy_compiler_body(_, true) :- !.
satisfy_compiler_body(Rows, (Left, Right)) :- !,
    satisfy_compiler_body(Rows, Left),
    satisfy_compiler_body(Rows, Right).
satisfy_compiler_body(Rows, not(Goal)) :-
    !,
    \+ ( member(Row, Rows), Row = Goal ).
satisfy_compiler_body(_, type_apply(Constructor, Arguments, Application)) :-
    !,
    ( ground(Constructor), is_list(Arguments), maplist(ground, Arguments)
    -> Application = application(Constructor, Arguments)
    ; throw(unsupported_construct(type_apply_non_ground_application(
                type_apply(Constructor, Arguments, Application))))
    ).
satisfy_compiler_body(_, Goal) :-
    compiler_bind_goal(Goal, Variable, Expression),
    !,
    eval_ground_expression(Expression, Value),
    Variable = Value.
satisfy_compiler_body(_, Goal) :-
    comparison_goal(Goal),
    !,
    holds_ground_comparison(Goal).
satisfy_compiler_body(Rows, Goal) :- member(Row, Rows), Row = Goal.

validate_functional_rows([], _).
validate_functional_rows([compiler_relation(_, _, []) | Rest], Rows) :- !,
    validate_functional_rows(Rest, Rows).
validate_functional_rows([compiler_relation(Ref, _, Keys) | Rest], Rows) :-
    relation_rows(Ref, Rows, RelationRows),
    validate_functional_relation(Ref, Keys, RelationRows),
    validate_functional_rows(Rest, Rows).

relation_rows(_, [], []).
relation_rows(Ref, [Row | Rest], Rows) :-
    atom_ref(Row, Ref),
    !,
    Rows = [Row | More],
    relation_rows(Ref, Rest, More).
relation_rows(Ref, [_ | Rest], Rows) :- relation_rows(Ref, Rest, Rows).

validate_functional_relation(_, _, []).
validate_functional_relation(Ref, Keys, [Row | Rest]) :-
    key_values(Row, Keys, Values),
    ( member(Other, Rest), key_values(Other, Keys, Values), Other \== Row
    -> throw(unsupported_construct(compiler_relation_functional_conflict(Ref,
                                                                           Values)))
    ; validate_functional_relation(Ref, Keys, Rest)
    ).

key_values(Row, Positions, Values) :-
    Row =.. [_ | Arguments],
    maplist(argument_at(Arguments), Positions, Values).

argument_at(Arguments, Position, Value) :- nth1(Position, Arguments, Value).
