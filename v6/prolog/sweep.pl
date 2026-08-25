% Phase C entry point. Compile every fixture, bucket the result, and write the
% emitted module and the fixture's schedule for runtime replay.
%
% Fixture files are read without consult so source variable identity is kept.
%
% Three entry points over one row-building core:
%
%   sweep, sweep(+Options)  the whole corpus in this process, then the manifest.
%   sweep_shard            the SWEEP_SHARD_INDEX'th slice of SWEEP_JOBS, into a
%                          fragment under out/.sweep-shards/.
%   sweep_merge            folds every fragment back into the one manifest.
%
% A fixture's position in corpus order decides which worker takes it and where
% its manifest row lands, so the row order is the same one the sequential door
% has always written and no worker's speed can move it.
%
% Compiling is skipped when out/sweep.digests already carries this fixture's
% key AND the files that key names still hash to what it recorded.
% SWEEP_FORCE=1 spends the cache. The key is over the compiler's own loaded
% source closure and the fixture's TERM, so a one-fixture edit recompiles one
% fixture and a compiler edit recompiles the corpus.
%
% Each recompiled fixture's wall lands in out/sweep.timings.tsv and the pass
% prints its slowest ten (sweep_timings.pl).
%
% Run: swipl -q -l v6/prolog/sweep.pl -g sweep -g halt

