% dl_in_prolog.pl : prolog as the language-making tier, sqlite as the fact tier.
%
% Run:     swipl -q -l books/v6/dl_in_prolog.pl
% Score:   ?- go.
%
% Three claims, each with a runnable receipt:
%   1. SYNTAX IS FREE. op/3 makes `Head <- Body` real syntax; read_term is the
%      parser. The dl program below is not a string, it is native terms.
%   2. DCGs RUN BOTH WAYS. One marble-diagram grammar parses "ab--c|" into
%      events AND prints events back into "ab--c|". Parser = printer.
%   3. THE SQLITE BRIDGE CARRIES OUR OWN ALGORITHM. rule_select/3 compiles a
%      dl rule into join SQL (shared holes become join conditions, the v6
%      lowerSql move), the driver runs OUR semi-naive delta loop over a
%      sqlite3 process on a pipe, and SWI's tabling engine is the oracle the
%      sqlite answer must match.

:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module(library(aggregate)).
:- use_module(library(apply)).

% ─── 1. the surface syntax: one operator, then rules are just terms ─────────

:- op(1150, xfx, <-).

dl_rule(reach(Node)  <- root(Node)).
dl_rule(reach(Child) <- (edge(Parent, Child), reach(Parent))).

% ─── 2. marble DCG, bidirectional ────────────────────────────────────────────
% "ab--c|"  <->  [at(0,a), at(1,b), at(4,c), complete(5)]
% Clause order is the whole design: base cases first so generation terminates,
% the frame clause last so parsing prefers consuming a real character.

marble(String, Events) :-
    (   var(String)
    ->  phrase(seq(0, Events), Codes), string_codes(String, Codes)
    ;   string_codes(String, Codes), phrase(seq(0, Events), Codes)
    ).

seq(Tick, [complete(Tick)]) --> "|".
seq(_, []) --> [].
seq(Tick, [at(Tick, Char) | Rest]) -->
    [Code],
    { code_type(Code, alpha), char_code(Char, Code), Next is Tick + 1 },
    seq(Next, Rest).
seq(Tick, Events) -->
    "-",
    { Next is Tick + 1 },
    seq(Next, Events).

% ─── 3a. lowering: dl rule -> one SELECT ────────────────────────────────────
% Shared holes across body atoms become join conditions; the first occurrence
% of a hole names its column, later occurrences emit `later = first`. Identity
% (==) not unification (=) does the lookup: two DIFFERENT holes must not merge.
% Atoms whose functor is RecPred read `<name>_delta` (semi-naive; this lab
% assumes at most one recursive atom per rule). A head hole with no body
% binding makes the whole lowering FAIL: the range-restriction (safety) check
% costs zero extra code.

rule_select(Rule, RecPred, Sql) :-
    copy_term(Rule, Head <- Body),
    body_atoms(Body, Atoms),
    atoms_from(Atoms, RecPred, 0, FromItems, [], Bindings, [], Conds),
    Head =.. [_ | HeadArgs],
    sel_cols(HeadArgs, Bindings, Cols),
    atomic_list_concat(Cols, ', ', SelectList),
    atomic_list_concat(FromItems, ', ', FromList),
    (   Conds == [] -> Where = '1=1'
    ;   atomic_list_concat(Conds, ' AND ', Where)
    ),
    format(atom(Sql), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [SelectList, FromList, Where]).

body_atoms((Atom, Rest), [Atom | Atoms]) :- !, body_atoms(Rest, Atoms).
body_atoms(Atom, [Atom]).

atoms_from([], _, _, [], Bindings, Bindings, Conds, Conds).
atoms_from([Atom | Rest], RecPred, Index, [FromItem | FromRest],
           Bindings0, Bindings, Conds0, Conds) :-
    Atom =.. [Name | Args],
    ( Name == RecPred -> atom_concat(Name, '_delta', Table) ; Table = Name ),
    format(atom(Alias), 'a~w', [Index]),
    format(atom(FromItem), '~w ~w', [Table, Alias]),
    args_bind(Args, Alias, 0, Bindings0, Bindings1, Conds0, Conds1),
    Next is Index + 1,
    atoms_from(Rest, RecPred, Next, FromRest, Bindings1, Bindings, Conds1, Conds).

