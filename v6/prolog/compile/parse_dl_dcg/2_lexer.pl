cst_extra(comment, '#.*').

ws(S0, S) :-
    ws_skip(S0, S),
    ( S0 == S -> true ; mark(S) ).

ws_skip(S0, S) :-
    ( S0 = [C | S1], code_type(C, space) -> ws_skip(S1, S)
    ; S0 = [0'# | S1] -> skip_to_eol(S1, S2), ws_skip(S2, S)
    ; S = S0
    ).

skip_to_eol(S0, S) :-
    ( S0 = [0'\n | S1] -> S = S1
    ; S0 = [_ | S1] -> skip_to_eol(S1, S)
    ; S = S0
    ).

@([], S, S) :- mark(S).
@([C | Cs], S0, S) :-
    S0 = [C | Rest],
    ( @(Cs, Rest, S) -> true ; mark(S0), fail ).

~(Cs, S0, S) :-
    @(Cs, S0, S),
    \+ (S = [C | _], id_code(C)).

peek(C, S, S) :- S = [C | _], !.

% kw//1: an already-chosen atom spelled as a word terminal
kw(Word) --> { atom_codes(Word, Cs) }, ~Cs.

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
    ( S0 = [0'- | S2] -> Sign = -1 ; S2 = S0, Sign = 1 ),
    S2 = [D | _], code_type(D, digit), !,
    digits0(Ds, S2, S),
    mark(S),
    number_codes(Mag, Ds),
    Value is Sign * Mag.

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
    here(Remaining), { mark(Remaining) },
    [M], { memberchk(M, `eE`) },
    ( [S], { memberchk(S, `+-`) } -> { Sign = [S] } ; { Sign = [] } ),
    digits1(Ds),
    { append(Sign, Ds, Cs) }.


atom_lit(Atom, S0, S) :- quoted(0'\', Cs, S0, S), atom_codes(Atom, Cs).
string_lit(Str, S0, S) :- quoted(0'", Cs, S0, S), string_codes(Str, Cs).

% quoted//4 decodes escapes; an editor wants the raw span these patterns match
lex_token(string_lit/1, '"([^"\\\\]|\\\\.)*"').
lex_token(atom_lit/1, '\'([^\'\\\\]|\\\\.)*\'').
lex_token(template_lit/1, '`([^`\\\\]|\\\\.)*`').

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

% the five recognized escapes collapse to one memberchk over Source-Decoded
% pairs; the Quote-Quote row closes over the active quote character.
escape(Quote, Source, [Decoded | M], M) :-
    memberchk(Source-Decoded,
              [ 0'n - 0'\n, 0't - 0'\t, 0'r - 0'\r,
                0'\\ - 0'\\, Quote - Quote ]), !.
escape(_, Other, [0'\\, Other | M], M).


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


% A quoted target is a file; a bare ident is an executor family the registry
% rosters (use_mod, resolved in executor_modules.pl).
