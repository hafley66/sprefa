:- module(dl7_syntax_macro_program,
          [ macro_protocol/3,
            evaluate_macro_program/5,
            macro_results/6
          ]).

:- use_module('0_evaluator', [evaluate/4]).

macro_protocol(
    checked_datalog(root_graph(_, Edges),
                    datalog_program(Relations, _, _), _, _),
    protocol(Frontier, Form, Atom, Literal, Variable, Source, Claim),
    Diagnostics) :-
    !,
    resolve_protocol_relation(Edges, Relations, syntax_frontier, 2,
                              Frontier, D0),
    resolve_protocol_relation(Edges, Relations, syntax_form, 1, Form, D1),
    resolve_protocol_relation(Edges, Relations, syntax_atom, 2, Atom, D2),
    resolve_protocol_relation(Edges, Relations, syntax_literal, 2,
                              Literal, D3),
    resolve_protocol_relation(Edges, Relations, syntax_variable, 3,
                              Variable, D4),
    resolve_protocol_relation(Edges, Relations, syntax_source, 8, Source, D5),
    resolve_protocol_relation(Edges, Relations, syntax_claim, 2, Claim, D6),
    append([D0, D1, D2, D3, D4, D5, D6], Diagnostics).
macro_protocol(Program, _,
               [diagnostic(macrotime, none,
                           invalid_macro_program(Program))]).

resolve_protocol_relation(Edges, Relations, Name, Arity, Relation,
                          Diagnostics) :-
    findall(
        Candidate,
        ( member(':'(_, Name, ref(Candidate), _), Edges),
          memberchk(relation(ref(Candidate), Arity, _), Relations)
        ),
        Candidates0),
    sort(Candidates0, Candidates),
    protocol_relation_result(Name, Arity, Candidates,
                             Relation, Diagnostics).

protocol_relation_result(_, _, [Relation], Relation, []) :- !.
protocol_relation_result(Name, Arity, [], _,
                         [diagnostic(
                              macrotime, none,
                              missing_protocol_relation(Name, Arity))]) :- !.
protocol_relation_result(Name, Arity, Relations, _,
                         [diagnostic(
                              macrotime, none,
                              ambiguous_protocol_relation(
                                  Name, Arity, Relations))]).

evaluate_macro_program(
    Protocol,
    checked_datalog(_, datalog_program(_, ProgramSeeds, AllRules), _, _),
    Rows, Closure, Diagnostics) :-
    macro_rules(Protocol, AllRules, Rules),
    syntax_seed_calls(Rows, Protocol, SyntaxSeeds),
    append(ProgramSeeds, SyntaxSeeds, Seeds0),
    sort(Seeds0, Seeds),
    evaluate(Rules, Seeds, Closure, Diagnostics).

macro_rules(Protocol, Rules, MacroRules) :-
    include(macro_root_rule(Protocol), Rules, Roots),
    macro_dependency_closure(Protocol, Rules, Roots, MacroRules0),
    sort(MacroRules0, MacroRules).

macro_root_rule(protocol(_, Form, Atom, Literal, Variable, Source, Claim),
                rule(call(ref(Relation), _), _)) :-
    memberchk(Relation, [Form, Atom, Literal, Variable, Source, Claim]),
    !.
macro_root_rule(_, rule(call(ref(kernel(node)), _), _)) :- !.
macro_root_rule(_, rule(call(ref(kernel(':')),
                             [_, const(Label), _, _]), _)) :-
    memberchk(Label, [item, expansion]).

macro_dependency_closure(Protocol, Rules, Selected0, Selected) :-
    findall(
        Relation,
        ( member(rule(_, Goals), Selected0),
          member(checked_goal(_, call(ref(Relation), _)), Goals),
          \+ macro_input_relation(Protocol, Relation)
        ),
        Dependencies0),
    sort(Dependencies0, Dependencies),
    include(rule_heads_one_of(Dependencies), Rules, DependencyRules),
    append(Selected0, DependencyRules, Next0),
    sort(Next0, Next),
    (   Next == Selected0
    ->  Selected = Next
    ;   macro_dependency_closure(Protocol, Rules, Next, Selected)
    ).

macro_input_relation(protocol(Frontier, Form, Atom, Literal, Variable,
                              Source, _), Relation) :-
    memberchk(Relation, [Frontier, Form, Atom, Literal, Variable, Source]).
macro_input_relation(_, kernel(node)).
macro_input_relation(_, kernel(':')).

rule_heads_one_of(Relations, rule(call(ref(Relation), _), _)) :-
    memberchk(Relation, Relations).

syntax_seed_calls([], _, []).
syntax_seed_calls([Row | Rows], Protocol, [Call | Calls]) :-
    syntax_row_call(Row, Protocol, Call),
    syntax_seed_calls(Rows, Protocol, Calls).

syntax_row_call(node(Id), _, call(ref(kernel(node)), [ref(Id)])).
syntax_row_call(':'(Owner, Label, Target, Index), _,
                call(ref(kernel(':')),
                     [ref(Owner), const(Label), Target, const(Index)])).
