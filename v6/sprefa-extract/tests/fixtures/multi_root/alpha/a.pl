:- module(a, [check/0]).
:- use_module('lib/b').

check :- b_fact(1).
