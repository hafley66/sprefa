% sweep.pl : Phase C entry point (plans/2026-07-27-tsv2-compile-target-header.md,
% PHASE C CONTRACT). Sweeps EVERY fixture(Name, Prog, Initial, Schedule,
% Expectations) fact across every conformance/fixtures/*.pl file through the
% phase B compiler (analyze/strat/lower/emit_ts, all unedited by this file),
% bucketing each into compiled or refused, and for every compiled program
% writing its emitted TypeScript module plus a JSON rendering of the
% fixture's own Schedule term (v6/tsv2/scripts/sweep-run.ts replays exactly
% that schedule against the emitted module, never re-deriving one, so a
% fixture is graded against what its own oracle run used).
%
% Reads fixture files the same read_term-without-consult way
% compile.pl:read_fixture_term/4 does (mining surface variable names off
% shared variable identity), generalized here to collect EVERY fixture in a
% file rather than stopping at the first Name match -- compile.pl itself is
% not edited; this is new code, not a rewrite of it.
%
% Never touches conformance/engine.pl, go.pl, body.pl, level_eval.pl,
% ticklog.pl, or fixtures/*.pl.
%
% Run: swipl -q -l v6/prolog/compile/sweep.pl -g sweep -g halt

:- module(sweep, [ sweep/0 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(filesex)).
:- use_module(compile, [ program_plan/2 ]).
:- use_module(lower, [ lower_program/2, boot_statements/4 ]).
:- use_module(emit_ts, [ emit_program/5 ]).
:- use_module('../conformance/body', [ rel_ref/2 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ paths ═══════════════════════════════════════════════════════════════
% prolog_load_context/2 only answers inside a directive running WHILE this
% file loads; sweep/0 runs later as a plain goal, so the directory is
% captured once at load time into a fact rather than re-queried at call time.

:- dynamic(compile_dir_fact/1).
:- prolog_load_context(directory, Here), assertz(compile_dir_fact(Here)).

compile_dir(Dir) :- compile_dir_fact(Dir).
out_dir(Dir) :- compile_dir(Here), atomic_list_concat([Here, '/out'], Dir).
fixtures_dir(Dir) :- compile_dir(Here), atomic_list_concat([Here, '/../conformance/fixtures'], Dir).

fixture_files(Files) :-
    fixtures_dir(Dir),
    directory_files(Dir, Entries),
    msort(Entries, Ordered),
    findall(Path,
            ( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl'),
              atomic_list_concat([Dir, '/', Entry], Path) ),
            Files).

% ═══ reading every fixture in a file (generalizes compile.pl's
% read_fixture_term/4, which stops at the first Name match; the directive-
% replay is the same reason compile.pl's own version does it: a fixture
% file's own `:- op(...)` lines only take effect for a raw read_term stream
% if this reader calls them itself, exactly what consult does) ════════════

read_all_fixtures(File, Entries) :-
    open(File, read, Stream),
    call_cleanup(scan_fixtures(Stream, Entries), close(Stream)).

scan_fixtures(Stream, Entries) :-
    read_term(Stream, Candidate, [variable_names(Bindings)]),
    ( Candidate == end_of_file
    -> Entries = []
    ; Candidate = (:- Directive)
    -> call(Directive), scan_fixtures(Stream, Entries)
    ; Candidate = fixture(Name, _, _, _, _)
    -> Entries = [entry(Name, Candidate, Bindings) | Rest], scan_fixtures(Stream, Rest)
    ; scan_fixtures(Stream, Entries)
    ).

% ═══ top level ═══════════════════════════════════════════════════════════

sweep :-
    out_dir(OutDir), make_directory_path(OutDir),
    clear_stale_compiled_outputs(OutDir),
    fixture_files(Files),
    findall(Result,
            ( member(File, Files), sweep_file(File, FileResults), member(Result, FileResults) ),
            Results),
    write_manifest(Results),
    summarize(Results).

% A fixture that COMPILED on a previous run and now falls out of the
% compiled set (a widening's own gate narrowing, e.g. the comparison/bind/
% head-arithmetic refusal) would otherwise leave its stale .ts/.schedule.json
% behind, no longer regenerated but also not removed -- misleading generated
% output nothing re-checks. Only .ts and .schedule.json are cleared (one
% per compiled fixture, by construction); manifest.json/run-results.json are
% overwritten wholesale every run regardless.
%
% PHASE C2 fix: '.schedule.json' is 14 characters, not 13 -- the Length=13
% off-by-one made every `sub_atom(Entry, _, 13, 0, '.schedule.json')` call
% fail outright (a 13-char substring can never unify with a 14-char atom),
% so this clause never actually matched anything and every fixture that
% ever fell OUT of the compiled set left its stale .schedule.json behind
% permanently (caught during the PHASE C2 RULING 2 sweep: two fixtures that
% briefly compiled before a later-added safety check refused them stayed on
% disk, untracked, across repeated re-sweeps).
clear_stale_compiled_outputs(OutDir) :-
    directory_files(OutDir, Entries),
    forall(( member(Entry, Entries),
             ( sub_atom(Entry, _, 3, 0, '.ts') ; sub_atom(Entry, _, 14, 0, '.schedule.json') )
           ),
           ( atomic_list_concat([OutDir, '/', Entry], Path), delete_file(Path) )).

sweep_file(File, Results) :-
    read_all_fixtures(File, Entries),
    findall(Result,
            ( member(entry(Name, Term, Bindings), Entries), sweep_one(File, Name, Term, Bindings, Result) ),
            Results).

sweep_one(File, Name, Term, Bindings, result(Name, File, Bucket, Reason)) :-
    catch(
        ( program_plan(Term-Bindings, Plan),
          lower_program(Plan, Lowered),
          Term = fixture(Name, _Prog, Initial, Schedule, _Expectations),
          Plan = plan(_, _, RelPlans, _, _, _),
          Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
          boot_statements(RelPlans, Initial, LevelStatements, BootStatements),
          call(emit_ts:emit_program, Name, Plan, Lowered, BootStatements, Text),
          out_dir(OutDir),
          format(atom(TsPath), '~w/~w.ts', [OutDir, Name]),
          setup_call_cleanup(open(TsPath, write, TsStream), format(TsStream, "~s", [Text]), close(TsStream)),
          schedule_json(Schedule, ScheduleJson),
          format(atom(SchedulePath), '~w/~w.schedule.json', [OutDir, Name]),
          setup_call_cleanup(open(SchedulePath, write, ScheduleStream), format(ScheduleStream, "~w", [ScheduleJson]), close(ScheduleStream)),
          Bucket = compiled, Reason = none
        ),
        Error,
        classify_error(Error, Bucket, Reason)
    ).

% unsupported_construct(What) is the compiler's own clean-refusal ball
% (analyze.pl/lower.pl/strat.pl); anything else reaching here is an
% UNANTICIPATED failure in this sweep's own harness or a genuine compiler
% bug, reported as its own bucket rather than folded into "unsupported" so
% it gets investigated, never silently swallowed.
classify_error(unsupported_construct(What), unsupported, What) :- !.
classify_error(Error, crash, Error).

% ═══ schedule -> JSON (the IArrivalBatch[] shape v6/tsv2/runtime/types.ts
% declares: one array per tick, each entry {rel, sign, row}) ════════════

schedule_json(Schedule, Json) :-
    maplist(tick_json, Schedule, TickJsons),
    atomic_list_concat(TickJsons, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).

tick_json(Batch, Json) :-
    maplist(arrival_json, Batch, ArrivalJsons),
    atomic_list_concat(ArrivalJsons, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).

arrival_json(+Atom, Json) :- !, arrival_json_signed(Atom, add, Json).
arrival_json(-Atom, Json) :- !, arrival_json_signed(Atom, del, Json).

arrival_json_signed(Atom, Sign, Json) :-
    rel_ref(Atom, Name/_Arity),
    Atom =.. [_ | Args],
    maplist(row_value_json, Args, ArgJsons),
    atomic_list_concat(ArgJsons, ',', RowInner),
    json_string(Name, NameJson),
    format(atom(Json), '{"rel":~w,"sign":"~w","row":[~w]}', [NameJson, Sign, RowInner]).

% Mirrors ticklog.pl's value_json/2 (that file is never touched; this is an
% independent small copy since ticklog.pl declares no module and this file
% must not depend on load order with it -- the compile sweep and the oracle
% dump run as two separate swipl processes, plans header "the grading loop").
row_value_json(Value, Json) :- integer(Value), !, format(atom(Json), '~w', [Value]).
row_value_json(Value, Json) :- compound(Value), !, term_text(Value, Text), json_string(Text, Json).
row_value_json(Value, Json) :- json_string(Value, Json).

term_text(Value, Text) :- atomic(Value), !, format(atom(Text), '~w', [Value]).
term_text(Value, Text) :- compound(Value), !,
    Value =.. [Name | Args], maplist(term_text, Args, ArgTexts),
    atomic_list_concat(ArgTexts, ',', Inner), format(atom(Text), '~w(~w)', [Name, Inner]).

json_string(Value, Json) :-
    format(atom(Raw), '~w', [Value]),
    atom_codes(Raw, Codes),
    escape_json_codes(Codes, EscapedCodes),
    atom_codes(Escaped, EscapedCodes),
    format(atom(Json), '"~w"', [Escaped]).

escape_json_codes([], []).
escape_json_codes([Code | Rest], Out) :-
    ( Code =:= 0'"  -> Escaped = [0'\\, 0'"]
    ; Code =:= 0'\\ -> Escaped = [0'\\, 0'\\]
    ; Code =:= 10   -> Escaped = [0'\\, 0'n]
    ; Code =:= 9    -> Escaped = [0'\\, 0't]
    ; Code < 32     -> format(atom(HexAtom), '\\u~`0t~16r~4|', [Code]), atom_codes(HexAtom, Escaped)
    ; Escaped = [Code]
    ),
    escape_json_codes(Rest, RestOut),
    append(Escaped, RestOut, Out).

% ═══ manifest + console summary ═══════════════════════════════════════════

write_manifest(Results) :-
    maplist(result_json, Results, Jsons),
    atomic_list_concat(Jsons, ',\n  ', Inner),
    format(atom(Text), '[\n  ~w\n]\n', [Inner]),
    out_dir(OutDir), atomic_list_concat([OutDir, '/manifest.json'], Path),
    setup_call_cleanup(open(Path, write, Stream), format(Stream, "~w", [Text]), close(Stream)).

result_json(result(Name, File, Bucket, Reason), Json) :-
    file_base_name(File, Base),
    json_string(Name, NameJson),
    json_string(Base, FileJson),
    json_string(Bucket, BucketJson),
    reason_text(Reason, ReasonText),
    json_string(ReasonText, ReasonJson),
    format(atom(Json), '{"name":~w,"file":~w,"bucket":~w,"reason":~w}', [NameJson, FileJson, BucketJson, ReasonJson]).

reason_text(none, '') :- !.
reason_text(Term, Text) :- format(atom(Text), '~q', [Term]).

summarize(Results) :-
    include(is_bucket(compiled), Results, Compiled),
    include(is_bucket(unsupported), Results, Unsupported),
    include(is_bucket(crash), Results, Crashed),
    length(Results, Total), length(Compiled, CompiledCount),
    length(Unsupported, UnsupportedCount), length(Crashed, CrashedCount),
    format("SWEEP total=~w compiled=~w unsupported=~w crash=~w~n", [Total, CompiledCount, UnsupportedCount, CrashedCount]),
    forall(member(result(Name, _, unsupported, Reason), Unsupported), format("  UNSUPPORTED ~w ~q~n", [Name, Reason])),
    forall(member(result(Name, _, crash, Reason), Crashed), format("  CRASH ~w ~q~n", [Name, Reason])).

is_bucket(Bucket, result(_, _, Bucket, _)).