:- module(sweep, [ sweep/0, sweep/1, sweep_shard/0, sweep_merge/0 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(filesex)).
:- use_module(library(readutil)).
:- use_module(library(sha)).
:- use_module(library(time)).
:- use_module(sweep_timings, [ append_timings/3, report_slowest/2 ]).
:- use_module(compile, [ program_plan/3, default_intern_mode/1 ]).
:- use_module('7_lower/lower', [ lower_program/2, boot_statements/7, catalog_decl_rows/6 ]).
:- use_module(emit_ts, [ emit_program/5 ]).
:- use_module('compile/4_emit_jsonschema', [ jsonschema_text/3, option_rows/3 ]).
:- use_module('compile/7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('compile/8_emit_rust_types', [ rust_types_text/3 ]).
:- use_module('0_dot_expand/body', [ rel_ref/2 ]).
:- use_module('3_analyze/0_rel_record', [ relplan_column_types/3 ]).
:- use_module('1_expansion/compile_messages', [dl6_debug/3]).
:- use_module('0_dot_expand/0_type_plane',
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
shard_dir(Dir) :- out_dir(Out), atomic_list_concat([Out, '/.sweep-shards'], Dir).
digest_store_path(Path) :- out_dir(Out), atomic_list_concat([Out, '/sweep.digests'], Path).

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
% One shard of one, so the sequential door and the sharded door build rows
% through the same clauses and cannot drift apart.
sweep(Options) :-
    shard_rows(Options, 0, 1, Rows),
    merge_rows([Rows]).

% ═══ sharded entry points ═══════════════════════════════════════════
% Each worker loads the compiler once and takes the fixtures whose corpus
% position is congruent to its index. Assignment is positional, so it is the
% same on every run whatever order the workers finish in.

sweep_shard :-
    default_intern_mode(Mode),
    env_number('SWEEP_SHARD_INDEX', 0, Index),
    env_number('SWEEP_JOBS', 1, Total),
    shard_rows([intern(Mode)], Index, Total, Rows),
    shard_dir(ShardDir), make_directory_path(ShardDir),
    format(atom(Path), '~w/shard.~w.pl', [ShardDir, Index]),
    length(Rows, Count),
    setup_call_cleanup(
        open(Path, write, Stream),
        ( forall(member(Row, Rows), write_fact(Stream, Row)),
          write_fact(Stream, shard_done(Index, Count)) ),
        close(Stream)).

sweep_merge :-
    env_number('SWEEP_JOBS', 1, Total),
    shard_dir(ShardDir),
    Last is Total - 1,
    numlist(0, Last, Indices),
    maplist(read_shard(ShardDir), Indices, Fragments),
    merge_rows(Fragments),
    delete_directory_and_contents(ShardDir).

% ═══ row building ═══════════════════════════════════════════════════

shard_rows(Options, Index, Total, Rows) :-
    out_dir(OutDir), make_directory_path(OutDir),
    compiler_digest(Digest),
    load_digest_store,
    corpus_entries(Entries),
    findall(Row,
            ( member(entry(Position, File, Name, Term, Bindings), Entries),
              Position mod Total =:= Index,
              shard_row(Options, Digest, File, Name, Term, Bindings, Position, Row) ),
            Rows).

% Corpus order: files in msort order, fixtures in the order their file lists
% them. The position number carries that order across worker boundaries, and
% merge_rows/1 sorts on it, so the manifest keeps the row order it has always
% had.
corpus_entries(Entries) :-
    fixture_files(Files),
    corpus_entries(Files, 0, Entries).

corpus_entries([], _, []).
corpus_entries([File | Rest], Position0, Entries) :-
    read_all_fixtures(File, FileEntries),
    number_entries(FileEntries, File, Position0, Position, Head),
    corpus_entries(Rest, Position, Tail),
    append(Head, Tail, Entries).

number_entries([], _, Position, Position, []).
number_entries([entry(Name, Term, Bindings) | Rest], File, Position0, Position,
               [entry(Position0, File, Name, Term, Bindings) | Tail]) :-
    Position1 is Position0 + 1,
    number_entries(Rest, File, Position1, Position, Tail).

% A cache hit keeps every file the last compile wrote and rebuilds the manifest
% row from the store. A miss drops that fixture's own outputs first, so a
% fixture that used to compile and now does not leaves nothing behind, and no
% worker ever touches a file another worker owns.
%
% The hit is checked against the CONTENT of those files, not their existence.
% scripts/intern-ab.sh compiles the corpus under one intern mode, then the
% other, then back, over the one out/ directory: the third run's key matches
% the first run's, and an existence check would hand it the SECOND run's bytes.
shard_row(Options, Digest, File, Name, Term, Bindings, Position, Row) :-
    fixture_key(Digest, Options, Name, Term, Bindings, Key),
    file_base_name(File, Base),
    (   \+ forced,
        cached_digest(Name, Key, Bucket, ReasonText, Outputs),
        outputs_match(Name, Outputs)
    ->  Row = row(Position, Name, Base, Bucket, ReasonText, Key, hit, Outputs, 0)
    ;   drop_outputs(Name),
        get_time(Start),
        (   sweep_one(Options, File, Name, Term, Bindings, Result)
        ->  true
        ;   Result = failed
        ),
        get_time(End),
        Seconds is End - Start,
        Millis is round(Seconds * 1000),
        (   Seconds > 10
        ->  format("SWEEP_SLOW ~w ~2f~n", [Name, Seconds])
        ;   true
        ),
        (   Result == failed
        ->  format("SWEEP_SILENT_FAIL ~w~n", [Name]), fail
        ;   Result = result(_, _, Bucket, Reason),
            reason_text(Reason, ReasonText),
            capture_outputs(Name, Outputs),
            Row = row(Position, Name, Base, Bucket, ReasonText, Key, miss, Outputs, Millis)
        )
    ).

merge_rows(Fragments) :-
    append(Fragments, Unsorted),
    msort(Unsorted, Rows),
    out_dir(OutDir),
    prune_orphan_outputs(OutDir, Rows),
    write_manifest(OutDir, Rows),
    write_digest_store(Rows),
    findall(Name-Millis,
            member(row(_, Name, _, _, _, _, miss, _, Millis), Rows),
            Timings),
    append_timings(OutDir, compile, Timings),
    report_slowest(compile, Timings),
    summarize(Rows).

read_shard(ShardDir, Index, Rows) :-
    format(atom(Path), '~w/shard.~w.pl', [ShardDir, Index]),
    (   exists_file(Path)
    ->  true
    ;   throw(error(existence_error(sweep_shard_fragment, Path), _))
    ),
    setup_call_cleanup(open(Path, read, Stream), read_facts(Stream, Terms), close(Stream)),
    (   memberchk(shard_done(Index, Count), Terms)
    ->  true
    ;   throw(error(sweep_shard_truncated(Index), _))
    ),
    findall(Row, ( member(Row, Terms), Row = row(_, _, _, _, _, _, _, _, _) ), Rows),
    (   length(Rows, Count)
    ->  true
    ;   throw(error(sweep_shard_short_count(Index), _))
    ).

% ═══ digests ════════════════════════════════════════════════════════
% The compiler half of the key is the use_module graph read back from the
% loader rather than a list kept by hand: source_file/1 enumerates every file
% this process has loaded, and once this file is loaded that is exactly the
% compile door its own imports root. Library files are dropped, a swipl upgrade
% being something a fixture cache cannot see anyway.

compiler_digest(Digest) :-
    compile_dir(Here),
    atom_length(Here, Length),
    findall(Relative-Path,
            ( source_file(Path),
              sub_atom(Path, 0, Length, _, Here),
              sub_atom(Path, Length, _, 0, Relative) ),
            Pairs0),
    msort(Pairs0, Pairs),
    findall(Line,
            ( member(Relative-Path, Pairs),
              file_sha256(Path, FileHash),
              format(atom(Line), '~w ~w', [Relative, FileHash]) ),
            Lines),
    atomic_list_concat(Lines, '\n', Text),
    text_sha256(Text, Digest).

% The fixture half is the TERM, not the file, so editing one fixture in a file
% that holds twenty-eight of them recompiles one. Options are in the key
% because intern mode changes the emitted text (scripts/intern-ab.sh compiles
% the corpus twice at one commit and must not be handed the other mode's
% output).
fixture_key(Digest, Options, Name, Term, Bindings, Key) :-
    copy_term(Term-Bindings, Copy),
    numbervars(Copy, 0, _),
    canonical_term_text(Copy, Rendered),
    canonical_term_text(Options, OptionsText),
    atomic_list_concat([Digest, OptionsText, Name, Rendered], '\n\u0001\n', Joined),
    text_sha256(Joined, Key).

canonical_term_text(Term, Text) :-
    with_output_to(atom(Text),
                   write_term(Term, [quoted(true), ignore_ops(true), numbervars(true)])).

file_sha256(Path, Hex) :-
    read_file_to_string(Path, Bytes, [encoding(octet)]),
    sha_hash(Bytes, Hash, [algorithm(sha256), encoding(octet)]),
    hash_atom(Hash, Hex).

text_sha256(Text, Hex) :-
    sha_hash(Text, Hash, [algorithm(sha256), encoding(utf8)]),
    hash_atom(Hash, Hex).

:- dynamic(cached_digest/5).

% A truncated store is a miss, never a wrong answer: every key is derived from
% content, so a partial read can only fail to match.
load_digest_store :-
    retractall(cached_digest(_, _, _, _, _)),
    digest_store_path(Path),
    (   exists_file(Path)
    ->  catch(setup_call_cleanup(open(Path, read, Stream),
                                 read_facts(Stream, Terms),
                                 close(Stream)),
              _,
              Terms = []),
        forall(member(digest(Name, Key, Bucket, ReasonText, Outputs), Terms),
               assertz(cached_digest(Name, Key, Bucket, ReasonText, Outputs)))
    ;   true
    ).

write_digest_store(Rows) :-
    digest_store_path(Path),
    setup_call_cleanup(
        open(Path, write, Stream),
        forall(member(row(_, Name, _, Bucket, ReasonText, Key, _, Outputs, _), Rows),
               write_fact(Stream, digest(Name, Key, Bucket, ReasonText, Outputs))),
        close(Stream)).

% ignore_ops so a fragment stays readable whatever operator table the fixture
% directives left behind in the writing process.
write_fact(Stream, Fact) :-
    write_term(Stream, Fact, [quoted(true), ignore_ops(true)]),
    write(Stream, '.'), nl(Stream).

read_facts(Stream, Terms) :-
    read_term(Stream, Term, []),
    (   Term == end_of_file
    ->  Terms = []
    ;   Terms = [Term | Rest], read_facts(Stream, Rest)
    ).

% ═══ per-fixture output files ═══════════════════════════════════════

output_suffix('.ts').
output_suffix('.schedule.json').
output_suffix('.schema.json').
output_suffix('.types.ts').
output_suffix('.types.rs').

output_path(Name, Suffix, Path) :-
    out_dir(OutDir), atomic_list_concat([OutDir, '/', Name, Suffix], Path).

drop_outputs(Name) :-
    forall(( output_suffix(Suffix), output_path(Name, Suffix, Path), exists_file(Path) ),
           delete_file(Path)).

% Called straight after a compile, with drop_outputs/1 having emptied the slot
% first, so what is on disk is what this compile wrote.
capture_outputs(Name, Outputs) :-
    findall(Suffix-Hash,
            ( output_suffix(Suffix), output_path(Name, Suffix, Path),
              exists_file(Path), file_sha256(Path, Hash) ),
            Outputs).

outputs_match(Name, Outputs) :-
    forall(member(Suffix-Hash, Outputs),
           ( output_path(Name, Suffix, Path), exists_file(Path),
             file_sha256(Path, Hash) )).

:- dynamic(claimed_output/2).

% What clear_stale_compiled_outputs/1 used to do at the head of a run, done at
% the end and only to files no fixture claims. Emptying the whole set up front
% would delete exactly the outputs the digest cache exists to keep; SWEEP_FORCE
% spends the cache in the shell driver instead.
prune_orphan_outputs(OutDir, Rows) :-
    retractall(claimed_output(_, _)),
    forall(( member(row(_, Name, _, _, _, _, _, Outputs, _), Rows),
             member(Suffix-_, Outputs) ),
           assertz(claimed_output(Name, Suffix))),
    directory_files(OutDir, Entries),
    forall(( member(Entry, Entries),
             output_entry(Entry, Name, Suffix),
             \+ claimed_output(Name, Suffix) ),
           ( atomic_list_concat([OutDir, '/', Entry], Path), delete_file(Path) )).

% Longest suffix wins: `x.types.ts` is a .types.ts file, never an `x.types`
% that happens to end in .ts.
output_entry(Entry, Name, Suffix) :-
    member(Suffix, ['.schedule.json', '.schema.json', '.types.ts', '.types.rs', '.ts']),
    atom_length(Suffix, Length),
    sub_atom(Entry, Before, Length, 0, Suffix),
    Before > 0,
    !,
    sub_atom(Entry, 0, Before, _, Name).

% ═══ environment ════════════════════════════════════════════════════

env_number(Name, Default, Value) :-
    (   getenv(Name, Text), atom_number(Text, Number), integer(Number), Number >= 0
    ->  Value = Number
    ;   Value = Default
    ).

forced :- getenv('SWEEP_FORCE', Text), Text \== '0', Text \== ''.

sweep_one(Options, File, Name, Term, Bindings, result(Name, File, Bucket, Reason)) :-
    dl6_debug(sweep, "fixture ~w (~w)", [Name, File]),
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
                       bounded_emit(Name, jsonschema, jsonschema_text(Name, SchemaRowsOpt, SchemaText)) ),
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
                       bounded_emit(Name, ts_types, ts_types_text(Name, TypeRowsOpt, TsTypesText)) ),
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
                       bounded_emit(Name, rust_types, rust_types_text(Name, TypeRowsOpt, RustTypesText)) ),
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
    ),
    dl6_debug(sweep, "~w bucket=~w reason=~q", [Name, Bucket, Reason]).

