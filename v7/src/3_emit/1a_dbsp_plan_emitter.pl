:- module(dl7_dbsp_plan_emitter,
          [ emit_dbsp_plan/3,
            dbsp_plan_json/2
          ]).

:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module('0_logical_program_reifier', [logical_program_rows/2]).

%% emit_dbsp_plan(+CheckedProgram, -Plan, -Diagnostics) is det.
%
% Lower the source-visible slice of a checked DL7 program to the JSON operator
% contract consumed by v6/dd-runner's pure RAM kernel. Relation names are the
% authored edge labels. A dot in one is ordinary atom content.
emit_dbsp_plan(
    CheckedProgram,
    Plan, Diagnostics) :-
    CheckedProgram = checked_datalog(
                         root_graph(_, Edges),
                         datalog_program(Relations, _, _), _, _),
    logical_program_rows(CheckedProgram, LogicalRows),
    logical_executable(LogicalRows, Seeds, Rules),
    runtime_identities(Relations, Rules, RuntimeIdentities),
    relation_map(Edges, Relations, RuntimeIdentities,
                 RelationMap, NameDiagnostics),
    rule_operators(Rules, RelationMap, Operators,
                   RuleDiagnostics, HeadRelations),
    seed_rows(Seeds, RelationMap, Initial),
    relation_rows(RelationMap, HeadRelations, Edges, RelationRows),
    sqlite_plan(Edges, RelationMap, Operators,
                Ddl, SqlRules, SqlDiagnostics),
    operator_wires(Operators, Wires),
    append([NameDiagnostics, RuleDiagnostics, SqlDiagnostics], Diagnostics0),
    sort(Diagnostics0, Diagnostics),
    Plan = _{ ir_version:1,
              runtime:"dd-runner-kernel-v1",
              ddl:Ddl,
              rels:RelationRows,
              rules:SqlRules,
              initial:Initial,
              schedule:[],
              tick_order:[ "absorb_arrivals", "index_delta",
                           "level_before_edges", "edge_arrivals",
                           "edge_departures", "level_after_edges",
                           "iterate", "consolidate", "retain",
                           "boundary", "carry", "drain"
                         ],
              arrangements:[],
              operators:Operators,
              wires:Wires
            }.

%% logical_executable(+ProgramRows, -Seeds, -Rules) is det.
%
% The DBSP lowering consumes only the public reified program graph. This
% reconstruction is temporary host rendering machinery; operator derivation
% can move into DL7 without changing the rows or the runtime contract.
logical_executable(ProgramRows, Seeds, Rules) :-
    findall(Index-Call,
            ( member(program_seed(seed_id(Index), CallId), ProgramRows),
              logical_call(ProgramRows, CallId, Call)
            ),
            IndexedSeeds0),
    keysort(IndexedSeeds0, IndexedSeeds),
    pairs_values(IndexedSeeds, Seeds),
    findall(Index-Rule,
            ( member(program_rule(rule_id(Index), HeadCallId), ProgramRows),
              logical_call(ProgramRows, HeadCallId, Head),
              logical_goals(ProgramRows, rule_id(Index), Goals),
              Rule = rule(Head, Goals)
            ),
            IndexedRules0),
    keysort(IndexedRules0, IndexedRules),
    pairs_values(IndexedRules, Rules).

logical_goals(ProgramRows, Rule, Goals) :-
    findall(Position-checked_goal(Polarity, Call),
            ( member(program_goal(Rule, Position, Polarity, CallId),
                     ProgramRows),
              logical_call(ProgramRows, CallId, Call)
            ),
            Indexed0),
    keysort(Indexed0, Indexed),
    pairs_values(Indexed, Goals).

