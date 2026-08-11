
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

:- dynamic(finding_fact/1).
:- dynamic(rel_column_order_fact/2).
:- dynamic(host_signature_fact/3).
:- dynamic(source_statement_fact/3).
:- discontiguous body_item/5.


record_finding(F) :- assertz(finding_fact(F)).
record_column_order(Name, Cols) :-
    retractall(rel_column_order_fact(Name, _)),
    assertz(rel_column_order_fact(Name, Cols)).
lookup_column_order(Name, Cols) :- rel_column_order_fact(Name, Cols).
record_host_signature(Name, Inputs, Outputs) :-
    retractall(host_signature_fact(Name, _, _)),
    assertz(host_signature_fact(Name, Inputs, Outputs)).


parse_dl_file(FilePath, Prog, Bindings, Findings) :-
    read_file_to_codes(FilePath, Codes, []),
    parse_dl_source(FilePath, Codes, Prog, Bindings, Findings).

parse_dl_dcg_entry(Source, Codes, Prog, Bindings, Findings) :-
    parse_dl_source(Source, Codes, Prog, Bindings, Findings).

parse_dl(Codes, Prog, Bindings, Findings) :-
    parse_dl_source(none, Codes, Prog, Bindings, Findings).

parse_dl_source(_Source, Codes, _Prog, _Bindings, _Findings) :-
    var(Codes),
    !,
    throw(dl_parse_error(invalid_input, position(1, 1))).
parse_dl_source(_Source, Codes, Prog, Bindings, Findings) :-
    retractall(finding_fact(_)),
    retractall(rel_column_order_fact(_, _)),
    retractall(host_signature_fact(_, _, _)),
    retractall(source_statement_fact(_, _, _)),
    length(Codes, InputLength),
    nb_setval(parse_input_length, InputLength),
    nb_setval(parse_furthest_remaining, InputLength),
    build_line_starts(Codes),
    statements(Codes, Left, [], VarsFinal, ParsedDecls, ParsedRules, Queries),
    ( Left == []
    -> true
    ;  mark_furthest(Left), parse_failure(trailing_input)
    ),
    resolve_module_path_collisions(ParsedDecls, PathResolvedDecls),
    normalize_relation_value_decls(PathResolvedDecls, Decls),
    normalize_host_calls(Decls, ParsedRules, Rules),
    maplist(swap_pair, VarsFinal, BindingsRev),
    reverse(BindingsRev, Bindings),
    findall(F, finding_fact(F), Findings),
    ( Queries == [],
      \+ member(sh_decl(_, _, _, _), Decls),
      \+ member(bind_decl(_, _), Decls)
    -> Prog = prog(Decls, Rules)
    ; Prog = program(Decls, Rules, Queries)
    ).

parse_failure(Reason) :-
    furthest_line_col(Line, Column),
    throw(dl_parse_error(Reason, position(Line, Column))).

mark_furthest(Suffix) :-
    length(Suffix, RemainingLength),
    nb_current(parse_furthest_remaining, FurthestRemaining),
    RemainingLength < FurthestRemaining,
    !,
    nb_setval(parse_furthest_remaining, RemainingLength).
mark_furthest(_).

furthest_line_col(Line, Column) :-
    nb_getval(parse_furthest_remaining, RemainingLength),
    remaining_line_column(RemainingLength, Line, Column).

remaining_line_column(RemainingLength, Line, Column) :-
    nb_getval(parse_input_length, InputLength),
    Index is InputLength - RemainingLength,
    nb_getval(parse_line_starts, LineStarts),
    nb_getval(parse_line_count, LineCount),
    line_containing(1, LineCount, Index, LineStarts, Line),
    arg(Line, LineStarts, LineStart),
    Column is Index - LineStart + 1.

line_containing(Low, High, Index, LineStarts, Line) :-
    (   Low >= High
    ->  Line = Low
    ;   Mid is (Low + High + 1) // 2,
        arg(Mid, LineStarts, MidStart),
        (   MidStart =< Index
        ->  line_containing(Mid, High, Index, LineStarts, Line)
        ;   Before is Mid - 1,
            line_containing(Low, Before, Index, LineStarts, Line)
        )
    ).

build_line_starts(Codes) :-
    split_string(Codes, "\n", "", Lines),
    line_start_offsets(Lines, 0, Offsets),
    LineStarts =.. [line_starts | Offsets],
    length(Offsets, LineCount),
    nb_setval(parse_line_starts, LineStarts),
    nb_setval(parse_line_count, LineCount).

line_start_offsets([], _, []).
line_start_offsets([Line | Rest], Offset, [Offset | More]) :-
    string_length(Line, Length),
    Next is Offset + Length + 1,
    line_start_offsets(Rest, Next, More).

prolog:message(dl_parse_error(Reason, position(Line, Column))) -->
    [ 'parse error at line ~d, column ~d: ~w'-[Line, Column, Reason] ].

parse_dl_line_for_reason(Reason, Line) :-
    findall(Ref, reason_relation_reference(Reason, Ref), References0),
    sort(References0, References),
    ( member(Ref, References), statement_line_for_reference(rule, Ref, Line)
    -> true
    ;  member(Ref, References), statement_line_for_reference(decl, Ref, Line)
    -> true
    ).

statement_location_for_reason(Reason, Line, Column) :-
    findall(Ref, reason_relation_reference(Reason, Ref), References0),
    sort(References0, References),
    ( member(Ref, References), statement_location_for_reference(rule, Ref, Line, Column)
    ; member(Ref, References), statement_location_for_reference(decl, Ref, Line, Column) ).

reason_relation_reference(Reason, Name/Arity) :-
    sub_term(Name/Arity, Reason),
    atom(Name),
    integer(Arity).

statement_location_for_reference(rule, Reference, Line, Column) :-
    source_statement_fact(rule, Item, RemainingLength),
    statement_head_reference(Item, Reference),
    !,
    remaining_line_column(RemainingLength, Line, Column).
statement_location_for_reference(Kind, Reference, Line, Column) :-
    source_statement_fact(Kind, Item, RemainingLength),
    statement_reference(Kind, Item, Reference),
    !,
    remaining_line_column(RemainingLength, Line, Column).

statement_head_reference((Head <- _), Name/Arity) :-
    !,
    functor(Head, Name, Arity).
statement_head_reference((Head <+ _), Name/Arity) :-
    functor(Head, Name, Arity).

statement_line_for_reference(Kind, Reference, Line) :-
    statement_location_for_reference(Kind, Reference, Line, _Column).

statement_reference(rule, Rule, Name/Arity) :-
    sub_term(Term, Rule),
    compound(Term),
    functor(Term, Name, Arity),
    atom(Name),
    !.
statement_reference(decl, Declarations, Reference) :-
    member(Declaration, Declarations),
    declaration_source_ref(Declaration, Reference),
    !.

record_statement_source_lines(decl_list, Declarations, RemainingLength) :-
    !,
    assertz(source_statement_fact(decl, Declarations, RemainingLength)).
record_statement_source_lines(rule, Rule, RemainingLength) :-
    !,
    assertz(source_statement_fact(rule, Rule, RemainingLength)).
record_statement_source_lines(_, _, _).

declaration_source_ref(kind(Ref, _), Ref).
declaration_source_ref(keyed(Ref, _), Ref).
declaration_source_ref(keep(Ref, _), Ref).
declaration_source_ref(type_decl(Name, Specs), Name/Arity) :-
    length(Specs, Arity).
declaration_source_ref(col_type(Ref, _, _), Ref).
declaration_source_ref(sh_decl(Name, Inputs, Outputs, _), Name/Arity) :-
    append(Inputs, Outputs, Columns),
    length(Columns, Arity).

swap_pair(Name-Var, Name=Var).


normalize_host_calls(_, [], []).
normalize_host_calls(Decls, [Rule | Rest], [Normalized | More]) :-
    normalize_host_rule(Decls, Rule, Normalized),
    normalize_host_calls(Decls, Rest, More).

normalize_host_rule(Decls, (Head <- Body), (Head <- Normalized)) :-
    !,
    normalize_host_body(Decls, Body, Normalized).
normalize_host_rule(Decls, (Head <+ Body), (Head <+ Normalized)) :-
    !,
    normalize_host_body(Decls, Body, Normalized).
normalize_host_rule(Decls, match(Source, Arms), match(Source, Normalized)) :-
    !,
    normalize_host_arms(Decls, Arms, Normalized).
normalize_host_rule(_, Rule, Rule).

normalize_host_arms(Decls, (Left ; Right), (LeftNormalized ; RightNormalized)) :-
    !,
    normalize_host_arms(Decls, Left, LeftNormalized),
    normalize_host_arms(Decls, Right, RightNormalized).
normalize_host_arms(Decls, Arm, Normalized) :-
    normalize_host_rule(Decls, Arm, Normalized).

