parse_dl_file(File, Prog, Bindings, Findings) :-
    read_file_to_codes(File, Codes, []),
    parse_dl_source(File, Codes, Prog, Bindings, Findings).

parse_dl_dcg_entry(Source, Codes, Prog, Bindings, Findings) :-
    parse_dl_source(Source, Codes, Prog, Bindings, Findings).

parse_dl(Codes, Prog, Bindings, Findings) :-
    parse_dl_source(none, Codes, Prog, Bindings, Findings).

parse_dl_source(_, Codes, _, _, _) :-
    var(Codes), !,
    throw(dl_parse_error(invalid_input, position(1, 1))).
% Named and punned arguments resolve against a rel's declared column order,
% and a rule may precede the declaration it reads, so a first pass records
% every declaration's column order and the real pass starts from that set.
parse_dl_source(Source, Codes, Prog, Bindings, Findings) :-
    nb_setval(dl_prepass_columns, []),
    catch(( parse_dl_pass(Source, Codes, _, _, _) -> true ; true ), _, true),
    findall(Name-Cols, rel_column_order_fact(Name, Cols), Known),
    nb_setval(dl_prepass_columns, Known),
    catch(parse_dl_pass(Source, Codes, Prog, Bindings, Findings),
          dl_parse_error(Reason, _),
          parse_dl_marked_failure(Source, Codes, Reason)).

% mark/1 walks the whole remaining input at every token and parse_failure/1 is
% the only reader of what it records, so the first pass runs with marks off and
% a throwing parse is replayed once with them on.
parse_dl_marked_failure(Source, Codes, Reason) :-
    setup_call_cleanup(
        assertz(parse_marks_on),
        catch(( parse_dl_pass(Source, Codes, _, _, _) -> true ; true ),
              Ball,
              true),
        retractall(parse_marks_on)),
    (   var(Ball)
    ->  throw(dl_parse_error(Reason, position(1, 1)))
    ;   throw(Ball)
    ).

parse_dl_pass(_, Codes, Prog, Bindings, Findings) :-
    maplist(retractall,
            [ finding_fact(_), rel_column_order_fact(_, _),
              host_signature_fact(_, _, _), host_path_fact(_, _),
              source_statement_fact(_, _, _) ]),
    ( nb_current(dl_prepass_columns, Known) -> true ; Known = [] ),
    forall(member(Name-Cols, Known), assertz(rel_column_order_fact(Name, Cols))),
    length(Codes, Len),
    nb_setval(parse_input_length, Len),
    nb_setval(parse_furthest_remaining, Len),
    build_line_starts(Codes),
    b_setval(dl_vars, []),
    phrase(statements(Decls0, Rules0, Queries), Codes, Left),
    ( Left == [] -> true ; mark(Left), parse_failure(trailing_input) ),
    resolve_module_path_collisions(Decls0, Decls1),
    normalize_relation_value_decls(Decls1, Decls),
    flatten_host_paths(Decls, Rules0, Rules1),
    normalize_host_calls(Decls, Rules1, Rules),
    b_getval(dl_vars, VarsFinal),
    maplist([Name-Var, Name=Var]>>true, VarsFinal, BindingsRev),
    reverse(BindingsRev, Bindings),
    findall(F, finding_fact(F), Findings),
    ( Queries == [],
      \+ member(sh_decl(_, _, _, _), Decls)
    -> Prog = prog(Decls, Rules)
    ; Prog = program(Decls, Rules, Queries)
    ).

parse_failure(Reason) :-
    nb_getval(parse_furthest_remaining, Rem),
    remaining_line_column(Rem, Line, Col),
    throw(dl_parse_error(Reason, position(Line, Col))).

% mark/1 records the furthest-reached suffix; error positions derive from it
mark(S) :-
    parse_marks_on,
    length(S, R),
    nb_current(parse_furthest_remaining, F),
    R < F, !,
    nb_setval(parse_furthest_remaining, R).
mark(_).

