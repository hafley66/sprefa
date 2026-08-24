% swipl -g main -t halt predmap.pl <file.pl> > <name>.predmap.json
% calls[] is functor-name matching, args included: it orients, it is not exact.

:- use_module(library(apply)).
:- use_module(library(lists)).
:- use_module(library(json)).

main :-
    current_prolog_flag(argv, Argv),
    last(Argv, File),
    map_file(File, Dict),
    json_write_dict(current_output, Dict, [width(0)]),
    nl.

map_file(File, json{ file: File, lines: Lines, terms: Terms,
                     predicates: Preds, directives: Dirs }) :-
    read_file_to_string(File, Str, [encoding(utf8)]),
    string_codes(Str, Codes),
    newline_table(Codes, NL),
    functor(NL, _, NLCount),
    Lines is NLCount + 1,
    open_string(Str, Stream),
    read_terms(Stream, NL, NLCount, 0, Terms),
    close(Stream),
    include(is_directive_row, Terms, Dirs),
    include(is_clause_row, Terms, Clauses),
    collect_predicates(Clauses, Preds).

is_directive_row(Row) :- get_dict(kind, Row, directive).
is_clause_row(Row) :- get_dict(kind, Row, K), memberchk(K, [clause, dcg]).

% Nth argument = character offset of the Nth newline, so offset -> line is a
% binary search rather than a scan per term.
newline_table(Codes, NL) :-
    newline_offsets(Codes, 0, Offsets),
    NL =.. [nl|Offsets].