syntax_row_call(syntax_frontier(Index, Node),
                protocol(Relation, _, _, _, _, _, _),
                call(ref(Relation), [const(Index), ref(Node)])).
syntax_row_call(syntax_form(Node),
                protocol(_, Relation, _, _, _, _, _),
                call(ref(Relation), [ref(Node)])).
syntax_row_call(syntax_atom(Node, Name),
                protocol(_, _, Relation, _, _, _, _),
                call(ref(Relation), [ref(Node), const(Text)])) :-
    atom_string(Name, Text).
syntax_row_call(syntax_literal(Node, Value),
                protocol(_, _, _, Relation, _, _, _),
                call(ref(Relation), [ref(Node), const(Value)])).
syntax_row_call(syntax_variable(Node, Variable, Name),
                protocol(_, _, _, _, Relation, _, _),
                call(ref(Relation),
                     [ref(Node), ref(Variable), const(Text)])) :-
    atom_string(Name, Text).
syntax_row_call(source(Node, Path, StartOffset, EndOffset,
                       StartLine, StartColumn, EndLine, EndColumn),
                protocol(_, _, _, _, _, Relation, _),
                call(ref(Relation),
                     [ ref(Node), const(Path),
                       const(StartOffset), const(EndOffset),
                       const(StartLine), const(StartColumn),
                       const(EndLine), const(EndColumn)
                     ])).

macro_results(Closure, Protocol, ActiveNodes,
              AvailableRows, Claims, Outputs) :-
    closure_syntax_rows(Closure, Protocol, AvailableRows),
    Protocol = protocol(_, _, _, _, _, _, ClaimRelation),
    findall(claim(Invocation, MacroIdentity),
            ( member(call(ref(ClaimRelation),
                          [ref(Invocation), MacroValue]), Closure),
              memberchk(Invocation, ActiveNodes),
              macro_identity(MacroValue, MacroIdentity)
            ),
            Claims0),
    findall(output(Invocation, Output, Ordinal),
            ( member(claim(Invocation, _), Claims0),
              member(call(ref(kernel(':')),
                          [ ref(Invocation), const(expansion), ref(Output),
                            const(Ordinal)
                          ]), Closure)
            ),
            Outputs0),
    sort(Claims0, Claims),
    sort(Outputs0, Outputs).

macro_identity(const(Value), Value).
macro_identity(ref(Identity), ref(Identity)).

closure_syntax_rows(Closure, Protocol, Rows) :-
    findall(Row, closure_syntax_row(Closure, Protocol, Row), Rows0),
    sort(Rows0, Rows).

closure_syntax_row(Closure, protocol(_, Form, _, _, _, _, _),
                   syntax_form(Node)) :-
    member(call(ref(Form), [ref(Node)]), Closure).
closure_syntax_row(Closure, protocol(_, _, Atom, _, _, _, _),
                   syntax_atom(Node, Name)) :-
    member(call(ref(Atom), [ref(Node), const(Text)]), Closure),
    atom_string(Name, Text).
closure_syntax_row(Closure, protocol(_, _, _, Literal, _, _, _),
                   syntax_literal(Node, Value)) :-
    member(call(ref(Literal), [ref(Node), const(Value)]), Closure).
closure_syntax_row(Closure, protocol(_, _, _, _, Variable, _, _),
                   syntax_variable(Node, VariableId, Name)) :-
    member(call(ref(Variable),
                [ref(Node), ref(VariableId), const(Text)]), Closure),
    atom_string(Name, Text).
closure_syntax_row(Closure, protocol(_, _, _, _, _, Source, _),
                   source(Node, Path, StartOffset, EndOffset,
                          StartLine, StartColumn, EndLine, EndColumn)) :-
    member(call(ref(Source),
                [ ref(Node), const(Path),
                  const(StartOffset), const(EndOffset),
                  const(StartLine), const(StartColumn),
                  const(EndLine), const(EndColumn)
                ]), Closure).
closure_syntax_row(Closure, Protocol, node(Node)) :-
    syntax_identity_in_closure(Closure, Protocol, Node),
    memberchk(call(ref(kernel(node)), [ref(Node)]), Closure).
closure_syntax_row(Closure, Protocol, ':'(Owner, item, ref(Target), Index)) :-
    syntax_identity_in_closure(Closure, Protocol, Owner),
    syntax_identity_in_closure(Closure, Protocol, Target),
    member(call(ref(kernel(':')),
                [ref(Owner), const(item), ref(Target), const(Index)]),
           Closure).

syntax_identity_in_closure(Closure, protocol(_, Form, Atom, Literal,
                                              Variable, _, _), Node) :-
    ( memberchk(call(ref(Form), [ref(Node)]), Closure)
    ; memberchk(call(ref(Atom), [ref(Node), _]), Closure)
    ; memberchk(call(ref(Literal), [ref(Node), _]), Closure)
    ; memberchk(call(ref(Variable), [ref(Node), _, _]), Closure)
    ).
