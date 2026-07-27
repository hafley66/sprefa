% json5.pl : JSON5 on top of the bare grammar (~35 lines of delta). The
% additions, each a clause or two: // and /* */ comments live inside ws,
% trailing commas fall out of making the after-comma recursion optional,
% unquoted identifier keys, single-quoted strings, hex integers, leading and
% trailing decimal points, explicit +, Infinity and NaN.
% Run: swipl -q -l json5.pl -g go -g halt

:- use_module(library(lists)).
:- use_module(library(apply)).

json5_parse(Text, Value) :-
    ( string(Text) -> string_codes(Text, Codes) ; atom_codes(Text, Codes) ),
    phrase((ws, jvalue(Value), ws), Codes).

jvalue(N)     --> "Infinity", !, { N is inf }.
jvalue(N)     --> "-Infinity", !, { N is -inf }.
jvalue(N)     --> "NaN", !, { N is nan }.
jvalue(null)  --> "null", !.
jvalue(true)  --> "true", !.
jvalue(false) --> "false", !.
jvalue(Str)   --> quoted(Str), !.
jvalue(Num)   --> j5number(Num), !.
jvalue(List)  --> "[", !, ws, elements(List), ws, "]".
jvalue(obj(Pairs)) --> "{", ws, members(Pairs), ws, "}".

% after a comma, recursing into the SAME (empty-allowing) rule = trailing commas
elements([V | Rest]) --> jvalue(V), ws, ( ",", ws, elements(Rest) ; { Rest = [] } ).
elements([]) --> [].

members([Key-Val | Rest]) -->
    j5key(Key), ws, ":", ws, jvalue(Val), ws,
    ( ",", ws, members(Rest) ; { Rest = [] } ).
members([]) --> [].

j5key(Str) --> quoted(Str), !.
j5key(Str) --> csyms(Cs), { Cs \== [], string_codes(Str, Cs) }.
csyms([C | Cs]) --> [C], { code_type(C, csym) }, !, csyms(Cs).
csyms([]) --> [].

quoted(Str) --> "\"", !, chars_until(0'\", Cs), "\"", { string_codes(Str, Cs) }.
quoted(Str) --> "'", chars_until(39, Cs), "'", { string_codes(Str, Cs) }.
chars_until(D, [C | Cs]) --> "\\", !, escape(C), chars_until(D, Cs).
chars_until(D, [C | Cs]) --> [C], { C =\= D, C =\= 0'\\ }, !, chars_until(D, Cs).
chars_until(_, []) --> [].
escape(0'\") --> "\"".    escape(0'\\) --> "\\".    escape(0'/) --> "/".
escape(39) --> "'".       escape(8)  --> "b".       escape(12) --> "f".
escape(10) --> "n".       escape(13) --> "r".       escape(9)  --> "t".
escape(C)  --> "u", [A, B, D, E], { foldl(hexacc, [A, B, D, E], 0, C) }.
hexacc(Code, Acc, Out) :- code_type(Code, xdigit(V)), Out is Acc * 16 + V.

j5number(N) --> sign(Sign), unsigned(U), { N is Sign * U }.
sign(-1) --> "-", !.
sign(1)  --> "+", !.
sign(1)  --> [].
unsigned(N) --> ( "0x" ; "0X" ), !, hexdigits(Cs), { Cs \== [], foldl(hexacc, Cs, 0, N) }.
unsigned(N) -->
    digits0(Int), dot(Dot), digits0(FracDs), expo(Exp),
    { ( Int == [], FracDs == [] -> fail ; true ),
      ( Int == [] -> IntC = [48] ; IntC = Int ),                 % .5  -> 0.5
      ( Dot == yes, FracDs == [] -> Fx = [46, 48]                % 5.  -> 5.0
      ; Dot == yes -> Fx = [46 | FracDs]
      ; Exp \== [] -> Fx = [46, 48]                              % 1e3 -> 1.0e3
      ; Fx = [] ),
      append([IntC, Fx, Exp], Codes),
      number_codes(N, Codes) }.
hexdigits([C | Cs]) --> [C], { code_type(C, xdigit(_)) }, !, hexdigits(Cs).
hexdigits([]) --> [].
dot(yes) --> ".", !.
dot(no)  --> [].
digits0([D | Ds]) --> [D], { code_type(D, digit) }, !, digits0(Ds).
digits0([]) --> [].
expo([101 | Rest]) --> ( "e" ; "E" ), !, esign(S), digits1(Ds), { append(S, Ds, Rest) }.
expo([]) --> [].
digits1([D | Ds]) --> [D], { code_type(D, digit) }, digits0(Ds).
esign([43]) --> "+", !.
esign([45]) --> "-", !.
esign([]) --> [].

% comments are whitespace
ws --> "//", !, line_comment, ws.
ws --> "/*", !, block_comment, ws.
ws --> [C], { memberchk(C, [32, 9, 10, 13]) }, !, ws.
ws --> [].
line_comment --> [C], { C =\= 10 }, !, line_comment.
line_comment --> [].
block_comment --> "*/", !.
block_comment --> [_], !, block_comment.

check(kitchen, ( Inf is inf,
                 json5_parse("{ a: 1, /* mid */ 'b': [0x1F, .5, +2, Infinity,], } // end", V),
                 V == obj(["a"-1, "b"-[31, 0.5, 2, Inf]]) )).
check(trailing_dot, ( json5_parse("[5., 5]", V), V == [5.0, 5] )).
check(nan_parses,   ( json5_parse("NaN", V), float(V) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
