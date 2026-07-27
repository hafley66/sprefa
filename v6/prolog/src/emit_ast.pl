% emit_ast.pl : the prolog frontend's exit door. `<-` rules (native terms via
% op/3) are serialized to the JSON shape of v6/sprefa-store/js/src/lower/ast.ts
% (`Program`), which that file's header calls "the shared contract a future
% parser produces". This is that parser. The js engine (evalProgramSql, the
% real semi-naive strata + Z-set machinery) runs the result unchanged.
%
% Emit:    swipl -q -l v6/prolog/src/emit_ast.pl -g "emit(reach, 'out.json'),halt"
% Score:   swipl -q -l v6/prolog/src/emit_ast.pl -g go -g halt
%
% Mapping (ast.ts type <- prolog surface):
%   Var {kind:"var",name}    a hole occurring 2+ times in the rule (the join)
%   Wild {kind:"wild"}       a hole occurring exactly once
%   Lit {kind:"lit",value}   a number or atom argument
%   Compare                  X > 3, X =< N ...   (op map below)
%   NegRelRef                \+ rel(Args)
%   HeadVar                  head holes (must be body-bound, else emit fails)
% Head aggregation (HeadAgg) is not surfaced yet.

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(http/json)).
:- use_module(library(aggregate)).

:- op(1150, xfx, <-).

% ── programs ────────────────────────────────────────────────────────────────

program(reach,
  [ edb(root, [node]),
    edb(edge, [parent, child]),
    idb(reach, [node])
  ],
  [ ( reach(Node)  <- root(Node) ),
    ( reach(Child) <- (edge(Parent, Child), reach(Parent)) )
  ]).

% ── emission ────────────────────────────────────────────────────────────────

emit(Name, Path) :-
    program(Name, Decls, Rules),
    maplist(decl_dict, Decls, DeclDicts),
    maplist(rule_dict, Rules, RuleDicts),
    setup_call_cleanup(open(Path, write, Stream),
        json_write_dict(Stream, _{rels: DeclDicts, rules: RuleDicts}),
        close(Stream)).

decl_dict(edb(Name, Cols), _{name: NameS, columns: ColsS, kind: Kind, origin: "EDB"}) :-
    atom_string(Name, NameS), maplist(atom_string, Cols, ColsS),
    Kind = _{shape: "event", temperature: "hot",
             buffer: _{replay: 0, onFull: "block"},
             origin: "EDB", materialization: "materialized"}.
decl_dict(idb(Name, Cols), _{name: NameS, columns: ColsS, kind: Kind, origin: "IDB"}) :-
    atom_string(Name, NameS), maplist(atom_string, Cols, ColsS),
    Kind = _{shape: "pipe", temperature: "cold",
             buffer: _{replay: 0, onFull: "block"},
             origin: "IDB", materialization: "lazy"}.

rule_dict(Rule, _{head: HeadS, headTerms: HeadTerms, body: BodyDicts}) :-
    copy_term(Rule, Head <- Body),
    Head =.. [HeadName | HeadArgs],
    atom_string(HeadName, HeadS),
    body_list(Body, Items),
    hole_occurrences(HeadArgs, Items, Occs),
    name_holes(Occs, [], Named),
    maplist(head_term_dict(Occs, Named), HeadArgs, HeadTerms),
    maplist(body_dict(Occs, Named), Items, BodyDicts).

body_list((Item, Rest), [Item | Items]) :- !, body_list(Rest, Items).
body_list(Item, [Item]).

% every hole occurrence, duplicates preserved: count 1 = wildcard, 2+ = join var
hole_occurrences(HeadArgs, Items, Occs) :-
    foldl(item_holes, Items, [], BodyOccs),
    include(var, HeadArgs, HeadHoles),
    append(HeadHoles, BodyOccs, Occs).

item_holes(\+ Atom, Acc, Occs) :- !, Atom =.. [_ | Args],
    include(var, Args, Holes), append(Acc, Holes, Occs).
