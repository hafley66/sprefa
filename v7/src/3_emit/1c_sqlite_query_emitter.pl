:- module(dl7_sqlite_query_emitter,
          [ emit_sqlite_query/4,
            sqlite_query_artifact/3,
            read_sqlite_layout/3,
            sqlite_layout_rows/3,
            sqlite_install_sql/3,
            sqlite_identifier/2,
            sqlite_literal/2
          ]).

:- use_module(library(http/json), [json_read_dict/2]).
:- use_module('1_artifact_emitter',
              [compiler_view/2, emit_compiled/4]).

%% emit_sqlite_query(+CompiledUnit, +LayoutRows, -Artifact, -Diagnostics) is det.
%
% Lower the selected checked relation from the immutable logical-program rows.
% LayoutRows is an ordinary relational target description:
%
%   sqlite_source(RelationName, TableName, OrderedColumns)
%   sqlite_output(RelationName, OrderedColumns)
%
% RelationName is an authored DL7 label. Table and column names are SQLite
% names and remain data supplied by the caller.
emit_sqlite_query(CompiledUnit, LayoutRows, Artifact, Diagnostics) :-
    compiler_view(CompiledUnit, View),
    catch(
        ( sqlite_query_from_view(View, LayoutRows, Artifact),
          Diagnostics = []
        ),
        sqlite_query_error(Reason),
        ( Artifact = _{},
          Diagnostics = [diagnostic(emit, none, Reason)]
        )).

%% sqlite_query_artifact(+LayoutRows, +CompilerView, -Artifact) is semidet.
%
% Closure-compatible arm for emit_compiled(prolog(Callable), ...). Invalid
% layouts fail through that host hook; callers needing the exact diagnostic use
% emit_sqlite_query/4.
sqlite_query_artifact(LayoutRows, View, Artifact) :-
    sqlite_query_from_view(View, LayoutRows, Artifact).

sqlite_query_from_view(
    compiler_view(_, _, LogicalRows, RuntimeProgram),
    LayoutRows, Artifact) :-
    normalize_layout(LayoutRows, SourceLayouts, OutputLayout),
    RuntimeProgram = checked_datalog(root_graph(_, Edges), _, _, _),
    resolve_layout_relations(
        Edges, LogicalRows, SourceLayouts, OutputLayout,
        Sources, Output),
    validate_program_boundary(LogicalRows, Sources, Output, Rules),
    lower_rules(Rules, Sources, Output, RulePlans),
    rule_select_sql(RulePlans, SelectSql),
    artifact_sources(Sources, SourcePlan),
    Output = bound_output(_, OutputName, OutputColumns),
    Artifact = _{
        ir_version:1,
        dialect:"sqlite",
        output:_{relation:OutputName, columns:OutputColumns},
        sources:SourcePlan,
        rules:RulePlans,
        select_sql:SelectSql,
        semantics:_{duplicates:"set",
                    input_domain:"every mapped source value is non-NULL"},
        boundary:[
            "positive rules",
            "projection",
            "equijoins",
            "constant filters",
            "nonrecursive rule union"
        ]
    }.

normalize_layout(LayoutRows, Sources, Output) :-
    must_be_layout_list(LayoutRows),
    findall(sqlite_source(Relation, Table, Columns),
            member(sqlite_source(Relation, Table, Columns), LayoutRows),
            SourceRows),
    findall(sqlite_output(Relation, Columns),
            member(sqlite_output(Relation, Columns), LayoutRows),
            OutputRows),
    length(LayoutRows, LayoutCount),
    length(SourceRows, SourceCount),
    length(OutputRows, OutputCount),
    ExpectedCount is SourceCount + OutputCount,
    require_layout_row_count(LayoutCount, ExpectedCount),
    require_one_output(OutputRows, Output0),
    maplist(normalize_source_layout, SourceRows, Sources0),
    normalize_output_layout(Output0, Output),
    require_unique_source_names(Sources0),
    Sources = Sources0.

must_be_layout_list(LayoutRows) :-
    (   is_list(LayoutRows)
    ->  true
    ;   throw(sqlite_query_error(invalid_sqlite_layout_rows(LayoutRows)))
    ).