logical_call(ProgramRows, CallId, call(ref(Relation), Arguments)) :-
    memberchk(program_apply(CallId, Relation), ProgramRows),
    findall(Position-Argument,
            ( member(program_argument(CallId, Position, ArgumentId),
                     ProgramRows),
              logical_argument(ProgramRows, ArgumentId, Argument)
            ),
            Indexed0),
    keysort(Indexed0, Indexed),
    pairs_values(Indexed, Arguments).

logical_argument(ProgramRows, ArgumentId, var(Variable)) :-
    memberchk(program_edge(ArgumentId, variable, const(Variable), 0),
              ProgramRows),
    !.
logical_argument(ProgramRows, ArgumentId, ref(Reference)) :-
    memberchk(program_edge(ArgumentId, reference, ref(Reference), 0),
              ProgramRows),
    !.
logical_argument(ProgramRows, ArgumentId, const(Value)) :-
    memberchk(program_edge(ArgumentId, literal, const(Value), 0),
              ProgramRows),
    !.
logical_argument(ProgramRows, ArgumentId, aggregate(Operator, Input)) :-
    memberchk(program_edge(ArgumentId, aggregate, const(Operator), 0),
              ProgramRows),
    memberchk(program_edge(ArgumentId, input, ref(InputId), 1), ProgramRows),
    logical_argument(ProgramRows, InputId, Input).

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

relation_rows([], _, _, []).
relation_rows([relation_name(Identity, Name, _, Columns) | Map], Heads, Edges,
              [_{name:Name, columns:Columns, select_all:SelectAll,
                 input:Input, output:Output} | Rows]) :-
    relation_select_sql(Edges, Identity, Name, Columns, SelectAll),
    ( memberchk(Name, Heads) -> Output = true ; Output = false ),
    ( Output == false -> Input = true ; Input = false ),
    relation_rows(Map, Heads, Edges, Rows).

sqlite_plan(Edges, RelationMap, Operators, Ddl, Rules, Diagnostics) :-
    findall(Diagnostic,
            sqlite_operator_diagnostic(Operators, Diagnostic),
            Diagnostics0),
    sort(Diagnostics0, Diagnostics),
    ( Diagnostics == []
    -> sqlite_ddl(Edges, RelationMap, Ddl),
       sqlite_rules(Operators, Rules)
    ;  Ddl = [],
       Rules = []
    ).

sqlite_operator_diagnostic(Operators,
                           diagnostic(emit, none,
                                      unsupported_sqlite_literal(Value))) :-
    member(Operator, Operators),
    operator_literal(Operator, Value),
    \+ number(Value).

operator_literal(Operator, Value) :-
    member(Predicate, Operator.predicates),
    get_dict(literal_equals, Predicate, Literal),
    Value = Literal.value.
operator_literal(Operator, Value) :-
    member(Projection, Operator.projection),
    get_dict(value, Projection, Value).

sqlite_ddl(Edges, RelationMap, Ddl) :-
    findall(Statement,
            ( member(Relation, RelationMap),
              relation_ddl(Edges, Relation, Statement)
            ),
            RelationDdl),
    Ddl = ["CREATE TABLE IF NOT EXISTS \"__str\" (\"__id\" INTEGER PRIMARY KEY, \"content\" TEXT NOT NULL UNIQUE)"
           | RelationDdl].

relation_ddl(Edges, relation_name(Identity, Name, Arity, Columns), Statement) :-
    Arity > 0,
    sqlite_identifier(Name, Table),
    findall(ColumnSql,
            ( nth0(Position, Columns, Column),
              sqlite_identifier(Column, QuotedColumn),
              relation_column_kind(Edges, Identity, Position, Kind),
              sqlite_storage_type(Kind, StorageType),
              format(string(ColumnSql), '~s ~s NOT NULL',
                     [QuotedColumn, StorageType])
            ),
            ColumnSqls),
    maplist(sqlite_identifier, Columns, QuotedColumns),
    atomics_to_string(ColumnSqls, ", ", ColumnList),
    atomics_to_string(QuotedColumns, ", ", UniqueColumns),
    format(string(Statement),
           'CREATE TABLE IF NOT EXISTS ~s (~s, UNIQUE (~s))',
           [Table, ColumnList, UniqueColumns]).

