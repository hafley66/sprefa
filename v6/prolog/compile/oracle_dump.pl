% Phase C oracle side. Run each fixture through ticklog.pl and write the
% captured JSONL output, reporting engine rejection paths as ORACLE_THROW.
%
% No `:- module(...)` header, matching ticklog.pl/go.pl themselves: this
% file has to call fixture/5, print_ticklog/3, and run_program/5 unqualified,
% which only works loaded into the same (user) context ticklog.pl loads into.
%
% Run: swipl -q -l v6/prolog/compile/oracle_dump.pl -g dump_all -g halt

:- ensure_loaded('../conformance/ticklog').
:- use_module(library(sha)).
:- use_module(library(readutil)).
:- use_module(library(time)).
:- use_module('../sweep_timings', [ append_timings/3, report_slowest/2 ]).

:- dynamic(oracle_dump_dir_fact/1).
:- prolog_load_context(directory, Here), atomic_list_concat([Here, '/out'], OutDir),
   assertz(oracle_dump_dir_fact(OutDir)).

:- dynamic(oracle_root_fact/1).
:- prolog_load_context(directory, Here), file_directory_name(Here, Root),
   assertz(oracle_root_fact(Root)).

dump_all :-
    oracle_dump_dir_fact(OutDir),
    ( exists_directory(OutDir) -> true ; make_directory(OutDir) ),
    engine_digest(EngineDigest),
    load_oracle_digests,
    findall(Name, fixture(Name, _, _, _, _), Names),
    findall(Entry,
            ( member(Name, Names), dump_entry(OutDir, EngineDigest, Name, Entry) ),
            Entries),
    write_oracle_digests(OutDir, Entries),
    findall(Name-Millis, member(entry(Name, _, _, miss, Millis, _), Entries), Timings),
    append_timings(OutDir, oracle, Timings),
    write_oracle_timings(OutDir, Entries),
    report_slowest(oracle, Timings),
    include(is_oracle_hit, Entries, Hits),
    include(is_oracle_capped, Entries, Cappeds),
    length(Entries, Total), length(Hits, HitCount), length(Cappeds, CappedCount),
    Redumped is Total - HitCount,
    format("ORACLE_CACHE hit=~w redumped=~w capped=~w~n",
           [HitCount, Redumped, CappedCount]).

is_oracle_hit(entry(_, _, _, hit, _, _)).
is_oracle_capped(entry(_, _, _, _, _, capped)).

% Ranks this stage's own work alone; sweep.timings.tsv is the cross-stage
% ledger and has no column for the cap.
oracle_timings_path(OutDir, Path) :-
    atomic_list_concat([OutDir, '/oracle.timings.tsv'], Path).

write_oracle_timings(OutDir, Entries) :-
    oracle_timings_path(OutDir, Path),
    findall(Millis-Name-Capped,
            member(entry(Name, _, _, miss, Millis, Capped), Entries),
            Pairs),
    sort(0, @>=, Pairs, Sorted),
    setup_call_cleanup(
        open(Path, write, Stream),
        ( format(Stream, "fixture\tms\tcapped~n", []),
          forall(member(Millis-Name-Capped, Sorted),
                 format(Stream, "~w\t~w\t~w~n", [Name, Millis, Capped])) ),
        close(Stream)).

