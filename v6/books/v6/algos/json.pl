% json.pl : bare JSON (ECMA-404) as one DCG. The whole spec is the ~45 lines
% between json_parse and the checks: 7 value forms, strings with escapes and
% \uXXXX, full number grammar. Objects come back as obj([Key-Value, ...]).
% (Production note: swipl ships library(http/json); this file is the spec
% made legible, and the base JSON5 borrows from.)
% Run: swipl -q -l json.pl -g go -g halt

:- use_module(library(lists)).
:- use_module(library(apply)).

json_parse(Text, Value) :-
    ( string(Text) -> string_codes(Text, Codes) ; atom_codes(Text, Codes) ),
    phrase((ws, jvalue(Value), ws), Codes).

jvalue(null)  --> "null", !.
jvalue(true)  --> "true", !.
jvalue(false) --> "false", !.
jvalue(Str)   --> jstring(Str), !.
jvalue(Num)   --> jnumber(Num), !.
jvalue(List)  --> "[", !, ws, elements(List), ws, "]".
jvalue(obj(Pairs)) --> "{", ws, members(Pairs), ws, "}".

elements([V | Rest]) --> jvalue(V), ws, ( ",", !, ws, elements1(Rest) ; { Rest = [] } ).
elements([]) --> [].
elements1([V | Rest]) --> jvalue(V), ws, ( ",", !, ws, elements1(Rest) ; { Rest = [] } ).

members([Key-Val | Rest]) -->
    jstring(Key), ws, ":", ws, jvalue(Val), ws,
    ( ",", !, ws, members1(Rest) ; { Rest = [] } ).
members([]) --> [].
members1([Key-Val | Rest]) -->
    jstring(Key), ws, ":", ws, jvalue(Val), ws,
    ( ",", !, ws, members1(Rest) ; { Rest = [] } ).

jstring(Str) --> "\"", jchars(Cs), "\"", { string_codes(Str, Cs) }.
jchars([C | Cs]) --> jchar(C), !, jchars(Cs).
jchars([]) --> [].
jchar(C) --> "\\", !, escape(C).
jchar(C) --> [C], { C =\= 0'\", C =\= 0'\\ }.
escape(0'\") --> "\"".    escape(0'\\) --> "\\".    escape(0'/) --> "/".
escape(8)  --> "b".       escape(12) --> "f".       escape(10) --> "n".
escape(13) --> "r".       escape(9)  --> "t".
escape(C)  --> "u", [A, B, D, E], { foldl(hexacc, [A, B, D, E], 0, C) }.
hexacc(Code, Acc, Out) :- code_type(Code, xdigit(V)), Out is Acc * 16 + V.

% number = -? int frac? exp?   (46 = '.', 48 = '0', 101 = 'e', 43/45 = +/-)
jnumber(N) -->
    minus(M), digits1(Int), frac(Frac), expo(Exp),
    { ( Exp \== [], Frac == [] -> Fx = [46, 48] ; Fx = Frac ),   % 1e3 -> 1.0e3
      append([M, Int, Fx, Exp], Codes),
      number_codes(N, Codes) }.
minus([45]) --> "-", !.
minus([]) --> [].
digits1([D | Ds]) --> [D], { code_type(D, digit) }, digits0(Ds).
digits0([D | Ds]) --> [D], { code_type(D, digit) }, !, digits0(Ds).
digits0([]) --> [].
frac([46 | Ds]) --> ".", !, digits1(Ds).
frac([]) --> [].
expo([101 | Rest]) --> ( "e" ; "E" ), !, esign(S), digits1(Ds), { append(S, Ds, Rest) }.
expo([]) --> [].
esign([43]) --> "+", !.
esign([45]) --> "-", !.
esign([]) --> [].

ws --> [C], { memberchk(C, [32, 9, 10, 13]) }, !, ws.
ws --> [].

check(scalars, ( json_parse("[null, true, false, 42, -3.5, 1e3]", V),
                 V == [null, true, false, 42, -3.5, 1000.0] )).
check(nested,  ( json_parse("{\"a\": [1, {\"b\": \"hi\"}]}", V),
                 V == obj(["a"-[1, obj(["b"-"hi"])]]) )).
check(escapes, ( json_parse("\"a\\n\\u0041\"", S), S == "a\nA" )).
check(strict_no_trailing_comma, ( \+ json_parse("[1,]", _) )).
check(strict_no_comments,       ( \+ json_parse("[1] // c", _) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
