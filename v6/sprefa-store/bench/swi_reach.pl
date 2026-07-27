% swi_reach.pl — SWI-Prolog incremental-tabling contender for the reach bench.
%
% Mirrors src/measure.rs benchgraph::gen(layers, width) exactly: nodes 0,1 are
% roots; layer-0 node ids are 2+w with parent 0 (plus parent 1 when w mod 3 = 0);
% deeper node 2+l*width+w has parents prev+w and prev+(w+1) mod width. The
% measured op is the same as the harness: retract root 0, recount survivors.
%
% Run:   swipl -q -s bench/swi_reach.pl -- <layers> <width>
% Emits: the harness CSV line on stderr (engine,nodes,edges,killed,setup_ms,
%        retract_ms,ops,rss_mb) and a human line on stdout, plus SWI's own
%        table-space number, which the other engines have no analog for.
%
% What this probes: SWI invalidates incremental tables at CALL-VARIANT
% granularity — alive/1 is ONE table, so one retracted root may mean a full
% re-evaluation, where the sqlite engines do row-level DRed/count repair.
% The retract_ms column is that behavior, measured.

:- use_module(library(process)).
:- use_module(library(aggregate)).

:- dynamic edge/2 as incremental.
:- dynamic root/1 as incremental.
:- table alive/1 as incremental.

alive(Node)  :- root(Node).
alive(Child) :- edge(Parent, Child), alive(Parent).

main(Argv) :-
    ( Argv = [LayersAtom, WidthAtom]
    -> atom_number(LayersAtom, Layers), atom_number(WidthAtom, Width)
    ;  Layers = 2, Width = 200 ),
    statistics(walltime, _),
    build(Layers, Width),
    aggregate_all(count, alive(_), AliveBefore),        % forces the table
    statistics(walltime, [_, SetupMs]),
    retract(root(0)),
    aggregate_all(count, alive(_), AliveAfter),         % incremental re-eval
    statistics(walltime, [_, RetractMs]),
    Killed is AliveBefore - AliveAfter,
    oracle_check(AliveAfter),
    Nodes is 2 + Layers * Width,
    predicate_property(edge(_, _), number_of_clauses(Edges)),
    rss_mb(RssMb),
    table_mb(TableMb),
    format(user_error, "CSV,swi-incr,~w,~w,~w,~w,~w,0,~2f~n",
           [Nodes, Edges, Killed, SetupMs, RetractMs, RssMb]),
    format("nodes=~w edges=~w alive_before=~w alive_after=~w killed=~w setup_ms=~w retract_ms=~w rss_mb=~2f table_mb=~2f~n",
           [Nodes, Edges, AliveBefore, AliveAfter, Killed, SetupMs, RetractMs,
            RssMb, TableMb]).

build(Layers, Width) :-
    assertz(root(0)),
    assertz(root(1)),
    LastLayer is Layers - 1,
    LastCol is Width - 1,
    forall(between(0, LastLayer, Layer),
           forall(between(0, LastCol, Col),
                  assert_node(Layer, Col, Width))).

assert_node(0, Col, _Width) :- !,
    Id is 2 + Col,
    assertz(edge(0, Id)),
    ( Col mod 3 =:= 0 -> assertz(edge(1, Id)) ; true ).
assert_node(Layer, Col, Width) :-
    Id is 2 + Layer * Width + Col,
    Prev is 2 + (Layer - 1) * Width,
    Parent_a is Prev + Col,
    Parent_b is Prev + (Col + 1) mod Width,
    assertz(edge(Parent_a, Id)),
    assertz(edge(Parent_b, Id)).

% Self-oracle: throw every table away, recompute from the current facts cold,
% and demand the incremental answer matches the from-scratch answer.
oracle_check(IncrementalAfter) :-
    abolish_all_tables,
    aggregate_all(count, alive(_), FreshAfter),
    ( FreshAfter =:= IncrementalAfter
    -> true
    ;  format(user_error, "MISMATCH incremental=~w fresh=~w~n",
              [IncrementalAfter, FreshAfter]),
       halt(2) ).

rss_mb(Mb) :-
    current_prolog_flag(pid, Pid),
    format(atom(Cmd), "ps -o rss= -p ~w", [Pid]),
    process_create(path(sh), ['-c', Cmd], [stdout(pipe(Out))]),
    read_string(Out, _, RssString),
    close(Out),
    normalize_space(atom(RssAtom), RssString),
    atom_number(RssAtom, Kb),
    Mb is Kb / 1024.

table_mb(Mb) :-
    (   catch(statistics(table_space_used, Bytes), _, fail)
    ->  Mb is Bytes / 1048576
    ;   Mb = 0 ).

:- initialization(main, main).