require_layout_row_count(Count, Count) :- !.
require_layout_row_count(_, _) :-
    throw(sqlite_query_error(unknown_sqlite_layout_row)).

require_one_output([Output], Output) :- !.
require_one_output(Outputs, _) :-
    length(Outputs, Count),
    throw(sqlite_query_error(sqlite_layout_output_count(Count))).

normalize_source_layout(
    sqlite_source(Relation0, Table0, Columns0),
    source_layout(Relation, Table, Columns)) :-
    layout_name(relation, Relation0, Relation),
    layout_name(table, Table0, Table),
    layout_columns(Relation, Columns0, Columns).

normalize_output_layout(
    sqlite_output(Relation0, Columns0),
    output_layout(Relation, Columns)) :-
    layout_name(relation, Relation0, Relation),
    layout_columns(Relation, Columns0, Columns),
    require_unique_output_columns(Columns).

layout_name(_Role, Value, Name) :-
    text_string(Value, Name),
    Name \== "",
    \+ sub_string(Name, _, _, _, "\u0000"),
    !.
layout_name(Role, Value, _) :-
    throw(sqlite_query_error(invalid_sqlite_layout_name(Role, Value))).

layout_columns(Relation, Columns0, Columns) :-
    (   is_list(Columns0)
    ->  maplist(layout_column(Relation), Columns0, Columns)
    ;   throw(sqlite_query_error(
                  invalid_sqlite_layout_columns(Relation, Columns0)))
    ).

layout_column(Relation, Value, Column) :-
    (   text_string(Value, Column),
        Column \== ""
    ->  true
    ;   throw(sqlite_query_error(
                  invalid_sqlite_layout_column(Relation, Value)))
    ).

require_unique_source_names(Sources) :-
    findall(Name, member(source_layout(Name, _, _), Sources), Names),
    maplist(string_lower, Names, FoldedNames),
    sort(FoldedNames, Unique),
    length(Names, Count),
    length(Unique, Count),
    !.
require_unique_source_names(_) :-
    throw(sqlite_query_error(duplicate_sqlite_source_relation)).

require_unique_output_columns(Columns) :-
    maplist(string_lower, Columns, FoldedColumns),
    sort(FoldedColumns, Unique),
    length(Columns, Count),
    length(Unique, Count),
    !.
require_unique_output_columns(_) :-
    throw(sqlite_query_error(duplicate_sqlite_output_column)).

resolve_layout_relations(
    Edges, LogicalRows, SourceLayouts, OutputLayout,
    Sources, Output) :-
    maplist(resolve_source(Edges, LogicalRows), SourceLayouts, Sources),
    resolve_output(Edges, LogicalRows, OutputLayout, Output),
    findall(Identity,
            member(bound_source(Identity, _, _, _), Sources),
            SourceIdentities),
    sort(SourceIdentities, UniqueSourceIdentities),
    length(SourceIdentities, Count),
    length(UniqueSourceIdentities, Count),
    !.
resolve_layout_relations(_, _, _, _, _, _) :-
    throw(sqlite_query_error(duplicate_sqlite_source_identity)).

resolve_source(Edges, LogicalRows,
               source_layout(Name, Table, Columns),
               bound_source(Identity, Name, Table, Columns)) :-
    resolve_authored_relation(Edges, Name, Identity),
    require_relation_arity(LogicalRows, Identity, Name, Columns).

resolve_output(Edges, LogicalRows,
               output_layout(Name, Columns),
               bound_output(Identity, Name, Columns)) :-
    resolve_authored_relation(Edges, Name, Identity),
    require_relation_arity(LogicalRows, Identity, Name, Columns).

resolve_authored_relation(Edges, RequestedName, Identity) :-
    findall(Candidate,
            ( member(':'(module(file(_)), Label,
                         ref(Candidate), _), Edges),
              text_string(Label, RequestedName)
            ),
            Candidates0),
    sort(Candidates0, Candidates),
    resolved_relation_result(RequestedName, Candidates, Identity).

