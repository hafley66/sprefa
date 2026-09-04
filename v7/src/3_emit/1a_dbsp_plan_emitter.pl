:- module(dl7_dbsp_plan_emitter,
          [ emit_dbsp_plan/3,
            dbsp_plan_json/2
          ]).

:- use_module(library(http/json), [atom_json_dict/3]).

%% emit_dbsp_plan(+CheckedProgram, -Plan, -Diagnostics) is det.
%
% Lower the source-visible slice of a checked DL7 program to the JSON operator
% contract consumed by v6/dd-runner's pure RAM kernel. Relation names are the
% authored edge labels. A dot in one is ordinary atom content.
emit_dbsp_plan(
    checked_datalog(root_graph(_, Edges),
                    datalog_program(Relations, Seeds, Rules), _, _),
    Plan, Diagnostics) :-
    runtime_identities(Relations, Rules, RuntimeIdentities),
    relation_map(Edges, Relations, RuntimeIdentities,
                 RelationMap, NameDiagnostics),
    rule_operators(Rules, RelationMap, Operators,
                   RuleDiagnostics, HeadRelations),
    seed_rows(Seeds, RelationMap, Initial),
    relation_rows(RelationMap, HeadRelations, RelationRows),
    operator_wires(Operators, Wires),
    append(NameDiagnostics, RuleDiagnostics, Diagnostics0),
    sort(Diagnostics0, Diagnostics),
    Plan = _{ ir_version:1,
              runtime:"dd-runner-kernel-v1",
              ddl:[],
              rels:RelationRows,
              rules:[],
              initial:Initial,
              schedule:[],
              tick_order:[],
              arrangements:[],
              operators:Operators,
              wires:Wires
            }.

dbsp_plan_json(Plan, Json) :-
    atom_json_dict(Json, Plan, [as(string), width(0)]).

runtime_identities(Relations, Rules, Identities) :-
    findall(Identity,
            ( member(relation(ref(Identity), _, _), Relations),
              Identity = owner(file(_), _)
            ),
            Authored0),
    sort(Authored0, Authored),
    dependency_closure(Rules, Authored, Identities).

dependency_closure(Rules, Identities0, Identities) :-
    findall(BodyIdentity,
            ( member(rule(call(ref(HeadIdentity), _), Goals), Rules),
              memberchk(HeadIdentity, Identities0),
              member(checked_goal(_, call(ref(BodyIdentity), _)), Goals)
            ),
            BodyIdentities),
    append(Identities0, BodyIdentities, Next0),
    sort(Next0, Next),
    ( Next == Identities0
    -> Identities = Next
    ;  dependency_closure(Rules, Next, Identities)
    ).

relation_map(Edges, Relations, RuntimeIdentities, Map, Diagnostics) :-
    findall(
        candidate(Label, Identity, Arity, Columns),
        ( member(relation(ref(Identity), Arity, _), Relations),
          memberchk(Identity, RuntimeIdentities),
          source_alias(Edges, Identity, Label),
          relation_columns(Edges, Identity, Arity, Columns)
        ),
        Candidates0),
    sort(Candidates0, Candidates),
    select_identity_names(Candidates, Map0),
    sort(Map0, Map),
    duplicate_name_diagnostics(Map, Diagnostics).

source_alias(Edges, Identity, Label) :-
    setof(Index-Name,
          Path^member(':'(module(file(Path)), Name,
                          ref(Identity), Index), Edges),
          [_-Label | _]).

select_identity_names([], []).
select_identity_names([candidate(Label, Identity, Arity, Columns) | Rest],
                      [relation_name(Identity, Label, Arity, Columns) | Map]) :-
    drop_identity_candidates(Rest, Identity, Remaining),
    select_identity_names(Remaining, Map).

drop_identity_candidates([], _, []).
drop_identity_candidates([candidate(_, Identity, _, _) | Rest], Identity,
                         Remaining) :-
    !,
    drop_identity_candidates(Rest, Identity, Remaining).
drop_identity_candidates([Candidate | Rest], Identity,
                         [Candidate | Remaining]) :-
    drop_identity_candidates(Rest, Identity, Remaining).

duplicate_name_diagnostics(Map, Diagnostics) :-
    findall(
        diagnostic(emit, none,
                   ambiguous_runtime_relation_name(Name, Identities)),
        ( setof(Identity,
                Arity^Columns^member(
                    relation_name(Identity, Name, Arity, Columns), Map),
                Identities),
          Identities = [_, _ | _]
        ),
        Diagnostics).