remaining_line_column(Rem, Line, Col) :-
    nb_getval(parse_input_length, Len),
    Index is Len - Rem,
    nb_getval(parse_line_starts, Starts),
    nb_getval(parse_line_count, Count),
    line_containing(1, Count, Index, Starts, Line),
    arg(Line, Starts, Start),
    Col is Index - Start + 1.

line_containing(Low, High, Index, Starts, Line) :-
    ( Low >= High
    -> Line = Low
    ; Mid is (Low + High + 1) // 2,
      arg(Mid, Starts, MidStart),
      ( MidStart =< Index
      -> line_containing(Mid, High, Index, Starts, Line)
      ; Before is Mid - 1,
        line_containing(Low, Before, Index, Starts, Line)
      )
    ).

build_line_starts(Codes) :-
    findall(P, ( nth0(I, Codes, 0'\n), P is I + 1 ), Ps),
    Starts =.. [line_starts, 0 | Ps],
    functor(Starts, _, Count),
    nb_setval(parse_line_starts, Starts),
    nb_setval(parse_line_count, Count).

prolog:message(dl_parse_error(Reason, position(Line, Col))) -->
    [ 'parse error at line ~d, column ~d: ~w'-[Line, Col, Reason] ].

parse_dl_line_for_reason(Reason, Line) :-
    reason_references(Reason, Refs),
    ( member(Ref, Refs), statement_location_for_reference(rule, Ref, Line, _)
    -> true
    ; member(Ref, Refs), statement_location_for_reference(decl, Ref, Line, _)
    -> true
    ).

statement_location_for_reason(Reason, Line, Col) :-
    reason_references(Reason, Refs),
    ( member(Ref, Refs), statement_location_for_reference(rule, Ref, Line, Col)
    ; member(Ref, Refs), statement_location_for_reference(decl, Ref, Line, Col)
    ).

reason_references(Reason, Refs) :-
    findall(Name/Arity,
            ( sub_term(Name/Arity, Reason), atom(Name), integer(Arity) ),
            Refs0),
    sort(Refs0, Refs).

statement_location_for_reference(Kind, Ref, Line, Col) :-
    ( Kind == rule,
      source_statement_fact(rule, Item, Rem),
      statement_head_reference(Item, Ref)
    -> true
    ; source_statement_fact(Kind, Item, Rem),
      statement_reference(Kind, Item, Ref)
    ),
    !,
    remaining_line_column(Rem, Line, Col).

statement_head_reference(Rule, Name/Arity) :-
    Rule =.. [Op, Head, _], memberchk(Op, [<-, <+]), !,
    functor(Head, Name, Arity).

statement_reference(rule, Rule, Name/Arity) :-
    sub_term(Term, Rule),
    compound(Term),
    functor(Term, Name, Arity),
    atom(Name),
    !.
statement_reference(decl, Decls, Ref) :-
    member(Decl, Decls),
    declaration_source_ref(Decl, Ref),
    !.

record_statement(Kind, Item, Rem) :-
    memberchk(Kind-Recorded, [decl_list-decl, rule-rule]), !,
    assertz(source_statement_fact(Recorded, Item, Rem)).
record_statement(_, _, _).

declaration_source_ref(Decl, Ref) :-
    Decl =.. [F, Ref | _],
    memberchk(F, [kind, keyed, keep, col_type]).
declaration_source_ref(type_decl(Name, Specs), Name/Arity) :-
    length(Specs, Arity).
declaration_source_ref(sh_decl(Name, Ins, Outs, _), Name/Arity) :-
    append(Ins, Outs, Cols),
    length(Cols, Arity).


% A dotted host goal is a NAME with segments, never a nested rel, so it
% flattens to its module-path atom here rather than reaching the dot-expansion
% phase: normalize_host_leaf/3 below reads the goal's FUNCTOR against the sh
% declarations, and rel_path/2's functor is `rel_path`.
flatten_host_paths(Decls, Rules, Out) :-
    (   member(sh_decl(_, _, _, _), Decls)
    ->  maplist(flatten_host_path_rule(Decls), Rules, Out)
    ;   Out = Rules
    ).

flatten_host_path_rule(Decls, Rule0, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    map_tree(',', flatten_host_path_leaf(Decls), Body, Flattened),
    Rule =.. [Op, Head, Flattened].
flatten_host_path_rule(Decls, match(Source, Arms), match(Source, Flattened)) :- !,
    map_tree(';', flatten_host_path_rule(Decls), Arms, Flattened).
flatten_host_path_rule(_, Rule, Rule).

flatten_host_path_leaf(Decls, rel_path(Segments, Args), Atom) :-
    module_path_name(Segments, Name),
    member(sh_decl(Name, _, _, _), Decls),
    !,
    Atom =.. [Name | Args].
flatten_host_path_leaf(_, Item, Item).

normalize_host_calls(Decls, Rules, Out) :-
    maplist(normalize_host_rule(Decls), Rules, Out).

% map_tree/4 rewrites the leaves of a ','- or ';'-tree; the leaf goal arrives
% partially applied and is reached through call/3.
map_tree(Functor, Goal, Tree, Out) :-
    Tree =.. [Functor, Left, Right], !,
    Out =.. [Functor, NewLeft, NewRight],
    map_tree(Functor, Goal, Left, NewLeft),
    map_tree(Functor, Goal, Right, NewRight).
map_tree(_, Goal, Leaf, Out) :- call(Goal, Leaf, Out).

normalize_host_rule(Decls, Rule0, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    map_tree(',', normalize_host_leaf(Decls), Body, N),
    Rule =.. [Op, Head, N].
normalize_host_rule(Decls, match(Source, Arms), match(Source, N)) :- !,
    map_tree(';', normalize_host_rule(Decls), Arms, N).
normalize_host_rule(_, Rule, Rule).

normalize_host_leaf(_, probe(A, B, C, D), probe(A, B, C, D)) :- !.
normalize_host_leaf(_, Item, Item) :-
    body_surface_for_term(Item, _, _, _, _, _),
    !.
normalize_host_leaf(Decls, Atom, probe(Name, Ins, Outs, Salts)) :-
    compound(Atom),
    functor(Atom, Name, _),
    member(sh_decl(Name, Cols, _, _), Decls),
    !,
    Atom =.. [_ | Values],
    length(Cols, N),
    split_probe_values(N, Values, SurfaceIns, Outs),
    ( memberchk(arrival_identity(Name, Positions), Decls)
    -> arrival_roles(Cols, Positions, Roles)
    ;  host_input_roles(Name, Cols, Roles)
    ),
    ( same_length(Cols, SurfaceIns)
    -> partition_hiv(Cols, SurfaceIns, Roles, Ins, Salts)
    ; Ins = SurfaceIns, Salts = []
    ).
normalize_host_leaf(_, Item, Item).


statements(Decls, Rules, Queries) -->
    ws, here(S1),
    ( { S1 == [] }
    -> { Decls = [], Rules = [], Queries = [] }
    ; ( statement(Kind, Item, Sites)
      -> { length(S1, Rem),
           record_statement_sites(Kind, Item, Rem, Sites) }
      ; { parse_failure(statement) }
      ),
      statements(Decls1, Rules1, Queries1),
      { attach(Kind, Item, Decls1, Rules1, Queries1, Decls, Rules, Queries) }
    ).

record_statement_sites(decl_list, _, _, Sites) :-
    Sites = [_ | _],
    !,
    maplist(record_decl_site, Sites).
record_statement_sites(Kind, Item, Rem, _) :-
    record_statement(Kind, Item, Rem).

record_decl_site(decl_site(Rem, Decls)) :-
    record_statement(decl_list, Decls, Rem).

attach(decl_list, I, Ds, Rs, Qs, Ds2, Rs, Qs) :- append(I, Ds, Ds2).
attach(rule, I, Ds, Rs, Qs, Ds, [I | Rs], Qs).
attach(query, I, Ds, Rs, Qs, Ds, Rs, [I | Qs]).


% ws//0 eats comments before any consumer sees them; an editor keeps them