resolved_relation_result(_, [Identity], Identity) :- !.
resolved_relation_result(Name, [], _) :-
    throw(sqlite_query_error(unknown_sqlite_relation(Name))).
resolved_relation_result(Name, Identities, _) :-
    throw(sqlite_query_error(
              ambiguous_sqlite_relation(Name, Identities))).

require_relation_arity(LogicalRows, Identity, Name, Columns) :-
    (   memberchk(program_relation(Identity, Arity), LogicalRows)
    ->  length(Columns, ColumnCount),
        require_matching_arity(Name, Arity, ColumnCount)
    ;   throw(sqlite_query_error(
                  missing_sqlite_logical_relation(Name, Identity)))
    ).

require_matching_arity(_, Arity, Arity) :- !.
require_matching_arity(Name, Arity, ColumnCount) :-
    throw(sqlite_query_error(
              sqlite_layout_arity(Name, Arity, ColumnCount))).

validate_program_boundary(LogicalRows, Sources, Output, Rules) :-
    Output = bound_output(OutputIdentity, OutputName, _),
    source_identities(Sources, SourceIdentities),
    relevant_identities(OutputIdentity, SourceIdentities, Relevant),
    reject_relevant_seeds(LogicalRows, Relevant),
    reject_derived_sources(LogicalRows, Sources),
    output_rules(LogicalRows, OutputIdentity, Rules),
    require_output_rules(OutputName, Rules),
    maplist(validate_rule(LogicalRows, Sources, OutputIdentity), Rules).

source_identities(Sources, Identities) :-
    findall(Identity,
            member(bound_source(Identity, _, _, _), Sources),
            Identities).

relevant_identities(Output, Sources, Relevant) :-
    sort([Output | Sources], Relevant).

reject_relevant_seeds(LogicalRows, Relevant) :-
    (   member(program_seed(Seed, CallId), LogicalRows),
        logical_call(LogicalRows, CallId, call(ref(Relation), _)),
        memberchk(Relation, Relevant)
    ->  throw(sqlite_query_error(
                  unsupported_sqlite_seed(Seed, Relation)))
    ;   true
    ).

reject_derived_sources(LogicalRows, Sources) :-
    (   member(bound_source(Relation, Name, _, _), Sources),
        member(program_rule(RuleId, HeadCallId), LogicalRows),
        logical_call(LogicalRows, HeadCallId, call(ref(Relation), _))
    ->  throw(sqlite_query_error(
                  unsupported_sqlite_derived_source(Name, RuleId)))
    ;   true
    ).

output_rules(LogicalRows, OutputIdentity, Rules) :-
    findall(Index-rule(RuleId, Head, Goals),
            ( member(program_rule(RuleId, HeadCallId), LogicalRows),
              RuleId = rule_id(Index),
              logical_call(LogicalRows, HeadCallId, Head),
              Head = call(ref(OutputIdentity), _),
              logical_goals(LogicalRows, RuleId, Goals)
            ),
            Indexed0),
    keysort(Indexed0, Indexed),
    indexed_values(Indexed, Rules).

require_output_rules(_, [_ | _]) :- !.
require_output_rules(Name, []) :-
    throw(sqlite_query_error(sqlite_output_without_rules(Name))).

validate_rule(LogicalRows, Sources, OutputIdentity,
              rule(RuleId, Head, Goals)) :-
    require_level_rule(LogicalRows, RuleId),
    require_nonempty_goals(RuleId, Goals),
    validate_arguments(RuleId, Head),
    maplist(validate_goal(RuleId, Sources, OutputIdentity), Goals).

require_nonempty_goals(_, [_ | _]) :- !.
require_nonempty_goals(RuleId, []) :-
    throw(sqlite_query_error(
              unsupported_sqlite_zero_body_rule(RuleId))).

require_level_rule(LogicalRows, RuleId) :-
    (   memberchk(program_rule_kind(RuleId, level), LogicalRows)
    ->  true
    ;   throw(sqlite_query_error(
                  unsupported_sqlite_rule_kind(RuleId)))
    ).

