% Phase C entry point. Compile every fixture, bucket the result, and write the
% emitted module and the fixture's schedule for runtime replay.
%
% Fixture files are read without consult so source variable identity is kept.
%
% Run: swipl -q -l v6/prolog/compile/sweep.pl -g sweep -g halt

:- module(sweep, [ sweep/0, sweep/1 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(filesex)).
:- use_module(compile, [ program_plan/3, default_intern_mode/1 ]).
:- use_module(lower, [ lower_program/2, boot_statements/7, catalog_decl_rows/6 ]).
:- use_module(emit_ts, [ emit_program/5 ]).
:- use_module('compile/4_emit_jsonschema', [ jsonschema_text/3, option_rows/3 ]).
:- use_module('compile/7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('compile/8_emit_rust_types', [ rust_types_text/3 ]).
:- use_module('conformance/body', [ rel_ref/2 ]).
:- use_module('0_rel_record', [ relplan_column_types/3 ]).
:- use_module('0_type_plane',
              [ type_canonical_json/4,
                canonical_json_text/2, escape_json_codes/2 ]).

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
out_dir(Dir) :- compile_dir(Here), atomic_list_concat([Here, '/compile/out'], Dir).
fixtures_dir(Dir) :- compile_dir(Here), atomic_list_concat([Here, '/conformance/fixtures'], Dir).

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
    default_intern_mode(Mode),
    sweep([intern(Mode)]).

% sweep(+Options): the A/B gate compiles the corpus twice at one commit, once
% per intern mode, and every differing line must be an interning class.
sweep(Options) :-
    out_dir(OutDir), make_directory_path(OutDir),
    clear_stale_compiled_outputs(OutDir),
    fixture_files(Files),
    findall(Result,
            ( member(File, Files), sweep_file(Options, File, FileResults), member(Result, FileResults) ),
            Results),
    write_manifest(Results),
    summarize(Results).

% Remove stale per-fixture outputs before rewriting the compiled set.
clear_stale_compiled_outputs(OutDir) :-
    directory_files(OutDir, Entries),
    forall(( member(Entry, Entries),
             ( sub_atom(Entry, _, 3, 0, '.ts')
             ; sub_atom(Entry, _, 14, 0, '.schedule.json')
             ; sub_atom(Entry, _, 11, 0, '.schema.json')
             ; sub_atom(Entry, _, 9, 0, '.types.rs') )
           ),
           ( atomic_list_concat([OutDir, '/', Entry], Path), delete_file(Path) )).

sweep_file(Options, File, Results) :-
    read_all_fixtures(File, Entries),
    findall(Result,
            ( member(entry(Name, Term, Bindings), Entries), sweep_one(Options, File, Name, Term, Bindings, Result) ),
            Results).

sweep_one(Options, File, Name, Term, Bindings, result(Name, File, Bucket, Reason)) :-
    catch(
        ( program_plan(Term-Bindings, Options, Plan),
          lower_program(Plan, Lowered),
          Term = fixture(Name, _Prog, Initial, Schedule, _Expectations),
          Plan = plan(_, prog(Decls, Rules), Types, RelPlans, _, _, _, _, Mode),
          Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
          boot_statements(Mode, Decls, Types, RelPlans, Initial, LevelStatements, BootStatements),
          call(emit_ts:emit_program, Name, Plan, Lowered, BootStatements, Text),
          out_dir(OutDir),
          format(atom(TsPath), '~w/~w.ts', [OutDir, Name]),
          setup_call_cleanup(open(TsPath, write, TsStream), format(TsStream, "~s", [Text]), close(TsStream)),
          schedule_json(Types, RelPlans, Schedule, ScheduleJson),
          format(atom(SchedulePath), '~w/~w.schedule.json', [OutDir, Name]),
          setup_call_cleanup(open(SchedulePath, write, ScheduleStream), format(ScheduleStream, "~w", [ScheduleJson]), close(ScheduleStream)),
          (   catch( ( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                        SchemaRows, _),
                       option_rows(Decls, SchemaRows, SchemaRowsOpt),
                       jsonschema_text(Name, SchemaRowsOpt, SchemaText) ),
                     _SchemaError,
                     fail )
          ->  format(atom(SchemaPath), '~w/~w.schema.json', [OutDir, Name]),
              setup_call_cleanup(open(SchemaPath, write, SchemaStream),
                                 format(SchemaStream, "~s", [SchemaText]),
                                 close(SchemaStream))
          ;   true
          ),
          (   catch( ( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                        TypeRows, _),
                       option_rows(Decls, TypeRows, TypeRowsOpt),
                       ts_types_text(Name, TypeRowsOpt, TsTypesText) ),
                     _TsTypesError,
                     fail )
          ->  format(atom(TsTypesPath), '~w/~w.types.ts', [OutDir, Name]),
              setup_call_cleanup(open(TsTypesPath, write, TsTypesStream),
                                 format(TsTypesStream, "~s", [TsTypesText]),
                                 close(TsTypesStream))
          ;   true
          ),
          (   catch( ( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                        TypeRows, _),
                       option_rows(Decls, TypeRows, TypeRowsOpt),
                       rust_types_text(Name, TypeRowsOpt, RustTypesText) ),
                     _RustTypesError,
                     fail )
          ->  format(atom(RustTypesPath), '~w/~w.types.rs', [OutDir, Name]),
              setup_call_cleanup(open(RustTypesPath, write, RustTypesStream),
                                 format(RustTypesStream, "~s", [RustTypesText]),
                                 close(RustTypesStream))
          ;   true
          ),
          Bucket = compiled, Reason = none
        ),
        Error,
        classify_error(Error, Bucket, Reason)
    ).

