% dd_panel_export.pl : the oracle side of the differential-dataflow crosscheck.
%
% Dumps a JSON panel that a Rust test crate replays through the real
% differential-dataflow ecosystem, tick for tick. The Rust side hand-builds one
% circuit per program name and asserts its per-tick output delta stream equals
% the stream below. Correctness only; nothing here times anything.
%
% Run (writes conformance/dd_panel.json):
%   cd v6/prolog/conformance
%   swipl -q -l dd_panel_export.pl -g dd_panel_export:go -t halt
% Check the committed file is current (exit 1 on drift):
%   swipl -q -l dd_panel_export.pl -g dd_panel_export:check -t halt
%
% The entry points are module-qualified rather than exported: nothing in the
% tree calls them, and prolog-lint.sh reads a bare export no caller reaches as
% an unused_export_candidate finding.
%
% WHY A SEPARATE SEED STATE. run_program/5 returns exactly one delta tick per
% SCHEDULE tick; rows derived from the fixture's Initial seed appear in no delta
% tick at all. Two runs therefore stand behind every panel entry: one with an
% empty schedule for the post-seed row set, one with the real schedule for the
% stream. The Rust circuit feeds the seed at dd time 0 and the schedule at
% times 1..N, so dd's time-0 batch is graded against seed_state and time i+1
% against deltas[i].
%
% WHY VALUES ARE TAGGED. JSON cannot tell the integer 1 from the float 1.0, and
% four panel entries turn on exact float identity
% (float_exact_join_has_no_epsilon pairs 0.3 against 0.30000000000000004). Every
% column value ships as {"t":"int"|"real"|"text","v":...} so the Rust side
% decodes into one Ord row type with no per-column type table.

:- module(dd_panel_export, []).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- use_module(library(http/json), [json_write_dict/3, json_read_dict/3]).
:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(engine, [run_program/5]).

:- multifile user:fixture/5.
:- discontiguous user:fixture/5.