item_holes(Cmp, Acc, Occs) :- cmp_op(Cmp, _, Lhs, _), !,
    ( var(Lhs) -> append(Acc, [Lhs], Occs) ; Occs = Acc ).
item_holes(Atom, Acc, Occs) :- Atom =.. [_ | Args],
    include(var, Args, Holes), append(Acc, Holes, Occs).

name_holes([], Named, Named).
name_holes([Hole | Rest], Acc, Named) :-
    (   hlookup(Hole, Acc, _) -> name_holes(Rest, Acc, Named)
    ;   length(Acc, I), format(atom(Name), 'v~w', [I]),
        name_holes(Rest, [Hole-Name | Acc], Named)
    ).

hlookup(Hole, [Key-Name | _], Name) :- Hole == Key, !.
hlookup(Hole, [_ | Rest], Name) :- hlookup(Hole, Rest, Name).

count_occ(Hole, Occs, N) :-
    aggregate_all(count, (member(O, Occs), O == Hole), N).

% head terms: holes only, and each must be body-bound (count 2+) or the rule
% is unsafe and emission FAILS, same check the SQL lowering got for free.
head_term_dict(Occs, Named, Arg, _{kind: "hvar", name: NameS}) :-
    var(Arg),
    count_occ(Arg, Occs, N), N >= 2,
    hlookup(Arg, Named, Name), atom_string(Name, NameS).

body_dict(Occs, Named, \+ Atom, _{kind: "notrel", rel: RelS, args: Args}) :- !,
    Atom =.. [Rel | RelArgs], atom_string(Rel, RelS),
    maplist(arg_dict(Occs, Named), RelArgs, Args).
body_dict(_Occs, Named, Cmp, _{kind: "cmp", op: OpS,
                               lhs: _{kind: "var", name: NameS},
                               rhs: _{kind: "lit", value: Value}}) :-
    cmp_op(Cmp, Op, Lhs, Rhs), !,
    atom_string(Op, OpS),
    hlookup(Lhs, Named, Name), atom_string(Name, NameS),
    lit_value(Rhs, Value).
body_dict(Occs, Named, Atom, _{kind: "rel", rel: RelS, args: Args}) :-
    Atom =.. [Rel | RelArgs], atom_string(Rel, RelS),
    maplist(arg_dict(Occs, Named), RelArgs, Args).

arg_dict(Occs, Named, Arg, Dict) :-
    (   var(Arg)
    ->  (   count_occ(Arg, Occs, 1) -> Dict = _{kind: "wild"}
        ;   hlookup(Arg, Named, Name), atom_string(Name, NameS),
            Dict = _{kind: "var", name: NameS}
        )
    ;   lit_value(Arg, Value), Dict = _{kind: "lit", value: Value}
    ).

lit_value(N, N) :- number(N), !.
lit_value(A, S) :- atom(A), atom_string(A, S).

cmp_op(A > B,   gt, A, B).
cmp_op(A < B,   lt, A, B).
cmp_op(A >= B,  ge, A, B).
cmp_op(A =< B,  le, A, B).
cmp_op(A =:= B, eq, A, B).
cmp_op(A =\= B, ne, A, B).

% ── grader ──────────────────────────────────────────────────────────────────

check(reach_emits, (
    tmp_file_stream(text, Path, S0), close(S0),
    emit(reach, Path),
    setup_call_cleanup(open(Path, read, S),
                       json_read_dict(S, Dict), close(S)),
    get_dict(rels, Dict, Rels), length(Rels, 3),
    get_dict(rules, Dict, Rules), length(Rules, 2),
    nth0(1, Rules, Rec),
    get_dict(head, Rec, "reach"),
    get_dict(body, Rec, [EdgeRef, ReachRef]),
    get_dict(args, EdgeRef, [EdgeParent, _]),
    get_dict(args, ReachRef, [ReachParent]),
    get_dict(name, EdgeParent, JoinName),
    get_dict(name, ReachParent, JoinName)   % SAME name: the join survived
)).
check(unsafe_head_fails, ( \+ rule_dict((bad(X) <- edge(_, _)), _),
                           copy_term(X, _) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
