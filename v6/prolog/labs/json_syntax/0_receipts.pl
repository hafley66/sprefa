% JSON syntax lab -- ONE entry point.
%
%   swipl -q -l v6/prolog/labs/json_syntax/0_receipts.pl -g go -g halt
%
% LAB, NOT PRODUCTION. Nothing in this directory is consulted by parse_dl.pl,
% print_dl.pl, registry.pl, lower.pl, engine.pl or any compile/ module; the
% only production predicates it CALLS are read-only imports from
% 0_type_plane.pl (canonical_json_text/2, declared_type_name/2), so the
% tick-log contract is measured against the shipped implementation rather than
% a copy of it.
%
% Files, in dependency order:
%   1_grammar.pl   prototype DCG: the json5-ish literal + the v3/v4/v5 holes
%   2_lowering.pl  prototype pattern -> sqlite json1 SQL, executed
%   3_lists.pl     list(T) grading, 3 options x 5 axes, measured
%   4_cards.pl     card reconciliation + the exact spellings for user sign-off
%
% Companion plan doc: plans/2026-07-30-json-syntax-lab.md

:- module(json_syntax_lab, [go/0]).

:- use_module('1_grammar',  [grammar_receipts/1]).
:- use_module('2_lowering', [lowering_receipts/1]).
:- use_module('3_lists',    [list_receipts/1]).
:- use_module('4_cards',    [card_receipts/1]).

go :-
    format("~n== 1_grammar =============================================~n"),
    grammar_receipts(Grammar),
    format("~n== 2_lowering ============================================~n"),
    lowering_receipts(Lowering),
    format("~n== 3_lists ===============================================~n"),
    list_receipts(Lists),
    format("~n== 4_cards ===============================================~n"),
    card_receipts(Cards),
    Total is Grammar + Lowering + Lists + Cards,
    format("~nJSON_SYNTAX_LAB ~d PASS (grammar ~d, lowering ~d, lists ~d, cards ~d)~n",
           [Total, Grammar, Lowering, Lists, Cards]).