relation_columns(Edges, Identity, Arity, Columns) :-
    findall(Index-Label,
            member(':'(Identity, Label, _, Index), Edges),
            Indexed0),
    sort(Indexed0, Indexed),
    ( length(Indexed, Arity)
    -> pairs_values(Indexed, Columns)
    ;  numbered_columns(0, Arity, Columns)
    ).

numbered_columns(Index, Arity, []) :-
    Index >= Arity,
    !.
numbered_columns(Index, Arity, [Column | Columns]) :-
    format(atom(Column), 'c~d', [Index]),
    Next is Index + 1,
    numbered_columns(Next, Arity, Columns).

relation_rows([], _, []).
relation_rows([relation_name(_, Name, _, Columns) | Map], Heads,
              [_{name:Name, columns:Columns, select_all:"",
                 input:Input, output:Output} | Rows]) :-
    ( memberchk(Name, Heads) -> Output = true ; Output = false ),
    ( Output == false -> Input = true ; Input = false ),
    relation_rows(Map, Heads, Rows).

seed_rows([], _, []).
seed_rows([call(ref(Identity), Arguments) | Seeds], Map, Rows) :-
    ( relation_info(Map, Identity, Name, _, _),
      Identity \= tsi_relation(_, _)
    -> maplist(argument_json, Arguments, Values),
       Rows = [_{rel:Name, values:Values} | Rest]
    ;  Rows = Rest
    ),
    seed_rows(Seeds, Map, Rest).

rule_operators(Rules, Map, Operators, Diagnostics, Heads) :-
    rule_operators(Rules, Map, 0, Operators, Diagnostics, Heads0),
    sort(Heads0, Heads).

rule_operators([], _, _, [], [], []).
rule_operators([Rule | Rules], Map, Index,
               Operators, Diagnostics, Heads) :-
    Next is Index + 1,
    rule_operator(Rule, Map, Index, Operator, RuleDiagnostics, Head),
    rule_operators(Rules, Map, Next,
                   RestOperators, RestDiagnostics, RestHeads),
    ( Operator == none
    -> Operators = RestOperators
    ;  Operators = [Operator | RestOperators]
    ),
    ( Head == none -> Heads = RestHeads ; Heads = [Head | RestHeads] ),
    append(RuleDiagnostics, RestDiagnostics, Diagnostics).

rule_operator(rule(call(ref(HeadIdentity), HeadArguments), Goals), Map,
              Index, Operator, Diagnostics, HeadName) :-
    ( relation_info(Map, HeadIdentity, HeadName0, _, HeadColumns)
    -> HeadName = HeadName0,
       rule_goal_diagnostics(Goals, Map, Index, Diagnostics),
       ( Diagnostics == []
       -> goal_bindings(Goals, Map, Bindings, Refs,
                        Occurrences, Predicates),
          projection(HeadArguments, HeadColumns, Occurrences, Projection),
          format(atom(Id), 'map_~d', [Index]),
          Operator = _{ id:Id,
                        kind:"map",
                        classification:"level",
                        head:HeadName,
                        refs:Refs,
                        bindings:Bindings,
                        predicates:Predicates,
                        projection:Projection
                      }
       ;  Operator = none
       )
    ;  Operator = none,
       Diagnostics = [],
       HeadName = none
    ).

rule_goal_diagnostics(Goals, Map, Index, Diagnostics) :-
    findall(Diagnostic,
            unsupported_goal(Goals, Map, Index, Diagnostic),
            Diagnostics).

unsupported_goal(Goals, _, Index,
                 diagnostic(emit, none,
                            unsupported_dbsp_negation(rule_id(Index)))) :-
    member(checked_goal(Polarity, _), Goals),
    Polarity \== positive.
unsupported_goal(Goals, Map, Index,
                 diagnostic(emit, none,
                            hidden_runtime_relation(rule_id(Index), Identity))) :-
    member(checked_goal(_, call(ref(Identity), _)), Goals),
    \+ relation_info(Map, Identity, _, _, _).

goal_bindings(Goals, Map, Bindings, Refs, Occurrences, Predicates) :-
    goal_bindings(Goals, Map, 0, [], Occurrences,
                  [], BindingPairs, [], Refs, [], Predicates),
    dict_create(Bindings, bindings, BindingPairs).

