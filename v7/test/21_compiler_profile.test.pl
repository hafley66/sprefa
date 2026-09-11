:- begin_tests(dl7_compiler_profile).

:- use_module(library(http/json), [json_read_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module(library(filesex), [make_directory_path/1,
                                 directory_file_path/3]).
:- use_module('../bench/1_compiler_profile',
              [ profile_main/0,
                require_fixture/2,
                duplicate_groups/2,
                duplicate_groups_from_hashes/2,
                duplicate_occurrence_summary/2,
                deterministic_profile_text/2
              ]).

fixture_path('v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7').

setup(profile_runs(_)).

%% ------------------------------------------------------------------
%% Synthetic duplicate semantics
%% ------------------------------------------------------------------

% 0% duplicate, repeated items, and count-descending order.
test(duplicate_groups_zero_repeats_and_order) :-
    duplicate_groups([a, a, b, c, c, c], Counts),
    Counts == [3-c, 2-a, 1-b].

test(duplicate_groups_all_unique_is_zero) :-
    duplicate_groups([a, b, c], Counts),
    Counts == [1-c, 1-b, 1-a].

% Two ==-distinct identities forced into one hash bucket stay distinct. Hash
% equality alone must never establish duplication.
test(duplicate_groups_reject_hash_collision) :-
    duplicate_groups_from_hashes([7-a, 7-b], Counts),
    Counts = [1-X, 1-Y],
    msort([X, Y], [a, b]).

test(duplicate_groups_merge_exact_identity) :-
    duplicate_groups_from_hashes([7-a, 7-a, 7-b], Counts),
    Counts == [2-a, 1-b].

% The same term under two semantic roles is never merged, so its occurrences
% do not inflate either category's duplicate count.
test(duplicate_summary_separates_categories) :-
    Occurrences = [ stratification_input-call(r, [a]),
                    evaluator_installed_seeds-call(r, [a]) ],
    duplicate_occurrence_summary(Occurrences, Report),
    get_dict(categories, Report, Entries),
    member(Entry, Entries),
    get_dict(category, Entry, stratification_input),
    get_dict(total, Entry, 1),
    get_dict(unique, Entry, 1),
    get_dict(duplicate, Entry, 0),
    get_dict(percent, Entry, Percent0),
    Percent0 =:= 0.0,
    !.

test(duplicate_summary_exact_percentages) :-
    Occurrences = [ stratification_input-call(r, [a]),
                    stratification_input-call(r, [a]),
                    stratification_input-call(r, [b]),
                    evaluator_installed_seeds-call(s, [x]) ],
    duplicate_occurrence_summary(Occurrences, Report),
    get_dict(categories, Report, Entries),
    member(Stratification, Entries),
    get_dict(category, Stratification, stratification_input),
    get_dict(total, Stratification, 3),
    get_dict(unique, Stratification, 2),
    get_dict(duplicate, Stratification, 1),
    get_dict(percent, Stratification, Percent1),
    Percent1 =:= 33.33,
    get_dict(overall, Report, Overall),
    get_dict(total, Overall, 4),
    get_dict(unique, Overall, 3),
    get_dict(duplicate, Overall, 1),
    get_dict(percent, Overall, Percent2),
    Percent2 =:= 25.0.

test(duplicate_summary_zero_when_all_unique) :-
    duplicate_occurrence_summary(
        [stratification_input-call(r, [a]),
         stratification_input-call(r, [b])],
        Report),
    get_dict(overall, Report, Overall),
    get_dict(total, Overall, 2),
    get_dict(duplicate, Overall, 0),
    get_dict(percent, Overall, OverallPercent),
    OverallPercent =:= 0.0.

%% ------------------------------------------------------------------
%% Real profile artifacts, produced twice in fresh processes
%% ------------------------------------------------------------------

bench_module_path(Path) :-
    source_file(dl7_compiler_profile:profile_main, Path).

run_profile(Fixture, Directory, ExitCode, Stderr) :-
    bench_module_path(BenchPath),
    process_create(
        path(swipl),
        [ '-q', '-s', BenchPath, '-g', profile_main, '-t', halt, '--',
          Fixture, Directory ],
        [ stdout(null), stderr(pipe(ErrorStream)), process(Process) ]),
    read_string(ErrorStream, _, Stderr),
    close(ErrorStream),
    process_wait(Process, exit(ExitCode)).

fresh_directory(Directory) :-
    tmp_file(dl7_profile_run, Created),
    (   exists_file(Created)
    ->  delete_file(Created)
    ;   true
    ),
    make_directory_path(Created),
    Directory = Created.

profile_runs(run(First, Second)) :-
    (   nb_current(dl7_profile_run_pair, Pair)
    ->  Pair = run(First, Second)
    ;   fixture_path(Fixture),
        fresh_directory(First),
        run_profile(Fixture, First, 0, _),
        fresh_directory(Second),
        run_profile(Fixture, Second, 0, _),
        nb_setval(dl7_profile_run_pair, run(First, Second)),
        Pair = run(First, Second)
    ).

artifact(Directory, Name, Path) :-
    directory_file_path(Directory, Name, Path).

read_artifact(Directory, Name, Text) :-
    artifact(Directory, Name, Path),
    read_file_to_string(Path, Text, []).

read_profile_dict(Directory, Dict) :-
    artifact(Directory, '0_profile.json', Path),
    setup_call_cleanup(
        open(Path, read, Stream, [encoding(utf8)]),
        json_read_dict(Stream, Dict, [value_string_as(atom)]),
        close(Stream)).

test(all_five_artifacts_written) :-
    profile_runs(run(First, _)),
    forall(member(Name, ['0_profile.json', '1_folded.txt',
                         '2_duplicates.tsv', '3_flamechart.html',
                         '4_summary.txt']),
           ( artifact(First, Name, Path), exists_file(Path) )).

test(json_structure_has_stable_ids_and_existing_parents) :-
    profile_runs(run(First, _)),
    read_profile_dict(First, Dict),
    get_dict(span_count, Dict, Count),
    get_dict(spans, Dict, Spans),
    length(Spans, Count),
    findall(SpanId, ( member(Span, Spans), get_dict(id, Span, SpanId) ), Ids),
    forall(member(Span, Spans),
           ( get_dict(parent, Span, Parent),
             (   Parent == null
             ->  true
             ;   memberchk(Parent, Ids)
             ) )),
    get_dict(width_metric, Dict, inferences),
    get_dict(deterministic, Dict, true).

test(json_has_expected_hierarchy) :-
    profile_runs(run(First, _)),
    read_profile_dict(First, Dict),
    get_dict(spans, Dict, Spans),
    Spans = [RootSpan | _],
    get_dict(parent, RootSpan, RootParent),
    RootParent == null,
    findall(Category,
            ( member(Span, Spans), get_dict(category, Span, Category) ),
            Categories0),
    sort(Categories0, Categories),
    forall(member(Required, [compile, phase, round, stratum,
                             install, collect, cleanup]),
           memberchk(Required, Categories)),
    % Every stratum span owns an install, collect, and cleanup child.
    findall(StratumId,
            ( member(Span, Spans),
              get_dict(category, Span, stratum),
              get_dict(id, Span, StratumId) ),
            StratumIds),
    StratumIds \== [],
    forall(member(ChildCategory, [install, collect, cleanup]),
           ( member(StratumId, StratumIds),
             member(Child, Spans),
             get_dict(parent, Child, StratumId),
             get_dict(category, Child, ChildCategory) )).

test(folded_tsv_summary_and_json_are_deterministic) :-
    profile_runs(run(First, Second)),
    forall(member(Name, ['1_folded.txt', '2_duplicates.tsv',
                         '4_summary.txt']),
           ( read_artifact(First, Name, Text1),
             read_artifact(Second, Name, Text2),
             Text1 == Text2 )),
    read_profile_dict(First, Dict1),
    read_profile_dict(Second, Dict2),
    deterministic_profile_text(Dict1, Projected1),
    deterministic_profile_text(Dict2, Projected2),
    Projected1 == Projected2.

test(duplicate_tsv_names_columns_and_percent) :-
    profile_runs(run(First, _)),
    read_artifact(First, '2_duplicates.tsv', Tsv),
    split_string(Tsv, "\n", "", [Header | _]),
    forall(member(Column, ["category", "total", "unique", "duplicate",
                           "percent", "top_repeated"]),
           sub_string(Header, _, _, _, Column)),
    sub_string(Tsv, _, _, _, "overall").

test(summary_names_denominator_and_formula) :-
    profile_runs(run(First, _)),
    read_artifact(First, '4_summary.txt', Summary),
    sub_string(Summary, _, _, _, "duplicate-work denominator"),
    sub_string(Summary, _, _, _, "duplicate-work formula"),
    sub_string(Summary, _, _, _, "duplicate_inference_percent: unavailable"),
    sub_string(Summary, _, _, _, "inferences").

test(html_embeds_profile_json_and_labels) :-
    profile_runs(run(First, _)),
    read_artifact(First, '3_flamechart.html', Html),
    read_artifact(First, '0_profile.json', FileText),
    string_concat(Trimmed, "\n", FileText),
    sub_string(Html, _, _, _, Trimmed),
    sub_string(Html, _, _, _, "profile-data"),
    forall(member(Label, ["compile", "install", "collect", "cleanup",
                          "round_1"]),
           sub_string(Html, _, _, _, Label)).

test(unknown_source_exits_nonzero_with_stage) :-
    fresh_directory(Directory),
    run_profile('v7/test/fixtures/not_a_real_fixture.dl7',
                Directory, ExitCode, Stderr),
    ExitCode == 2,
    sub_string(Stderr, _, _, _, "stage=source").

test(require_fixture_rejects_unknown) :-
    fixture_path(Fixture),
    require_fixture(Fixture, _),
    \+ require_fixture('v7/test/fixtures/not_a_real_fixture.dl7', _).

shell_script_path(Path) :-
    source_file(dl7_compiler_profile:profile_main, BenchPath),
    file_directory_name(BenchPath, BenchDirectory),
    directory_file_path(BenchDirectory, '2_compiler_flamechart.sh', Path).

test(shell_writes_five_artifacts) :-
    fixture_path(Fixture),
    fresh_directory(Directory),
    shell_script_path(Script),
    process_create(path(bash), [Script, Fixture, Directory],
                   [stdout(null), stderr(null), process(Process)]),
    process_wait(Process, exit(0)),
    forall(member(Name, ['0_profile.json', '1_folded.txt',
                         '2_duplicates.tsv', '3_flamechart.html',
                         '4_summary.txt']),
           ( artifact(Directory, Name, Artifact), exists_file(Artifact) )).

:- end_tests(dl7_compiler_profile).
