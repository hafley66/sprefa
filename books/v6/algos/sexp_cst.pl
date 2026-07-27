% sexp_cst.pl : tree-sitter CST -> prolog terms, then ast-grep matching by
% unification. The reflection is a ~20-line DCG over the CLI's S-expression
% output; a metavariable is a hole, a non-linear pattern is the same hole
% twice, subtree search is a 4-line walk, capture is span -> source slice.
% Run: swipl -q -l sexp_cst.pl -g go -g halt

:- use_module(library(lists)).

source_lines(["let x = f(a + b);", "y = y;"]).

cst_sexp(Text) :-
    atomic_list_concat(
      [ '(program [0, 0] - [2, 0]'
      , '  (lexical_declaration [0, 0] - [0, 17]'
      , '    (variable_declarator [0, 4] - [0, 16]'
      , '      name: (identifier [0, 4] - [0, 5])'
      , '      value: (call_expression [0, 8] - [0, 16]'
      , '        function: (identifier [0, 8] - [0, 9])'
      , '        arguments: (arguments [0, 9] - [0, 16]'
      , '          (binary_expression [0, 10] - [0, 15]'
      , '            left: (identifier [0, 10] - [0, 11])'
      , '            right: (identifier [0, 14] - [0, 15]))))))'
      , '  (expression_statement [1, 0] - [1, 6]'
      , '    (assignment_expression [1, 0] - [1, 5]'
      , '      left: (identifier [1, 0] - [1, 1])'
      , '      right: (identifier [1, 4] - [1, 5]))))'
      ], '\n', Text).

cst(Root) :-
    cst_sexp(Text), atom_codes(Text, Codes),
    phrase((ws, sexp_node(Root), ws), Codes).

sexp_node(node(Kind, span(SR, SC, ER, EC), Children)) -->
    "(", ident(Kind), ws, span(SR, SC, ER, EC), children(Children), ws, ")".

children([Child | Rest]) --> ws, child(Child), !, children(Rest).
children([]) --> [].

child(field(Name, Node)) --> ident(Name), ":", ws, sexp_node(Node).
child(Node) --> sexp_node(Node).

span(SR, SC, ER, EC) -->
    "[", int(SR), ",", ws, int(SC), "]", ws, "-", ws,
    "[", int(ER), ",", ws, int(EC), "]".

ident(Name) --> csyms(Cs), { Cs \== [], atom_codes(Name, Cs) }.
csyms([C | Cs]) --> [C], { code_type(C, csym) }, !, csyms(Cs).
csyms([]) --> [].
int(N) --> digits(Cs), { Cs \== [], number_codes(N, Cs) }.
digits([C | Cs]) --> [C], { code_type(C, digit) }, !, digits(Cs).
digits([]) --> [].
ws --> [C], { code_type(C, space) }, !, ws.
ws --> [].

subnode(Node, Node).
subnode(node(_, _, Children), Sub) :-
    member(Child, Children), unwrap(Child, Node), subnode(Node, Sub).
unwrap(field(_, Node), Node) :- !.
unwrap(Node, Node).

node_text(node(_, span(Row, Col, Row, EndCol), _), Text) :-
    source_lines(Lines), nth0(Row, Lines, Line),
    Len is EndCol - Col, sub_string(Line, Col, Len, _, Text).

check(capture_arg, ( cst(Root),
                     subnode(Root, node(call_expression, _,
                       [ field(function, Fn),
                         field(arguments, node(arguments, _, [Arg])) ])),
                     node_text(Fn, "f"), node_text(Arg, "a + b") )).
check(self_assign, ( cst(Root),
                     subnode(Root, node(assignment_expression, _,
                       [ field(left, L), field(right, R) ])),
                     node_text(L, Same), node_text(R, Same) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
