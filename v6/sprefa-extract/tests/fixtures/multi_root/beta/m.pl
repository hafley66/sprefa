:- module(m, [go/0]).
:- use_module('lib/c').

go :- c_fact(2).