normalize_host_body(Decls, (Left, Right), (LeftNormalized, RightNormalized)) :-
    !,
    normalize_host_body(Decls, Left, LeftNormalized),
    normalize_host_body(Decls, Right, RightNormalized).
normalize_host_body(_, Probe, Probe) :-
    Probe = probe(_, _, _, _),
    !.
normalize_host_body(_, Item, Item) :-
    body_surface_for_term(Item, _, _, _, _, _),
    !.
normalize_host_body(Decls, Atom,
                    probe(Name, InputValues, OutputValues, Salts)) :-
    compound(Atom),
    functor(Atom, Name, _),
    member(sh_decl(Name, Inputs, _, _), Decls),
    !,
    Atom =.. [_ | Values],
    length(Inputs, InputCount),
    split_probe_values(InputCount, Values, SurfaceInputValues, OutputValues),
    host_input_roles(Name, Inputs, Roles),
    partition_host_input_values(Inputs, SurfaceInputValues, Roles,
                                InputValues, Salts).
normalize_host_body(_, Item, Item).


statements(S0, S, Vars0, Vars, Decls, Rules, Queries) :-
    skip_ws(S0, S1),
    mark_furthest(S1),
    ( S1 == []
    -> Decls = [], Rules = [], Queries = [], Vars = Vars0, S = S1
    ; ( statement(Kind, Item, Vars0, Vars1, S1, S2)
      -> length(S1, StatementRemaining),
         record_statement_source_lines(Kind, Item, StatementRemaining)
      ;  mark_furthest(S1), parse_failure(statement)
      ),
      statements(S2, S, Vars1, Vars, Decls1, Rules1, Queries1),
      ( Kind == decl_list -> append(Item, Decls1, Decls), Rules = Rules1, Queries = Queries1
      ; Kind == rule -> Decls = Decls1, Rules = [Item | Rules1], Queries = Queries1
      ; Kind == query -> Decls = Decls1, Rules = Rules1, Queries = [Item | Queries1]
      ; Kind == skip -> Decls = Decls1, Rules = Rules1, Queries = Queries1
      )
    ).