% unsupported_construct(What) is the compiler's own clean-unsupported construct ball
% (analyze.pl/lower.pl/strat.pl); anything else reaching here is an
% UNANTICIPATED failure in this sweep's own harness or a genuine compiler
% bug, reported as its own bucket rather than folded into "unsupported" so
% it gets investigated, never silently swallowed.
classify_error(unsupported_construct(What), unsupported, What) :- !.
classify_error(Error, crash, Error).


% ═══ bounded optional emitters ══════════════════════════════════════
% catch/3 cannot catch a goal that never comes back, and one looping emitter
% takes the whole corpus with it: the sweep stops dead at that fixture's
% position and every later fixture goes unmeasured. Each optional emitter gets
% its own alarm and names itself when the alarm fires, so a loop reads as one
% fixture's defect instead of a stalled sweep. The two REQUIRED writes, the
% module and the schedule, stay unbounded: those failing is a compile failure
% and already has a bucket.
%
% The budget is the 10-second law's number, not a generous one: the corpus's
% next-slowest optional emit is under a tenth of a second, so nothing
% legitimate comes near the alarm and a longer default only spends more wall on
% a fixture that is already looping.
bounded_emit(Name, Step, Goal) :-
    env_number('SWEEP_EMIT_BUDGET_S', 10, Seconds),
    catch(call_with_time_limit(Seconds, Goal),
          Error,
          ( Error == time_limit_exceeded
          ->  format("SWEEP_EMIT_TIMEOUT ~w ~w ~ws~n", [Name, Step, Seconds]),
              fail
          ;   fail
          )).

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