% unsupported_construct(What) is the compiler's own clean-unsupported construct ball
% (analyze.pl/lower.pl/strat.pl); anything else reaching here is an
% UNANTICIPATED failure in this sweep's own harness or a genuine compiler
% bug, reported as its own bucket rather than folded into "unsupported" so
% it gets investigated, never silently swallowed.
classify_error(unsupported_construct(What), unsupported, What) :- !.
classify_error(Error, crash, Error).

% ═══ schedule -> JSON (the IArrivalBatch[] shape v6/tsv2/runtime/types.ts
% declares: one array per tick, each entry {rel, sign, row}) ════════════

schedule_json(Types, RelPlans, Schedule, Json) :-
    maplist(tick_json(Types, RelPlans), Schedule, TickJsons),
    atomic_list_concat(TickJsons, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).

tick_json(Types, RelPlans, Batch, Json) :-
    maplist(arrival_json(Types, RelPlans), Batch, ArrivalJsons),
    atomic_list_concat(ArrivalJsons, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).

arrival_json(Types, RelPlans, +Atom, Json) :- !, arrival_json_signed(Types, RelPlans, Atom, add, Json).
arrival_json(Types, RelPlans, -Atom, Json) :- !, arrival_json_signed(Types, RelPlans, Atom, del, Json).

arrival_json_signed(Types, RelPlans, Atom, Sign, Json) :-
    rel_ref(Atom, Ref), Ref = Name/_Arity,
    Atom =.. [_ | Args],
    ( relplan_column_types(RelPlans, Ref, ColumnTypes) -> true ; ColumnTypes = [] ),
    maplist(arrival_value_json(Types), ColumnTypes, Args, ArgJsons),
    atomic_list_concat(ArgJsons, ',', RowInner),
    json_string(Name, NameJson),
    format(atom(Json), '{"rel":~w,"sign":"~w","row":[~w]}', [NameJson, Sign, RowInner]).

% STRUCT-AS-ROWS: a ref column's schedule entry is a real JSON OBJECT, not the
% prolog term text every other compound column gets. The emitted runtime
% interns it (StructPlane.intern) and the oracle reads the same value out of
% the fixture term, so the two sides are fed one value in the two spellings
% each already speaks.
arrival_value_json(Types, ref(TypeName), Value, Json) :- !,
    type_canonical_json(Types, TypeName, Value, Json).
% A `json` column stores canonical JSON TEXT, so its schedule entry is a JSON
% STRING carrying that text, not a nested JSON value: the emitted runtime
% binds it straight into the TEXT column. canonical_json_text/2 is the single
% canonicalizer both doors use -- json1 will not canonicalize for us at any
% point (json() minifies but PRESERVES key order), so the bytes have to be
% right on the way in or the tick log cannot agree.
arrival_value_json(_, json, Value, Json) :- !,
    canonical_json_text(Value, Text),
    json_string(Text, Json).
arrival_value_json(_, json_list(_), Value, Json) :- !,
    canonical_json_text(Value, Text),
    json_string(Text, Json).
arrival_value_json(_, _, Value, Json) :- row_value_json(Value, Json).

row_value_json(Value, Json) :- integer(Value), !, format(atom(Json), '~w', [Value]).
row_value_json(bool_lit(Boolean), Json) :- !, format(atom(Json), '~w', [Boolean]).
row_value_json(Value, Json) :- float(Value), !,
    canonical_json_text(Value, Json).
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
% A unsupported construct reason holds the program's variables, and `_12345` is the process's
% counter, so the tracked manifest churns on every run unless they are numbered.
reason_text(Term, Text) :-
    copy_term(Term, Copy),
    numbervars(Copy, 0, _),
    format(atom(Text), '~q', [Copy]).

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