skip_ws(S0, S) :-
    ( S0 = [C | S1], (code_type(C, space) ; C == 0'\n ; C == 0'\r)
    -> skip_ws(S1, S)
    ; S0 = [0'# | S1]
    -> skip_to_eol(S1, S2), skip_ws(S2, S)
    ; S = S0, mark_furthest(S0)
    ).

skip_to_eol(S0, S) :-
    ( S0 = [C | S1], C \== 0'\n -> skip_to_eol(S1, S)
    ; S0 = [0'\n | S1] -> S = S1
    ; S = S0
    ).

ws0(S0, S) :- skip_ws(S0, S).


lit_dcg([], S, S) :-
    mark_furthest(S).
lit_dcg([Code | Codes], Suffix, S) :-
    Suffix = [Code | Rest],
    (   lit_dcg(Codes, Rest, S)
    ->  true
    ;   mark_furthest(Suffix),
        fail
    ).

word(Codes, S0, S) :-
    lit_dcg(Codes, S0, S1),
    \+ (S1 = [C | _], (code_type(C, alnum) ; C == 0'_)),
    S = S1.

peek(C, S, S) :- S = [C | _], !.


ident(Name, S0, S) :-
    mark_furthest(S0),
    S0 = [C0 | Rest0],
    ( code_type(C0, alpha) ; C0 == 0'_ ), !,
    ident_rest_codes(Rest0, RestCodes, S),
    atom_codes(Name, [C0 | RestCodes]).

ident_rest_codes([C | Cs], [C | More], S) :-
    ( code_type(C, alnum) ; C == 0'_ ), !,
    ident_rest_codes(Cs, More, S).
ident_rest_codes(S, [], S).


integer_lit(Value, S0, S) :-
    mark_furthest(S0),
    ( S0 = [0'- | S1] -> Neg = true, S2 = S1 ; Neg = false, S2 = S0 ),
    S2 = [D0 | _], code_type(D0, digit), !,
    digits(S2, Digits, S),
    number_codes(Magnitude, Digits),
    ( Neg == true -> Value is -Magnitude ; Value = Magnitude ).

digits([C | Cs], [C | More], S) :- code_type(C, digit), !, digits(Cs, More, S).
digits(S, [], S) :- mark_furthest(S).

float_lit(Value, S0, S) :-
    mark_furthest(S0),
    phrase(float_codes(Codes), S0, S),
    number_codes(Value, Codes),
    float(Value),
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]).

float_codes(Codes) -->
    optional_minus(Sign),
    digits_codes(Int),
    float_tail(Tail),
    { append([Sign, Int, Tail], Codes) }.

optional_minus([0'-]) --> `-`, !.
optional_minus([]) --> [].

digits_codes([Digit | Rest]) -->
    [Digit], { code_type(Digit, digit) }, !,
    digits_codes_rest(Rest).

digits_codes_rest([Digit | Rest]) -->
    [Digit], { code_type(Digit, digit) }, !,
    digits_codes_rest(Rest).
digits_codes_rest([]) --> [].

float_tail(Codes) -->
    `.`, digits_codes(Fraction), exponent_codes(Exponent),
    { append([[0'.], Fraction, Exponent], Codes) }.
float_tail(Codes) -->
    exponent_codes_required(Codes).

exponent_codes(Codes) --> exponent_codes_required(Codes), !.
exponent_codes([]) --> [].

exponent_codes_required([Marker | Codes]) -->
    [Marker], { memberchk(Marker, [0'e, 0'E]) },
    exponent_sign(Sign),
    digits_codes(Digits),
    { append(Sign, Digits, Codes) }.

exponent_sign([Sign]) --> [Sign], { memberchk(Sign, [0'+, 0'-]) }, !.
exponent_sign([]) --> [].


quoted_atom_lit(Atom, S0, S) :-
    mark_furthest(S0),
    S0 = [0'\' | S1], !,
    quoted_chars(0'\', S1, Codes, S),
    atom_codes(Atom, Codes).

string_lit(Str, S0, S) :-
    mark_furthest(S0),
    S0 = [0'" | S1], !,
    quoted_chars(0'", S1, Codes, S),
    string_codes(Str, Codes).

quoted_chars(Quote, [Quote, Quote | Rest], [Quote | More], S) :- !,
    mark_furthest([Quote, Quote | Rest]),
    quoted_chars(Quote, Rest, More, S).
quoted_chars(Quote, [Quote | Rest], [], Rest) :- !.
quoted_chars(Quote, [0'\\, Esc | Rest], Codes, S) :- !,
    mark_furthest([0'\\, Esc | Rest]),
    escape_codes(Quote, Esc, Codes, More),
    quoted_chars(Quote, Rest, More, S).
quoted_chars(Quote, [C | Rest], [C | More], S) :-
    quoted_chars(Quote, Rest, More, S).

escape_codes(_, 0'n,  [0'\n  | More], More) :- !.
escape_codes(_, 0't,  [0'\t  | More], More) :- !.
escape_codes(_, 0'r,  [0'\r  | More], More) :- !.
escape_codes(_, 0'\\, [0'\\  | More], More) :- !.
escape_codes(Quote, Quote, [Quote | More], More) :- !.
escape_codes(_, Other, [0'\\, Other | More], More).


get_or_make_var(Name, Vars0, Var, Vars) :-
    ( memberchk(Name-Existing, Vars0)
    -> Var = Existing, Vars = Vars0
    ; Vars = [Name-Var | Vars0]
    ).


use_item(Item, S0, S) :-
    skip_ws(S0, S1),
    use_visibility(Visibility, S1, S2),
    skip_ws(S2, S3),
    string_lit(Text, S3, S4),
    skip_ws(S4, S5),
    (   use_alias_clause(Alias, S5, S6)
    ->  use_item_term(Visibility, Text, Alias, Item), S7 = S6
    ;   use_item_term(Visibility, Text, none, Item), S7 = S5
    ),
    skip_ws(S7, S8),
    S8 = [0'. | S].

use_visibility(private, S0, S) :- word(`use`, S0, S).
use_visibility(public, S0, S) :-
    word(`pub`, S0, S1),
    skip_ws(S1, S2),
    word(`use`, S2, S).

use_item_term(private, Text, none, use(Text)).
use_item_term(private, Text, Alias, use(Text, Alias)).
use_item_term(public, Text, none, pub_use(Text)).
use_item_term(public, Text, Alias, pub_use(Text, Alias)).

use_alias_clause(Alias, S0, S) :-
    word(`as`, S0, S1),
    skip_ws(S1, S2),
    ident(Alias, S2, S).


statement(Kind, Item, Vars0, Vars, S0, S) :-
    skip_ws(S0, S1),
    ( bind_decl_stmt(Item0, S1, S2) -> Kind = decl_list, Item = [Item0], Vars = Vars0, S = S2
    ; decl_a_stmt(Item0, S1, S2) -> Kind = decl_list, Item = Item0, Vars = Vars0, S = S2
    ; decl_b_stmt(Item0, S1, S2) -> Kind = decl_list, Item = Item0, Vars = Vars0, S = S2
    ; sh_decl_stmt(Item0, S1, S2) -> Kind = decl_list, Item = [Item0], Vars = Vars0, S = S2
    ; query_stmt(Item0, Vars0, Vars1, S1, S2) -> Kind = query, Item = Item0, Vars = Vars1, S = S2
    ; match_stmt(Item0, Vars0, Vars1, S1, S2) -> Kind = rule, annotate_cst_item(Item0, Vars1, Item), Vars = Vars1, S = S2
    ; rule_stmt(Item0, Vars0, Vars1, S1, S2) -> Kind = rule, annotate_cst_item(Item0, Vars1, Item), Vars = Vars1, S = S2
    ).


decl_a_stmt([enum_decl(Name, VariantTerms)], S0, S) :-
    word(`rel`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    enum_decl_variants(VariantTerms, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S),
    record_enum_column_orders(Name, VariantTerms).

decl_a_stmt(DeclList, S0, S) :-
    word(`rel`, S0, S1),
    ws0(S1, S2),
    dotted_path(Segments, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_a_columns(Specs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    length(Specs, Arity),
    module_path_name(Segments, Name),
    Ref = Name/Arity,
    column_spec_names(Specs, Cols),
    record_column_order(Name, Cols),
    ws0(S8, S9),
    typed_decl_entries(Ref, Specs, TypedDecls),
    decl_a_modifiers(Ref, Modifiers, S9, S10),
    module_path_decls(Segments, Ref, PathDecls),
    column_less_decls(Ref, Specs, Modifiers, UnitDecls),
    append([TypedDecls, Modifiers, PathDecls, UnitDecls], DeclList),
    ws0(S10, S11),
    lit_dcg(`.`, S11, S).

column_less_decls(_, Specs, _, []) :- Specs \== [], !.
column_less_decls(Ref, _, Modifiers, Decls) :-
    (   memberchk(kind(Ref, _), Modifiers)
    ->  Decls = []
    ;   Decls = [kind(Ref, set)]
    ).

module_path_name(Segments, Name) :-
    atomic_list_concat(Segments, '__', Name).

module_path_decls([_Single], _, []) :- !.
module_path_decls(Segments, Ref, [rel_path_decl(Ref, Segments)]).

decl_a_modifiers(Ref, [Decl | Rest], S0, S) :-
    ( word(`log`, S0, S1) -> Decl = kind(Ref, log)
    ; keep_clause(Policy, S0, S1) -> Decl = keep(Ref, Policy)
    ; key_clause(Positions, S0, S1) -> Decl = keyed(Ref, Positions)
    ), !,
    ws0(S1, S2),
    decl_a_modifiers(Ref, Rest, S2, S).
decl_a_modifiers(Ref, Decls, S0, S) :-
    word(`set`, S0, S1), !,
    record_finding(unsupported_surface(removed_word(set))),
    ws0(S1, S2),
    decl_a_modifiers(Ref, Decls, S2, S).
decl_a_modifiers(_, [], S, S).

decl_a_columns([], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
decl_a_columns([Spec | Rest], S0, S) :-
    decl_a_column(Spec, S0, S1), ws0(S1, S2),
    ( lit_dcg(`,`, S2, S3) -> decl_a_columns(Rest, S3, S) ; Rest = [], S = S2 ).

decl_a_column(column(Name, Type), S0, S) :-
    ws0(S0, S1), ident(Name, S1, S2), ws0(S2, S3),
    ( lit_dcg(`:`, S3, S4)
    -> ws0(S4, S5), typed_column_type(Type, S5, S)
    ; Type = none, S = S3
    ).

typed_column_type(Type, S0, S) :-
    typed_column_type_base(BaseType, S0, S1),
    ( lit_dcg(`?`, S1, S2)
    -> Type = option(BaseType), S = S2
    ; Type = BaseType, S = S1
    ).

typed_column_type_base(int, S0, S) :- word(`int`, S0, S), !.
typed_column_type_base(text, S0, S) :- word(`text`, S0, S), !.
typed_column_type_base(json, S0, S) :- word(`json`, S0, S), !.
typed_column_type_base(bool, S0, S) :- word(`bool`, S0, S), !.
typed_column_type_base(option(Element), S0, S) :-
    word(`option`, S0, S1), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    typed_column_type(Element, S4, S5), ws0(S5, S6),
    lit_dcg(`)`, S6, S).
typed_column_type_base(json_list(Element), S0, S) :-
    word(`json_list`, S0, S1), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    typed_column_type(Element, S4, S5), ws0(S5, S6),
    lit_dcg(`)`, S6, S).
typed_column_type_base(list(Element), S0, S) :-
    word(`list`, S0, S1), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    typed_column_type(Element, S4, S5), ws0(S5, S6),
    lit_dcg(`)`, S6, S).
typed_column_type_base(list_entity_dense_sequence(Element), S0, S) :-
    word(`list_entity_dense_sequence`, S0, S1), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    typed_column_type(Element, S4, S5), ws0(S5, S6),
    lit_dcg(`)`, S6, S).
typed_column_type_base(list_interned_set(Element), S0, S) :-
    word(`list_interned_set`, S0, S1), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    typed_column_type(Element, S4, S5), ws0(S5, S6),
    lit_dcg(`)`, S6, S).
typed_column_type_base(list_entity_linked_sequence(Element), S0, S) :-
    word(`list_entity_linked_sequence`, S0, S1), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    typed_column_type(Element, S4, S5), ws0(S5, S6),
    lit_dcg(`)`, S6, S).
typed_column_type_base(float, S0, S) :- word(`float`, S0, S), !.
typed_column_type_base(Name, S0, S) :- ident(Name, S0, S).

enum_decl_variants((First ; Rest), S0, S) :-
    enum_decl_variant(First, S0, S1),
    ws0(S1, S2),
    lit_dcg(`;`, S2, S3),
    ws0(S3, S4),
    enum_decl_variants(Rest, S4, S).
enum_decl_variants(Variant, S0, S) :-
    enum_decl_variant(Variant, S0, S).

enum_decl_variant(Variant, S0, S) :-
    ws0(S0, S1),
    ident(VariantName, S1, S2),
    ws0(S2, S3),
    lit_dcg(`(`, S3, S4),
    enum_decl_columns(Fields, S4, S5),
    ws0(S5, S6),
    lit_dcg(`)`, S6, S),
    Variant =.. [VariantName | Fields].

enum_decl_columns([], S0, S) :-
    ws0(S0, S1),
    peek(0'), S1, S),
    !.
enum_decl_columns([Field | Rest], S0, S) :-
    ws0(S0, S1),
    ident(ColumnName, S1, S2),
    ws0(S2, S3),
    lit_dcg(`:`, S3, S4),
    ws0(S4, S5),
    ident(TypeName, S5, S6),
    Field =.. [':', ColumnName, TypeName],
    ws0(S6, S7),
    ( lit_dcg(`,`, S7, S8)
    -> enum_decl_columns(Rest, S8, S)
    ; Rest = [], S = S7
    ).

record_enum_column_orders(RelName, VariantTerms) :-
    tag_rel_name(RelName, TagName),
    record_column_order(TagName, [id, tag]),
    forall(enum_decl_variant_term(VariantTerms, Variant),
           record_enum_variant_column_order(RelName, Variant)).

enum_decl_variant_term((Left ; Right), Variant) :-
    !,
    ( enum_decl_variant_term(Left, Variant)
    ; enum_decl_variant_term(Right, Variant)
    ).
enum_decl_variant_term(Variant, Variant).

record_enum_variant_column_order(RelName, Variant) :-
    Variant =.. [VariantName | Fields],
    maplist(enum_field_column_name, Fields, ColumnNames),
    atomic_list_concat([RelName, VariantName], '_', VariantRelName),
    record_column_order(VariantRelName, [id | ColumnNames]).

enum_field_column_name(Field, ColumnName) :-
    Field =.. [':', ColumnName, _].

tag_rel_name(RelName, TagName) :-
    atomic_list_concat([RelName, tag], '_', TagName).

column_spec_names([], []).
column_spec_names([column(Name, _) | Rest], [Name | More]) :-
    column_spec_names(Rest, More).

typed_decl_entries(_, [], []).
typed_decl_entries(Ref, [column(Column, Type) | Rest], Decls) :-
    ( Type == none
    -> Decls = More
    ; Decls = [col_type(Ref, Column, Type) | More]
    ),
    typed_decl_entries(Ref, Rest, More).

keep_clause(Policy, S0, S) :-
    word(`keep`, S0, S1), ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    ( word(`all`, S4, S5) -> Policy = all
    ; word(`count`, S4, S5a), ws0(S5a, S5b), lit_dcg(`(`, S5b, S5c),
      ws0(S5c, S5d), integer_lit(N, S5d, S5e), ws0(S5e, S5f), lit_dcg(`)`, S5f, S5)
    -> Policy = count(N)
    ),
    ws0(S5, S6), lit_dcg(`)`, S6, S).

key_clause(Positions, S0, S) :-
    word(`key`, S0, S1), ws0(S1, S2), lit_dcg(`(`, S2, S3),
    int_list(Positions, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S).

int_list([N | Rest], S0, S) :-
    ws0(S0, S1), integer_lit(N, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> int_list(Rest, S4, S) ; Rest = [], S = S3 ).


decl_b_stmt(DeclList, S0, S) :-
    word(`rel`, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`(`, S2, S3a)
    -> ws0(S3a, S3b), integer_lit(Retention, S3b, S3c), ws0(S3c, S3d),
       lit_dcg(`)`, S3d, S3), HasRetention = true
    ; S3 = S2, HasRetention = false
    ),
    ws0(S3, S4),
    ident(Name, S4, S5),
    ws0(S5, S6),
    lit_dcg(`(`, S6, S7),
    decl_b_columns(Name, Specs, S7, S8),
    ws0(S8, S9),
    lit_dcg(`)`, S9, S10),
    ws0(S10, S11),
    lit_dcg(`.`, S11, S),
    length(Specs, Arity),
    Ref = Name/Arity,
    column_spec_names(Specs, Cols),
    record_column_order(Name, Cols),
    ( HasRetention == true -> record_finding(unsupported_surface(retention_marker(Ref, Retention))) ; true ),
    typed_decl_entries(Ref, Specs, DeclList).


decl_b_columns(_, [], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
decl_b_columns(RelName, [column(ColName, Type) | Rest], S0, S) :-
    ws0(S0, S1), ident(ColName, S1, S2), ws0(S2, S3), lit_dcg(`:`, S3, S4),
    ws0(S4, S5), decl_b_column_type(RelName, ColName, Type, S5, S6), ws0(S6, S7),
    ( lit_dcg(`,`, S7, S8) -> decl_b_columns(RelName, Rest, S8, S) ; Rest = [], S = S7 ).

decl_b_column_type(RelName, ColName, none, S0, S) :-
    coltype(Wrapper, S0, S),
    Wrapper \== none,
    !,
    record_finding(unsupported_surface(column_type_wrapper(RelName, ColName, Wrapper))).
decl_b_column_type(_, _, Type, S0, S) :-
    typed_column_type(Type, S0, S).

coltype(Wrapper, S0, S) :-
    ( word(`Key`, S0, S1) -> Wrapper = 'Key'
    ; word(`Min`, S0, S1) -> Wrapper = 'Min'
    ; word(`Max`, S0, S1) -> Wrapper = 'Max'
    ), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4), ident(_, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S).
coltype(none, S0, S) :- ident(_, S0, S).


resolve_module_path_collisions(Decls0, Decls) :-
    findall(Ref-Segments, member(rel_path_decl(Ref, Segments), Decls0),
            PathEntries),
    (   PathEntries == []
    ->  Decls = Decls0
    ;   reserved_rel_names(Decls0, PathEntries, Reserved),
        foldl(disambiguate_module_path(Reserved), PathEntries, Decls0, Decls)
    ).

disambiguate_module_path(Reserved, Name/Arity-Segments, Decls0, Decls) :-
    (   memberchk(Name, Reserved)
    ->  variant_sha1(Segments, Sha),
        sub_atom(Sha, 0, 16, _, Digest),
        atomic_list_concat([Name, '__', Digest], Digested),
        (   lookup_column_order(Name, Cols)
        ->  record_column_order(Digested, Cols)
        ;   true
        ),
        maplist(rename_decl_ref(Name/Arity, Digested/Arity), Decls0, Decls)
    ;   Decls = Decls0
    ).

rename_decl_ref(Old, New, col_type(Old, Column, Type),
                col_type(New, Column, Type)) :- !.
rename_decl_ref(Old, New, kind(Old, Kind), kind(New, Kind)) :- !.
rename_decl_ref(Old, New, keep(Old, Policy), keep(New, Policy)) :- !.
rename_decl_ref(Old, New, keyed(Old, Positions), keyed(New, Positions)) :- !.
rename_decl_ref(Old, New, rel_path_decl(Old, Segments),
                rel_path_decl(New, Segments)) :- !.
rename_decl_ref(_, _, Decl, Decl).

reserved_rel_names(Decls, PathEntries, Reserved) :-
    findall(Ref, member(Ref-_, PathEntries), PathRefs),
    findall(Name,
            ( declared_rel_name(Decls, Ref, Name),
              \+ memberchk(Ref, PathRefs)
            ; minted_rel_name(Decls, Name)
            ),
            Names),
    sort(Names, Reserved).

declared_rel_name(Decls, Name/Arity, Name) :-
    member(col_type(Name/Arity, _, _), Decls).
declared_rel_name(Decls, Name/Arity, Name) :- member(kind(Name/Arity, _), Decls).
declared_rel_name(Decls, Name/Arity, Name) :- member(keyed(Name/Arity, _), Decls).
declared_rel_name(Decls, Name/Arity, Name) :- member(keep(Name/Arity, _), Decls).
declared_rel_name(Decls, enum_decl(Name), Name) :-
    member(enum_decl(Name, _), Decls).

minted_rel_name(Decls, Companion) :-
    member(col_type(Parent/_, Column, option(_)), Decls),
    atomic_list_concat([Parent, '__', Column], Companion).
minted_rel_name(Decls, EnumName) :-
    member(col_type(_, _, option(Element)), Decls),
    atom(Element),
    atomic_list_concat(['__opt_', Element], EnumName).
minted_rel_name(Decls, VariantRelName) :-
    member(enum_decl(RelName, VariantTerms), Decls),
    enum_decl_variant_term(VariantTerms, Variant),
    Variant =.. [VariantName | _],
    atomic_list_concat([RelName, VariantName], '_', VariantRelName).
minted_rel_name(Decls, TagName) :-
    member(enum_decl(RelName, _), Decls),
    tag_rel_name(RelName, TagName).


normalize_relation_value_decls(Decls0, Decls) :-
    findall(Name,
            ( declared_column_type_name(Decls0, Name),
              relation_schema(Decls0, Name, _, _) ),
            Names0),
    sort(Names0, ValueRelationNames),
    normalize_relation_value_decls(Decls0, ValueRelationNames, [], Decls).

normalize_relation_value_decls([], _, _, []).
normalize_relation_value_decls([Head | Rest],
                               ValueNames, Seen,
                               [type_decl(Name, Specs), Head | More]) :-
    Head = col_type(Name/Arity, _, _),
    memberchk(Name, ValueNames),
    \+ memberchk(Name, Seen),
    !,
    relation_schema([Head | Rest],
                    Name, Name/Arity, Specs),
    normalize_relation_value_decls(Rest, ValueNames, [Name | Seen], More).
normalize_relation_value_decls([Head | Rest],
                               ValueNames, Seen, [Head | More]) :-
    Head = col_type(Name/_, _, _),
    memberchk(Name, ValueNames),
    !,
    normalize_relation_value_decls(Rest, ValueNames, Seen, More).
normalize_relation_value_decls([Decl | Rest], ValueNames, Seen,
                               [Decl | More]) :-
    normalize_relation_value_decls(Rest, ValueNames, Seen, More).

relation_schema(Decls, Name, Ref, Specs) :-
    once(member(col_type(Name/Arity, _, _), Decls)),
    Ref = Name/Arity,
    findall(col(Column, Type),
            member(col_type(Ref, Column, Type), Decls),
            Specs),
    length(Specs, Arity).

declared_column_type_name(Decls, Name) :-
    member(col_type(_, _, Name), Decls),
    \+ scalar_column_type(Name).
declared_column_type_name(Decls, Name) :-
    member(col_type(_, _, Type), Decls),
    list_element_type_name(Type, Name),
    \+ scalar_column_type(Name).
declared_column_type_name(Decls, Name) :-
    member(sh_decl(_, Inputs, Outputs, _), Decls),
    ( member(col(_, Name), Inputs) ; member(col(_, Name), Outputs) ),
    \+ scalar_column_type(Name).
declared_column_type_name(Decls, Name) :-
    member(bind_decl(_, Columns), Decls),
    member(col(_, Name), Columns),
    \+ scalar_column_type(Name).
declared_column_type_name(Decls, Name) :-
    member(enum_decl(_, VariantTerms), Decls),
    enum_decl_variant_term(VariantTerms, Variant),
    Variant =.. [_ | VariantFields],
    member(VariantField, VariantFields),
    VariantField =.. [':', _, Name],
    \+ scalar_column_type(Name).

list_element_type_name(list(Element), Name) :-
    list_element_type_name(Element, Name).
list_element_type_name(list_entity_dense_sequence(Element), Name) :-
    list_element_type_name(Element, Name).
list_element_type_name(list_interned_set(Element), Name) :-
    list_element_type_name(Element, Name).
list_element_type_name(list_entity_linked_sequence(Element), Name) :-
    list_element_type_name(Element, Name).
list_element_type_name(Name, Name) :- atom(Name).

scalar_column_type(int).
scalar_column_type(text).
scalar_column_type(json).
scalar_column_type(bool).
scalar_column_type(float).

bind_decl_stmt(bind_decl(Name, Columns), S0, S) :-
    word(`bind`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_b_columns(Name, Specs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S),
    specs_to_columns(Specs, Columns),
    column_spec_names(Specs, Names),
    record_column_order(Name, Names).

sh_decl_stmt(sh_decl(Name, Inputs, Outputs, template(Template)), S0, S) :-
    word(`sh`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_b_columns(Name, InputSpecs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`->`, S9, S10),
    ws0(S10, S11),
    lit_dcg(`(`, S11, S12),
    host_output_columns(Name, OutputSpecs, S12, S13),
    ws0(S13, S14),
    lit_dcg(`)`, S14, S15),
    ws0(S15, S16),
    lit_dcg(`=`, S16, S17),
    ws0(S17, S18),
    template_lit(Template, S18, S19),
    ws0(S19, S20),
    lit_dcg(`.`, S20, S),
    specs_to_columns(InputSpecs, Inputs),
    specs_to_columns(OutputSpecs, Outputs),
    append(InputSpecs, OutputSpecs, Specs),
    column_spec_names(Specs, Names),
    record_column_order(Name, Names),
    record_host_signature(Name, Inputs, Outputs).

sh_decl_stmt(unsupported_host_decl(Name, Columns), S0, S) :-
    word(`sh`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_b_columns(Name, Specs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`=`, S9, S10),
    ws0(S10, S11),
    template_lit(_, S11, S12),
    ws0(S12, S13),
    lit_dcg(`.`, S13, S),
    specs_to_columns(Specs, Columns),
    length(Columns, Arity),
    column_spec_names(Specs, Names),
    record_column_order(Name, Names),
    record_finding(unsupported_surface(host_decl_inferred(Name/Arity))).

host_output_columns(_, [], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
host_output_columns(RelName, [column(ColName, Type) | Rest], S0, S) :-
    ws0(S0, S1), ident(ColName, S1, S2), ws0(S2, S3), lit_dcg(`:`, S3, S4),
    ws0(S4, S5), host_output_column_type(RelName, ColName, Type, S5, S6),
    ws0(S6, S7),
    ( lit_dcg(`,`, S7, S8)
    -> host_output_columns(RelName, Rest, S8, S)
    ; Rest = [], S = S7
    ).

host_output_column_type(RelName, ColName, none, S0, S) :-
    coltype(Wrapper, S0, S),
    Wrapper \== none,
    record_finding(
      unsupported_surface(column_type_wrapper(RelName, ColName, Wrapper))).
host_output_column_type(_, _, Type, S0, S) :-
    typed_column_type(Type, S0, S).

specs_to_columns([], []).
specs_to_columns([column(Name, Type) | Rest], [col(Name, Type) | Columns]) :-
    specs_to_columns(Rest, Columns).

template_lit(Template, S0, S) :-
    S0 = [0'` | S1], !,
    template_codes(S1, Codes, S),
    string_codes(Template, Codes).

template_codes([0'` | S], [], S) :- !.
template_codes([0'\\, 0'` | Rest], [0'` | Codes], S) :- !,
    template_codes(Rest, Codes, S).
template_codes([0'\\, 0'\\ | Rest], [0'\\ | Codes], S) :- !,
    template_codes(Rest, Codes, S).
template_codes([Code | Rest], [Code | Codes], S) :-
    template_codes(Rest, Codes, S).


query_stmt(query(Atom), Vars0, Vars, S0, S) :-
    lit_dcg(`?`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    head_args(Args, Vars0, Vars, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S),
    resolve_named_args(head, Name, Args, Positional),
    Atom =.. [Name | Positional].


match_stmt(match(SourceAtom, Arms), Vars0, Vars, S0, S) :-
    word(`match`, S0, S1),
    ws0(S1, S2),
    head_atom(SourceAtom, Vars0, Vars1, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    match_arm_list(Arms, Vars1, Vars, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S).

match_arm_list(Arms, Vars0, Vars, S0, S) :-
    ws0(S0, S00),
    ( lit_dcg(`;`, S00, S01)
    -> ws0(S01, SArm)
    ; SArm = S00
    ),
    match_arm(First, Vars0, Vars1, SArm, S1),
    match_arm_tail(First, Arms, Vars1, Vars, S1, S).

match_arm_tail(First, Arms, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    ( lit_dcg(`;`, S1, S2)
    -> ws0(S2, S3),
       match_arm(Next, Vars0, Vars1, S3, S4),
       match_arm_tail(Next, Rest, Vars1, Vars, S4, S),
       Arms = (First ; Rest)
    ; Arms = First,
      Vars = Vars0,
      S = S1
    ).

match_arm(Arm, Vars0, Vars, S0, S) :-
    body(Guards, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`|->`, S2, S3)
    -> Arrow = (<-)
    ; lit_dcg(`|+>`, S2, S3)
    -> Arrow = (<+)
    ),
    ws0(S3, S4),
    head_atom(Head, Vars1, Vars, S4, S),
    ( Arrow == (<-)
    -> Arm = (Head <- Guards)
    ; Arm = (Head <+ Guards)
    ).

rule_stmt(Rule, Vars0, Vars, S0, S) :-
    head_atom(Head, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`<-`, S2, S3) -> Arrow = (<-), ws0(S3, S4), body(Body, Vars1, Vars, S4, S5)
    ; lit_dcg(`<+`, S2, S3) -> Arrow = (<+), ws0(S3, S4), body(Body, Vars1, Vars, S4, S5)
    ; Arrow = (<-), Body = true, Vars = Vars1, S5 = S2
    ),
    ws0(S5, S6),
    lit_dcg(`.`, S6, S),
    ( Arrow == (<-) -> Rule = (Head <- Body) ; Rule = (Head <+ Body) ).


head_atom(Term, Vars0, Vars, S0, S) :-
    dotted_path(Segments, S0, S1),
    ws0(S1, S2),
    lit_dcg(`(`, S2, S3),
    head_args(Args, Vars0, Vars, S3, S4),
    ws0(S4, S5),
    lit_dcg(`)`, S5, S),
    last(Segments, LocalName),
    module_path_name(Segments, ResolvedName),
    resolve_named_args(head, ResolvedName, Args, PositionalArgs),
    path_atom_term(Segments, LocalName, PositionalArgs, Term).

path_atom_term([_Single], LocalName, Args, Term) :- !, Term =.. [LocalName | Args].
path_atom_term(Segments, _LocalName, Args, rel_path(Segments, Args)).

head_args([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
head_args([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), atom_arg(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> head_args(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).


atom_arg(named(Name, Value), Vars0, Vars, S0, S) :-
    ident(Name, S0, S1), ws0(S1, S2),
    S2 = [0': , Next | _], Next \== 0'=, Next \== 0':, !,
    lit_dcg(`:`, S2, S3), ws0(S3, S4), expr(Value, Vars0, Vars, S4, S).
atom_arg(pos(Value), Vars0, Vars, S0, S) :-
    expr(Value, Vars0, Vars, S0, S).


resolve_named_args(_, _, [], []) :- !.
resolve_named_args(_, _, Args, Positional) :-
    ( forall(member(A, Args), A = pos(_))
    -> maplist(arg_value, Args, Positional)
    ),
    !.
resolve_named_args(Mode, RelName, Args, Positional) :-
    ( lookup_column_order(RelName, Cols)
    -> resolve_mixed_args(Mode, RelName, Args, Cols, Positional)
    ; record_finding(unsupported_surface(named_args_unresolved(RelName))),
      maplist(arg_value, Args, Positional)
    ).

arg_value(pos(V), V) :- !.
arg_value(named(_, V), V).

resolve_mixed_args(Mode, RelName, Args, Cols, Positional) :-
    length(Cols, N),
    length(Positional, N),
    validate_named_columns(RelName, Args, Cols),
    place_named(Cols, 1, Args, Positional),
    findall(ColName, member(named(ColName, _), Args), NamedCols),
    findall(Idx, ( nth1(Idx, Cols, ColName), \+ memberchk(ColName, NamedCols) ), FreeIdxs),
    findall(V, member(pos(V), Args), PosValues),
    fill_partial_slots(Mode, RelName, N, FreeIdxs, PosValues, Positional).

validate_named_columns(RelName, Args, Cols) :-
    findall(Name, member(named(Name, _), Args), Names),
    ( member(Name, Names), \+ memberchk(Name, Cols)
    -> record_finding(unsupported_surface(unknown_named_arg(RelName, Name)))
    ; duplicate_name(Names, Duplicate)
    -> record_finding(unsupported_surface(duplicate_named_arg(RelName, Duplicate)))
    ; true
    ).

duplicate_name(Names, Name) :-
    select(Name, Names, Rest),
    memberchk(Name, Rest),
    !.

place_named([], _, _, _).
place_named([ColName | Cols], Idx, Args, Positional) :-
    ( member(named(ColName, V), Args) -> nth1(Idx, Positional, V) ; true ),
    Idx1 is Idx + 1,
    place_named(Cols, Idx1, Args, Positional).

fill_free_slots([], [], _).
fill_free_slots([Idx | Idxs], [V | Vs], Positional) :-
    nth1(Idx, Positional, V),
    fill_free_slots(Idxs, Vs, Positional).

fill_partial_slots(Mode, RelName, Arity, FreeIdxs, PosValues, Positional) :-
    length(PosValues, PosCount),
    length(FilledIdxs, PosCount),
    append(FilledIdxs, OmittedIdxs, FreeIdxs),
    fill_free_slots(FilledIdxs, PosValues, Positional),
    finish_omitted_slots(Mode, RelName/Arity, OmittedIdxs, Positional).

finish_omitted_slots(body, _, Idxs, Positional) :-
    fill_anonymous_slots(Idxs, Positional).
finish_omitted_slots(head, Ref, Idxs, Positional) :-
    ( Idxs == []
    -> true
    ; record_finding(unsupported_surface(partial_head(Ref))),
      fill_anonymous_slots(Idxs, Positional)
    ).

fill_anonymous_slots([], _).
fill_anonymous_slots([Idx | Rest], Positional) :-
    nth1(Idx, Positional, _),
    fill_anonymous_slots(Rest, Positional).


body(Body, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    ( lit_dcg(`(`, S1, S2)
    -> body(Inner, Vars0, Vars1, S2, S3), ws0(S3, S4), lit_dcg(`)`, S4, S5),
       ws0(S5, S6),
       ( lit_dcg(`,`, S6, S7) -> ws0(S7, S8), body(Rest, Vars1, Vars, S8, S), Body = (Inner, Rest)
       ; Body = Inner, Vars = Vars1, S = S6
       )
    ; body_item(Item, Vars0, Vars1, S1, S2), ws0(S2, S3),
      ( lit_dcg(`,`, S3, S4) -> ws0(S4, S5), body(Rest, Vars1, Vars, S5, S), Body = (Item, Rest)
      ; Body = Item, Vars = Vars1, S = S3
      )
    ).


body_item(Item, Vars0, Vars, S0, S) :-
    cst_item(Item, Vars0, Vars, S0, S),
    !.
body_item(pre(Atom, Seed), Vars0, Vars, S0, S) :-
    keyword_call(pre, InnerCodes, S0, S),
    parse_surface_wrapper(rel_atom_default, 2, InnerCodes, [Atom, Seed],
                          Vars0, Vars),
    !.
body_item(Item, Vars0, Vars, S0, S) :-
    surface(Name/Arity, _, AnalyzeRole, LowerRole, Status),
    wrapper_lower_role(LowerRole, Shape, _),
    keyword_call(Name, InnerCodes, S0, S),
    parse_surface_wrapper(Shape, Arity, InnerCodes, Args, Vars0, Vars),
    !,
    build_surface_item(Name, AnalyzeRole, Status, Args, Item).

cst_item(cst(Path, Digest, Language, Query), Vars0, Vars, S0, S) :-
    word(`cst`, S0, S1),
    ws0(S1, S2), lit_dcg(`(`, S2, S3),
    expr(Path, Vars0, Vars1, S3, S4),
    ws0(S4, S5), lit_dcg(`,`, S5, S6),
    expr(Digest, Vars1, Vars2, S6, S7),
    ws0(S7, S8), lit_dcg(`,`, S8, S9),
    ws0(S9, S10), ident(Language, S10, S11),
    ws0(S11, S12), lit_dcg(`)`, S12, S13),
    ws0(S13, S14), lit_dcg(`{`, S14, S15),
    cst_block_codes(S15, InnerCodes, S16),
    parse_cst_query_or_error(InnerCodes, S16, Query),
    S = S16,
    Vars = Vars2.

parse_cst_query_or_error(Codes, Remaining, Query) :-
    catch(parse_cst_query(Codes, Query), _,
          ( mark_furthest(Remaining), parse_failure(cst_query) )),
    !.

cst_block_codes([0'} | Rest], [], Rest) :- !.
cst_block_codes([0'" | Rest], Codes, S) :-
    cst_block_string(Rest, StringCodes, S1),
    cst_block_codes(S1, MoreCodes, S),
    append([0'" | StringCodes], MoreCodes, Codes),
    !.
cst_block_codes([Code | Rest], [Code | More], S) :-
    cst_block_codes(Rest, More, S).

cst_block_string([0'" | Rest], [0'"], Rest) :- !.
cst_block_string([0'\\, Code | Rest], [0'\\, Code | More], S) :-
    !,
    cst_block_string(Rest, More, S).
cst_block_string([Code | Rest], [Code | More], S) :-
    cst_block_string(Rest, More, S).

annotate_cst_item((Head <- Body), Vars, (Head <- Annotated)) :-
    !,
    term_variables((Head, Body), RuleVariables),
    annotate_cst_body(Body, Head, Vars, RuleVariables, Annotated).
annotate_cst_item((Head <+ Body), Vars, (Head <+ Annotated)) :-
    !,
    term_variables((Head, Body), RuleVariables),
    annotate_cst_body(Body, Head, Vars, RuleVariables, Annotated).
annotate_cst_item(match(Source, Arms), Vars, match(Source, AnnotatedArms)) :-
    !,
    annotate_cst_arms(Arms, Vars, AnnotatedArms).
annotate_cst_item(Item, _, Item).

annotate_cst_arms((Left ; Right), Vars, (AnnotatedLeft ; AnnotatedRight)) :-
    !,
    annotate_cst_item(Left, Vars, AnnotatedLeft),
    annotate_cst_arms(Right, Vars, AnnotatedRight).
annotate_cst_arms(Arm, Vars, Annotated) :-
    annotate_cst_item(Arm, Vars, Annotated).

annotate_cst_body((Left, Right), Head, Vars, RuleVariables,
                  (AnnotatedLeft, AnnotatedRight)) :-
    !,
    annotate_cst_body(Left, Head, Vars, RuleVariables, AnnotatedLeft),
    annotate_cst_body(Right, Head, Vars, RuleVariables, AnnotatedRight).
annotate_cst_body(cst(Path, Digest, Language, Query), Head, Vars,
                  RuleVariables,
                  cst(Path, Digest, Language, Query,
                      cst_bindings(CaptureNames, CandidateNames, RuleNames))) :-
    !,
    ts_query_capture_names(Query, CaptureNames),
    term_variables((Path, Digest), InputVariables),
    cst_variable_names(RuleVariables, Vars, RuleNames),
    cst_variable_names(InputVariables, Vars, InputNames),
    cst_body_variable_names(Head, Path, Digest, Vars, InputNames,
                            CandidateNames).
annotate_cst_body(Item, _, _, _, Item).

cst_body_variable_names(Head, Path, Digest, Vars, InputNames, Names) :-
    term_variables(Head, HeadVariables),
    term_variables((Path, Digest), InputVariables),
    cst_variable_names(HeadVariables, Vars, HeadNames),
    cst_variable_names(InputVariables, Vars, InputNames0),
    append(InputNames, InputNames0, ExcludedNames0),
    sort(ExcludedNames0, ExcludedNames),
    subtract(HeadNames, ExcludedNames, WithoutInputs),
    subtract(WithoutInputs, [line, end_line], Names).

cst_variable_names([], _, []).
cst_variable_names([Variable | Rest], Vars, Names) :-
    ( cst_variable_name(Variable, Vars, Name) -> Names = [Name | More] ; Names = More ),
    cst_variable_names(Rest, Vars, More).

cst_variable_name(Variable, [Name-Candidate | _], Name) :-
    Variable == Candidate,
    !.
cst_variable_name(Variable, [_ | Rest], Name) :-
    cst_variable_name(Variable, Rest, Name).
body_item(Item, Vars, Vars, S0, S) :-
    surface(Name/0, _, _, word(_), _),
    atom_codes(Name, NameCodes),
    word(NameCodes, S0, S), !,
    Item = Name.
body_item(Item, Vars0, Vars, S0, S) :-
    bind_item(Item, Vars0, Vars, S0, S), !.
body_item(Item, Vars0, Vars, S0, S) :-
    comparison_item(Item, Vars0, Vars, S0, S), !.
body_item(not(Atom), Vars0, Vars, S0, S) :-
    lit_dcg(`!`, S0, S1), ident(Name, S1, S2), ws0(S2, S3), lit_dcg(`(`, S3, S4),
    args_positional(Args, Vars0, Vars, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S), !,
    Atom =.. [Name | Args].
body_item(Item, Vars0, Vars, S0, S) :-
    relatom_item(Item, Vars0, Vars, S0, S).

keyword_call(Keyword, InnerCodes, S0, S) :-
    atom_codes(Keyword, KeywordCodes),
    word(KeywordCodes, S0, S1),
    ws0(S1, S2),
    lit_dcg(`(`, S2, S3),
    balanced_parens(S3, InnerCodes, S).

balanced_parens(S0, Inner, S) :-
    balanced_parens_(S0, 0, [], RevInner, S),
    reverse(RevInner, Inner).

balanced_parens_([Quote | Rest], Depth, Acc, Out, S) :-
    quote_code(Quote), !,
    balanced_parens_quoted(Quote, Rest, [Quote | Acc], Acc1, S1),
    balanced_parens_(S1, Depth, Acc1, Out, S).
balanced_parens_([0'( | Rest], Depth, Acc, Out, S) :- !,
    Depth1 is Depth + 1, balanced_parens_(Rest, Depth1, [0'( | Acc], Out, S).
balanced_parens_([0') | Rest], 0, Acc, Acc, Rest) :- !.
balanced_parens_([0') | Rest], Depth, Acc, Out, S) :- !,
    Depth1 is Depth - 1, balanced_parens_(Rest, Depth1, [0') | Acc], Out, S).
balanced_parens_([C | Rest], Depth, Acc, Out, S) :- !,
    balanced_parens_(Rest, Depth, [C | Acc], Out, S).

quote_code(0'").
quote_code(0'\').

balanced_parens_quoted(Quote, [Quote, Quote | Rest], Acc, Out, S) :- !,
    balanced_parens_quoted(Quote, Rest, [Quote, Quote | Acc], Out, S).
balanced_parens_quoted(Quote, [Quote | Rest], Acc, [Quote | Acc], Rest) :- !.
balanced_parens_quoted(Quote, [0'\\, Esc | Rest], Acc, Out, S) :- !,
    balanced_parens_quoted(Quote, Rest, [Esc, 0'\\ | Acc], Out, S).
balanced_parens_quoted(Quote, [Code | Rest], Acc, Out, S) :-
    balanced_parens_quoted(Quote, Rest, [Code | Acc], Out, S).

parse_full(Goal, Codes) :-
    call(Goal, Codes, Left),
    skip_ws(Left, Left2),
    ( Left2 == [] -> true ; throw(dl_parse_error(trailing_input(Left2))) ).

rel_atom_term(Term, Vars0, Vars, S0, S) :-
    ident(Name, S0, S1), ws0(S1, S2), lit_dcg(`(`, S2, S3),
    args_positional(Args, Vars0, Vars, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S),
    Term =.. [Name | Args].

parse_two_args(Codes, A, B, Vars0, Vars) :-
    skip_ws(Codes, Codes1),
    expr(A, Vars0, Vars1, Codes1, Rest1),
    skip_ws(Rest1, Rest2),
    lit_dcg(`,`, Rest2, Rest3),
    skip_ws(Rest3, Rest4),
    expr(B, Vars1, Vars, Rest4, Rest5),
    skip_ws(Rest5, Rest6),
    ( Rest6 == [] -> true ; throw(dl_parse_error(trailing_input(Rest6))) ).

parse_atom_list(Codes, Atoms, Vars0, Vars) :-
    parse_full(atom_list(Atoms, Vars0, Vars), Codes).

atom_list([Atom | Rest], Vars0, Vars, S0, S) :-
    rel_atom_term(Atom, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`,`, S2, S3)
    -> ws0(S3, S4), atom_list(Rest, Vars1, Vars, S4, S)
    ; Rest = [], Vars = Vars1, S = S2
    ).

combine_body([Atom], Atom) :- !.
combine_body([Left | Rest], Body) :-
    combine_body(Rest, Right), Body = (Left, Right).

parse_surface_wrapper(rel_atom, 1, Codes, [Atom], Vars0, Vars) :-
    parse_full(rel_atom_term(Atom, Vars0, Vars), Codes).
parse_surface_wrapper(atom_list, Arity, Codes, Atoms, Vars0, Vars) :-
    parse_atom_list(Codes, Atoms, Vars0, Vars),
    surface_arity_matches(Arity, Atoms).
parse_surface_wrapper(body_item, 1, Codes, [Inner], Vars0, Vars) :-
    parse_full(body_item(Inner, Vars0, Vars), Codes).
parse_surface_wrapper(expr, 1, Codes, [Expr], Vars0, Vars) :-
    parse_full(expr(Expr, Vars0, Vars), Codes).
parse_surface_wrapper(expr_pair, 2, Codes, [Left, Right], Vars0, Vars) :-
    parse_two_args(Codes, Left, Right, Vars0, Vars).
parse_surface_wrapper(rel_atom_default, 2, Codes, [Atom, Default], Vars0, Vars) :-
    parse_full(rel_atom_default_args(Atom, Default, Vars0, Vars), Codes).

rel_atom_default_args(Atom, Default, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    rel_atom_term(Atom, Vars0, Vars1, S1, S2),
    ws0(S2, S3),
    lit_dcg(`,`, S3, S4),
    ws0(S4, S5),
    expr(Default, Vars1, Vars, S5, S).

surface_arity_matches(variadic, _).
surface_arity_matches(Arity, Args) :- integer(Arity), length(Args, Arity).

build_surface_item(Name, _, _, Args, Item) :-
    Item =.. [Name | Args].

args_positional([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
args_positional([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), expr(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> args_positional(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).


bind_item(BindTerm, Vars0, Vars, S0, S) :-
    expr(Lhs, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    registered_infix_op(bind, Op, S2, S3),
    ws0(S3, S4),
    expr(Rhs, Vars1, Vars, S4, S),
    BindTerm =.. [Op, Lhs, Rhs].


comparison_item(Term, Vars0, Vars, S0, S) :-
    expr(Lhs, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
comp_op(OpAtom, S2, S3),
    ws0(S3, S4),
    expr(Rhs, Vars1, Vars, S4, S),
    Term =.. [OpAtom, Lhs, Rhs].

comp_op(=<, S0, S) :- lit_dcg(`<=`, S0, S), !.
comp_op(\==, S0, S) :- lit_dcg(`!=`, S0, S), !.
comp_op(Op, S0, S) :- registered_infix_op(guard, Op, S0, S), !.
comp_op(==, S0, S) :- lit_dcg(`=`, S0, S), !.

registered_infix_op(Axis, Op, S0, S) :-
    findall(NegLength-(Codes-Candidate),
            ( surface(Candidate/2, Axis, no_refs, infix(_), _),
              atom_codes(Candidate, Codes),
              length(Codes, Length),
              NegLength is -Length
            ),
            Candidates),
    keysort(Candidates, Ordered),
    member(_-(Codes-Op), Ordered),
    operator_codes(Codes, S0, S).

operator_codes(Codes, S0, S) :-
    Codes = [First | _],
    ( code_type(First, alpha)
    -> word(Codes, S0, S)
    ; lit_dcg(Codes, S0, S)
    ).


split_probe_values(InputCount, Values, InputValues, OutputValues) :-
    length(Values, ValueCount),
    ( ValueCount >= InputCount
    -> length(InputValues, InputCount),
       append(InputValues, OutputValues, Values)
    ; InputValues = Values,
      OutputValues = []
    ).

partition_host_input_values(Columns, Values, Roles, IdentityValues, Salts) :-
    ( same_length(Columns, Values)
    -> partition_host_input_values_(Columns, Values, Roles,
                                    IdentityValues, Salts)
    ; IdentityValues = Values,
      Salts = []
    ).

partition_host_input_values_([], [], [], [], []).
partition_host_input_values_([col(Name, _) | Columns], [Value | Values],
                             [Role | Roles], IdentityValues, Salts) :-
    ( Role == identity
    -> IdentityValues = [Value | IdentityRest],
       Salts = SaltRest
    ; Role == freshness
    -> Salts = [salt(Name, Value) | SaltRest],
       IdentityValues = IdentityRest
    ),
    partition_host_input_values_(Columns, Values, Roles,
                                 IdentityRest, SaltRest).

relatom_item(Item, Vars0, Vars, S0, S) :-
    dotted_path(Segments, S0, S1), last(Segments, Name),
    module_path_name(Segments, ResolvedName), ws0(S1, S2),
    ( peek(0'!, S2, S2)
    -> lit_dcg(`!`, S2, S2a), ws0(S2a, S3), lit_dcg(`(`, S3, S4),
       head_args(Args, Vars0, Vars, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S),
       length(Args, Arity), record_finding(unsupported_surface(mutation(Name/Arity))),
       resolve_named_args(body, ResolvedName, Args, Positional),
       Item =.. [Name | Positional]
    ; lit_dcg(`(`, S2, S3),
      head_args(Args, Vars0, Vars1, S3, S4), ws0(S4, S5),
      lit_dcg(`)`, S5, S6),
      resolve_named_args(body, ResolvedName, Args, Positional),
      ( Segments = [_Single]
      -> host_or_relation_item(Name, Positional, Item, Vars1, Vars, S6, S)
      ;  Item = rel_path(Segments, Positional), Vars = Vars1, S = S6
      )
    ).

host_or_relation_item(Name, Values, Item, Vars0, Vars, S0, S) :-
    Vars = Vars0,
    S = S0,
    Item =.. [Name | Values].


expr(E, Vars0, Vars, S0, S) :-
    arithmetic_tiers(Tiers),
    tier_expr(Tiers, E, Vars0, Vars, S0, S).

arithmetic_tiers(Tiers) :-
    findall(Precedence, expression(_/2, arithmetic, Precedence, _, _),
            Precedences),
    sort(Precedences, Tiers).

tier_operators(Precedence, Operators) :-
    findall(NegLength-Operator,
            ( expression(Operator/2, arithmetic, Precedence, _, _),
              atom_length(Operator, Length),
              NegLength is -Length
            ),
            Candidates),
    keysort(Candidates, Ordered),
    findall(Operator, member(_-Operator, Ordered), Operators).

tier_expr([], E, Vars0, Vars, S0, S) :-
    factor(E, Vars0, Vars, S0, S).
tier_expr([Precedence | Tighter], E, Vars0, Vars, S0, S) :-
    tier_expr(Tighter, Acc, Vars0, Vars1, S0, S1),
    tier_operators(Precedence, Operators),
    tier_expr_rest(Operators, Tighter, Acc, E, Vars1, Vars, S1, S).

tier_expr_rest(Operators, Tighter, Acc, E, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    ( tier_operator(Operators, Operator, S1, S2)
    -> ws0(S2, S3), tier_expr(Tighter, Rhs, Vars0, Vars1, S3, S4),
       Next =.. [Operator, Acc, Rhs],
       tier_expr_rest(Operators, Tighter, Next, E, Vars1, Vars, S4, S)
    ; E = Acc, Vars = Vars0, S = S0
    ).

tier_operator([Operator | Rest], Matched, S0, S) :-
    atom_codes(Operator, Codes),
    ( operator_codes(Codes, S0, S1)
    -> Matched = Operator, S = S1
    ; tier_operator(Rest, Matched, S0, S)
    ).


factor(E, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    refuse_tagged_brace(S1),
    ( lit_dcg(`(`, S1, S2)
    -> ws0(S2, S3), expr(E, Vars0, Vars, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S)
    ; bool_lit(E, S1, S) -> Vars = Vars0
    ; float_lit(E, S1, S) -> Vars = Vars0
    ; integer_lit(E, S1, S) -> Vars = Vars0
    ; quoted_atom_lit(E, S1, S) -> Vars = Vars0
    ; string_lit(E, S1, S) -> Vars = Vars0
    ; dollar_var(E, Vars0, Vars, S1, S)
    ; braces_term(E, Vars0, Vars, S1, S)
    ; list_term(E, Vars0, Vars, S1, S)
    ; wildcard_var(E, S1, S) -> Vars = Vars0
    ; compound_or_var(E, Vars0, Vars, S1, S)
    ).

refuse_tagged_brace(S) :-
    (   tagged_brace_name(S, Name)
    ->  throw(unsupported_construct(tagged_brace_reserved(Name)))
    ;   true
    ).

tagged_brace_name(S, Name) :-
    ident(Name, S, Rest),
    Rest = [0'{ | _].

dollar_var(Var, Vars0, Vars, S0, S) :-
    S0 = [0'$ | S1],
    ident(Name, S1, S),
    hole_var(Name, Vars0, Var, Vars).

hole_var('_', Vars, _Fresh, Vars) :- !.
hole_var(Name, Vars0, Var, Vars) :- get_or_make_var(Name, Vars0, Var, Vars).

bool_lit(bool_lit(true), S0, S) :- word(`true`, S0, S), !.
bool_lit(bool_lit(false), S0, S) :- word(`false`, S0, S).

wildcard_var(Var, S0, S) :-
    lit_dcg(`_`, S0, S1),
    \+ (S1 = [C | _], (code_type(C, alnum) ; C == 0'_)),
    Var = _,
    S = S1.

compound_or_var(E, Vars0, Vars, S0, S) :-
    ident(Name, S0, S1),
    ws0(S1, S2),
    ( peek(0'(, S2, S2)
    -> lit_dcg(`(`, S2, S3), call_args(Args, Vars0, Vars, S3, S4), ws0(S4, S5),
       lit_dcg(`)`, S5, S), E =.. [Name | Args]
    ; get_or_make_var(Name, Vars0, Receiver, Vars),
      dot_chain(Receiver, S1, S, E)
    ).

dotted_path([Segment | Rest], S0, S) :-
    ident(Segment, S0, S1),
    (   dot_then_ident(S1, S2)
    ->  dotted_path(Rest, S2, S)
    ;   Rest = [], S = S1
    ).

dot_chain(Receiver, S0, S, Final) :-
    ( dot_then_ident(S0, S1)
    -> ident(Field, S1, S2),
       dot_chain(dot_get(Receiver, Field), S2, S, Final)
    ; Final = Receiver, S = S0
    ).

dot_then_ident([0'. | S1], S1) :-
    S1 = [C | _],
    ( code_type(C, alpha) ; C == 0'_ ),
    !.

call_args([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
call_args([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), expr(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> call_args(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).


braces_term(Term, Vars0, Vars, S0, S) :-
    lit_dcg(`{`, S0, S1), !,
    ws0(S1, S2),
    (   peek(0'}, S2, S2)
    ->  Term = '{}', Vars = Vars0, S3 = S2
    ;   Term = '{}'(Pairs),
        brace_pairs(Pairs, Vars0, Vars, S2, S3)
    ),
    ws0(S3, S4), lit_dcg(`}`, S4, S).

brace_pairs((Pair, Rest), Vars0, Vars, S0, S) :-
    brace_pair(Pair, Vars0, Vars1, S0, S1), ws0(S1, S2),
    lit_dcg(`,`, S2, S3), !, ws0(S3, S4),
    brace_pairs(Rest, Vars1, Vars, S4, S).
brace_pairs(Pair, Vars0, Vars, S0, S) :-
    brace_pair(Pair, Vars0, Vars, S0, S).

brace_pair(Key:Typed, Vars0, Vars, S0, S) :-
    brace_key(Key, Vars0, Vars1, S0, S1), ws0(S1, S2),
    lit_dcg(`:`, S2, S3), ws0(S3, S4),
    expr(Value, Vars1, Vars, S4, S5),
    brace_value_type(Value, Typed, S5, S).

brace_value_type(Value, Value : Type, S0, S) :-
    ws0(S0, S1), lit_dcg(`:`, S1, S2), !, ws0(S2, S3),
    ident(Type, S3, S).
brace_value_type(Value, Value, S, S).

brace_key('**', Vars, Vars, S0, S) :- lit_dcg(`**`, S0, S), !.
brace_key($(Var), Vars0, Vars, S0, S) :-
    S0 = [0'$ | S1], !,
    ident(Name, S1, S),
    hole_var(Name, Vars0, Var, Vars).
brace_key(Key, Vars, Vars, S0, S) :- quoted_atom_lit(Key, S0, S), !.
brace_key(Key, Vars, Vars, S0, S) :- string_lit(Text, S0, S), !, atom_string(Key, Text).
brace_key(Key, Vars, Vars, S0, S) :- ident(Key, S0, S).


list_term(Term, Vars0, Vars, S0, S) :-
    lit_dcg(`[`, S0, S1), !,
    ws0(S1, S2),
    ( lit_dcg(`...`, S2, S3)
    -> ws0(S3, S4), expr(Element, Vars0, Vars, S4, S5),
       Term = spread(Element), S6 = S5
    ; peek(0'], S2, S2) -> Term = [], Vars = Vars0, S6 = S2
    ; list_items(Term, Vars0, Vars, S2, S6)
    ),
    ws0(S6, S7), lit_dcg(`]`, S7, S).

list_items([Item | Rest], Vars0, Vars, S0, S) :-
    expr(Item, Vars0, Vars1, S0, S1), ws0(S1, S2),
    ( lit_dcg(`,`, S2, S3) -> ws0(S3, S4), list_items(Rest, Vars1, Vars, S4, S)
    ; Rest = [], Vars = Vars1, S = S2
    ).