args_bind([], _, _, Bindings, Bindings, Conds, Conds).
args_bind([Arg | Rest], Alias, Position, Bindings0, Bindings, Conds0, Conds) :-
    format(atom(Col), '~w.c~w', [Alias, Position]),
    (   var(Arg)
    ->  (   vlookup(Arg, Bindings0, FirstCol)
        ->  format(atom(Cond), '~w = ~w', [Col, FirstCol]),
            Bindings1 = Bindings0, Conds1 = [Cond | Conds0]
        ;   Bindings1 = [Arg-Col | Bindings0], Conds1 = Conds0
        )
    ;   number(Arg)
    ->  format(atom(Cond), '~w = ~w', [Col, Arg]),
        Bindings1 = Bindings0, Conds1 = [Cond | Conds0]
    ;   format(atom(Cond), '~w = ''~w''', [Col, Arg]),
        Bindings1 = Bindings0, Conds1 = [Cond | Conds0]
    ),
    Next is Position + 1,
    args_bind(Rest, Alias, Next, Bindings1, Bindings, Conds1, Conds).

vlookup(Var, [Key-Col | _], Col) :- Var == Key, !.
vlookup(Var, [_ | Rest], Col) :- vlookup(Var, Rest, Col).

sel_cols([], _, []).
sel_cols([Arg | Rest], Bindings, [Col | Cols]) :-
    (   var(Arg)    -> vlookup(Arg, Bindings, Col)      % fails if unsafe
    ;   number(Arg) -> Col = Arg
    ;   format(atom(Col), '''~w''', [Arg])
    ),
    sel_cols(Rest, Bindings, Cols).

% ─── 3b. the sqlite3 process on a pipe ──────────────────────────────────────
% One child process for the whole run (the persistent-pipe tier, never
% shell-per-statement). A sentinel SELECT marks end-of-answer.

sqlite_start(sqlite(In, Out)) :-
    process_create(path(sqlite3), ['-batch', '-noheader', '-list', ':memory:'],
                   [stdin(pipe(In)), stdout(pipe(Out))]).

sqlite_stop(sqlite(In, Out)) :- close(In), close(Out).

sqlite_lines(sqlite(In, Out), Sql, Lines) :-
    format(In, '~w;~nSELECT ''EOQ'';~n', [Sql]),
    flush_output(In),
    collect(Out, Lines).

collect(Out, Lines) :-
    read_line_to_string(Out, Line),
    (   Line == "EOQ"         -> Lines = []
    ;   Line == end_of_file   -> Lines = []
    ;   Lines = [Line | Rest], collect(Out, Rest)
    ).

sqlite_do(Proc, Sql) :- sqlite_lines(Proc, Sql, _).

sqlite_scalar(Proc, Sql, Value) :-
    sqlite_lines(Proc, Sql, [Line | _]),
    number_string(Value, Line).

% ─── 3c. the driver: OUR semi-naive loop, statements built by the compiler ──