relation_select_sql(Edges, Identity, Name, Columns, Statement) :-
    sqlite_identifier(Name, Table),
    findall(Expression,
            ( nth0(Position, Columns, Column),
              relation_column_kind(Edges, Identity, Position, Kind),
              sqlite_select_expression(Kind, Column, Expression)
            ),
            Expressions),
    atomics_to_string(Expressions, ", ", SelectList),
    format(string(Statement), 'SELECT ~s FROM ~s t', [SelectList, Table]).

relation_column_kind(Edges, Identity, Position, Kind) :-
    member(':'(Identity, _, ref(Target), Position), Edges),
    !,
    storage_kind(Target, Edges, Kind).
relation_column_kind(_, _, _, json).

storage_kind(primitive(text), _, text) :- !.
storage_kind(primitive(int), _, integer) :- !.
storage_kind(Target, Edges, Kind) :-
    member(':'(module(prelude), Name, ref(Target), _), Edges),
    !,
    prelude_storage_kind(Name, Kind).
storage_kind(_, _, json).

prelude_storage_kind(Name, integer) :-
    memberchk(Name, [bool,i8,i16,i32,i64,i128,u8,u16,u32,u64,u128,
                     usize,isize]),
    !.
prelude_storage_kind(Name, real) :-
    memberchk(Name, [number,f32,f64]),
    !.
prelude_storage_kind(Name, text) :-
    memberchk(Name, [text,str,string,char]),
    !.
prelude_storage_kind(_, json).

sqlite_storage_type(text, "INTEGER").
sqlite_storage_type(integer, "INTEGER").
sqlite_storage_type(real, "REAL").
sqlite_storage_type(json, "TEXT").

sqlite_select_expression(text, Column, Expression) :-
    !,
    sqlite_identifier(Column, Quoted),
    format(string(Expression),
           '(SELECT s."content" FROM "__str" s WHERE s."__id" = t.~s) AS ~s',
           [Quoted, Quoted]).
sqlite_select_expression(json, Column, Expression) :-
    !,
    sqlite_identifier(Column, Quoted),
    format(string(Expression), 'json(t.~s) AS ~s', [Quoted, Quoted]).
sqlite_select_expression(_, Column, Expression) :-
    sqlite_identifier(Column, Quoted),
    format(string(Expression), 't.~s AS ~s', [Quoted, Quoted]).

sqlite_rules(Operators, Rules) :-
    maplist(operator_head, Operators, Heads1),
    sort(Heads1, Heads),
    findall(Rule,
            ( member(Head, Heads),
              head_sql_rule(Head, Operators, Rule)
            ),
            Rules).

operator_head(Operator, Head) :-
    get_dict(head, Operator, Head).

head_sql_rule(Head, Operators,
              _{id:Id, head:Head, delete:Delete, inserts:Inserts}) :-
    include(operator_has_head(Head), Operators, HeadOperators),
    HeadOperators = [First | _],
    get_dict(id, First, Id),
    sqlite_identifier(Head, Table),
    format(string(Delete), 'DELETE FROM ~s', [Table]),
    maplist(operator_insert_sql, HeadOperators, Inserts).

operator_has_head(Head, Operator) :-
    get_dict(head, Operator, OperatorHead),
    OperatorHead == Head.

