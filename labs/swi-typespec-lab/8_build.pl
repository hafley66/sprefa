:- use_module('6_lsp').

build :-
    (exists_file('generated/soup-lsp') -> delete_file('generated/soup-lsp') ; true),
    qsave_program('generated/soup-lsp', [
        goal(soup_lsp:main),
        stand_alone(true)
    ]).