newline_offsets([], _, []).
newline_offsets([C|Cs], I, Out) :-
    I1 is I + 1,
    ( C =:= 0'\n -> Out = [I|Rest] ; Out = Rest ),
    newline_offsets(Cs, I1, Rest).

offset_line(_, 0, _, 1) :- !.
offset_line(NL, Count, Off, Line) :-
    search_line(NL, 1, Count, Off, 0, Below),
    Line is Below + 1.

search_line(_, Lo, Hi, _, Acc, Acc) :- Lo > Hi, !.
search_line(NL, Lo, Hi, Off, Acc, Out) :-
    Mid is (Lo + Hi) // 2,
    arg(Mid, NL, MidOff),
    (   MidOff < Off
    ->  Lo1 is Mid + 1,
        search_line(NL, Lo1, Hi, Off, Mid, Out)
    ;   Hi1 is Mid - 1,
        search_line(NL, Lo, Hi1, Off, Acc, Out)
    ).

read_terms(Stream, NL, NLCount, Index, Rows) :-
    read_term(Stream, Term,
              [ subterm_positions(Pos), variable_names(_),
                syntax_errors(error) ]),
    (   Term == end_of_file
    ->  Rows = []
    ;   pos_range(Pos, From, To),
        offset_line(NL, NLCount, From, Start),
        To1 is max(From, To - 1),
        offset_line(NL, NLCount, To1, End),
        classify(Term, Kind, Name, Arity),
        row(Index, Kind, Name, Arity, Start, End, Term, Row),
        run_directive(Term),
        Index1 is Index + 1,
        Rows = [Row|Rest],
        read_terms(Stream, NL, NLCount, Index1, Rest)
    ).

row(Index, directive, Name, Arity, Start, End, Term, Row) :- !,
    Term = (:- Body),
    term_to_atom(Body, Text0),
    truncate_atom(Text0, 120, Text),
    Row = json{ index: Index, kind: directive, name: Name, arity: Arity,
                start: Start, end: End, text: Text }.
row(Index, Kind, Name, Arity, Start, End, Term, Row) :-
    body_of(Term, Body),
    body_calls(Body, Calls),
    Row = json{ index: Index, kind: Kind, name: Name, arity: Arity,
                start: Start, end: End, calls: Calls }.

truncate_atom(A, Max, Out) :-
    atom_length(A, Len),
    (   Len =< Max
    ->  Out = A
    ;   sub_atom(A, 0, Max, _, P), atom_concat(P, '...', Out)
    ).

pos_range(From-To, From, To) :- !.
pos_range(term_position(F, T, _, _, _), F, T) :- !.
pos_range(parentheses_term_position(F, T, _), F, T) :- !.
pos_range(list_position(F, T, _, _), F, T) :- !.
pos_range(brace_position(F, T, _), F, T) :- !.
pos_range(string_position(F, T), F, T) :- !.
pos_range(dict_position(F, T, _, _, _), F, T) :- !.
pos_range(_, 0, 0).

classify((:- D), directive, Name, Arity) :- !, head_key(D, Name, Arity).
classify((H --> _), dcg, Name, Arity) :- !,
    dcg_head(H, H1), head_key(H1, Name, A0), Arity is A0 + 2.
classify((H :- _), clause, Name, Arity) :- !, head_key(H, Name, Arity).
classify(H, clause, Name, Arity) :- head_key(H, Name, Arity).

dcg_head((H, _), H) :- !.
dcg_head(H, H).

head_key(_:H, Name, Arity) :- !, head_key(H, Name, Arity).
head_key(H, Name, Arity) :- atom(H), !, Name = H, Arity = 0.
head_key(H, Name, Arity) :- compound(H), !, functor(H, Name, Arity).
head_key(_, '<nonhead>', 0).

body_of((_ --> B), B) :- !.
body_of((_ :- B), B) :- !.
body_of(_, true).

% The file's later terms do not parse without its own op and flag directives.
run_directive((:- op(P, T, N))) :- !, catch(op(P, T, N), _, true).
run_directive((:- set_prolog_flag(F, V))) :- !,
    catch(set_prolog_flag(F, V), _, true).
run_directive(_).

body_calls(Body, Calls) :-
    findall(Key, sub_functor(Body, Key), Keys0),
    sort(Keys0, Calls).

sub_functor(T, _) :- var(T), !, fail.
sub_functor(T, Key) :-
    compound(T), !,
    functor(T, Name, Arity),
    (   atom(Name),
        format(atom(Key), '~w/~w', [Name, Arity])
    ;   arg(_, T, Sub), sub_functor(Sub, Key)
    ).
sub_functor(T, Key) :-
    atom(T), format(atom(Key), '~w/0', [T]).

collect_predicates(Rows, Preds) :-
    findall(Key-Row,
            ( member(Row, Rows),
              get_dict(name, Row, N), get_dict(arity, Row, A),
              format(atom(Key), '~w/~w', [N, A]) ),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs(Pairs, Grouped),
    findall(P, ( member(K-Rs, Grouped), pred_row(K, Rs, P) ), Preds).

group_pairs([], []).
group_pairs([K-V|T], [K-[V|Vs]|Rest]) :-
    same_key(K, T, Vs, T1),
    group_pairs(T1, Rest).

same_key(K, [K-V|T], [V|Vs], T1) :- !, same_key(K, T, Vs, T1).
same_key(_, T, [], T).

pred_row(Key, Rows, json{ key: Key, name: Name, arity: Arity, dcg: Dcg,
                          clauses: Count, first: First, last: Last,
                          spans: Spans, calls: Calls }) :-
    Rows = [R0|_],
    get_dict(name, R0, Name), get_dict(arity, R0, Arity),
    ( get_dict(kind, R0, dcg) -> Dcg = true ; Dcg = false ),
    length(Rows, Count),
    findall(S, ( member(R, Rows), get_dict(start, R, S) ), Starts),
    findall(E, ( member(R, Rows), get_dict(end, R, E) ), Ends),
    min_list(Starts, First), max_list(Ends, Last),
    findall(json{start: S, end: E},
            ( member(R, Rows), get_dict(start, R, S), get_dict(end, R, E) ),
            Spans),
    findall(C, ( member(R, Rows), get_dict(calls, R, Cs), member(C, Cs) ), Cs0),
    sort(Cs0, Calls).