operator_insert_sql(Operator, Statement) :-
    get_dict(head, Operator, Head),
    get_dict(projection, Operator, Projections),
    get_dict(bindings, Operator, Bindings),
    get_dict(predicates, Operator, Predicates),
    sqlite_identifier(Head, HeadTable),
    findall(HeadColumn,
            ( member(Projection, Projections),
              get_dict(head, Projection, Column),
              sqlite_identifier(Column, HeadColumn)
            ),
            HeadColumns),
    findall(Expression,
            ( member(Projection, Projections),
              projection_sql(Projection, Expression)
            ),
            ProjectionSql),
    dict_pairs(Bindings, _, BindingPairs),
    maplist(binding_sql, BindingPairs, FromParts),
    findall(PredicateSql,
            ( member(Predicate, Predicates),
              predicate_sql(Predicate, PredicateSql)
            ),
            PredicateParts),
    atomics_to_string(HeadColumns, ", ", HeadList),
    atomics_to_string(ProjectionSql, ", ", ProjectionList),
    atomics_to_string(FromParts, ", ", FromList),
    sqlite_where(PredicateParts, Where),
    format(string(Statement),
           'INSERT OR IGNORE INTO ~s (~s) SELECT ~s FROM ~s~s',
           [HeadTable, HeadList, ProjectionList, FromList, Where]).

binding_sql(Alias-Relation, Sql) :-
    sqlite_identifier(Relation, Table),
    sqlite_identifier(Alias, QuotedAlias),
    format(string(Sql), '~s ~s', [Table, QuotedAlias]).

projection_sql(Projection, Sql) :-
    get_dict(source, Projection, Source),
    !,
    source_column_sql(Source, Sql).
projection_sql(Projection, Sql) :-
    get_dict(value, Projection, Value),
    sqlite_literal_sql(Value, Sql).

predicate_sql(Predicate, Sql) :-
    get_dict(column_equals, Predicate, [Left, Right]),
    !,
    source_column_sql(Left, LeftSql),
    source_column_sql(Right, RightSql),
    format(string(Sql), '~s = ~s', [LeftSql, RightSql]).
predicate_sql(Predicate, Sql) :-
    get_dict(literal_equals, Predicate, Literal),
    get_dict(column, Literal, Column),
    get_dict(value, Literal, Value),
    source_column_sql(Column, ColumnSql),
    sqlite_literal_sql(Value, LiteralSql),
    format(string(Sql), '~s = ~s', [ColumnSql, LiteralSql]).

source_column_sql(Source, Sql) :-
    text_atom(Source, SourceAtom),
    sub_atom(SourceAtom, Before, 1, After, '.'),
    !,
    sub_atom(SourceAtom, 0, Before, _, Alias),
    Start is Before + 1,
    sub_atom(SourceAtom, Start, After, 0, Column),
    sqlite_identifier(Alias, QuotedAlias),
    sqlite_identifier(Column, QuotedColumn),
    format(string(Sql), '~s.~s', [QuotedAlias, QuotedColumn]).

sqlite_literal_sql(Value, Sql) :-
    integer(Value),
    !,
    format(string(Sql), '~d', [Value]).
sqlite_literal_sql(Value, Sql) :-
    float(Value),
    format(string(Sql), '~16g', [Value]).

sqlite_where([], "").
sqlite_where(Predicates, Where) :-
    Predicates = [_ | _],
    atomics_to_string(Predicates, " AND ", Body),
    string_concat(" WHERE ", Body, Where).

sqlite_identifier(Name, Quoted) :-
    text_codes(Name, Codes),
    duplicate_quotes(Codes, Escaped),
    string_codes(Body, Escaped),
    format(string(Quoted), '"~s"', [Body]).

duplicate_quotes([], []).
duplicate_quotes([0'" | Codes], [0'", 0'" | Escaped]) :-
    !,
    duplicate_quotes(Codes, Escaped).
duplicate_quotes([Code | Codes], [Code | Escaped]) :-
    duplicate_quotes(Codes, Escaped).

text_atom(Value, Atom) :-
    atom(Value),
    !,
    Atom = Value.
text_atom(Value, Atom) :-
    atom_string(Atom, Value).

text_codes(Value, Codes) :-
    string(Value),
    !,
    string_codes(Value, Codes).
text_codes(Value, Codes) :-
    atom_codes(Value, Codes).

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
