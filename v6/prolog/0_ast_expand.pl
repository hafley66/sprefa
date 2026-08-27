:- module(ast_expand,
          [ expand_ast_program/2,
            expand_ast_program_with_bindings/3,
            expand_ast_in_context/3
          ]).

:- use_module(library(lists)).
:- use_module('1_expansion/0_program_check',
              [ first_violation/3, ast_capture_names/2 ]).
:- use_module('2_host_expand/0_cst_query', [ serialize_ts_query/2 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700, xfx, :=).

expand_ast_program(Program, Expanded) :-
    expand_ast_program_with_bindings(Program, [], Expanded).

expand_ast_program_with_bindings(Program, Bindings, Expanded) :-
    cst_unsupported(Program),
    normalize_cst_program(Program, Normalized),
    ast_unsupported(Normalized),
    Normalized = prog(Decls, Rules0),
    rewrite_rules(Rules0, Bindings, 1, [], Rules, HostDecls, RelationDecls),
    append([Decls, HostDecls, RelationDecls], ExpandedDecls),
    Expanded = prog(ExpandedDecls, Rules).

expand_ast_in_context(Context, Program, Expanded) :-
    context_bindings(Context, Bindings),
    expand_ast_program_with_bindings(Program, Bindings, Expanded).

context_bindings(expansion_context(_, Bindings), Bindings) :- !.
context_bindings(_, []).

ast_unsupported(Program) :-
    first_violation(
        Program,
        [ast_query_not_literal, ast_lang_unknown,
         ast_query_single_quote, ast_no_named_capture],
        violation(Name, Payload)),
    ast_unsupported_term(Name, Payload, Reason),
    throw(unsupported_construct(Reason)).
ast_unsupported(_).

cst_unsupported(Program) :-
    first_violation(
        Program,
        [cst_capture_unused, cst_variable_uncaptured,
         cst_regexp_pattern_not_literal, cst_regexp_pattern_outside_subset,
         cst_regexp_pattern_invalid],
        violation(Name, Payload)),
    cst_unsupported_term(Name, Payload, Reason),
    throw(unsupported_construct(Reason)).
cst_unsupported(_).

cst_unsupported_term(cst_capture_unused, Name, cst_capture_unused(Name)).
cst_unsupported_term(cst_variable_uncaptured, Name,
                 cst_variable_uncaptured(Name)).
cst_unsupported_term(cst_regexp_pattern_not_literal, Payload, Payload).
cst_unsupported_term(cst_regexp_pattern_outside_subset, Payload, Payload).
cst_unsupported_term(cst_regexp_pattern_invalid, Payload, Payload).

normalize_cst_program(prog(Decls, Rules), prog(Decls, NormalizedRules)) :-
    maplist(normalize_cst_rule, Rules, NormalizedRules).

normalize_cst_rule((Head <- Body), (Head <- Normalized)) :-
    !,
    normalize_cst_body(Body, Normalized).
normalize_cst_rule((Head <+ Body), (Head <+ Normalized)) :-
    !,
    normalize_cst_body(Body, Normalized).
normalize_cst_rule(Rule, Rule).

normalize_cst_body((Left, Right), (NormalizedLeft, NormalizedRight)) :-
    !,
    normalize_cst_body(Left, NormalizedLeft),
    normalize_cst_body(Right, NormalizedRight).
normalize_cst_body(cst(Path, Digest, Language, Query, _),
                   ast(Path, Digest, Language, Text)) :-
    !,
    serialize_ts_query(Query, Text).
normalize_cst_body(cst(Path, Digest, Language, Query),
                   ast(Path, Digest, Language, Text)) :-
    !,
    serialize_ts_query(Query, Text).
normalize_cst_body(Body, Body).

ast_unsupported_term(ast_query_not_literal, _, ast_query_not_literal).
ast_unsupported_term(ast_lang_unknown, Lang, ast_lang_unknown(Lang)).
ast_unsupported_term(ast_query_single_quote, _, ast_query_single_quote).
ast_unsupported_term(ast_no_named_capture, _, ast_no_named_capture).

rewrite_rules([], _, _, _, [], [], []).
rewrite_rules([Rule0 | Rest], Bindings, Counter0, Mappings0,
              Rules, HostDecls, RelationDecls) :-
    rewrite_rule(Rule0, Bindings, Counter0, Mappings0,
                 RuleRules, Counter1, Mappings1,
                 RuleHostDecls, RuleRelationDecls),
    rewrite_rules(Rest, Bindings, Counter1, Mappings1,
                  RestRules, RestHostDecls, RestRelationDecls),
    append(RuleRules, RestRules, Rules),
    append(RuleHostDecls, RestHostDecls, HostDecls),
    append(RuleRelationDecls, RestRelationDecls, RelationDecls).

rewrite_rule((Head <- Body0), Bindings, Counter0, Mappings0,
             Rules, Counter, Mappings, HostDecls, RelationDecls) :-
    !,
    rewrite_rule_body(level, Head, Body0, Bindings, Counter0, Mappings0,
                      Rules, Counter, Mappings, HostDecls, RelationDecls).
rewrite_rule((Head <+ Body0), Bindings, Counter0, Mappings0,
             Rules, Counter, Mappings, HostDecls, RelationDecls) :-
    !,
    rewrite_rule_body(edge, Head, Body0, Bindings, Counter0, Mappings0,
                      Rules, Counter, Mappings, HostDecls, RelationDecls).
rewrite_rule(Rule, _, Counter, Mappings, [Rule], Counter, Mappings, [], []).

rewrite_rule_body(Arrow, Head, Body0, Bindings, Counter0, Mappings0,
                  Rules, Counter, Mappings, HostDecls, RelationDecls) :-
    body_goals(Body0, Goals),
    rewrite_goals(Goals, [], Bindings, Counter0, Mappings0,
                  NewGoals, Counter, Mappings, DemandRules,
                  HostDecls, RelationDecls),
    goals_body(NewGoals, Body),
    build_rule(Arrow, Head, Body, Rule),
    append(DemandRules, [Rule], Rules).

rewrite_goals([], Prefix, _, Counter, Mappings,
              Prefix, Counter, Mappings, [], [], []).
rewrite_goals([Goal | Rest], Prefix, Bindings, Counter0, Mappings0,
              NewGoals, Counter, Mappings, DemandRules,
              HostDecls, RelationDecls) :-
    (   Goal = ast(Path, Digest, Language, Query)
    ->  ast_host(Language, Query, Bindings, Counter0, Mappings0,
                 Counter1, Mappings, HostName, OutputVariables,
                 NewHostDecls, NewRelationDecls),
        host_atoms(HostName, Path, Digest, OutputVariables,
                   DemandAtom, WitnessBind, ResponseAtom),
        append(Prefix, [WitnessBind, ResponseAtom], Prefix1),
        goals_body(Prefix, DemandBody),
        DemandRule = (DemandAtom <- DemandBody),
        rewrite_goals(Rest, Prefix1, Bindings, Counter1, Mappings,
                      NewGoals, Counter, Mappings, MoreDemandRules,
                      MoreHostDecls, MoreRelationDecls),
        DemandRules = [DemandRule | MoreDemandRules],
        append(NewHostDecls, MoreHostDecls, HostDecls),
        append(NewRelationDecls, MoreRelationDecls, RelationDecls)
    ;   append(Prefix, [Goal], Prefix1),
        rewrite_goals(Rest, Prefix1, Bindings, Counter0, Mappings0,
                      NewGoals, Counter, Mappings, DemandRules,
                      HostDecls, RelationDecls)
    ).

ast_host(Language, Query, Bindings, Counter0, Mappings0,
         Counter, Mappings, HostName, OutputVariables,
         HostDecls, RelationDecls) :-
    (   memberchk(mapping(Language, Query, HostName, OutputNames,
                          _, _, _), Mappings0)
    ->  Counter = Counter0,
        Mappings = Mappings0,
        output_variables(OutputNames, Bindings, OutputVariables),
        HostDecls = [],
        RelationDecls = []
    ;   atom_concat('__ast_q', Counter0, HostName),
        ast_capture_names(Query, CaptureNames),
        append(CaptureNames, [line, end_line], OutputNames),
        output_variables(OutputNames, Bindings, OutputVariables),
        ast_host_decl(HostName, Language, Query, OutputNames, HostDecl,
                      RelationDecl),
        Counter is Counter0 + 1,
        Mappings = [mapping(Language, Query, HostName, OutputNames,
                            OutputVariables, HostDecl, RelationDecl) | Mappings0],
        HostDecls = [HostDecl],
        RelationDecls = RelationDecl
    ).

output_variables([], _, []).
output_variables([Name | Rest], Bindings, [Variable | Variables]) :-
    ( memberchk(Name=BoundVariable, Bindings)
    -> Variable = BoundVariable
    ; true
    ),
    output_variables(Rest, Bindings, Variables).

ast_host_decl(HostName, Language, Query, OutputNames,
              sh_decl(HostName, Inputs, Outputs, template(Command)),
              RelationDecl) :-
    Inputs = [col(path, text), col(digest, text)],
    output_columns(OutputNames, Outputs),
    format(string(Command),
           '"$DL_EXTRACT_BIN" query --lang ~w --query \'~s\' --digest {digest} {path}',
           [Language, Query]),
    host_relation_refs(HostName, DemandName, ResponseName),
    length(Inputs, InputCount),
    DemandArity is 2 + InputCount,
    length(Outputs, OutputCount),
    ResponseArity is 2 + InputCount + OutputCount,
    DemandRef = DemandName/DemandArity,
    ResponseRef = ResponseName/ResponseArity,
    column_type_decls(DemandRef,
                      [col(identity_digest, text), col(witness_digest, text),
                       col(path, text), col(digest, text)],
                      DemandDecls),
    column_type_decls(ResponseRef,
                      [col(witness_digest, text), col(ordinal, int),
                       col(path, text), col(digest, text) | Outputs],
                      ResponseDecls),
    append([keyed(ResponseRef, [1, 2]) | DemandDecls],
           ResponseDecls, RelationDecl).

output_columns([], []).
output_columns([line | Rest], [col(line, int) | Columns]) :-
    !,
    output_columns(Rest, Columns).
output_columns([end_line | Rest], [col(end_line, int) | Columns]) :-
    !,
    output_columns(Rest, Columns).
output_columns([Name | Rest], [col(Name, text) | Columns]) :-
    output_columns(Rest, Columns).

column_type_decls(_, [], []).
column_type_decls(Ref, [col(Name, Type) | Rest],
                  [col_type(Ref, Name, Type) | Decls]) :-
    column_type_decls(Ref, Rest, Decls).

host_relation_refs(HostName, DemandRef, ResponseRef) :-
    atom_concat('__host_demand_', HostName, DemandRef),
    atom_concat('__host_response_', HostName, ResponseRef).

host_atoms(HostName, Path, Digest, OutputVariables,
           DemandAtom, WitnessBind, ResponseAtom) :-
    digest_expr(identity, HostName, Path, Digest, Identity),
    digest_expr(witness, HostName, Path, Digest, Witness),
    host_relation_refs(HostName, DemandName, ResponseName),
    DemandAtom =.. [DemandName, Identity, Witness, Path, Digest],
    WitnessBind = (WitnessValue := Witness),
    ResponseAtom =.. [ResponseName, WitnessValue, _Ordinal,
                      Path, Digest | OutputVariables].

digest_expr(Role, HostName, Path, Digest, concat(Parts)) :-
    format(string(Prefix), '~w|~w', [Role, HostName]),
    Parts = [Prefix, '|path:text=', Path, '|digest:text=', Digest].

body_goals(Body, Goals) :-
    ( nonvar(Body), Body = (Left, Right)
    -> body_goals(Left, LeftGoals),
       body_goals(Right, RightGoals),
       append(LeftGoals, RightGoals, Goals)
    ; Body == true
    -> Goals = []
    ; Goals = [Body]
    ).

goals_body([], true) :- !.
goals_body([Goal], Goal) :- !.
goals_body([Goal | Rest], (Goal, Body)) :-
    goals_body(Rest, Body).

build_rule(level, Head, Body, (Head <- Body)).
build_rule(edge, Head, Body, (Head <+ Body)).
