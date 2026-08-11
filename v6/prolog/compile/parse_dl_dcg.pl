
:- module(parse_dl_dcg,
          [ parse_dl_dcg_entry/5,
            parse_dl/4,
            parse_dl_file/4,
            parse_dl_line_for_reason/2,
            remaining_line_column/3,
            statement_location_for_reason/3,
            statement_location_for_reference/4,
            use_item/3,
            parse_dl_source/5
          ]).

:- set_prolog_flag(back_quotes, codes).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(registry,
              [ surface/5, body_surface_for_term/6,
                wrapper_lower_role/3, host_input_roles/3, expression/5 ]).
:- use_module('../0_cst_query',
              [ parse_cst_query/2, ts_query_capture_names/2 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- dynamic finding_fact/1, rel_column_order_fact/2,
           host_signature_fact/3, source_statement_fact/3.

record_finding(F) :- assertz(finding_fact(F)).
record_cols(Name, Cols) :-
    retractall(rel_column_order_fact(Name, _)),
    assertz(rel_column_order_fact(Name, Cols)).
lookup_column_order(Name, Cols) :- rel_column_order_fact(Name, Cols).
record_host_signature(Name, Ins, Outs) :-
    retractall(host_signature_fact(Name, _, _)),
    assertz(host_signature_fact(Name, Ins, Outs)).


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
parse_dl_source(_, Codes, Prog, Bindings, Findings) :-
    maplist(retractall,
            [ finding_fact(_), rel_column_order_fact(_, _),
              host_signature_fact(_, _, _), source_statement_fact(_, _, _) ]),
    length(Codes, Len),
    nb_setval(parse_input_length, Len),
    nb_setval(parse_furthest_remaining, Len),
    build_line_starts(Codes),
    b_setval(dl_vars, []),
    phrase(statements(Decls0, Rules0, Queries), Codes, Left),
    ( Left == [] -> true ; mark(Left), parse_failure(trailing_input) ),
    resolve_module_path_collisions(Decls0, Decls1),
    normalize_relation_value_decls(Decls1, Decls),
    normalize_host_calls(Decls, Rules0, Rules),
    b_getval(dl_vars, VarsFinal),
    maplist([Name-Var, Name=Var]>>true, VarsFinal, BindingsRev),
    reverse(BindingsRev, Bindings),
    findall(F, finding_fact(F), Findings),
    ( Queries == [],
      \+ member(sh_decl(_, _, _, _), Decls),
      \+ member(bind_decl(_, _), Decls)
    -> Prog = prog(Decls, Rules)
    ; Prog = program(Decls, Rules, Queries)
    ).

parse_failure(Reason) :-
    nb_getval(parse_furthest_remaining, Rem),
    remaining_line_column(Rem, Line, Col),
    throw(dl_parse_error(Reason, position(Line, Col))).

% mark/1 records the furthest-reached suffix; error positions derive from it
mark(S) :-
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

statement_location_for_reference(rule, Ref, Line, Col) :-
    source_statement_fact(rule, Item, Rem),
    statement_head_reference(Item, Ref),
    !,
    remaining_line_column(Rem, Line, Col).
statement_location_for_reference(Kind, Ref, Line, Col) :-
    source_statement_fact(Kind, Item, Rem),
    statement_reference(Kind, Item, Ref),
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

record_statement(decl_list, Decls, Rem) :- !,
    assertz(source_statement_fact(decl, Decls, Rem)).
record_statement(rule, Rule, Rem) :- !,
    assertz(source_statement_fact(rule, Rule, Rem)).
record_statement(_, _, _).

declaration_source_ref(Decl, Ref) :-
    Decl =.. [F, Ref | _],
    memberchk(F, [kind, keyed, keep, col_type]).
declaration_source_ref(type_decl(Name, Specs), Name/Arity) :-
    length(Specs, Arity).
declaration_source_ref(sh_decl(Name, Ins, Outs, _), Name/Arity) :-
    append(Ins, Outs, Cols),
    length(Cols, Arity).


normalize_host_calls(Decls, Rules, Out) :-
    maplist(normalize_host_rule(Decls), Rules, Out).

normalize_host_rule(Decls, Rule0, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    normalize_host_body(Decls, Body, N),
    Rule =.. [Op, Head, N].
normalize_host_rule(Decls, match(Source, Arms), match(Source, N)) :- !,
    normalize_host_arms(Decls, Arms, N).
normalize_host_rule(_, Rule, Rule).

normalize_host_arms(Decls, (L ; R), (NL ; NR)) :- !,
    normalize_host_arms(Decls, L, NL),
    normalize_host_arms(Decls, R, NR).
normalize_host_arms(Decls, Arm, N) :- normalize_host_rule(Decls, Arm, N).

normalize_host_body(Decls, (L, R), (NL, NR)) :- !,
    normalize_host_body(Decls, L, NL),
    normalize_host_body(Decls, R, NR).
normalize_host_body(_, probe(A, B, C, D), probe(A, B, C, D)) :- !.
normalize_host_body(_, Item, Item) :-
    body_surface_for_term(Item, _, _, _, _, _),
    !.
normalize_host_body(Decls, Atom, probe(Name, Ins, Outs, Salts)) :-
    compound(Atom),
    functor(Atom, Name, _),
    member(sh_decl(Name, Cols, _, _), Decls),
    !,
    Atom =.. [_ | Values],
    length(Cols, N),
    split_probe_values(N, Values, SurfaceIns, Outs),
    host_input_roles(Name, Cols, Roles),
    ( same_length(Cols, SurfaceIns)
    -> partition_hiv(Cols, SurfaceIns, Roles, Ins, Salts)
    ; Ins = SurfaceIns, Salts = []
    ).
normalize_host_body(_, Item, Item).


statements(Decls, Rules, Queries) -->
    ws, here(S1),
    ( { S1 == [] }
    -> { Decls = [], Rules = [], Queries = [] }
    ; ( statement(Kind, Item)
      -> { length(S1, Rem), record_statement(Kind, Item, Rem) }
      ; { parse_failure(statement) }
      ),
      statements(Decls1, Rules1, Queries1),
      { attach(Kind, Item, Decls1, Rules1, Queries1, Decls, Rules, Queries) }
    ).

attach(decl_list, I, Ds, Rs, Qs, Ds2, Rs, Qs) :- append(I, Ds, Ds2).
attach(rule, I, Ds, Rs, Qs, Ds, [I | Rs], Qs).
attach(query, I, Ds, Rs, Qs, Ds, Rs, [I | Qs]).


ws(S0, S) :-
    ( S0 = [C | S1], code_type(C, space) -> ws(S1, S)
    ; S0 = [0'# | S1] -> skip_to_eol(S1, S2), ws(S2, S)
    ; S = S0, mark(S0)
    ).

skip_to_eol(S0, S) :-
    ( S0 = [0'\n | S1] -> S = S1
    ; S0 = [_ | S1] -> skip_to_eol(S1, S)
    ; S = S0
    ).

lit([], S, S) :- mark(S).
lit([C | Cs], S0, S) :-
    S0 = [C | Rest],
    ( lit(Cs, Rest, S) -> true ; mark(S0), fail ).

word(Cs, S0, S) :-
    lit(Cs, S0, S),
    \+ (S = [C | _], id_code(C)).

peek(C, S, S) :- S = [C | _], !.

id_code(0'_) :- !.
id_code(C) :- code_type(C, alnum).

% here//1 zero-width capture of the remaining input; back//1 pushback to it
here(S, S, S).
back(S, _, S).


ident(Name, S0, S) :-
    mark(S0),
    S0 = [C | Rest],
    ( code_type(C, alpha) ; C == 0'_ ), !,
    ident_rest(Rest, Cs, S),
    atom_codes(Name, [C | Cs]).

ident_rest([C | Cs], [C | More], S) :- id_code(C), !, ident_rest(Cs, More, S).
ident_rest(S, [], S).


int_lit(Value, S0, S) :-
    mark(S0),
    ( S0 = [0'- | S1] -> Neg = true, S2 = S1 ; Neg = false, S2 = S0 ),
    S2 = [D | _], code_type(D, digit), !,
    digits0(Ds, S2, S),
    mark(S),
    number_codes(Mag, Ds),
    ( Neg == true -> Value is -Mag ; Value = Mag ).

float_lit(Value, S0, S) :-
    mark(S0),
    phrase(float_codes(Cs), S0, S),
    number_codes(Value, Cs),
    float(Value),
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]).

float_codes(Cs) -->
    ( `-` -> { Sign = [0'-] } ; { Sign = [] } ),
    digits1(Int), float_tail(Tail),
    { append([Sign, Int, Tail], Cs) }.

digits0([C | More]) --> [C], { code_type(C, digit) }, !, digits0(More).
digits0([]) --> [].
digits1([C | More]) --> [C], { code_type(C, digit) }, !, digits0(More).

float_tail(Cs) -->
    `.`, digits1(F),
    ( exp(E) -> [] ; { E = [] } ),
    { append([0'. | F], E, Cs) }.
float_tail(Cs) --> exp(Cs).

exp([M | Cs]) -->
    [M], { memberchk(M, `eE`) },
    ( [S], { memberchk(S, `+-`) } -> { Sign = [S] } ; { Sign = [] } ),
    digits1(Ds),
    { append(Sign, Ds, Cs) }.


atom_lit(Atom, S0, S) :- quoted(0'\', Cs, S0, S), atom_codes(Atom, Cs).
string_lit(Str, S0, S) :- quoted(0'", Cs, S0, S), string_codes(Str, Cs).

quoted(Q, Cs, S0, S) :-
    mark(S0),
    S0 = [Q | S1], !,
    quoted_chars(Q, S1, Cs, S).

quoted_chars(Q, [Q, Q | Rest], [Q | More], S) :- !,
    mark([Q, Q | Rest]),
    quoted_chars(Q, Rest, More, S).
quoted_chars(Q, [Q | Rest], [], Rest) :- !.
quoted_chars(Q, [0'\\, E | Rest], Cs, S) :- !,
    mark([0'\\, E | Rest]),
    escape(Q, E, Cs, More),
    quoted_chars(Q, Rest, More, S).
quoted_chars(Q, [C | Rest], [C | More], S) :-
    quoted_chars(Q, Rest, More, S).

escape(_, 0'n,  [0'\n  | M], M) :- !.
escape(_, 0't,  [0'\t  | M], M) :- !.
escape(_, 0'r,  [0'\r  | M], M) :- !.
escape(_, 0'\\, [0'\\  | M], M) :- !.
escape(Q, Q, [Q | M], M) :- !.
escape(_, O, [0'\\, O | M], M).


% dl_vars: b_setval backtrackable global replaces the old V0/V accumulator
% threading; the trail unwinds it exactly as the threaded pair used to.
get_or_make_var(Name, Var) :-
    b_getval(dl_vars, Vars0),
    ( memberchk(Name-Existing, Vars0)
    -> Var = Existing
    ; b_setval(dl_vars, [Name-Var | Vars0])
    ).

hole_var('_', _) :- !.
hole_var(Name, Var) :- get_or_make_var(Name, Var).


use_item(Item) -->
    ws,
    ( word(`pub`) -> ws, word(`use`), { F = pub_use } ; word(`use`), { F = use } ),
    ws, string_lit(Text), ws,
    ( word(`as`), ws, ident(Alias)
    -> { Item =.. [F, Text, Alias] }
    ; { Item =.. [F, Text] }
    ),
    ws, [0'.].


statement(Kind, Item) -->
    ws,
    ( bind_decl_stmt(D) -> { Kind = decl_list, Item = [D] }
    ; rel_stmt(Ds) -> { Kind = decl_list, Item = Ds }
    ; sh_decl_stmt(D) -> { Kind = decl_list, Item = [D] }
    ; query_stmt(Q) -> { Kind = query, Item = Q }
    ; ( match_stmt(R) -> [] ; rule_stmt(R) -> [] ),
      { Kind = rule, b_getval(dl_vars, Vars), annotate_cst_item(R, Vars, Item) }
    ).


% parameterized nonterminals via call//N; one arity now that dl_vars is global
sep(P, [X | Xs]) -->
    call(P, X), ws,
    ( lit(`,`) -> ws, sep(P, Xs) ; { Xs = [] } ).
args(P, Xs) --> ws, ( peek(0')) -> { Xs = [] } ; sep(P, Xs) ).
tok(Cs) --> ws, lit(Cs).


rel_stmt(Decls) -->
    word(`rel`), ws,
    ( ident(Name), tok(`(`), enum_variants(Variants), tok(`)`), tok(`.`),
      { Decls = [enum_decl(Name, Variants)],
        record_enum_column_orders(Name, Variants) }
    ; dotted_path(Segs), tok(`(`),
      args(decl_a_column, Specs), tok(`)`),
      { length(Specs, Arity),
        module_path_name(Segs, Name),
        Ref = Name/Arity,
        spec_names(Specs, Cols),
        record_cols(Name, Cols) },
      ws,
      { typed_decl_entries(Ref, Specs, Typed) },
      rel_modifiers(Ref, Mods),
      { module_path_decls(Segs, Ref, PathDecls),
        column_less_decls(Ref, Specs, Mods, UnitDecls),
        append([Typed, Mods, PathDecls, UnitDecls], Decls) },
      tok(`.`)
    ; decl_b_tail(Decls)
    ).

column_less_decls(_, Specs, _, []) :- Specs \== [], !.
column_less_decls(Ref, _, Mods, Decls) :-
    ( memberchk(kind(Ref, _), Mods) -> Decls = [] ; Decls = [kind(Ref, set)] ).

module_path_name(Segs, Name) :- atomic_list_concat(Segs, '__', Name).

module_path_decls([_], _, []) :- !.
module_path_decls(Segs, Ref, [rel_path_decl(Ref, Segs)]).

rel_modifiers(Ref, Decls) -->
    ( word(`log`) -> { Decl = kind(Ref, log) }
    ; keep_clause(Policy) -> { Decl = keep(Ref, Policy) }
    ; key_clause(Positions) -> { Decl = keyed(Ref, Positions) }
    ; word(`set`)
    -> { record_finding(unsupported_surface(removed_word(set))), Decl = none }
    ), !,
    ws, rel_modifiers(Ref, Rest),
    { Decl == none -> Decls = Rest ; Decls = [Decl | Rest] }.
rel_modifiers(_, []) --> [].

decl_a_column(column(Name, Type)) -->
    ident(Name), ws,
    ( lit(`:`) -> ws, type_expr(Type) ; { Type = none } ).

type_expr(Type) -->
    type_base(Base),
    ( lit(`?`) -> { Type = option(Base) } ; { Type = Base } ).

type_base(T) --> { scalar_column_type(T), atom_codes(T, Cs) }, word(Cs), !.
type_base(T) -->
    { member(W, [option, json_list, list, list_entity_dense_sequence,
                 list_interned_set, list_entity_linked_sequence]),
      atom_codes(W, Cs) },
    word(Cs), !,
    tok(`(`), ws, type_expr(E), tok(`)`),
    { T =.. [W, E] }.
type_base(Name) --> ident(Name).

enum_variants((First ; Rest)) -->
    enum_variant(First), tok(`;`), ws, enum_variants(Rest).
enum_variants(Variant) --> enum_variant(Variant).

enum_variant(Variant) -->
    ws, ident(Name), tok(`(`), args(enum_field, Fields), tok(`)`),
    { Variant =.. [Name | Fields] }.

enum_field(Col:Type) --> ident(Col), tok(`:`), ws, ident(Type).

record_enum_column_orders(Rel, Variants) :-
    tag_rel_name(Rel, Tag),
    record_cols(Tag, [id, tag]),
    forall(enum_decl_variant_term(Variants, V), record_enum_variant(Rel, V)).

enum_decl_variant_term((L ; R), V) :- !,
    ( enum_decl_variant_term(L, V) ; enum_decl_variant_term(R, V) ).
enum_decl_variant_term(V, V).

record_enum_variant(Rel, Variant) :-
    Variant =.. [Name | Fields],
    maplist([Col:_, Col]>>true, Fields, Cols),
    atomic_list_concat([Rel, Name], '_', VariantRel),
    record_cols(VariantRel, [id | Cols]).

tag_rel_name(Rel, Tag) :- atomic_list_concat([Rel, tag], '_', Tag).

spec_names(Specs, Names) :- maplist([column(N, _), N]>>true, Specs, Names).

typed_decl_entries(Ref, Specs, Decls) :-
    findall(col_type(Ref, Col, Type),
            ( member(column(Col, Type), Specs), Type \== none ),
            Decls).

keep_clause(Policy) -->
    word(`keep`), tok(`(`), ws,
    ( word(`all`) -> { Policy = all }
    ; word(`count`), tok(`(`), ws, int_lit(N), tok(`)`)
    -> { Policy = count(N) }
    ),
    tok(`)`).

key_clause(Positions) -->
    word(`key`), tok(`(`), ws, sep(int_lit, Positions), tok(`)`).


decl_b_tail(Decls) -->
    ( lit(`(`) -> ws, int_lit(Ret), tok(`)`), { HasRetention = true }
    ; { HasRetention = false }
    ),
    ws, ident(Name), tok(`(`),
    decl_b_columns(Name, Specs), tok(`)`), tok(`.`),
    { length(Specs, Arity),
      Ref = Name/Arity,
      spec_names(Specs, Cols),
      record_cols(Name, Cols),
      ( HasRetention == true
      -> record_finding(unsupported_surface(retention_marker(Ref, Ret)))
      ; true
      ),
      typed_decl_entries(Ref, Specs, Decls) }.

decl_b_columns(Rel, Specs) --> args(typed_col(decl_b_column_type(Rel)), Specs).

typed_col(TypeP, column(Col, Type)) -->
    ident(Col), tok(`:`), ws, call(TypeP, Col, Type).

decl_b_column_type(Rel, Col, none) -->
    coltype(W), { W \== none }, !,
    { record_finding(unsupported_surface(column_type_wrapper(Rel, Col, W))) }.
decl_b_column_type(_, _, Type) --> type_expr(Type).

coltype(W) -->
    { member(W, ['Key', 'Min', 'Max']), atom_codes(W, Cs) },
    word(Cs), !,
    tok(`(`), ws, ident(_), tok(`)`).
coltype(none) --> ident(_).


resolve_module_path_collisions(Decls0, Decls) :-
    findall(Ref-Segs, member(rel_path_decl(Ref, Segs), Decls0), Paths),
    ( Paths == []
    -> Decls = Decls0
    ; reserved_rel_names(Decls0, Paths, Reserved),
      foldl(disambiguate_module_path(Reserved), Paths, Decls0, Decls)
    ).

disambiguate_module_path(Reserved, Name/Arity-Segs, Decls0, Decls) :-
    ( memberchk(Name, Reserved)
    -> variant_sha1(Segs, Sha),
       sub_atom(Sha, 0, 16, _, Digest),
       atomic_list_concat([Name, '__', Digest], Digested),
       ( lookup_column_order(Name, Cols)
       -> record_cols(Digested, Cols)
       ; true
       ),
       maplist(rename_decl_ref(Name/Arity, Digested/Arity), Decls0, Decls)
    ; Decls = Decls0
    ).

rename_decl_ref(Old, New, Decl0, Decl) :-
    Decl0 =.. [F, Old | Args],
    memberchk(F, [col_type, kind, keep, keyed, rel_path_decl]),
    !,
    Decl =.. [F, New | Args].
rename_decl_ref(_, _, Decl, Decl).

reserved_rel_names(Decls, Paths, Reserved) :-
    findall(Name,
            ( declared_rel_name(Decls, Ref, Name),
              \+ memberchk(Ref-_, Paths)
            ; minted_rel_name(Decls, Name)
            ),
            Names),
    sort(Names, Reserved).

declared_rel_name(Decls, Name/Arity, Name) :-
    member(Decl, Decls),
    Decl =.. [F, Name/Arity | _],
    memberchk(F, [col_type, kind, keyed, keep]).
declared_rel_name(Decls, enum_decl(Name), Name) :-
    member(enum_decl(Name, _), Decls).

minted_rel_name(Decls, Minted) :-
    ( member(col_type(Parent/_, Col, option(_)), Decls),
      atomic_list_concat([Parent, '__', Col], Minted)
    ; member(col_type(_, _, option(Element)), Decls),
      atom(Element),
      atomic_list_concat(['__opt_', Element], Minted)
    ; member(enum_decl(Rel, Variants), Decls),
      ( enum_decl_variant_term(Variants, Variant),
        Variant =.. [Name | _],
        atomic_list_concat([Rel, Name], '_', Minted)
      ; tag_rel_name(Rel, Minted)
      )
    ).


normalize_relation_value_decls(Decls0, Decls) :-
    findall(Name,
            ( declared_column_type_name(Decls0, Name),
              relation_schema(Decls0, Name, _, _) ),
            Names0),
    sort(Names0, ValueNames),
    normalize_relation_value_decls(Decls0, ValueNames, [], Decls).

normalize_relation_value_decls([], _, _, []).
normalize_relation_value_decls([Head | Rest], VNames, Seen, Out) :-
    ( Head = col_type(Name/Arity, _, _), memberchk(Name, VNames)
    -> ( memberchk(Name, Seen)
       -> Out = [Head | More], Seen1 = Seen
       ; relation_schema([Head | Rest], Name, Name/Arity, Specs),
         Out = [type_decl(Name, Specs), Head | More],
         Seen1 = [Name | Seen]
       )
    ; Out = [Head | More], Seen1 = Seen
    ),
    normalize_relation_value_decls(Rest, VNames, Seen1, More).

relation_schema(Decls, Name, Ref, Specs) :-
    once(member(col_type(Name/Arity, _, _), Decls)),
    Ref = Name/Arity,
    findall(col(Col, Type), member(col_type(Ref, Col, Type), Decls), Specs),
    length(Specs, Arity).

declared_column_type_name(Decls, Name) :-
    ( member(col_type(_, _, Type), Decls),
      ( Name = Type ; list_element_type_name(Type, Name) )
    ; member(sh_decl(_, Ins, Outs, _), Decls),
      ( member(col(_, Name), Ins) ; member(col(_, Name), Outs) )
    ; member(bind_decl(_, Cols), Decls),
      member(col(_, Name), Cols)
    ; member(enum_decl(_, Variants), Decls),
      enum_decl_variant_term(Variants, Variant),
      Variant =.. [_ | Fields],
      member(_:Name, Fields)
    ),
    \+ scalar_column_type(Name).

list_element_type_name(Type, Name) :-
    compound(Type),
    Type =.. [F, Element],
    memberchk(F, [list, list_entity_dense_sequence, list_interned_set,
                  list_entity_linked_sequence]),
    list_element_type_name(Element, Name).
list_element_type_name(Name, Name) :- atom(Name).

scalar_column_type(T) :- member(T, [int, text, json, bool, float]).


bind_decl_stmt(bind_decl(Name, Cols)) -->
    word(`bind`), ws, ident(Name), tok(`(`),
    decl_b_columns(Name, Specs), tok(`)`), tok(`.`),
    { specs_to_columns(Specs, Cols),
      spec_names(Specs, Names),
      record_cols(Name, Names) }.

sh_decl_stmt(sh_decl(Name, Ins, Outs, template(Template))) -->
    word(`sh`), ws, ident(Name), tok(`(`),
    decl_b_columns(Name, InSpecs), tok(`)`), ws,
    lit(`->`), tok(`(`),
    host_output_columns(Name, OutSpecs), tok(`)`), ws,
    lit(`=`), ws, template_lit(Template), tok(`.`),
    { specs_to_columns(InSpecs, Ins),
      specs_to_columns(OutSpecs, Outs),
      append(InSpecs, OutSpecs, Specs),
      spec_names(Specs, Names),
      record_cols(Name, Names),
      record_host_signature(Name, Ins, Outs) }.

sh_decl_stmt(unsupported_host_decl(Name, Cols)) -->
    word(`sh`), ws, ident(Name), tok(`(`),
    decl_b_columns(Name, Specs), tok(`)`), ws,
    lit(`=`), ws, template_lit(_), tok(`.`),
    { specs_to_columns(Specs, Cols),
      length(Cols, Arity),
      spec_names(Specs, Names),
      record_cols(Name, Names),
      record_finding(unsupported_surface(host_decl_inferred(Name/Arity))) }.

host_output_columns(Rel, Specs) --> args(typed_col(host_col_type(Rel)), Specs).

host_col_type(Rel, Col, none) -->
    coltype(W), { W \== none },
    { record_finding(unsupported_surface(column_type_wrapper(Rel, Col, W))) }.
host_col_type(_, _, Type) --> type_expr(Type).

specs_to_columns(Specs, Cols) :- maplist([column(N, T), col(N, T)]>>true, Specs, Cols).

template_lit(Template) -->
    [0'`], !,
    template_codes(Cs),
    { string_codes(Template, Cs) }.

template_codes([]) --> [0'`], !.
template_codes([0'` | Cs]) --> [0'\\, 0'`], !, template_codes(Cs).
template_codes([0'\\ | Cs]) --> [0'\\, 0'\\], !, template_codes(Cs).
template_codes([C | Cs]) --> [C], template_codes(Cs).


query_stmt(query(Atom)) -->
    lit(`?`), ws, ident(Name), tok(`(`),
    head_args(Args), tok(`)`), tok(`.`),
    { resolve_named_args(head, Name, Args, Pos),
      Atom =.. [Name | Pos] }.


match_stmt(match(Source, Arms)) -->
    word(`match`), ws, head_atom(Source), ws,
    lit(`(`), match_arms(Arms), tok(`)`), tok(`.`).

match_arms(Arms) -->
    ws, ( lit(`;`) -> ws ; [] ),
    match_arm(First),
    match_arm_tail(First, Arms).

match_arm_tail(First, Arms) -->
    ws,
    ( lit(`;`)
    -> ws, match_arm(Next),
       match_arm_tail(Next, Rest),
       { Arms = (First ; Rest) }
    ; { Arms = First }
    ).

match_arm(Arm) -->
    body(Guards), ws,
    ( lit(`|->`) -> { Arrow = (<-) } ; lit(`|+>`) -> { Arrow = (<+) } ),
    ws, head_atom(Head),
    { Arm =.. [Arrow, Head, Guards] }.

rule_stmt(Rule) -->
    head_atom(Head), ws,
    ( lit(`<-`) -> { Arrow = (<-) }, ws, body(Body)
    ; lit(`<+`) -> { Arrow = (<+) }, ws, body(Body)
    ; { Arrow = (<-), Body = true }
    ),
    tok(`.`),
    { Rule =.. [Arrow, Head, Body] }.


head_atom(Term) -->
    dotted_path(Segs), tok(`(`),
    head_args(Args), tok(`)`),
    { last(Segs, Local),
      module_path_name(Segs, Resolved),
      resolve_named_args(head, Resolved, Args, Pos),
      ( Segs = [_]
      -> Term =.. [Local | Pos]
      ; Term = rel_path(Segs, Pos)
      ) }.

head_args(Args) --> args(atom_arg, Args).

atom_arg(named(Name, Value)) -->
    ident(Name), ws,
    here([0':, Next | _]), { Next \== 0'=, Next \== 0': }, !,
    lit(`:`), ws, expr(Value).
atom_arg(pos(Value)) --> expr(Value).


resolve_named_args(_, _, [], []) :- !.
resolve_named_args(_, _, Args, Pos) :-
    ( forall(member(A, Args), A = pos(_))
    -> maplist(arg_value, Args, Pos)
    ),
    !.
resolve_named_args(Mode, Rel, Args, Pos) :-
    ( lookup_column_order(Rel, Cols)
    -> resolve_mixed_args(Mode, Rel, Args, Cols, Pos)
    ; record_finding(unsupported_surface(named_args_unresolved(Rel))),
      maplist(arg_value, Args, Pos)
    ).

arg_value(pos(V), V) :- !.
arg_value(named(_, V), V).

resolve_mixed_args(Mode, Rel, Args, Cols, Pos) :-
    length(Cols, N),
    length(Pos, N),
    validate_named_columns(Rel, Args, Cols),
    place_named(Cols, 1, Args, Pos),
    findall(Col, member(named(Col, _), Args), NamedCols),
    findall(I, ( nth1(I, Cols, Col), \+ memberchk(Col, NamedCols) ), FreeIdxs),
    findall(V, member(pos(V), Args), PosValues),
    fill_partial_slots(Mode, Rel, N, FreeIdxs, PosValues, Pos).

validate_named_columns(Rel, Args, Cols) :-
    findall(Name, member(named(Name, _), Args), Names),
    ( member(Name, Names), \+ memberchk(Name, Cols)
    -> record_finding(unsupported_surface(unknown_named_arg(Rel, Name)))
    ; select(Dup, Names, Rest), memberchk(Dup, Rest)
    -> record_finding(unsupported_surface(duplicate_named_arg(Rel, Dup)))
    ; true
    ).

place_named([], _, _, _).
place_named([Col | Cols], Idx, Args, Pos) :-
    ( member(named(Col, V), Args) -> nth1(Idx, Pos, V) ; true ),
    Idx1 is Idx + 1,
    place_named(Cols, Idx1, Args, Pos).

fill_free_slots(Is, Vs, Pos) :-
    maplist({Pos}/[I, V]>>nth1(I, Pos, V), Is, Vs).

fill_partial_slots(Mode, Rel, Arity, FreeIdxs, PosValues, Pos) :-
    length(PosValues, PosCount),
    length(FilledIdxs, PosCount),
    append(FilledIdxs, OmittedIdxs, FreeIdxs),
    fill_free_slots(FilledIdxs, PosValues, Pos),
    finish_omitted_slots(Mode, Rel/Arity, OmittedIdxs, Pos).

finish_omitted_slots(body, _, Idxs, Pos) :-
    fill_anonymous_slots(Idxs, Pos).
finish_omitted_slots(head, Ref, Idxs, Pos) :-
    ( Idxs == []
    -> true
    ; record_finding(unsupported_surface(partial_head(Ref))),
      fill_anonymous_slots(Idxs, Pos)
    ).

fill_anonymous_slots(Is, Pos) :-
    maplist({Pos}/[I]>>nth1(I, Pos, _), Is).


body(Body) -->
    ws,
    ( lit(`(`) -> body(Inner), tok(`)`)
    ; body_item(Inner)
    ),
    ws,
    ( lit(`,`) -> ws, body(Rest), { Body = (Inner, Rest) }
    ; { Body = Inner }
    ).


body_item(Item) --> cst_item(Item), !.
body_item(Item) -->
    { Name = pre, Arity = 2, Shape = rel_atom_default
    ; surface(Name/Arity, _, _, LowerRole, _),
      wrapper_lower_role(LowerRole, Shape, _)
    },
    keyword_call(Name, Inner),
    { parse_surface_wrapper(Shape, Arity, Inner, Args) },
    !,
    { Item =.. [Name | Args] }.
body_item(Name) -->
    { surface(Name/0, _, _, word(_), _), atom_codes(Name, Cs) },
    word(Cs), !.
body_item(Item) --> bind_item(Item), !.
body_item(Item) --> cmp_item(Item), !.
body_item(not(Atom)) -->
    lit(`!`), ident(Name), tok(`(`),
    args(expr, Args), tok(`)`), !,
    { Atom =.. [Name | Args] }.
body_item(Item) --> relatom_item(Item).


cst_item(cst(Path, Digest, Language, Query)) -->
    word(`cst`), tok(`(`),
    expr(Path), tok(`,`),
    expr(Digest), tok(`,`),
    ws, ident(Language), tok(`)`),
    tok(`{`),
    cst_block(Inner), here(S),
    { parse_cst_query_or_error(Inner, S, Query) }.

parse_cst_query_or_error(Codes, Rem, Query) :-
    catch(parse_cst_query(Codes, Query), _,
          ( mark(Rem), parse_failure(cst_query) )),
    !.

cst_block([]) --> [0'}], !.
cst_block(Codes) -->
    [0'"], cst_block_string(Str), !, cst_block(More),
    { append([0'" | Str], More, Codes) }.
cst_block([C | More]) --> [C], cst_block(More).

cst_block_string([0'"]) --> [0'"], !.
cst_block_string([0'\\, C | More]) --> [0'\\, C], !, cst_block_string(More).
cst_block_string([C | More]) --> [C], cst_block_string(More).

annotate_cst_item(Rule0, Vars, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    term_variables((Head, Body), RVars),
    annotate_cst_body(Body, Head, Vars, RVars, Annotated),
    Rule =.. [Op, Head, Annotated].
annotate_cst_item(match(Source, Arms), Vars, match(Source, Annotated)) :- !,
    annotate_cst_arms(Arms, Vars, Annotated).
annotate_cst_item(Item, _, Item).

annotate_cst_arms((L ; R), Vars, (AL ; AR)) :- !,
    annotate_cst_item(L, Vars, AL),
    annotate_cst_arms(R, Vars, AR).
annotate_cst_arms(Arm, Vars, Annotated) :-
    annotate_cst_item(Arm, Vars, Annotated).

annotate_cst_body((L, R), Head, Vars, RVars, (AL, AR)) :- !,
    annotate_cst_body(L, Head, Vars, RVars, AL),
    annotate_cst_body(R, Head, Vars, RVars, AR).
annotate_cst_body(cst(Path, Digest, Language, Query), Head, Vars, RVars,
                  cst(Path, Digest, Language, Query,
                      cst_bindings(Caps, Cands, RNames))) :- !,
    ts_query_capture_names(Query, Caps),
    term_variables((Path, Digest), IVars),
    cst_variable_names(RVars, Vars, RNames),
    cst_variable_names(IVars, Vars, InNames),
    cst_body_variable_names(Head, Path, Digest, Vars, InNames, Cands).
annotate_cst_body(Item, _, _, _, Item).

cst_body_variable_names(Head, Path, Digest, Vars, InNames, Names) :-
    term_variables(Head, HVars),
    term_variables((Path, Digest), IVars),
    cst_variable_names(HVars, Vars, HNames),
    cst_variable_names(IVars, Vars, InNames0),
    append(InNames, InNames0, Excluded0),
    sort(Excluded0, Excluded),
    subtract(HNames, Excluded, WithoutInputs),
    subtract(WithoutInputs, [line, end_line], Names).

cst_variable_names([], _, []).
cst_variable_names([Var | Rest], Vars, Names) :-
    ( member(Name-Candidate, Vars), Candidate == Var
    -> Names = [Name | More]
    ; Names = More
    ),
    cst_variable_names(Rest, Vars, More).


keyword_call(Keyword, Inner) -->
    { atom_codes(Keyword, Cs) },
    word(Cs), tok(`(`),
    balanced(Inner).

balanced(Inner, S0, S) :- bp(S0, 0, [], Rev, S), reverse(Rev, Inner).

bp([C | T], D, A, Out, S) :-
    ( quote_code(C) -> bp_quoted(C, T, [C | A], A1, S1), bp(S1, D, A1, Out, S)
    ; C == 0'( -> D1 is D + 1, bp(T, D1, [C | A], Out, S)
    ; C == 0') ->
        ( D == 0 -> Out = A, S = T
        ; D1 is D - 1, bp(T, D1, [C | A], Out, S)
        )
    ; bp(T, D, [C | A], Out, S)
    ).

quote_code(0'").
quote_code(0'\').

bp_quoted(Q, [C | T], A, Out, S) :-
    ( C == Q, T = [Q | T1] -> bp_quoted(Q, T1, [Q, Q | A], Out, S)
    ; C == Q -> Out = [Q | A], S = T
    ; C == 0'\\, T = [E | T1] -> bp_quoted(Q, T1, [E, 0'\\ | A], Out, S)
    ; bp_quoted(Q, T, [C | A], Out, S)
    ).

parse_full(Goal, Codes) :-
    call(Goal, Codes, Left),
    ws(Left, Left1),
    ( Left1 == [] -> true ; throw(dl_parse_error(trailing_input(Left1))) ).

rel_atom_term(Term) -->
    ident(Name), tok(`(`),
    args(expr, Args), tok(`)`),
    { Term =.. [Name | Args] }.

comma_pair(P1, P2, A, B) -->
    ws, call(P1, A), tok(`,`), ws, call(P2, B).

parse_surface_wrapper(atom_list, Arity, Codes, Atoms) :-
    parse_full(sep(rel_atom_term, Atoms), Codes),
    ( Arity == variadic -> true ; integer(Arity), length(Atoms, Arity) ).
parse_surface_wrapper(Shape, Arity, Codes, Args) :-
    wrapper_parser(Shape, Arity, Args, Goal),
    parse_full(Goal, Codes).

wrapper_parser(rel_atom, 1, [Atom], rel_atom_term(Atom)).
wrapper_parser(body_item, 1, [Item], body_item(Item)).
wrapper_parser(expr, 1, [E], expr(E)).
wrapper_parser(expr_pair, 2, [A, B], comma_pair(expr, expr, A, B)).
wrapper_parser(rel_atom_default, 2, [Atom, D],
               comma_pair(rel_atom_term, expr, Atom, D)).


bind_item(Term) -->
    expr(Lhs), ws, infix_op(bind, Op), ws, expr(Rhs),
    { Term =.. [Op, Lhs, Rhs] }.

cmp_item(Term) -->
    expr(Lhs), ws, cmp_op(Op), ws, expr(Rhs),
    { Term =.. [Op, Lhs, Rhs] }.

cmp_op(=<) --> lit(`<=`), !.
cmp_op(\==) --> lit(`!=`), !.
cmp_op(Op) --> infix_op(guard, Op), !.
cmp_op(==) --> lit(`=`).

infix_op(Axis, Op) -->
    { findall(C, surface(C/2, Axis, no_refs, infix(_), _), Cands),
      longest_first(Cands, Ordered),
      member(Op, Ordered),
      atom_codes(Op, Cs) },
    op_codes(Cs).

neg_length(Atom, NegLen) :- atom_length(Atom, Len), NegLen is -Len.
longest_first(Atoms, Sorted) :-
    map_list_to_pairs(neg_length, Atoms, Keyed),
    keysort(Keyed, Ordered),
    pairs_values(Ordered, Sorted).

op_codes([F | R]) -->
    ( { code_type(F, alpha) } -> word([F | R]) ; lit([F | R]) ).


split_probe_values(InCount, Values, Ins, Outs) :-
    length(Values, Count),
    ( Count >= InCount
    -> length(Ins, InCount), append(Ins, Outs, Values)
    ; Ins = Values, Outs = []
    ).

partition_hiv([], [], [], [], []).
partition_hiv([col(Name, _) | Cols], [V | Vs], [Role | Roles], Ids, Salts) :-
    ( Role == identity -> Ids = [V | Ids1], Salts = Salts1
    ; Role == freshness -> Salts = [salt(Name, V) | Salts1], Ids = Ids1
    ),
    partition_hiv(Cols, Vs, Roles, Ids1, Salts1).

relatom_item(Item) -->
    dotted_path(Segs),
    { last(Segs, Name), module_path_name(Segs, Resolved) },
    ws,
    ( lit(`!`) -> { Mut = true }, ws ; { Mut = false } ),
    lit(`(`), head_args(Args), tok(`)`),
    { ( Mut == true
      -> length(Args, Arity),
         record_finding(unsupported_surface(mutation(Name/Arity))),
         resolve_named_args(body, Resolved, Args, Pos),
         Item =.. [Name | Pos]
      ; resolve_named_args(body, Resolved, Args, Pos),
        ( Segs = [_] -> Item =.. [Name | Pos] ; Item = rel_path(Segs, Pos) )
      ) }.


expr(E) --> { arithmetic_tiers(Tiers) }, tier_expr(Tiers, E).

arithmetic_tiers(Tiers) :-
    findall(P, expression(_/2, arithmetic, P, _, _), Ps),
    sort(Ps, Tiers).

tier_operators(Prec, Ops) :-
    findall(Op, expression(Op/2, arithmetic, Prec, _, _), Ops0),
    longest_first(Ops0, Ops).

tier_expr([], E) --> factor(E).
tier_expr([P | Tighter], E) -->
    tier_expr(Tighter, Acc),
    { tier_operators(P, Ops) },
    tier_rest(Ops, Tighter, Acc, E).

tier_rest(Ops, Tighter, Acc, E) -->
    here(Start), ws,
    ( tier_op(Ops, Op)
    -> ws, tier_expr(Tighter, Rhs),
       { Next =.. [Op, Acc, Rhs] },
       tier_rest(Ops, Tighter, Next, E)
    ; { E = Acc }, back(Start)
    ).

tier_op([Op | Rest], Matched) -->
    { atom_codes(Op, Cs) },
    ( op_codes(Cs) -> { Matched = Op } ; tier_op(Rest, Matched) ).


factor(E) -->
    ws, here(S), { no_tagged_brace(S) },
    ( lit(`(`) -> ws, expr(E), tok(`)`)
    ; bool_lit(E) -> []
    ; float_lit(E) -> []
    ; int_lit(E) -> []
    ; atom_lit(E) -> []
    ; string_lit(E) -> []
    ; dollar_var(E)
    ; braces_term(E)
    ; list_term(E)
    ; wildcard_var(E) -> []
    ; compound_or_var(E)
    ).

no_tagged_brace(S) :-
    ( ident(Name, S, [0'{ | _])
    -> throw(unsupported_construct(tagged_brace_reserved(Name)))
    ; true
    ).

dollar_var(Var) -->
    [0'$], ident(Name),
    { hole_var(Name, Var) }.

bool_lit(bool_lit(B)) -->
    { member(B, [true, false]), atom_codes(B, Cs) }, word(Cs), !.

wildcard_var(_) --> word(`_`).

compound_or_var(E) -->
    ident(Name), here(S1), ws,
    ( peek(0'()
    -> lit(`(`), args(expr, Args), tok(`)`),
       { E =.. [Name | Args] }
    ; { get_or_make_var(Name, Rec) },
      back(S1), dot_chain(Rec, E)
    ).

dotted_path([Segment | Rest]) -->
    ident(Segment),
    ( dot_then_ident -> dotted_path(Rest) ; { Rest = [] } ).

dot_chain(Rec, Final) -->
    ( dot_then_ident
    -> ident(Field), dot_chain(dot_get(Rec, Field), Final)
    ; { Final = Rec }
    ).

dot_then_ident([0'. | S], S) :-
    S = [C | _],
    ( code_type(C, alpha) ; C == 0'_ ),
    !.


braces_term(Term) -->
    lit(`{`), !, ws,
    ( peek(0'})
    -> { Term = '{}' }
    ; { Term = '{}'(Pairs) },
      brace_pairs(Pairs)
    ),
    tok(`}`).

brace_pairs((Pair, Rest)) -->
    brace_pair(Pair), tok(`,`), !, ws,
    brace_pairs(Rest).
brace_pairs(Pair) --> brace_pair(Pair).

brace_pair(Key:Typed) -->
    brace_key(Key), tok(`:`), ws,
    expr(Value),
    ( tok(`:`) -> ws, ident(Type), { Typed = Value:Type } ; { Typed = Value } ).

brace_key('**') --> lit(`**`), !.
brace_key($(Var)) -->
    [0'$], !, ident(Name),
    { hole_var(Name, Var) }.
brace_key(Key) --> atom_lit(Key), !.
brace_key(Key) --> string_lit(Text), !, { atom_string(Key, Text) }.
brace_key(Key) --> ident(Key).


list_term(Term) -->
    lit(`[`), !, ws,
    ( lit(`...`) -> ws, expr(Element), { Term = spread(Element) }
    ; peek(0']) -> { Term = [] }
    ; sep(expr, Term)
    ),
    tok(`]`).
