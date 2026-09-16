% marble.pl : rxjs marble diagrams as a bidirectional DCG. One grammar both
% parses "ab--c|" into events and prints events back into "ab--c|". Clause
% order is the design: base cases first so generation terminates, the frame
% clause last so parsing prefers a real character.
% Run: swipl -q -l marble.pl -g go -g halt

marble(String, Events) :-
    (   var(String)
    ->  phrase(seq(0, Events), Codes), string_codes(String, Codes)
    ;   string_codes(String, Codes), phrase(seq(0, Events), Codes)
    ).

seq(Tick, [complete(Tick)]) --> "|".
seq(_, []) --> [].
seq(Tick, [at(Tick, Char) | Rest]) -->
    [Code],
    { code_type(Code, alpha), char_code(Char, Code), Next is Tick + 1 },
    seq(Next, Rest).
seq(Tick, Events) --> "-", { Next is Tick + 1 }, seq(Next, Events).

check(parse, ( marble("ab--c|", [at(0,a), at(1,b), at(4,c), complete(5)]) )).
check(print, ( marble(S, [at(0,a), at(1,b), at(4,c), complete(5)]),
               S == "ab--c|" )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
