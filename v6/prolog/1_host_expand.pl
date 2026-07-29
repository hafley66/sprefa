% 1_host_expand.pl: selected host, bind, probe, and query term forms.
%
% Phase 1 turns a probe into ordinary relations:
%
%   __host_demand_Name(Identity, Witness, Inputs..., Salts...)
%   __host_response_Name(Witness, Inputs..., Outputs...)
%
% The demand relation is derived by a level rule. The response relation is
% EDB, keyed by Witness, and receives answers through the fixture schedule.

:- module(host_expand,
          [ prepare_program/5,
            compile_host_decl/2,
            compile_query/2,
            compile_ts_query/2,
            host_relation_refs/3,
            % Exported as the characterization seam for the shared-walker
            % consolidation (rank R1): the plainest comma flatten in the tree.
            body_goals/2
          ]).

:- use_module(library(lists)).
:- use_module('compile/registry', [bind_definition/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700, xfx, :=).

prepare_program(Input, prog(Decls, Rules), HostPlans, BindPlans, QueryPlans) :-
    program_parts(Input, RawDecls, RawRules, Queries),
    maplist(normalize_rule, RawRules, NormalizedRules),
    findall(HostPlan,
            ( member(Decl, RawDecls),
              Decl = sh_decl(_, _, _, _),
              compile_host_decl(Decl, HostPlan)
            ),
            HostPlans),
    findall(bind_plan(Name, Columns),
            ( member(bind_decl(Name, Columns), RawDecls),
              validate_bind_decl(Name, Columns, NormalizedRules)
            ),
            BindPlans),
    maplist(compile_query, Queries, QueryPlans),
    expand_probe_rules(NormalizedRules, HostPlans, RawDecls,
                       ExpandedRules, GeneratedDecls),
    bind_column_decls(BindPlans, BindColumnDecls),
    append([RawDecls, Queries, GeneratedDecls, BindColumnDecls], Decls0),
    dedupe_terms(Decls0, Decls),
    append(ExpandedRules, [], Rules).

program_parts(prog(Decls, Rules), Decls, Rules, []).
program_parts(program(Decls, Rules, Queries), Decls, Rules, Queries).

normalize_rule(Raw, Rule) :-
    normalize_rule_shape(Raw, Shaped),
    compile_value_terms(Shaped, Rule).

normalize_rule_shape(fact(Head), (Head <- true)) :- !.
normalize_rule_shape(rule(Head, Items), (Head <- Body)) :- !,
    body_from_list(Items, Body).
normalize_rule_shape(match(Source, Arms), match(Source, Arms)) :- !.
normalize_rule_shape((Head <- Body), (Head <- Body)).
normalize_rule_shape((Head <+ Body), (Head <+ Body)).

compile_value_terms(Term, Term) :-
    var(Term),
    !.
compile_value_terms(Term, Text) :-
    nonvar(Term),
    Term = ts_query(_),
    !,
    compile_ts_query(Term, Text).
compile_value_terms(Term, Term) :-
    atomic(Term),
    !.
compile_value_terms(Term, Compiled) :-
    Term =.. [Name | Args],
    maplist(compile_value_terms, Args, CompiledArgs),
    Compiled =.. [Name | CompiledArgs].

body_from_list([], true).
body_from_list([Only], Only) :- !.
body_from_list([First | Rest], (First, Body)) :-
    body_from_list(Rest, Body).

compile_host_decl(
    sh_decl(Name, Inputs, Outputs, template(Template)),
    host_plan(Name, Inputs, Outputs, template(Template),
              demand_ref(DemandRef), response_ref(ResponseRef))) :-
    atom(Name),
    string(Template),
    validate_columns(Inputs, input),
    validate_columns(Outputs, output),
    column_names(Inputs, InputNames),
    column_names(Outputs, OutputNames),
    disjoint_columns(InputNames, OutputNames),
    validate_template(Template, InputNames, OutputNames),
    host_relation_refs(Name, DemandRef, ResponseRef),
    !.
compile_host_decl(Decl, _) :-
    throw(refused_host_decl(Decl)).

validate_columns(Columns, Role) :-
    is_list(Columns),
    column_names(Columns, Names),
    ( duplicate(Names, Name)
    -> throw(column_mismatch(Role, duplicate(Name)))
    ; forall(member(col(Name, Type), Columns),
             ( atom(Name), memberchk(Type, [int, text, json]) ))
    ).

column_names(Columns, Names) :-
    maplist(column_name, Columns, Names).

column_name(col(Name, _), Name).

duplicate(Names, Name) :-
    select(Name, Names, Rest),
    memberchk(Name, Rest),
    !.

disjoint_columns(Inputs, Outputs) :-
    ( member(Name, Inputs), memberchk(Name, Outputs)
    -> throw(column_mismatch(input_output_overlap(Name)))
    ; true
    ).

validate_template(Template, Inputs, Outputs) :-
    ( member(Name, Inputs), \+ template_mentions(Template, Name)
    -> throw(template_mismatch(unreferenced_input(Name)))
    ; member(Name, Outputs), template_mentions(Template, Name)
    -> throw(template_mismatch(output_used_as_input(Name)))
    ; brace_template_reference(Template, Name), \+ memberchk(Name, Inputs)
    -> throw(template_mismatch(unknown_column(Name)))
    ; true
    ).

template_mentions(Template, Name) :-
    template_reference(Template, Name),
    !.

template_reference(Template, Name) :-
    string_codes(Template, Codes),
    ( brace_reference(Codes, Name)
    ; dollar_reference(Codes, Name)
    ).

brace_template_reference(Template, Name) :-
    string_codes(Template, Codes),
    brace_reference(Codes, Name).

brace_reference(Codes, Name) :-
    append(_, [0'{ | Tail], Codes),
    append(NameCodes, [0'} | _], Tail),
    NameCodes = [First | _],
    identifier_start_code(First),
    maplist(identifier_code, NameCodes),
    atom_codes(Name, NameCodes).

dollar_reference(Codes, Name) :-
    append(_, [0'$ , First | Tail], Codes),
    identifier_start_code(First),
    take_identifier_codes(Tail, Rest, _),
    atom_codes(Name, [First | Rest]).

identifier_start_code(Code) :-
    code_type(Code, alpha)
    ; Code =:= 0'_.

identifier_code(Code) :-
    code_type(Code, alnum)
    ; Code =:= 0'_.

take_identifier_codes([Code | Rest], [Code | More], Tail) :-
    identifier_code(Code),
    !,
    take_identifier_codes(Rest, More, Tail).
take_identifier_codes(Tail, [], Tail).

host_relation_refs(Name, DemandRef, ResponseRef) :-
    atom_concat('__host_demand_', Name, DemandName),
    atom_concat('__host_response_', Name, ResponseName),
    DemandRef = DemandName,
    ResponseRef = ResponseName.

validate_bind_decl(Name, Columns, Rules) :-
    ( bind_definition(Name, Columns)
    -> true
    ; throw(bind_mismatch(Name, Columns))
    ),
    length(Columns, Arity),
    Ref = Name/Arity,
    ( member(Rule, Rules), rule_head_ref(Rule, Ref)
    -> throw(bind_and_rule_head(Name))
    ; true
    ).

rule_head_ref((Head <- _), Name/Arity) :-
    functor(Head, Name, Arity).
rule_head_ref((Head <+ _), Name/Arity) :-
    functor(Head, Name, Arity).

compile_query(query(Atom),
              query_plan(Name/Arity, columns(Args), snapshot(current))) :-
    compound(Atom),
    Atom =.. [Name | Args],
    atom(Name),
    length(Args, Arity),
    !.
compile_query(Query, _) :-
    throw(query_mismatch(Query)).

compile_ts_query(ts_query(Patterns), Text) :-
    maplist(ts_pattern_text, Patterns, Parts),
    atomics_to_string(Parts, "\n", Text),
    !.
compile_ts_query(Term, _) :-
    Term = sg_pattern(_, _, _),
    throw(unmapped_feature(slot_sg_metavariable_semantics, Term)).
compile_ts_query(Term, _) :-
    throw(unmapped_feature(slot_ts_query_term, Term)).

ts_pattern_text(group(Root, Predicates), Text) :-
    ts_pattern_text(Root, RootText),
    maplist(ts_pattern_text, Predicates, PredicateTexts),
    append([RootText], PredicateTexts, Parts),
    atomics_to_string(Parts, " ", Inner),
    format(string(Text), "(~s)", [Inner]).
ts_pattern_text(node(Type, Children), Text) :-
    atom(Type),
    maplist(ts_pattern_text, Children, ChildTexts),
    ( ChildTexts == []
    -> format(string(Text), "(~w)", [Type])
    ; atomics_to_string(ChildTexts, " ", ChildrenText),
      format(string(Text), "(~w ~s)", [Type, ChildrenText])
    ).
ts_pattern_text(field(Name, Pattern), Text) :-
    atom(Name),
    ts_pattern_text(Pattern, PatternText),
    format(string(Text), "~w: ~s", [Name, PatternText]).
ts_pattern_text(capture(Name, Pattern), Text) :-
    atom(Name),
    ts_pattern_text(Pattern, PatternText),
    format(string(Text), "~s @~w", [PatternText, Name]).
ts_pattern_text(capture_ref(Name), Text) :-
    atom(Name),
    format(string(Text), "@~w", [Name]).
ts_pattern_text(anonymous(Value), Text) :-
    ts_quoted(Value, Text).
ts_pattern_text(string(Value), Text) :-
    ts_quoted(Value, Text).
ts_pattern_text(predicate(eq, Left, Right), Text) :-
    ts_pattern_text(Left, LeftText),
    ts_pattern_text(Right, RightText),
    format(string(Text), "(#eq? ~s ~s)", [LeftText, RightText]).
ts_pattern_text(predicate(match, Left, Right), Text) :-
    ts_pattern_text(Left, LeftText),
    ts_pattern_text(Right, RightText),
    format(string(Text), "(#match? ~s ~s)", [LeftText, RightText]).
ts_pattern_text(quant(optional, Pattern), Text) :-
    ts_quantified(Pattern, "?", Text).
ts_pattern_text(quant(zero_or_more, Pattern), Text) :-
    ts_quantified(Pattern, "*", Text).
ts_pattern_text(quant(one_or_more, Pattern), Text) :-
    ts_quantified(Pattern, "+", Text).
ts_pattern_text(alternative(Patterns), Text) :-
    maplist(ts_pattern_text, Patterns, Parts),
    atomics_to_string(Parts, " ", Inner),
    format(string(Text), "[~s]", [Inner]).
ts_pattern_text(wildcard, "_").
ts_pattern_text(named_wildcard, "(_)").
ts_pattern_text(Term, _) :-
    throw(unmapped_feature(slot_ts_pattern_form, Term)).

ts_quantified(Pattern, Glyph, Text) :-
    ts_pattern_text(Pattern, PatternText),
    string_concat(PatternText, Glyph, Text).

ts_quoted(Value, Quoted) :-
    string_codes(Value, Codes),
    phrase(ts_escaped_codes(Codes), Escaped),
    string_codes(EscapedString, Escaped),
    format(string(Quoted), "\"~s\"", [EscapedString]).

ts_escaped_codes([]) --> [].
ts_escaped_codes([0'\\ | Rest]) --> "\\\\", ts_escaped_codes(Rest).
ts_escaped_codes([0'" | Rest]) --> "\\\"", ts_escaped_codes(Rest).
ts_escaped_codes([Code | Rest]) --> [Code], ts_escaped_codes(Rest).

expand_probe_rules([], _, _, [], []).
expand_probe_rules([Rule | Rest], HostPlans, RawDecls,
                   Expanded, GeneratedDecls) :-
    expand_probe_rule(Rule, HostPlans, RawDecls,
                      RuleExpanded, RuleDecls),
    expand_probe_rules(Rest, HostPlans, RawDecls,
                       RestExpanded, RestDecls),
    append(RuleExpanded, RestExpanded, Expanded),
    append(RuleDecls, RestDecls, GeneratedDecls).

expand_probe_rule((Head <- Body), HostPlans, RawDecls,
                  [DemandRule, (Head <- JoinedBody)], Decls) :-
    body_goals(Body, Goals),
    select(Probe, Goals, RemainingGoals),
    Probe = probe(_, _, _, _),
    !,
    ( member(OtherProbe, RemainingGoals), OtherProbe = probe(_, _, _, _)
    -> throw(probe_mismatch(multiple_probes(Body)))
    ; true
    ),
    expand_probe(Probe, HostPlans, RawDecls,
                 DemandAtom, WitnessBind, ResponseAtom, Decls),
    body_from_list(RemainingGoals, DemandBody),
    append(RemainingGoals, [WitnessBind, ResponseAtom], JoinedGoals),
    body_from_list(JoinedGoals, JoinedBody),
    DemandRule = (DemandAtom <- DemandBody).
expand_probe_rule(Rule, _, _, [Rule], []).

body_goals((Left, Right), Goals) :-
    !,
    body_goals(Left, LeftGoals),
    body_goals(Right, RightGoals),
    append(LeftGoals, RightGoals, Goals).
body_goals(Goal, [Goal]).

expand_probe(Probe, HostPlans, RawDecls,
             DemandAtom, WitnessBind, ResponseAtom, Decls) :-
    Probe = probe(Name, InputValues, OutputValues, Salts),
    ( member(HostPlan, HostPlans),
      HostPlan = host_plan(Name, Inputs, Outputs, _,
                           demand_ref(DemandName),
                           response_ref(ResponseName))
    -> true
    ; throw(probe_mismatch(Probe))
    ),
    ( same_length(Inputs, InputValues),
      same_length(Outputs, OutputValues),
      valid_salts(Salts)
    -> true
    ; throw(probe_mismatch(Probe))
    ),
    digest_expr(identity, Name, Inputs, InputValues, [], Identity),
    digest_expr(witness, Name, Inputs, InputValues, Salts, Witness),
    salt_values(Salts, SaltValues),
    append([Identity, Witness | InputValues], SaltValues, DemandArgs),
    DemandAtom =.. [DemandName | DemandArgs],
    WitnessBind = (WitnessValue := Witness),
    append([WitnessValue | InputValues], OutputValues, ResponseArgs),
    ResponseAtom =.. [ResponseName | ResponseArgs],
    generated_host_decls(DemandName, ResponseName, Inputs, Outputs,
                         Salts, RawDecls, Decls).

digest_expr(Role, Name, Inputs, InputValues, Salts, concat(Parts)) :-
    format(string(Prefix), "~w|~w", [Role, Name]),
    input_digest_parts(Inputs, InputValues, InputParts),
    salt_digest_parts(Salts, SaltParts),
    append([[Prefix], InputParts, SaltParts], Parts).

input_digest_parts([], [], []).
input_digest_parts([col(Name, Type) | Cols], [Value | Values],
                   ["|", Name, ":", Type, "=", Value | Parts]) :-
    input_digest_parts(Cols, Values, Parts).

salt_digest_parts([], []).
salt_digest_parts([salt(Name, Value) | Rest],
                  ["|", Name, "=", Value | Parts]) :-
    salt_digest_parts(Rest, Parts).

valid_salts(Salts) :-
    is_list(Salts),
    forall(member(salt(Name, _), Salts), atom(Name)),
    findall(Name, member(salt(Name, _), Salts), Names),
    \+ duplicate(Names, _).

salt_values([], []).
salt_values([salt(_, Value) | Rest], [Value | Values]) :-
    salt_values(Rest, Values).

generated_host_decls(DemandName, ResponseName, Inputs, Outputs,
                     Salts, RawDecls, Decls) :-
    length(Inputs, InputCount),
    length(Salts, SaltCount),
    DemandArity is 2 + InputCount + SaltCount,
    length(Outputs, OutputCount),
    ResponseArity is 1 + InputCount + OutputCount,
    DemandRef = DemandName/DemandArity,
    ResponseRef = ResponseName/ResponseArity,
    column_type_decls(DemandRef,
                      [col(identity_digest, text), col(witness_digest, text) | Inputs],
                      DemandBaseDecls),
    salt_column_decls(DemandRef, Salts, RawDecls, SaltDecls),
    column_type_decls(ResponseRef,
                      [col(witness_digest, text) | Inputs],
                      ResponseInputDecls),
    column_type_decls(ResponseRef, Outputs, ResponseOutputDecls),
    append([[keyed(ResponseRef, [1])],
            DemandBaseDecls, SaltDecls,
            ResponseInputDecls, ResponseOutputDecls],
           Decls).

column_type_decls(_, [], []).
column_type_decls(Ref, [col(Name, Type) | Rest],
                  [col_type(Ref, Name, Type) | Decls]) :-
    column_type_decls(Ref, Rest, Decls).

salt_column_decls(_, [], _, []).
salt_column_decls(Ref, [salt(Name, _) | Rest], RawDecls,
                  [col_type(Ref, Name, Type) | Decls]) :-
    ( declared_column_type(RawDecls, Name, FoundType)
    -> Type = FoundType
    ; Type = text
    ),
    salt_column_decls(Ref, Rest, RawDecls, Decls).

declared_column_type(Decls, Name, Type) :-
    member(col_type(_, Name, Type), Decls),
    !.
declared_column_type(Decls, Name, Type) :-
    member(bind_decl(_, Columns), Decls),
    memberchk(col(Name, Type), Columns),
    !.
declared_column_type(Decls, Name, Type) :-
    member(sh_decl(_, Inputs, Outputs, _), Decls),
    ( memberchk(col(Name, Type), Inputs)
    ; memberchk(col(Name, Type), Outputs)
    ),
    !.

bind_column_decls([], []).
bind_column_decls([bind_plan(Name, Columns) | Rest], Decls) :-
    length(Columns, Arity),
    Ref = Name/Arity,
    column_type_decls(Ref, Columns, Here),
    bind_column_decls(Rest, More),
    append(Here, More, Decls).

dedupe_terms(Terms, Deduped) :-
    dedupe_terms(Terms, [], Deduped).

dedupe_terms([], _, []).
dedupe_terms([Term | Rest], Seen, Deduped) :-
    ( memberchk(Term, Seen)
    -> dedupe_terms(Rest, Seen, Deduped)
    ; Deduped = [Term | More],
      dedupe_terms(Rest, [Term | Seen], More)
    ).
