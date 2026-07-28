:- module(hosts_extraction_hosts,
          [ compile_host_decl/2,
            compile_program/2,
            demand_row/4,
            active_magic_binds/3,
            active_declared_binds/3,
            rel_origin/3,
            compile_query/2,
            refusal/2
          ]).

:- use_module(library(lists)).

compile_host_decl(
    sh_decl(Name, Inputs, Outputs, template(Template)),
    host_plan(Name, Inputs, Outputs, template(Template))) :-
    validate_columns(Inputs, input),
    validate_columns(Outputs, output),
    column_names(Inputs, InputNames),
    column_names(Outputs, OutputNames),
    disjoint_columns(InputNames, OutputNames),
    template_declared_refs(Template, InputNames, OutputNames),
    !.
compile_host_decl(
    sh_decl_inferred(Name, Columns, template(Template)),
    host_plan(Name, Inputs, Outputs, template(Template))) :-
    validate_columns(Columns, column),
    partition(template_mentions(Template), Columns, Inputs, Outputs),
    Inputs \= [],
    Outputs \= [],
    !.
compile_host_decl(Decl, _) :-
    throw(refused_host_decl(Decl)).

validate_columns(Columns, Role) :-
    column_names(Columns, Names),
    ( duplicate(Names, Name)
    -> throw(column_mismatch(Role, duplicate(Name)))
    ;  forall(member(col(Name, Type), Columns),
              ( atom(Name), memberchk(Type, [int, text, json]) ))
    ).

column_names(Columns, Names) :-
    findall(Name, member(col(Name, _), Columns), Names).

duplicate(Names, Name) :-
    select(Name, Names, Rest),
    memberchk(Name, Rest),
    !.

disjoint_columns(Inputs, Outputs) :-
    ( member(Name, Inputs), memberchk(Name, Outputs)
    -> throw(column_mismatch(input_output_overlap(Name)))
    ;  true
    ).

template_declared_refs(Template, InputNames, OutputNames) :-
    ( member(Name, InputNames), \+ template_mentions(Template, col(Name, _))
    -> throw(template_mismatch(unreferenced_input(Name)))
    ; member(Name, OutputNames), template_mentions(Template, col(Name, _))
    -> throw(template_mismatch(output_used_as_input(Name)))
    ; unknown_brace_reference(Template, InputNames, Unknown)
    -> throw(template_mismatch(unknown_column(Unknown)))
    ; true
    ).

template_mentions(Template, col(Name, _)) :-
    atom_string(Name, NameString),
    string_concat("{", NameString, Brace0),
    string_concat(Brace0, "}", Brace),
    string_concat("$", NameString, Dollar),
    ( sub_string(Template, _, _, _, Brace)
    ; dollar_reference(Template, Dollar)
    ).

dollar_reference(Template, Dollar) :-
    sub_string(Template, Before, Length, After, Dollar),
    ( After =:= 0
    ; NextAt is Before + Length,
      sub_string(Template, NextAt, 1, _, NextString),
      string_codes(NextString, [Next]),
      \+ code_type(Next, alnum),
      Next =\= 0'_
    ).

unknown_brace_reference(Template, Known, Unknown) :-
    sub_string(Template, Open, _, _, "{"),
    Start is Open + 1,
    sub_string(Template, Start, _, _, Tail),
    first_close(Tail, Close),
    sub_string(Tail, 0, Close, _, NameString),
    atom_string(Unknown, NameString),
    \+ memberchk(Unknown, Known),
    !.

first_close(Tail, Close) :-
    sub_string(Tail, Close, 1, _, "}"),
    !.

demand_row(
    host_plan(Name, InputCols, OutputCols, _),
    probe(Name, InputValues, OutputValues, SaltPairs),
    request(Name,
            identity_digest(host(Name, InputPairs)),
            witness_digest(host(Name, InputPairs, SaltPairs))),
    response_shape(OutputPairs)) :-
    same_length(InputCols, InputValues),
    same_length(OutputCols, OutputValues),
    pairs(InputCols, InputValues, InputPairs),
    pairs(OutputCols, OutputValues, OutputPairs),
    valid_salts(SaltPairs),
    !.
demand_row(_, Probe, _, _) :-
    throw(probe_mismatch(Probe)).

pairs([], [], []).
pairs([col(Name, Type) | Cols], [Value | Values],
      [field(Name, Type, Value) | Pairs]) :-
    pairs(Cols, Values, Pairs).

valid_salts(Salts) :-
    forall(member(salt(Name, _), Salts), atom(Name)),
    findall(Name, member(salt(Name, _), Salts), Names),
    \+ duplicate(Names, _).

% The pre-declaration implementation activated a bind from any matching EDB
% relation name. This predicate exists only to grade that hazard.
active_magic_binds(Registry, program(Decls, Rules, _), Active) :-
    findall(Name,
            ( member(bind_def(Name, _), Registry),
              member(rel_decl(Name, _), Decls),
              \+ rule_heads(Name, Rules) ),
            Names),
    sort(Names, Active).

% The candidate requires bind_decl. Name and column shape must match the
% registered bind definition before its cold source is subscribed.
active_declared_binds(Registry, program(Decls, Rules, _), Active) :-
    findall(Name,
            ( member(bind_decl(Name, Columns), Decls),
              member(bind_def(Name, Columns), Registry),
              \+ rule_heads(Name, Rules) ),
            Names),
    sort(Names, Active).

rule_heads(Name, Rules) :-
    member(rule(Head, _), Rules),
    functor(Head, Name, _).

rel_origin(Name, program(Decls, Rules, _), Origin) :-
    ( member(bind_decl(Name, _), Decls)
    -> ( rule_heads(Name, Rules)
       -> Origin = refused(bind_and_rule_head(Name))
       ;  Origin = edb(bind_declaration) )
    ; rule_heads(Name, Rules)
    -> Origin = idb(rule_head)
    ; member(rel_decl(Name, _), Decls)
    -> Origin = edb(never_headed)
    ;  Origin = absent
    ).

compile_query(query(Atom),
              query_plan(Name/Arity, columns(Args), snapshot(current))) :-
    compound(Atom),
    Atom =.. [Name | Args],
    atom(Name),
    length(Args, Arity).

compile_program(program(Decls, Rules, Queries),
                compiled(HostPlans, BindNames, QueryPlans)) :-
    findall(Plan,
            ( member(Decl, Decls),
              Decl = sh_decl(_, _, _, _),
              compile_host_decl(Decl, Plan) ),
            HostPlans),
    findall(Name, member(bind_decl(Name, _), Decls), BindNames),
    maplist(compile_query, Queries, QueryPlans),
    forall(member(rule(_, Body), Rules),
           validate_rule_body(Body, HostPlans)).

validate_rule_body(Body, HostPlans) :-
    forall(member(Item, Body),
           ( Item = probe(Name, _, _, _)
           -> member(HostPlan, HostPlans),
              HostPlan = host_plan(Name, _, _, _),
              demand_row(HostPlan, Item, _, _)
           ; true )).

refusal(Goal, Error) :-
    catch((call(Goal), fail), Error, true).