write_manifest(OutDir, Rows) :-
    maplist(row_json, Rows, Jsons),
    atomic_list_concat(Jsons, ',\n  ', Inner),
    format(atom(Text), '[\n  ~w\n]\n', [Inner]),
    atomic_list_concat([OutDir, '/manifest.json'], Path),
    setup_call_cleanup(open(Path, write, Stream), format(Stream, "~w", [Text]), close(Stream)).

row_json(row(_, Name, Base, Bucket, ReasonText, _, _, _, _), Json) :-
    json_string(Name, NameJson),
    json_string(Base, FileJson),
    json_string(Bucket, BucketJson),
    json_string(ReasonText, ReasonJson),
    format(atom(Json), '{"name":~w,"file":~w,"bucket":~w,"reason":~w}', [NameJson, FileJson, BucketJson, ReasonJson]).

reason_text(none, '') :- !.
% A unsupported construct reason holds the program's variables, and `_12345` is the process's
% counter, so the tracked manifest churns on every run unless they are numbered.
reason_text(Term, Text) :-
    copy_term(Term, Copy),
    numbervars(Copy, 0, _),
    format(atom(Text), '~q', [Copy]).

% Reasons print through the same numbervars'd text the manifest carries, so a
% restatement of the summary is comparable between runs. The raw term this used
% to print held the process's own variable counter and never repeated.
summarize(Rows) :-
    include(is_bucket(compiled), Rows, Compiled),
    include(is_bucket(unsupported), Rows, Unsupported),
    include(is_bucket(crash), Rows, Crashed),
    include(is_hit, Rows, Hits),
    length(Rows, Total), length(Compiled, CompiledCount),
    length(Unsupported, UnsupportedCount), length(Crashed, CrashedCount),
    length(Hits, HitCount), Recompiled is Total - HitCount,
    dl6_debug(sweep, "total=~w compiled=~w unsupported=~w crash=~w cache_hit=~w",
              [Total, CompiledCount, UnsupportedCount, CrashedCount, HitCount]),
    format("SWEEP total=~w compiled=~w unsupported=~w crash=~w~n", [Total, CompiledCount, UnsupportedCount, CrashedCount]),
    format("SWEEP_CACHE hit=~w recompiled=~w~n", [HitCount, Recompiled]),
    forall(member(row(_, Name, _, unsupported, ReasonText, _, _, _, _), Unsupported), format("  UNSUPPORTED ~w ~w~n", [Name, ReasonText])),
    forall(member(row(_, Name, _, crash, ReasonText, _, _, _, _), Crashed), format("  CRASH ~w ~w~n", [Name, ReasonText])).

is_bucket(Bucket, row(_, _, _, Bucket, _, _, _, _, _)).

is_hit(row(_, _, _, _, _, _, hit, _, _)).