goal_bindings([], _, _, Occurrences, Occurrences,
              BindingPairs, BindingPairs, Refs, Refs,
              Predicates, Predicates).
goal_bindings([checked_goal(positive,
                            call(ref(Identity), Arguments)) | Goals],
              Map, Index, Occurrences0, Occurrences,
              BindingPairs0, BindingPairs, Refs0, Refs,
              Predicates0, Predicates) :-
    relation_info(Map, Identity, Name, _, Columns),
    format(atom(Alias), 'b~d', [Index]),
    argument_predicates(Arguments, Columns, Alias,
                        Occurrences0, Occurrences1,
                        Predicates0, Predicates1),
    append(BindingPairs0, [Alias-Name], BindingPairs1),
    append(Refs0, [Name], Refs1),
    Next is Index + 1,
    goal_bindings(Goals, Map, Next, Occurrences1, Occurrences,
                  BindingPairs1, BindingPairs, Refs1, Refs,
                  Predicates1, Predicates).

argument_predicates([], [], _, Occurrences, Occurrences,
                    Predicates, Predicates).
argument_predicates([Argument | Arguments], [Column | Columns], Alias,
                    Occurrences0, Occurrences,
                    Predicates0, Predicates) :-
    column_source(Alias, Column, Source),
    argument_predicate(Argument, Source,
                       Occurrences0, Occurrences1,
                       Predicates0, Predicates1),
    argument_predicates(Arguments, Columns, Alias,
                        Occurrences1, Occurrences,
                        Predicates1, Predicates).

argument_predicate(var(Variable), Source,
                   Occurrences0, Occurrences, Predicates0, Predicates) :-
    !,
    ( memberchk(Variable-First, Occurrences0)
    -> Occurrences = Occurrences0,
       append(Predicates0,
              [_{column_equals:[First, Source]}], Predicates)
    ;  append(Occurrences0, [Variable-Source], Occurrences),
       Predicates = Predicates0
    ).
argument_predicate(Argument, Source,
                   Occurrences, Occurrences, Predicates0, Predicates) :-
    argument_json(Argument, Value),
    append(Predicates0,
           [_{literal_equals:_{column:Source, value:Value}}], Predicates).

projection([], [], _, []).
projection([Argument | Arguments], [Column | Columns], Occurrences,
           [Projection | Projections]) :-
    projection_argument(Argument, Column, Occurrences, Projection),
    projection(Arguments, Columns, Occurrences, Projections).

projection_argument(var(Variable), Column, Occurrences,
                    _{head:Column, source:Source}) :-
    !,
    memberchk(Variable-Source, Occurrences).
projection_argument(Argument, Column, _, _{head:Column, value:Value}) :-
    argument_json(Argument, Value).

operator_wires([], []).
operator_wires([Operator | Operators], Wires) :-
    findall(_{from:Ref, kind:"delta", to:Operator.id},
            member(Ref, Operator.refs),
            InputWires),
    OutputWire = _{from:Operator.id, kind:"delta", to:Operator.head},
    operator_wires(Operators, RestWires),
    append(InputWires, [OutputWire | RestWires], Wires).

relation_info([relation_name(StoredIdentity, Name, Arity, Columns) | _],
              Identity, Name, Arity, Columns) :-
    StoredIdentity == Identity,
    !.
relation_info([_ | Map], Identity, Name, Arity, Columns) :-
    relation_info(Map, Identity, Name, Arity, Columns).

column_source(Alias, Column, Source) :-
    format(atom(Source), '~w.~w', [Alias, Column]).

argument_json(const(Value), Json) :-
    !,
    constant_json(Value, Json).
argument_json(ref(Identity), _{ref:Text}) :-
    !,
    term_string(Identity, Text, [quoted(true), numbervars(true)]).
argument_json(Term, _{term:Text}) :-
    term_string(Term, Text, [quoted(true), numbervars(true)]).

constant_json(Value, Value) :-
    ( integer(Value)
    ; float(Value)
    ; string(Value)
    ),
    !.
constant_json(Value, Text) :-
    atom(Value),
    !,
    atom_string(Value, Text).
constant_json(span(Digest, Start, End),
              _{span:_{digest:DigestText, start:Start, end:End}}) :-
    !,
    atom_string(Digest, DigestText).
constant_json(Values, Json) :-
    is_list(Values),
    !,
    maplist(constant_json, Values, Json).
constant_json(Value, _{term:Text}) :-
    term_string(Value, Text, [quoted(true), numbervars(true)]).
