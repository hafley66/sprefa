% pure_reach.pl : one semi-naive reach, portable across SWI and Scryer.
%
% No tabling, no engine extensions: the fixpoint loop is hand-written over
% sorted lists, so running the SAME file under both engines measures raw
% engine speed (term building, findall, sort, first-arg indexing), nothing
% else. Same benchgraph::gen graph as swi_reach.pl / the rust engines.
%
% SWI:    swipl -q -l bench/pure_reach.pl -g 'main(8,20000),halt'
% Scryer: scryer-prolog -f --no-add-history bench/pure_reach.pl -g 'main(8,20000),halt'
%
% "Retract" here is honest recompute: pure prolog has no incremental story,
% so the measured second phase re-derives reach from root 1 alone.

% Scryer needs all four; SWI lacks library(between)/library(format) and prints
% a load error for each on stderr, then keeps loading (both are builtins there).
:- use_module(library(lists)).
:- use_module(library(between)).
:- use_module(library(format)).
:- use_module(library(time)).   % LAST: scryer resolves '$cpu_now' only from the
                                % most recently loaded module's private preds

:- dynamic(kids/2).

% engine-portable clock (seconds, float) and engine name. The scryer builtin
% must sit in its own compiled clause: scryer resolves '$cpu_now' at compile
% time only; meta-called (catch/goal-arg) occurrences raise existence errors.
scryer_now(T) :- '$cpu_now'(T).

now_s(T) :- catch(scryer_now(T), _, fail), !.
now_s(T) :- statistics(cputime, T).

engine(trealla) :- catch(current_prolog_flag(dialect, trealla), _, fail), !.
engine(swipl)   :- catch(current_prolog_flag(dialect, swi), _, fail), !.
engine(scryer)  :- catch(scryer_now(_), _, fail), !.   % no dialect flag there
engine(unknown).

% ── benchgraph::gen mirror, no asserts for edges: pairs -> grouped kids ─────

node_edge(0, Col, _, 0, Id) :- Id is 2 + Col.
node_edge(0, Col, _, 1, Id) :- Col mod 3 =:= 0, Id is 2 + Col.
node_edge(Layer, Col, Width, Parent, Id) :-
    Layer > 0,
    Id is 2 + Layer * Width + Col,
    Prev is 2 + (Layer - 1) * Width,
    (   Parent is Prev + Col
    ;   Parent is Prev + (Col + 1) mod Width
    ).

gen_pairs(Layers, Width, Pairs) :-
    LastLayer is Layers - 1,
    LastCol is Width - 1,
    findall(Parent-Child,
            ( between(0, LastLayer, Layer),
              between(0, LastCol, Col),
              node_edge(Layer, Col, Width, Parent, Child) ),
            Pairs).

build_kids(Pairs) :-
    keysort(Pairs, Sorted),
    group(Sorted, Groups),
    assert_all(Groups).

group([], []).
group([Key-Val | Rest], [kids(Key, [Val | Vals]) | Groups]) :-
    same_key(Key, Rest, Vals, Tail),
    group(Tail, Groups).

same_key(Key, [Key-Val | Rest], [Val | Vals], Tail) :- !,
    same_key(Key, Rest, Vals, Tail).
same_key(_, Rest, [], Rest).

assert_all([]).
assert_all([Fact | Rest]) :- assertz(Fact), assert_all(Rest).

% ── own ordset ops: no doubt about either stdlib ────────────────────────────

osub([], _, []).
osub([X | Xs], [], [X | Xs]) :- !.
osub([X | Xs], [Y | Ys], Out) :-
    compare(Order, X, Y),
    (   Order = (<) -> Out = [X | Rest], osub(Xs, [Y | Ys], Rest)
    ;   Order = (=) -> osub(Xs, Ys, Out)
    ;   osub([X | Xs], Ys, Out)
    ).

ounion([], Ys, Ys).
ounion([X | Xs], [], [X | Xs]) :- !.
ounion([X | Xs], [Y | Ys], Out) :-
    compare(Order, X, Y),
    (   Order = (<) -> Out = [X | Rest], ounion(Xs, [Y | Ys], Rest)
    ;   Order = (=) -> Out = [X | Rest], ounion(Xs, Ys, Rest)
    ;   Out = [Y | Rest], ounion([X | Xs], Ys, Rest)
    ).

% ── the fixpoint: known/frontier, expand, subtract, union, repeat ───────────

reach(Roots, Count) :-
    sort(Roots, Known0),
    loop(Known0, Known0, Known),
    length(Known, Count).

loop(Known, [], Known) :- !.
loop(Known0, Frontier, Known) :-
    findall(Child,
            ( member(Parent, Frontier),
              kids(Parent, Children),
              member(Child, Children) ),
            Children0),
    sort(Children0, Children),
    osub(Children, Known0, New),
    ounion(Known0, New, Known1),
    loop(Known1, New, Known).

% ── driver ──────────────────────────────────────────────────────────────────

main(Layers, Width) :-
    engine(Engine),
    now_s(T0),
    gen_pairs(Layers, Width, Pairs),
    length(Pairs, EdgeCount),
    build_kids(Pairs),
    reach([0, 1], AliveBefore),
    now_s(T1),
    reach([1], AliveAfter),
    now_s(T2),
    Killed is AliveBefore - AliveAfter,
    Nodes is 2 + Layers * Width,
    SetupMs is (T1 - T0) * 1000,
    RetractMs is (T2 - T1) * 1000,
    format("CSV,~w-pure,~w,~w,~w,~w,~w~n",
           [Engine, Nodes, EdgeCount, Killed, SetupMs, RetractMs]).