validate_goal(RuleId, Sources, OutputIdentity,
              checked_goal(Polarity, call(ref(Relation), Arguments))) :-
    require_positive_goal(RuleId, Polarity),
    require_source_relation(RuleId, Sources, OutputIdentity, Relation),
    validate_arguments(RuleId, call(ref(Relation), Arguments)).

require_positive_goal(_, positive) :- !.
require_positive_goal(RuleId, Polarity) :-
    throw(sqlite_query_error(
              unsupported_sqlite_goal_polarity(RuleId, Polarity))).

require_source_relation(RuleId, _, OutputIdentity, OutputIdentity) :-
    !,
    throw(sqlite_query_error(
              unsupported_sqlite_recursion(RuleId, OutputIdentity))).
require_source_relation(_, Sources, _, Relation) :-
    memberchk(bound_source(Relation, _, _, _), Sources),
    !.
require_source_relation(RuleId, _, _, Relation) :-
    throw(sqlite_query_error(
              unsupported_sqlite_derived_input(RuleId, Relation))).

validate_arguments(RuleId, call(_, Arguments)) :-
    maplist(validate_argument(RuleId), Arguments).

validate_argument(_, var(_)) :- !.
validate_argument(_, const(Value)) :-
    !,
    sqlite_literal(Value, _).
validate_argument(RuleId, aggregate(Operator, _)) :-
    !,
    throw(sqlite_query_error(
              unsupported_sqlite_aggregate(RuleId, Operator))).
validate_argument(RuleId, ref(Reference)) :-
    !,
    throw(sqlite_query_error(
              unsupported_sqlite_reference_value(RuleId, Reference))).
validate_argument(RuleId, Argument) :-
    throw(sqlite_query_error(
              unsupported_sqlite_argument(RuleId, Argument))).

lower_rules([], _, _, []).
lower_rules([Rule | Rules], Sources, Output,
            [Plan | Plans]) :-
    lower_rule(Rule, Sources, Output, Plan),
    lower_rules(Rules, Sources, Output, Plans).

lower_rule(rule(rule_id(Index), Head, Goals), Sources,
           bound_output(_, _, OutputColumns), Plan) :-
    lower_goal_sources(
        Goals, Sources, rule_id(Index), 0,
        FromItems, [], Bindings),
    Head = call(_, HeadArguments),
    lower_projection(
        HeadArguments, OutputColumns, Bindings, rule_id(Index),
        Projection),
    render_rule_select(Projection, FromItems, Sql),
    Plan = _{rule:Index, sql:Sql}.

lower_goal_sources([], _, _, _, [], Bindings, Bindings).
lower_goal_sources(
    Goals0,
    Sources, RuleId, Index,
    [FromItem | FromItems], Bindings0, Bindings) :-
    select_scheduled_goal(
        RuleId, Index, Bindings0, Goals0,
        checked_goal(positive, call(ref(Relation), Arguments)), Goals),
    memberchk(bound_source(Relation, _, Table, Columns), Sources),
    format(string(Alias), "g~d", [Index]),
    lower_goal_arguments(
        Arguments, Columns, Alias, Bindings0, Bindings1,
        [], GoalPredicates),
    source_from_item(
        RuleId, Index, Table, Alias, GoalPredicates, FromItem),
    NextIndex is Index + 1,
    lower_goal_sources(
        Goals, Sources, RuleId, NextIndex, FromItems,
        Bindings1, Bindings).

select_scheduled_goal(_, 0, _, [Goal | Goals], Goal, Goals) :-
    !.
select_scheduled_goal(RuleId, _, Bindings, Goals, Goal, Rest) :-
    (   select(Goal, Goals, Rest),
        goal_shares_binding(Goal, Bindings)
    ->  true
    ;   throw(sqlite_query_error(
                  unsupported_sqlite_disconnected_join(RuleId)))
    ).

goal_shares_binding(checked_goal(_, call(_, Arguments)), Bindings) :-
    member(var(Variable), Arguments),
    memberchk(Variable-_, Bindings),
    !.

source_from_item(_, 0, Table, Alias, Predicates,
                 from_base(Table, Alias, Predicates)) :-
    !.
