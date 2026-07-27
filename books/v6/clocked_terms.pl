% clocked_terms.pl : two extensions to the arrow language, prolog-tier both.
%
% Run:     swipl -q -l books/v6/clocked_terms.pl
% Score:   ?- go.
%
% PART A : the Lustre clock system ALONGSIDE the safety system. Two passes,
%   same rule terms, different term grammars. clock ::= base | on(Clock, S).
%   A body joining rels on different clocks is the unsafe-combineLatest bug,
%   rejected at compile time with the offending rule named.
%
% PART B : terms as first-class column values. Datalog's zeroth-order law
%   binds the FIXPOINT tier only: a column may hold a GROUND term (stored in
%   sqlite as canonical text, term_to_atom both ways). The prolog tier
%   rehydrates it and then pattern-matching is plain unification:
%     - metavariable ($A)         = a prolog hole
%     - non-linear pattern ($A=$A) = the same hole twice (the join, again)
%     - subtree search             = a recursive walk
%   The CST arrives as a tree-sitter S-expression; a ~20-line DCG reflects it
%   into node(Kind, Span, Children) terms automatically. The canned text below
%   is verbatim `tree-sitter parse` output shape; with a grammar installed the
%   same DCG eats the live CLI through one process_create per batch.

:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <=).
:- op(1150, xfx, <+).
:- op(1150, xfx, <~).

rule_kind((Head <= Body), deduce, Head, Body).
rule_kind((Head <+ Body), next,   Head, Body).
rule_kind((Head <~ Body), async,  Head, Body).

body_list((Item, Rest), [Item | Items]) :- !, body_list(Rest, Items).
body_list(Item, [Item]).

% ═════════════════════════════════════════════════════════════════════════════
% PART A — clock checking over arrow rules
% ═════════════════════════════════════════════════════════════════════════════

:- discontiguous cprog/2, cclocks/2.

% Declared clock environments (check-only here; swap ground clocks for holes
% and the same code becomes clock INFERENCE, exactly the hm.pl move).
cprog(good,
  [ ( total(N)   <= seed(N) )
  , ( doubled(D) <= total(N), D is N * 2 )
  , ( carry(N1)  <+ total(N), N1 is N + 1 )
  , ( log(D)     <~ doubled(D) )
  ]).
cclocks(good,
  [ seed-base, total-base, doubled-base, carry-base, log-base ]).

% slow ticks on a subclock; joining it with total same-tick is the bug.
cprog(bad,
  [ ( broken(N) <= total(N), slow(N) )
  ]).
cclocks(bad,
  [ total-base, slow-on(base, second), broken-base ]).

clock_errors(Name, Errors) :-
    cprog(Name, Rules),
    cclocks(Name, Env),
    findall(Rule, ( member(Rule, Rules), \+ rule_clock_ok(Rule, Env) ), Errors).

rule_clock_ok(Rule, Env) :-
    rule_kind(Rule, Kind, Head, Body),
    body_list(Body, Items),
    findall(Clock, ( member(Item, Items), rel_clock(Env, Item, Clock) ), Clocks),
    (   Clocks == [] -> true
    ;   all_equal(Clocks, BodyClock),
        (   Kind == async -> true          % <~ hands off to the host clock
        ;   functor(Head, HeadRel, _),
            (   member(HeadRel-HeadClock, Env)
            ->  HeadClock == BodyClock     % <= same instant; <+ same clock,
            ;   true                       %    one tick late (a register)
            )
        )
    ).

% is/cmp goals have no entry in Env, so membership is the rel filter.
rel_clock(Env, Item, Clock) :-
    Item =.. [Rel | _],
    member(Rel-Clock, Env).

all_equal([Clock | Rest], Clock) :-
    forall(member(Other, Rest), Other == Clock).

% ═════════════════════════════════════════════════════════════════════════════
% PART B — CST reflection + unification matching
% ═════════════════════════════════════════════════════════════════════════════

% Source under analysis (the debugger's job is a SELECT; the matcher's job
% is a unification):
source_lines(["let x = f(a + b);", "y = y;"]).

% Verbatim tree-sitter S-expression shape for that source (named nodes only,
% field labels, [row, col] - [row, col] spans):
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

% ── the reflection DCG: S-expression -> node(Kind, Span, Children) ──────────

cst(Root) :-
    cst_sexp(Text),
    atom_codes(Text, Codes),
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

ident(Name) --> csyms(Codes), { Codes \== [], atom_codes(Name, Codes) }.
csyms([Code | Rest]) --> [Code], { code_type(Code, csym) }, !, csyms(Rest).
csyms([]) --> [].

int(N) --> digits(Codes), { Codes \== [], number_codes(N, Codes) }.
digits([Code | Rest]) --> [Code], { code_type(Code, digit) }, !, digits(Rest).
digits([]) --> [].

ws --> [Code], { code_type(Code, space) }, !, ws.
ws --> [].

% ── matching: search is a walk, binding is unification ──────────────────────

subnode(Node, Node).
subnode(node(_, _, Children), Sub) :-
    member(Child, Children),
    unwrap(Child, Node),
    subnode(Node, Sub).

unwrap(field(_, Node), Node) :- !.
unwrap(Node, Node).

ast_match(Root, Pattern) :- subnode(Root, Pattern).

% span -> source text; this is what turns a captured hole into "$A"'s value.
node_text(node(_, span(Row, Col, Row, EndCol), _), Text) :-
    source_lines(Lines),
    nth0(Row, Lines, Line),
    Len is EndCol - Col,
    sub_string(Line, Col, Len, _, Text).

% ═════════════════════════════════════════════════════════════════════════════
% grader
% ═════════════════════════════════════════════════════════════════════════════

check(clock_pass,   ( clock_errors(good, []) )).
check(clock_clash,  ( clock_errors(bad, [Rule]),
                      Rule = (broken(_) <= _) )).
check(cst_reflects, ( cst(node(program, _, [_, _])) )).
% ast-grep "f($A)": function name checked, argument hole captured, text out.
check(capture_arg,  ( cst(Root),
                      ast_match(Root, node(call_expression, _,
                        [ field(function, Fn), field(arguments, node(arguments, _, [Arg])) ])),
                      node_text(Fn, "f"),
                      node_text(Arg, "a + b") )).
% non-linear "$A = $A": one hole used twice IS the constraint.
check(self_assign,  ( cst(Root),
                      ast_match(Root, node(assignment_expression, _,
                        [ field(left, Left), field(right, Right) ])),
                      node_text(Left, Same),
                      node_text(Right, Same) )).
% the sqlite bridge: a CST column is a ground term serialized both ways.
check(term_column,  ( cst(Root),
                      term_to_atom(Root, Blob), atom(Blob),
                      term_to_atom(Back, Blob), Back == Root )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
