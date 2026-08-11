#!/usr/bin/env swipl
:- encoding(utf8).
:- use_module(library(http/json)).
:- initialization(main, main).

main(Argv) :-
    ( append(_, [Input, Output], Argv)
    -> true
    ; format(user_error, 'usage: emit_grammar.pl INPUT.pl OUTPUT.js~n', []),
      halt(2)
    ),
    read_source_terms(Input, Terms),
    findall(dcg(Name, Arity, Body),
            ( member((Head --> Body), Terms),
              callable(Head),
              functor(Head, Name, Arity),
              js_identifier(Name)
            ),
            Dcgs),
    findall(Name, member(dcg(Name, _, _), Dcgs), Names0),
    sort(Names0, Names),
    setup_call_cleanup(
        open(Output, write, Stream, [encoding(utf8)]),
        emit_file(Stream, Input, Names, Dcgs),
        close(Stream)),
    include(translatable_clause(Names), Dcgs, Translatable),
    length(Dcgs, ClauseCount),
    length(Translatable, EmittedCount),
    length(Names, RuleCount),
    format('DCG_EMIT clauses=~d translatable=~d rule_names=~d output=~w~n',
           [ClauseCount, EmittedCount, RuleCount, Output]).

read_source_terms(Path, Terms) :-
    setup_call_cleanup(
        open(Path, read, Stream, [encoding(utf8)]),
        read_terms(Stream, Terms),
        close(Stream)).

read_terms(Stream, Terms) :-
    read_term(Stream, Term, [module(user), syntax_errors(error)]),
    ( Term == end_of_file
    -> Terms = []
    ; apply_reader_directive(Term),
      Terms = [Term | Rest],
      read_terms(Stream, Rest)
    ).

apply_reader_directive((:- op(Priority, Type, Name))) :- !,
    op(Priority, Type, Name).
apply_reader_directive((:- set_prolog_flag(back_quotes, Value))) :- !,
    set_prolog_flag(back_quotes, Value).
apply_reader_directive(_).

emit_file(Stream, Input, Names, Dcgs) :-
    format(Stream, '// Generated from ~w by emit_grammar.pl.~n', [Input]),
    format(Stream, '// Only clauses with terminals, conjunctions, alternatives, and DCG calls are emitted.~n', []),
    format(Stream, 'module.exports = grammar({~n  name: "dl6_dcg_probe",~n  rules: {~n', []),
    emit_rules(Stream, Names, Dcgs),
    format(Stream, '  },~n});~n', []).

emit_rules(_, [], _).
emit_rules(Stream, [Name | Rest], Dcgs) :-
    all_rule_names(Dcgs, AllNames),
    findall(Js,
            ( member(dcg(Name, _, Body), Dcgs),
              translate(Body, AllNames, Dcgs, Js)
            ),
            JsBodies0),
    sort(JsBodies0, JsBodies),
    ( JsBodies == []
    -> true
    ; js_atom(Name, JsName),
      emit_rule(Stream, JsName, JsBodies)
    ),
    emit_rules(Stream, Rest, Dcgs).

all_rule_names(Dcgs, Names) :-
    findall(Name, member(dcg(Name, _, _), Dcgs), Names0),
    sort(Names0, Names).

emit_rule(Stream, Name, [Body]) :- !,
    format(Stream, '    ~w: $ => ~w,~n', [Name, Body]).
emit_rule(Stream, Name, Bodies) :-
    atomic_list_concat(Bodies, ', ', Joined),
    format(Stream, '    ~w: $ => choice(~w),~n', [Name, Joined]).

translatable_clause(Names, dcg(_, _, Body)) :-
    translate(Body, Names, [], _).

translate((A, B), Names, Dcgs, Js) :- !,
    translate(A, Names, Dcgs, JA),
    translate(B, Names, Dcgs, JB),
    format(atom(Js), 'seq(~w, ~w)', [JA, JB]).
translate((A ; B), Names, Dcgs, Js) :- !,
    translate(A, Names, Dcgs, JA),
    translate(B, Names, Dcgs, JB),
    format(atom(Js), 'choice(~w, ~w)', [JA, JB]).
translate({_}, _, _, 'blank()') :- !.
translate(!, _, _, 'blank()') :- !.
translate([], _, _, 'blank()') :- !.
translate(List, _, _, Js) :-
    is_list(List), !,
    maplist(integer, List),
    string_codes(String, List),
    with_output_to(atom(Js), json_write(current_output, String)).
translate(Goal, Names, _, Js) :-
    callable(Goal),
    functor(Goal, Name, _),
    memberchk(Name, Names),
    js_atom(Name, JsName),
    format(atom(Js), '$.~w', [JsName]).

js_identifier(Name) :-
    atom(Name),
    atom_codes(Name, [First | Rest]),
    js_start(First),
    maplist(js_continue, Rest).

js_start(Code) :- code_type(Code, alpha), !.
js_start(0'_).
js_continue(Code) :- code_type(Code, alnum), !.
js_continue(0'_).

js_atom(Name, Name) :- js_identifier(Name), !.