% ═══ snapshot cache ═══
% out/*.oracle.jsonl and out/*.oracle.final.jsonl are frozen snapshots
% (conformance/rulings.pl, oracle_demoted_to_snapshots), so a fixture is
% re-dumped only when its own program/initial/schedule changed or the engine
% under it did. Expectations are out of the key: this stage never reads them.
%
% A hit is checked against the CONTENT of the snapshot files, so a hand-edited
% snapshot is re-dumped rather than trusted, and a fixture whose oracle threw
% (no files at all) is a hit on an empty output list.
%
% A miss drops the fixture's own snapshots BEFORE re-dumping. A fixture that
% used to dump and now throws otherwise keeps a snapshot written by an engine
% that no longer exists, and sweep.ts grades the emitted module against it:
% the missing tick log is exactly how that script tells a `rejection` (both
% doors refuse the schedule) from an `emitted_crash` (a defect).
dump_entry(OutDir, EngineDigest, Name, entry(Name, Key, Outputs, Hit, Millis, Capped)) :-
    oracle_key(EngineDigest, Name, Key),
    (   \+ oracle_forced,
        cached_oracle(Name, Key, Cached),
        oracle_outputs_match(OutDir, Name, Cached)
    ->  Outputs = Cached, Hit = hit, Millis = 0, Capped = no
    ;   drop_oracle_outputs(OutDir, Name),
        get_time(Start),
        dump_one_capped(OutDir, Name, Capped),
        get_time(End),
        Seconds is End - Start,
        Millis is round(Seconds * 1000),
        (   Seconds > 10
        ->  format("SWEEP_SLOW ~w ~2f~n", [Name, Seconds])
        ;   true
        ),
        capture_oracle_outputs(OutDir, Name, Outputs),
        Hit = miss
    ).

% Per fixture, not per stage: one slow fixture used to run out sweep.sh's
% whole-stage budget and take every fixture after it down with the process.
oracle_fixture_budget(Seconds) :-
    (   getenv('SWEEP_ORACLE_FIXTURE_BUDGET_S', Text),
        atom_number(Text, Value),
        number(Value),
        Value > 0
    ->  Seconds = Value
    ;   Seconds = 10
    ).

% call_with_time_limit/2 raises inside the fixture's own goal, so dump_one_/2
% re-throws time_limit_exceeded instead of reporting it as an ORACLE_THROW.
dump_one_capped(OutDir, Name, Capped) :-
    oracle_fixture_budget(Budget),
    catch(( call_with_time_limit(Budget, dump_one(OutDir, Name)), Capped = no ),
          time_limit_exceeded,
          ( Capped = capped,
            drop_oracle_outputs(OutDir, Name),
            format("ORACLE_CAPPED ~w ~w~n", [Name, Budget]) )).

oracle_suffix('.oracle.jsonl').
oracle_suffix('.oracle.final.jsonl').

oracle_path(OutDir, Name, Suffix, Path) :-
    atomic_list_concat([OutDir, '/', Name, Suffix], Path).

drop_oracle_outputs(OutDir, Name) :-
    forall(( oracle_suffix(Suffix), oracle_path(OutDir, Name, Suffix, Path),
             exists_file(Path) ),
           delete_file(Path)).

capture_oracle_outputs(OutDir, Name, Outputs) :-
    findall(Suffix-Hash,
            ( oracle_suffix(Suffix), oracle_path(OutDir, Name, Suffix, Path),
              exists_file(Path), oracle_file_sha256(Path, Hash) ),
            Outputs).

oracle_outputs_match(OutDir, Name, Outputs) :-
    findall(Suffix, ( oracle_suffix(Suffix), oracle_path(OutDir, Name, Suffix, Path),
                      exists_file(Path) ), Present),
    findall(Suffix, member(Suffix-_, Outputs), Expected),
    Present == Expected,
    forall(member(Suffix-Hash, Outputs),
           ( oracle_path(OutDir, Name, Suffix, Path),
             oracle_file_sha256(Path, Hash) )).

% The engine half of the key is the loaded source closure minus the fixture
% files: go.pl ensure_loads every conformance/fixtures/*.pl, and folding those
% into one digest would re-dump the whole corpus for a one-fixture edit. The
% per-fixture half already covers the fixture's own text.
engine_digest(Digest) :-
    oracle_root_fact(Root),
    atom_length(Root, Length),
    findall(Relative-Path,
            ( source_file(Path),
              sub_atom(Path, 0, Length, _, Root),
              sub_atom(Path, Length, _, 0, Relative),
              \+ sub_atom(Relative, _, _, _, '/conformance/fixtures/') ),
            Pairs0),
    msort(Pairs0, Pairs),
    findall(Line,
            ( member(Relative-Path, Pairs),
              oracle_file_sha256(Path, FileHash),
              format(atom(Line), '~w ~w', [Relative, FileHash]) ),
            Lines),
    atomic_list_concat(Lines, '\n', Text),
    oracle_text_sha256(Text, Digest).

oracle_key(EngineDigest, Name, Key) :-
    once(fixture(Name, Prog, Initial, Schedule, _Expectations)),
    copy_term(prog(Prog, Initial, Schedule), Copy),
    numbervars(Copy, 0, _),
    with_output_to(atom(Rendered),
                   write_term(Copy, [quoted(true), ignore_ops(true), numbervars(true)])),
    atomic_list_concat([EngineDigest, Name, Rendered], '\n\u0001\n', Joined),
    oracle_text_sha256(Joined, Key).

oracle_file_sha256(Path, Hex) :-
    read_file_to_string(Path, Bytes, [encoding(octet)]),
    sha_hash(Bytes, Hash, [algorithm(sha256), encoding(octet)]),
    hash_atom(Hash, Hex).

oracle_text_sha256(Text, Hex) :-
    sha_hash(Text, Hash, [algorithm(sha256), encoding(utf8)]),
    hash_atom(Hash, Hex).

:- dynamic(cached_oracle/3).

oracle_digest_path(OutDir, Path) :-
    atomic_list_concat([OutDir, '/oracle.digests'], Path).

% A truncated or unreadable store is a miss, never a wrong answer: every key is
% derived from content.
load_oracle_digests :-
    retractall(cached_oracle(_, _, _)),
    oracle_dump_dir_fact(OutDir),
    oracle_digest_path(OutDir, Path),
    (   exists_file(Path)
    ->  catch(setup_call_cleanup(open(Path, read, Stream),
                                 read_oracle_facts(Stream, Terms),
                                 close(Stream)),
              _,
              Terms = []),
        forall(member(oracle_digest(Name, Key, Outputs), Terms),
               assertz(cached_oracle(Name, Key, Outputs)))
    ;   true
    ).

% A capped fixture gets no row, so the next pass retries it rather than
% caching the truncated dump under a key that would then hit.
write_oracle_digests(OutDir, Entries) :-
    oracle_digest_path(OutDir, Path),
    setup_call_cleanup(
        open(Path, write, Stream),
        forall(( member(entry(Name, Key, Outputs, _, _, Capped), Entries),
                 Capped \== capped ),
               ( write_term(Stream, oracle_digest(Name, Key, Outputs),
                            [quoted(true), ignore_ops(true)]),
                 write(Stream, '.'), nl(Stream) )),
        close(Stream)).

read_oracle_facts(Stream, Terms) :-
    read_term(Stream, Term, []),
    (   Term == end_of_file
    ->  Terms = []
    ;   Terms = [Term | Rest], read_oracle_facts(Stream, Rest)
    ).

oracle_forced :-
    ( getenv('ORACLE_FORCE', Text) ; getenv('SWEEP_FORCE', Text) ),
    Text \== '0', Text \== '', !.

% A fixture whose oracle FAILS rather than throws used to take the whole dump
% down with it, silently and with no name: forall/2 propagated the failure and
% the process exited 1 after the last ORACLE_OK line.
dump_one(OutDir, Name) :-
    ( dump_one_(OutDir, Name) -> true ; format("ORACLE_FAIL ~w~n", [Name]) ).

dump_one_(OutDir, Name) :-
    catch(
        ( fixture(Name, Prog, Initial, Schedule, _Expectations),
          with_output_to(string(Text), print_ticklog(Prog, Initial, Schedule)),
          format(atom(OutFile), '~w/~w.oracle.jsonl', [OutDir, Name]),
          setup_call_cleanup(open(OutFile, write, Stream), format(Stream, "~w", [Text]), close(Stream)),
          dump_final_state(OutDir, Name, Prog, Initial, Schedule),
          format("ORACLE_OK ~w~n", [Name])
        ),
        Error,
        (   Error == time_limit_exceeded
        ->  throw(Error)
        ;   format("ORACLE_THROW ~w ~q~n", [Name, Error]),
            oracle_throw_marker(OutDir, Name, Error)
        )
    ).

% ═══ final-state leg (EXPRESSION + AGGREGATE LIFT arc) ══════════════════════
% The tick log alone cannot grade a fixture whose Schedule is EMPTY: both
% sides print zero lines and the sweep calls that IDENTICAL. SCOREBOARD.md
% already flags four such vacuous passes; the expression and aggregate
% buckets this arc lifts are almost entirely empty-schedule fixtures (25 of
% the 30 target fixtures), so a tick-log-only grade would have claimed them
% on no evidence at all.
%
% This writes the SAME envelope shape the tick log uses, over run_program/5's
% FinalAll (store rows + the final level closure, engine.pl:run_ticks/7's own
% first clause) instead of over one tick's deltas:
%
%   {"final":{"relName":[[..],[..]],...}}
%
% Rel names ascending, rows sorted lexicographically by their own JSON text
% (the same msort/2 rule rel_delta_json/3 applies to add/del), a rel with
% zero rows omitted, values encoded by ticklog.pl's own value_json/2 (reused,
% never reimplemented: integers as JSON numbers, atoms and canonical compound
% text as JSON strings). Log-rel duplicate rows survive as duplicate entries,
% since FinalAll is a multiset (engine.pl store_rows/2 msorts without
% dedup).
dump_final_state(OutDir, Name, Prog, Initial, Schedule) :-
    run_program(Prog, Initial, Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, Line),
    format(atom(FinalFile), '~w/~w.oracle.final.jsonl', [OutDir, Name]),
    setup_call_cleanup(open(FinalFile, write, Stream),
                       format(Stream, "~w~n", [Line]),
                       close(Stream)).

% keysort/2 on Ref-Row puts the rel names in the same ascending order sort/2 on
% the bare refs did, in one pass over FinalAll instead of one pass per rel.
final_state_line(FinalAll, Line) :-
    findall(Ref-Row, ( member(Row, FinalAll), rel_ref(Row, Ref) ), Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Grouped),
    findall(RelJson, ( member(Ref-Rows, Grouped), final_rel_json(Ref, Rows, RelJson) ), RelJsons),
    atomic_list_concat(RelJsons, ',', Inner),
    format(atom(Line), '{"final":{~w}}', [Inner]).

final_rel_json(Name/_Arity, Rows, Json) :-
    maplist(row_json, Rows, RowJsonsRaw),
    msort(RowJsonsRaw, RowJsons),
    atomic_list_concat(RowJsons, ',', Inner),
    format(atom(Json), '"~w":[~w]', [Name, Inner]).

% The marker lets the snapshots-only sweep tell "minted, throws" from
% "never minted" without running the engine.
oracle_throw_marker(OutDir, Name, Error) :-
    format(atom(File), '~w/~w.oracle.throw', [OutDir, Name]),
    setup_call_cleanup(open(File, write, S), format(S, "~q.~n", [Error]), close(S)).