source_from_item(RuleId, _, Table, Alias, Predicates,
                 from_join(Table, Alias, Predicates)) :-
    (   member(equals(column(Alias, _), column(OtherAlias, _)),
               Predicates),
        OtherAlias \== Alias
    ->  true
    ;   throw(sqlite_query_error(
                  unsupported_sqlite_disconnected_join(RuleId, Alias)))
    ).

lower_goal_arguments([], [], _, Bindings, Bindings,
                     Predicates, Predicates).
lower_goal_arguments([Argument | Arguments], [Column | Columns], Alias,
                     Bindings0, Bindings, Predicates0, Predicates) :-
    column_expression(Alias, Column, Expression),
    append(Predicates0, [not_null(Expression)], PredicatesWithDomain),
    lower_goal_argument(
        Argument, Expression, Bindings0, Bindings1,
        PredicatesWithDomain, Predicates1),
    lower_goal_arguments(
        Arguments, Columns, Alias, Bindings1, Bindings,
        Predicates1, Predicates).

lower_goal_argument(var(Variable), Expression,
                    Bindings0, Bindings, Predicates0, Predicates) :-
    (   memberchk(Variable-BoundExpression, Bindings0)
    ->  Bindings = Bindings0,
        append(Predicates0,
               [equals(Expression, BoundExpression)], Predicates)
    ;   append(Bindings0, [Variable-Expression], Bindings),
        Predicates = Predicates0
    ).
lower_goal_argument(const(Value), Expression,
                    Bindings, Bindings, Predicates0, Predicates) :-
    append(Predicates0, [equals(Expression, literal(Value))], Predicates).

lower_projection([], [], _, _, []).
lower_projection([Argument | Arguments], [Column | Columns],
                 Bindings, RuleId,
                 [projection(Expression, Column) | Projection]) :-
    projection_expression(Argument, Bindings, RuleId, Expression),
    lower_projection(Arguments, Columns, Bindings, RuleId, Projection).

projection_expression(var(Variable), Bindings, RuleId, Expression) :-
    (   memberchk(Variable-Expression0, Bindings)
    ->  Expression = Expression0
    ;   throw(sqlite_query_error(
                  unsupported_sqlite_unbound_output(RuleId, Variable)))
    ).
projection_expression(const(Value), _, _, literal(Value)).

column_expression(Alias, Column, column(Alias, Column)).

render_rule_select(Projection, [Base], Sql) :-
    !,
    maplist(render_projection, Projection, ProjectionSql),
    atomics_to_string(ProjectionSql, ", ", SelectList),
    render_base_item(Base, BaseSql, BasePredicates),
    render_predicate_suffix(BasePredicates, PredicateSuffix),
    format(string(Sql), "SELECT DISTINCT ~s FROM ~s~s",
           [SelectList, BaseSql, PredicateSuffix]).
render_rule_select(Projection, [Base, FirstJoin | Joins], Sql) :-
    maplist(render_projection, Projection, ProjectionSql),
    atomics_to_string(ProjectionSql, ", ", SelectList),
    render_base_item(Base, BaseSql, BasePredicates),
    FirstJoin = from_join(Table, Alias, JoinPredicates),
    append(BasePredicates, JoinPredicates, FirstPredicates),
    render_join_item(
        from_join(Table, Alias, FirstPredicates), FirstJoinSql),
    maplist(render_join_item, Joins, JoinSqls),
    atomics_to_string(JoinSqls, "", JoinSql),
    format(string(Sql), "SELECT DISTINCT ~s FROM ~s~s~s",
           [SelectList, BaseSql, FirstJoinSql, JoinSql]).

render_projection(projection(Expression, Column), Sql) :-
    render_expression(Expression, ExpressionSql),
    sqlite_identifier(Column, ColumnSql),
    format(string(Sql), "~s AS ~s", [ExpressionSql, ColumnSql]).

render_base_item(from_base(Table, Alias, Predicates), Sql, Predicates) :-
    sqlite_identifier(Table, TableSql),
    sqlite_identifier(Alias, AliasSql),
    format(string(Sql), "~s AS ~s", [TableSql, AliasSql]).