run_reach(Proc, Count) :-
    findall(Rule, dl_rule(Rule), Rules),
    partition(recursive_rule(reach), Rules, RecRules, InitRules),
    sqlite_do(Proc,
        'CREATE TABLE root(c0); CREATE TABLE edge(c0, c1);\c
         CREATE TABLE reach(c0); CREATE TABLE reach_delta(c0);\c
         CREATE TABLE reach_new(c0)'),
    load_graph(Proc),
    maplist([R, S]>>rule_select(R, '$none', S), InitRules, InitSelects),
    atomic_list_concat(InitSelects, ' UNION ', InitUnion),
    format(atom(InitSql), 'INSERT INTO reach ~w', [InitUnion]),
    sqlite_do(Proc, InitSql),
    sqlite_do(Proc, 'INSERT INTO reach_delta SELECT * FROM reach'),
    maplist([R, S]>>rule_select(R, reach, S), RecRules, RecSelects),
    atomic_list_concat(RecSelects, ' UNION ', RecUnion),
    format(atom(Step),
        'DELETE FROM reach_new;\c
         INSERT INTO reach_new SELECT * FROM (~w EXCEPT SELECT c0 FROM reach);\c
         INSERT INTO reach SELECT * FROM reach_new;\c
         DELETE FROM reach_delta;\c
         INSERT INTO reach_delta SELECT * FROM reach_new;\c
         SELECT changes()', [RecUnion]),
    fixpoint(Proc, Step),
    sqlite_scalar(Proc, 'SELECT count(*) FROM reach', Count).

recursive_rule(RecPred, _ <- Body) :-
    body_atoms(Body, Atoms),
    member(Atom, Atoms),
    functor(Atom, RecPred, _), !.

fixpoint(Proc, Step) :-
    sqlite_scalar(Proc, Step, NewRows),
    (   NewRows =:= 0 -> true
    ;   fixpoint(Proc, Step)
    ).

% ─── graph: same benchgraph::gen shape as bench/swi_reach.pl ────────────────

:- dynamic edge_fact/2, root_fact/1.

build_graph(Layers, Width) :-
    retractall(edge_fact(_, _)), retractall(root_fact(_)),
    assertz(root_fact(0)), assertz(root_fact(1)),
    LastLayer is Layers - 1, LastCol is Width - 1,
    forall(between(0, LastLayer, Layer),
           forall(between(0, LastCol, Col),
                  add_node(Layer, Col, Width))).

add_node(0, Col, _) :- !,
    Id is 2 + Col,
    assertz(edge_fact(0, Id)),
    ( Col mod 3 =:= 0 -> assertz(edge_fact(1, Id)) ; true ).
add_node(Layer, Col, Width) :-
    Id is 2 + Layer * Width + Col,
    Prev is 2 + (Layer - 1) * Width,
    Parent_a is Prev + Col,
    Parent_b is Prev + (Col + 1) mod Width,
    assertz(edge_fact(Parent_a, Id)),
    assertz(edge_fact(Parent_b, Id)).

load_graph(Proc) :-
    findall(row(A, B), edge_fact(A, B), EdgeRows),
    chunked_insert(Proc, edge, [A, B]>>format(atom(_), '', []), EdgeRows),
    findall(N, root_fact(N), Roots),
    maplist([N, V]>>format(atom(V), '(~w)', [N]), Roots, RootVals),
    atomic_list_concat(RootVals, ',', RootList),
    format(atom(RootSql), 'INSERT INTO root VALUES ~w', [RootList]),
    sqlite_do(Proc, RootSql).

% 400 rows per INSERT: under SQLITE_MAX_COMPOUND_SELECT, one pipe roundtrip
% per chunk, never per row (the N+1 law holds at the lab bench too).
chunked_insert(Proc, Table, _, Rows) :-
    chunk(Rows, 400, Chunks),
    forall(member(Chunk, Chunks),
           ( maplist([row(A, B), V]>>format(atom(V), '(~w,~w)', [A, B]),
                     Chunk, Vals),
             atomic_list_concat(Vals, ',', ValList),
             format(atom(Sql), 'INSERT INTO ~w VALUES ~w', [Table, ValList]),
             sqlite_do(Proc, Sql) )).

chunk([], _, []) :- !.
chunk(List, N, [Head | Rest]) :-
    length(Head, N), append(Head, Tail, List), !,
    chunk(Tail, N, Rest).
chunk(List, _, [List]).

% ─── the oracle: same rules, SWI tabling engine ─────────────────────────────

:- table treach/1.
treach(Node)  :- root_fact(Node).
treach(Child) :- edge_fact(Parent, Child), treach(Parent).

% ─── grader ─────────────────────────────────────────────────────────────────

end_to_end(SqliteCount, OracleCount) :-
    build_graph(6, 2000),
    abolish_all_tables,
    sqlite_start(Proc),
    call_cleanup(run_reach(Proc, SqliteCount), sqlite_stop(Proc)),
    aggregate_all(count, treach(_), OracleCount).

check(marble_parse, ( marble("ab--c|", Events),
                      Events == [at(0,a), at(1,b), at(4,c), complete(5)] )).
check(marble_print, ( marble(String, [at(0,a), at(1,b), at(4,c), complete(5)]),
                      String == "ab--c|" )).
check(lower_joins,  ( rule_select(reach(C) <- (edge(P, C), reach(P)), reach, Sql),
                      sub_atom(Sql, _, _, _, 'reach_delta'),
                      sub_atom(Sql, _, _, _, 'a1.c0 = a0.c0') )).
check(lower_unsafe, ( \+ rule_select(bad(X) <- edge(_, _), '$none', _),
                      copy_term(f(X), _) )).
check(sqlite_vs_tabling, ( end_to_end(SqliteCount, OracleCount),
                           SqliteCount =:= OracleCount,
                           SqliteCount =:= 12002 )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
