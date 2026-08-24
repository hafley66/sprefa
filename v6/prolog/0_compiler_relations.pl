% Compiler-plane relations are Datalog relations over semantic type IDs.  They
% are evaluated during expansion and never reach the runtime planner.
:- module(compiler_relations,
          [ partition_compiler_relations/3,
            partition_compiler_program/5,
            evaluate_compiler_relations/3,
            compiler_type_apply_requests/3
          ]).

:- use_module(library(lists)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

%! partition_compiler_relations(+Decls, -CompilerDecls, -RuntimeDecls) is det.
%
% A relation is compiler-plane when a declared column has the `type` value
% domain.  Every column in that relation must then have that domain.  The
% returned compiler declaration is deliberately small: runtime-only modifiers
% have no compiler meaning except `keyed/2`, whose positions state functional
% compiler outputs.
partition_compiler_relations(Decls, compiler_relations(Relations, []), RuntimeDecls) :-
    declared_relation_refs(Decls, Refs),
    maplist(classify_relation(Decls), Refs, Classifications),
    include(compiler_classification, Classifications, CompilerClasses),
    pairs_values(CompilerClasses, Relations),
    pairs_keys(CompilerClasses, CompilerRefs),
    exclude(compiler_runtime_decl(CompilerRefs), Decls, RuntimeDecls).

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

% The original type-only relations remain compiler relations.  An arrow
% result `return: type` additionally admits ordinary compile-time inputs such
% as `Value: int`; annotation signature validation owns their meaning.
compiler_relation_columns(_, Columns) :-
    memberchk(return-type, Columns),
    !.
compiler_relation_columns(_, Columns) :-
    member(_-type, Columns),
    forall(member(_-Type, Columns), Type == type).
compiler_relation_columns(Ref, Columns) :-
    member(_-type, Columns),
    throw(unsupported_construct(compiler_relation_mixed_domain(Ref))).

compiler_classification(_-compiler_relation(_, _, _)).

compiler_runtime_decl(CompilerRefs, Decl) :-
    declaration_ref(Decl, Ref),
    memberchk(Ref, CompilerRefs).

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
    findall(compiler_relation(Ref, Arity, []),
            ( compiler_builtin_ref(Ref),
              Ref = _/Arity,
              compiler_builtin_is_used(Decls, Rules, Ref) ),
            Relations).

compiler_builtin_is_used(_, Rules, Ref) :- rule_contains_ref(Rules, Ref), !.
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

compiler_builtin_ref(type_decl/4).
compiler_builtin_ref(type_member/5).
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
    ( named_negation_compiler_ref(Body, CompilerRefs, Ref)
    -> throw(unsupported_construct(compiler_relation_negation_unsupported(Ref)))
    ; true
    ),
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
    body_atoms(Body, Atoms),
    rule_head(Rule, Head),
    term_variables(Head, HeadVariables),
    term_variables(Atoms, BodyVariables),
    rule_head_ref(Rule, HeadRef),
    forall(member(Variable, HeadVariables),
           ( member_variable(Variable, BodyVariables)
           -> true
           ; throw(unsupported_construct(compiler_relation_unsafe_rule(HeadRef)))
           )).

member_variable(Variable, [Candidate | _]) :- Variable == Candidate, !.
member_variable(Variable, [_ | Rest]) :- member_variable(Variable, Rest).

named_negation_ref(not(Atom), Ref) :- !,
    ( atom_ref(Atom, Ref) -> true ; Ref = not ).
named_negation_ref(Body, Ref) :-
    nonvar(Body), Body = (Left, Right),
    ( named_negation_ref(Left, Ref) ; named_negation_ref(Right, Ref) ).

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

body_atoms(true, []) :- !.
body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms),
    body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(Atom, [Atom]).

%! evaluate_compiler_relations(+CompilerDecls, +SeedRows, -ClosureRows) is det.
%
% Positive safe rules use ordinary Datalog joins.  Every round is sorted before
% comparison, yielding deterministic set semantics independently of rule or
% fact source order.  Functional keys are checked after the complete closure.
evaluate_compiler_relations(compiler_relations(Relations, Rules), SeedRows,
                            ClosureRows) :-
    maplist(validate_compiler_seed(Relations), SeedRows),
    maplist(validate_compiler_rule_plane_with_relations(Relations), Rules),
    validate_type_apply_recursive_construction(Rules),
    sort(SeedRows, SeedSet),
    compiler_fixpoint(Rules, SeedSet, Closure0),
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

compiler_fixpoint(Rules, Rows0, Rows) :-
    findall(Row,
            ( member(Rule, Rules),
              derive_compiler_row(Rows0, Rule, Row) ),
            Derived),
    append(Rows0, Derived, Next0),
    sort(Next0, Next),
    ( Next == Rows0 -> Rows = Rows0 ; compiler_fixpoint(Rules, Next, Rows) ).

derive_compiler_row(Rows, Rule0, Row) :-
    copy_term(Rule0, Rule),
    rule_head(Rule, Head),
    rule_body(Rule, Body),
    satisfy_compiler_body(Rows, Body),
    ground(Head),
    Row = Head.

satisfy_compiler_body(_, true) :- !.
satisfy_compiler_body(Rows, (Left, Right)) :- !,
    satisfy_compiler_body(Rows, Left),
    satisfy_compiler_body(Rows, Right).
satisfy_compiler_body(_, type_apply(Constructor, Arguments, Application)) :-
    !,
    ( ground(Constructor), is_list(Arguments), maplist(ground, Arguments)
    -> Application = application(Constructor, Arguments)
    ; throw(unsupported_construct(type_apply_non_ground_application(
                type_apply(Constructor, Arguments, Application))))
    ).
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