render_join_item(from_join(Table, Alias, Predicates), Sql) :-
    sqlite_identifier(Table, TableSql),
    sqlite_identifier(Alias, AliasSql),
    maplist(render_predicate, Predicates, PredicateSql),
    atomics_to_string(PredicateSql, " AND ", OnSql),
    format(string(Sql), " INNER JOIN ~s AS ~s ON ~s",
           [TableSql, AliasSql, OnSql]).

render_predicate_suffix([], "").
render_predicate_suffix(Predicates, Suffix) :-
    Predicates = [_ | _],
    maplist(render_predicate, Predicates, PredicateSql),
    atomics_to_string(PredicateSql, " AND ", Where),
    format(string(Suffix), " WHERE ~s", [Where]).

render_predicate(equals(Left, Right), Sql) :-
    render_expression(Left, LeftSql),
    render_expression(Right, RightSql),
    format(string(Sql), "~s = ~s", [LeftSql, RightSql]).
render_predicate(not_null(Expression), Sql) :-
    render_expression(Expression, ExpressionSql),
    format(string(Sql), "~s IS NOT NULL", [ExpressionSql]).

render_expression(column(Alias, Column), Sql) :-
    sqlite_identifier(Alias, AliasSql),
    sqlite_identifier(Column, ColumnSql),
    format(string(Sql), "~s.~s", [AliasSql, ColumnSql]).
render_expression(literal(Value), Sql) :-
    sqlite_literal(Value, Sql).

rule_select_sql([Rule], Sql) :-
    !,
    Sql = Rule.sql.
rule_select_sql(Rules, Sql) :-
    findall(RuleSql, (member(Rule, Rules), RuleSql = Rule.sql), Sqls),
    atomics_to_string(Sqls, " UNION ", Sql).

artifact_sources([], []).
artifact_sources([bound_source(_, Relation, Table, Columns) | Sources],
                 [_{relation:Relation, table:Table, columns:Columns} | Plans]) :-
    artifact_sources(Sources, Plans).

%% sqlite_install_sql(+Name, +SelectSql, -Sql) is det.
sqlite_install_sql(Name, SelectSql, Sql) :-
    sqlite_identifier(Name, QuotedName),
    sqlite_literal(SelectSql, QuotedSelect),
    format(string(Sql),
           "CREATE VIRTUAL TABLE ~s USING sqlite_ivm(~s);",
           [QuotedName, QuotedSelect]).

%% sqlite_identifier(+Name, -Sql) is det.
sqlite_identifier(Name0, Sql) :-
    layout_name(identifier, Name0, Name),
    split_string(Name, "\"", "", Parts),
    atomics_to_string(Parts, "\"\"", Escaped),
    format(string(Sql), "\"~s\"", [Escaped]).

%% sqlite_literal(+Value, -Sql) is det.
sqlite_literal(Value, Sql) :-
    integer(Value),
    !,
    number_string(Value, Sql).
sqlite_literal(Value, Sql) :-
    float(Value),
    !,
    (   float_class(Value, Class),
        memberchk(Class, [normal, subnormal, zero])
    ->  format(string(Sql), "~17g", [Value])
    ;   throw(sqlite_query_error(
                  unsupported_sqlite_float(Value)))
    ).
sqlite_literal(Value, Sql) :-
    text_string(Value, Text),
    !,
    (   \+ sub_string(Text, _, _, _, "\u0000")
    ->  split_string(Text, "'", "", Parts),
        atomics_to_string(Parts, "''", Escaped),
        format(string(Sql), "'~s'", [Escaped])
    ;   throw(sqlite_query_error(
                  unsupported_sqlite_nul_literal))
    ).
sqlite_literal(Value, _) :-
    throw(sqlite_query_error(
              unsupported_sqlite_literal(Value))).

logical_goals(LogicalRows, RuleId, Goals) :-
    findall(Position-checked_goal(Polarity, Call),
            ( member(program_goal(RuleId, Position, Polarity, CallId),
                     LogicalRows),
              logical_call(LogicalRows, CallId, Call)
            ),
            Indexed0),
    keysort(Indexed0, Indexed),
    indexed_values(Indexed, Goals).