% The fixture corpus is one flat directory of loose files; go.pl loads it the
% same way, and this module has to stand alone under `swipl -l`. The load runs
% qualified into `user` because the fixture files carry no module declaration:
% a bare ensure_loaded/1 from inside a module lands every fixture/5 clause in
% THIS module and engine.pl never sees one.
load_fixture_files :-
    prolog_load_context(directory, Here),
    atomic_list_concat([Here, '/fixtures'], FixturesDir),
    directory_files(FixturesDir, Entries),
    msort(Entries, Ordered),
    forall(( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl') ),
           ( atomic_list_concat([FixturesDir, '/', Entry], Path),
             user:ensure_loaded(Path) )).

:- load_fixture_files.

% ═══ the panel ══════════════════════════════════════════════════════════════
% One row per program, with the shape it exists to cover. Every shape a hand
% circuit has to spell is named by at least one row, and no row is here for
% coverage alone.

panel_program(float_avg_is_grouped,
              'aggregate: avg over a float column, grouped').
panel_program(float_exact_join_has_no_epsilon,
              'two-way join whose key is an exact float bit pattern').
panel_program(retraction_only_tick_retracts_level_view,
              'a tick that only retracts, and the level view follows').
panel_program(callgraph_derivation_over_extraction,
              'three-way join with an inequality, plus a join retraction').
panel_program(callgraph_unused_inverts_with_the_call_set,
              'negation, anti-monotone in both directions across five ticks').
panel_program(ordered_program_level_fold_reaches_three_links,
              'recursion: a self-referential level head folding a three-link chain').
panel_program(mutual_recursion_matches_oracle,
              'mutual recursion, and one row derived two ways').
panel_program(recount_retraction_reaches_two_heads_same_tick,
              'retraction cascade reaching two derived heads in one tick').
panel_program(coalesce_defaults_the_absent_row,
              'coalesce: present and absent rows in one answer, seed only').
panel_program(coalesce_default_returns_when_source_retracts,
              'coalesce: the default leaves and comes back over two ticks').

% ═══ export ═════════════════════════════════════════════════════════════════

panel_dict(_{programs: Programs}) :-
    findall(Name-Note, panel_program(Name, Note), Pairs),
    maplist(program_dict, Pairs, Programs).

program_dict(Name-Note, Dict) :-
    user:fixture(Name, Prog, Initial, Schedule, _Expectations),
    run_program(Prog, Initial, [], SeedState, []),
    run_program(Prog, Initial, Schedule, Final, DeltaTicks),
    maplist(row_dict, Initial, SeedArrivals),
    maplist(row_dict, SeedState, SeedRows),
    maplist(signed_tick_dicts, Schedule, ScheduleDicts),
    maplist(signed_tick_dicts, DeltaTicks, DeltaDicts),
    maplist(row_dict, Final, FinalRows),
    program_rels(Prog, Initial, Schedule, SeedState, Final, DeltaTicks, Rels),
    Dict = _{ name: Name,
              note: Note,
              rels: Rels,
              seed_arrivals: SeedArrivals,
              seed_state: SeedRows,
              schedule: ScheduleDicts,
              deltas: DeltaDicts,
              final: FinalRows }.

signed_tick_dicts(Tick, Dicts) :- maplist(signed_dict, Tick, Dicts).

signed_dict(+Row, Dict) :- !, row_dict(Row, Base), Dict = Base.put(sign, 1).
signed_dict(-Row, Dict) :- !, row_dict(Row, Base), Dict = Base.put(sign, -1).

row_dict(Row, _{rel: Name, arity: Arity, values: Values}) :-
    Row =.. [Name | Args],
    length(Args, Arity),
    maplist(value_dict, Args, Values).

value_dict(Value, _{t: int, v: Value})  :- integer(Value), !.
value_dict(Value, _{t: real, v: Value}) :- float(Value), !.
value_dict(Value, _{t: text, v: Text})  :- atom(Value), !, atom_string(Value, Text).
value_dict(Value, _{t: text, v: Text})  :- string(Value), !, Text = Value.
% A compound column value reaches this door only through the value plane; it is
% compared as its canonical text, which is what both runtimes store.
value_dict(Value, _{t: text, v: Text})  :- term_string(Value, Text).

% Declared column names and types where the fixture states them, arity alone
% where it does not. Most fixtures carry no col_type at all.
program_rels(prog(Decls, _Rules), Initial, Schedule, SeedState, Final, DeltaTicks, Rels) :-
    append(Schedule, ScheduleRows0),
    maplist(unsign, ScheduleRows0, ScheduleRows),
    append(DeltaTicks, DeltaRows0),
    maplist(unsign, DeltaRows0, DeltaRows),
    append([Initial, ScheduleRows, SeedState, Final, DeltaRows], AllRows),
    findall(Name/Arity,
            ( member(Row, AllRows), functor(Row, Name, Arity) ),
            Refs0),
    sort(Refs0, Refs),
    maplist(rel_dict(Decls), Refs, Rels).

unsign(+Row, Row) :- !.
unsign(-Row, Row) :- !.
unsign(Row, Row).

rel_dict(Decls, Name/Arity, _{name: Name, arity: Arity, columns: Columns}) :-
    findall(_{name: ColumnText, type: TypeText},
            ( member(col_type(Name/Arity, Column, Type), Decls),
              atom_string(Column, ColumnText),
              atom_string(Type, TypeText) ),
            Columns).

panel_path(Path) :-
    prolog_load_context(directory, Here),
    atomic_list_concat([Here, '/dd_panel.json'], Path).

:- panel_path(P), assertz(panel_file(P)).

panel_text(Text) :-
    panel_dict(Dict),
    with_output_to(string(Text),
                   ( json_write_dict(current_output, Dict, [width(80)]), nl )).

go :-
    panel_file(Path),
    panel_text(Text),
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)),
    findall(N, panel_program(N, _), Names),
    length(Names, Count),
    format("dd-panel wrote ~w programs to ~w~n", [Count, Path]).

check :-
    panel_file(Path),
    panel_text(Text),
    (   \+ exists_file(Path)
    ->  format("dd-panel MISSING ~w~n", [Path]), halt(1)
    ;   read_file_to_string(Path, OnDisk, []),
        (   OnDisk == Text
        ->  findall(N, panel_program(N, _), Names),
            length(Names, Count),
            format("dd-panel current, ~w programs~n", [Count])
        ;   format("dd-panel STALE ~w, regenerate with -g go~n", [Path]), halt(1)
        )
    ).
