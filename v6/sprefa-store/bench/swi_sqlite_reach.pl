% swi_sqlite_reach.pl : the dish path, end to end. Prolog compiles the dl
% rules to SQL (rule_select: shared holes -> join conditions), one sqlite3
% process on a pipe executes OUR semi-naive fixpoint, prolog only orchestrates.
% Same benchgraph::gen graph and CSV contract as the other engines.
% "Retract" = recompute: drop root 0, clear derived tables, re-derive.
% ops column = pipe roundtrips (statement batches sent to sqlite3).
%
% Run: swipl -q -s bench/swi_sqlite_reach.pl -- <layers> <width>

:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <-).

dl_rule(reach(Node)  <- root(Node)).
dl_rule(reach(Child) <- (edge(Parent, Child), reach(Parent))).

% ── the compiler (lower_sql.pl's algorithm) ─────────────────────────────────

rule_select(Rule, RecPred, Sql) :-
    copy_term(Rule, Head <- Body),
    body_atoms(Body, Atoms),
    atoms_from(Atoms, RecPred, 0, FromItems, [], Bindings, [], Conds),
    Head =.. [_ | HeadArgs],
    sel_cols(HeadArgs, Bindings, Cols),
    atomic_list_concat(Cols, ', ', SelectList),
    atomic_list_concat(FromItems, ', ', FromList),
    ( Conds == [] -> Where = '1=1'
    ; atomic_list_concat(Conds, ' AND ', Where) ),
    format(atom(Sql), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [SelectList, FromList, Where]).

body_atoms((Atom, Rest), [Atom | Atoms]) :- !, body_atoms(Rest, Atoms).
body_atoms(Atom, [Atom]).

atoms_from([], _, _, [], Binds, Binds, Conds, Conds).
atoms_from([Atom | Rest], RecPred, I, [FromItem | FromRest], B0, B, C0, C) :-
    Atom =.. [Name | Args],
    ( Name == RecPred -> atom_concat(Name, '_delta', Table) ; Table = Name ),
    format(atom(Alias), 'a~w', [I]),
    format(atom(FromItem), '~w ~w', [Table, Alias]),
    args_bind(Args, Alias, 0, B0, B1, C0, C1),
    I1 is I + 1,
    atoms_from(Rest, RecPred, I1, FromRest, B1, B, C1, C).

args_bind([], _, _, B, B, C, C).
args_bind([Arg | Rest], Alias, J, B0, B, C0, C) :-
    format(atom(Col), '~w.c~w', [Alias, J]),
    (   var(Arg)
    ->  (   vlookup(Arg, B0, First)
        ->  format(atom(Cond), '~w = ~w', [Col, First]),
            B1 = B0, C1 = [Cond | C0]
        ;   B1 = [Arg-Col | B0], C1 = C0 )
    ;   format(atom(Cond), '~w = ~w', [Col, Arg]), B1 = B0, C1 = [Cond | C0]
    ),
    J1 is J + 1,
    args_bind(Rest, Alias, J1, B1, B, C1, C).

vlookup(Var, [Key-Col | _], Col) :- Var == Key, !.
vlookup(Var, [_ | Rest], Col) :- vlookup(Var, Rest, Col).

sel_cols([], _, []).
sel_cols([Arg | Rest], Binds, [Col | Cols]) :-
    ( var(Arg) -> vlookup(Arg, Binds, Col) ; Col = Arg ),
    sel_cols(Rest, Binds, Cols).

% ── sqlite3 on a pipe, roundtrips counted ───────────────────────────────────

:- dynamic roundtrips/1.

sqlite_start(sqlite(In, Out)) :-
    process_create(path(sqlite3), ['-batch', '-noheader', '-list', ':memory:'],
                   [stdin(pipe(In)), stdout(pipe(Out))]).

sqlite_lines(sqlite(In, Out), Sql, Lines) :-
    retract(roundtrips(N)), N1 is N + 1, assertz(roundtrips(N1)),
    format(In, '~w;~nSELECT ''EOQ'';~n', [Sql]),
    flush_output(In),
    collect(Out, Lines).

collect(Out, Lines) :-
    read_line_to_string(Out, Line),
    (   Line == "EOQ"       -> Lines = []
    ;   Line == end_of_file -> Lines = []
    ;   Lines = [Line | Rest], collect(Out, Rest)
    ).

sqlite_do(Proc, Sql) :- sqlite_lines(Proc, Sql, _).
sqlite_scalar(Proc, Sql, V) :- sqlite_lines(Proc, Sql, [L | _]), number_string(V, L).

% ── benchgraph::gen mirror ──────────────────────────────────────────────────

node_edge(0, Col, _, 0, Id) :- Id is 2 + Col.
node_edge(0, Col, _, 1, Id) :- Col mod 3 =:= 0, Id is 2 + Col.
node_edge(Layer, Col, Width, Parent, Id) :-
    Layer > 0,
    Id is 2 + Layer * Width + Col,
    Prev is 2 + (Layer - 1) * Width,
    ( Parent is Prev + Col ; Parent is Prev + (Col + 1) mod Width ).

gen_pairs(Layers, Width, Pairs) :-
    L1 is Layers - 1, W1 is Width - 1,
    findall(P-C, ( between(0, L1, L), between(0, W1, W),
                   node_edge(L, W, Width, P, C) ), Pairs).

load_edges(Proc, Pairs) :-
    chunk(Pairs, 400, Chunks),
    forall(member(Ch, Chunks),
           ( maplist([P-C, V]>>format(atom(V), '(~w,~w)', [P, C]), Ch, Vals),
             atomic_list_concat(Vals, ',', VList),
             format(atom(Sql), 'INSERT INTO edge VALUES ~w', [VList]),
             sqlite_do(Proc, Sql) )).

chunk([], _, []) :- !.
chunk(L, N, [H | R]) :- length(H, N), append(H, T, L), !, chunk(T, N, R).
chunk(L, _, [L]).

% ── the fixpoint driver ─────────────────────────────────────────────────────

fix(Proc, Count) :-
    findall(R, dl_rule(R), Rules),
    partition(recursive_rule(reach), Rules, RecRules, InitRules),
    maplist([R, S]>>rule_select(R, '$none', S), InitRules, InitSelects),
    atomic_list_concat(InitSelects, ' UNION ', InitUnion),
    format(atom(InitSql),
           'INSERT INTO reach ~w; INSERT INTO reach_delta SELECT * FROM reach',
           [InitUnion]),
    sqlite_do(Proc, InitSql),
    maplist([R, S]>>rule_select(R, reach, S), RecRules, RecSelects),
    atomic_list_concat(RecSelects, ' UNION ', RecUnion),
    format(atom(Step),
        'DELETE FROM reach_new;\c
         INSERT INTO reach_new SELECT * FROM (~w EXCEPT SELECT c0 FROM reach);\c
         INSERT INTO reach SELECT * FROM reach_new;\c
         DELETE FROM reach_delta;\c
         INSERT INTO reach_delta SELECT * FROM reach_new;\c
         SELECT changes()', [RecUnion]),
    fixloop(Proc, Step),
    sqlite_scalar(Proc, 'SELECT count(*) FROM reach', Count).

recursive_rule(RecPred, _ <- Body) :-
    body_atoms(Body, Atoms),
    member(Atom, Atoms), functor(Atom, RecPred, _), !.

fixloop(Proc, Step) :-
    sqlite_scalar(Proc, Step, New),
    ( New =:= 0 -> true ; fixloop(Proc, Step) ).

rss_mb(Mb) :-
    current_prolog_flag(pid, Pid),
    format(atom(Cmd), 'ps -o rss= -p ~w', [Pid]),
    process_create(path(sh), ['-c', Cmd], [stdout(pipe(Out))]),
    read_string(Out, _, S), close(Out),
    normalize_space(atom(A), S), atom_number(A, Kb), Mb is Kb / 1024.

main(Argv) :-
    ( Argv = [LA, WA] -> atom_number(LA, Layers), atom_number(WA, Width)
    ; Layers = 2, Width = 200 ),
    assertz(roundtrips(0)),
    statistics(walltime, _),
    sqlite_start(Proc),
    sqlite_do(Proc,
        'CREATE TABLE root(c0); CREATE TABLE edge(c0, c1);\c
         CREATE TABLE reach(c0); CREATE TABLE reach_delta(c0);\c
         CREATE TABLE reach_new(c0)'),
    gen_pairs(Layers, Width, Pairs),
    length(Pairs, EdgeCount),
    load_edges(Proc, Pairs),
    sqlite_do(Proc, 'INSERT INTO root VALUES (0),(1)'),
    fix(Proc, Before),
    statistics(walltime, [_, SetupMs]),
    sqlite_do(Proc,
        'DELETE FROM root WHERE c0 = 0; DELETE FROM reach;\c
         DELETE FROM reach_delta; DELETE FROM reach_new'),
    fix(Proc, After),
    statistics(walltime, [_, RetractMs]),
    Killed is Before - After,
    Nodes is 2 + Layers * Width,
    roundtrips(Ops),
    rss_mb(Rss),
    format(user_error, 'CSV,swi-sqlite,~w,~w,~w,~w,~w,~w,~2f~n',
           [Nodes, EdgeCount, Killed, SetupMs, RetractMs, Ops, Rss]).

:- initialization(main, main).