logical_call(LogicalRows, CallId, call(ref(Relation), Arguments)) :-
    memberchk(program_apply(CallId, Relation), LogicalRows),
    findall(Position-Argument,
            ( member(program_argument(CallId, Position, ArgumentId),
                     LogicalRows),
              logical_argument(LogicalRows, ArgumentId, Argument)
            ),
            Indexed0),
    keysort(Indexed0, Indexed),
    indexed_values(Indexed, Arguments).

logical_argument(LogicalRows, ArgumentId, var(Variable)) :-
    memberchk(program_edge(ArgumentId, variable, const(Variable), 0),
              LogicalRows),
    !.
logical_argument(LogicalRows, ArgumentId, ref(Reference)) :-
    memberchk(program_edge(ArgumentId, reference, ref(Reference), 0),
              LogicalRows),
    !.
logical_argument(LogicalRows, ArgumentId, const(Value)) :-
    memberchk(program_edge(ArgumentId, literal, const(Value), 0),
              LogicalRows),
    !.
logical_argument(LogicalRows, ArgumentId, aggregate(Operator, Input)) :-
    memberchk(program_edge(ArgumentId, aggregate, const(Operator), 0),
              LogicalRows),
    memberchk(program_edge(ArgumentId, input, ref(InputId), 1),
              LogicalRows),
    logical_argument(LogicalRows, InputId, Input).

indexed_values([], []).
indexed_values([_-Value | Indexed], [Value | Values]) :-
    indexed_values(Indexed, Values).

text_string(Value, Text) :-
    string(Value),
    !,
    Text = Value.
text_string(Value, Text) :-
    atom(Value),
    atom_string(Value, Text).

%% read_sqlite_layout(+Path, -Rows, -Diagnostics) is det.
read_sqlite_layout(Path, Rows, Diagnostics) :-
    catch(
        setup_call_cleanup(
            open(Path, read, Stream, [encoding(utf8)]),
            json_read_dict(Stream, Dict),
            close(Stream)),
        Error,
        Dict = layout_read_error(Error)),
    read_layout_result(Dict, Rows, Diagnostics).

read_layout_result(layout_read_error(Error), [],
                   [diagnostic(layout, file, sqlite_layout_read(Error))]) :-
    !.
read_layout_result(Dict, Rows, Diagnostics) :-
    sqlite_layout_rows(Dict, Rows, Diagnostics).

%% sqlite_layout_rows(+Dict, -Rows, -Diagnostics) is det.
sqlite_layout_rows(Dict, Rows, Diagnostics) :-
    catch(
        ( layout_dict_rows(Dict, Rows),
          Diagnostics = []
        ),
        sqlite_query_error(Reason),
        ( Rows = [],
          Diagnostics = [diagnostic(layout, none, Reason)]
        )).

layout_dict_rows(Dict, Rows) :-
    (   is_dict(Dict),
        get_dict(sources, Dict, SourceDicts),
        is_list(SourceDicts),
        get_dict(output, Dict, OutputDict),
        is_dict(OutputDict)
    ->  maplist(source_dict_row, SourceDicts, SourceRows),
        output_dict_row(OutputDict, OutputRow),
        append(SourceRows, [OutputRow], Rows0),
        normalize_layout(Rows0, _, _),
        Rows = Rows0
    ;   throw(sqlite_query_error(invalid_sqlite_layout_json))
    ).

source_dict_row(Dict, sqlite_source(Relation, Table, Columns)) :-
    (   is_dict(Dict),
        get_dict(relation, Dict, Relation),
        get_dict(table, Dict, Table),
        get_dict(columns, Dict, Columns)
    ->  true
    ;   throw(sqlite_query_error(invalid_sqlite_source_json(Dict)))
    ).

output_dict_row(Dict, sqlite_output(Relation, Columns)) :-
    (   get_dict(relation, Dict, Relation),
        get_dict(columns, Dict, Columns)
    ->  true
    ;   throw(sqlite_query_error(invalid_sqlite_output_json(Dict)))
    ).
