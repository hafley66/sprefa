% DL7 deterministic compiler flamechart and duplicate-work report.
%
% One bounded process compiles one fixture inside a profile scope, then writes
% five artifacts in reading order:
%
%   0_profile.json    structured span tree plus the duplicate-work report
%   1_folded.txt      folded stacks, deterministic inference self-weights
%   2_duplicates.tsv  per-category occurrence duplication
%   3_flamechart.html self-contained viewer, embeds 0_profile.json verbatim
%   4_summary.txt     human-readable denominator and formula
%
% Deterministic width is the inference count the compiler tracer already
% measures per phase and step; a fresh-process comparison showed inference
% counts byte-stable while wall milliseconds are not. Wall time is written to
% stderr only, so every generated artifact remains byte-stable.
%
% Diagnostic instrumentation only: no compiler, graph, evaluator, or IR
% semantics change.

:- module(dl7_compiler_profile,
          [ profile_main/0,
            require_fixture/2,
            repository_root/1,
            render_identity_text/3,
            render_fixture_text/3,
            build_spans/4,
            duplicate_groups/2,
            duplicate_groups_from_hashes/2,
            duplicate_occurrence_summary/2,
            duplicate_occurrence_summary/3,
            deterministic_profile_text/2
          ]).

:- use_module(library(http/json), [json_write_dict/3]).
:- use_module(library(pairs), [group_pairs_by_key/2]).
:- use_module(library(filesex), [make_directory_path/1]).
:- use_module(library(lists), [sum_list/2, max_list/2]).
:- use_module('../src/2_comptime/1b_compiler_tracer',
              [ with_profile_scope/1,
                collected_profile_debug_events/1,
                collected_profile_occurrences/1,
                latest_compile_trace/4
              ]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../src/2_comptime/1c_compiler_cacher', [clear_compiler_caches/0]).

profile_categories(
    [ stratification_input,
      checker_input,
      evaluator_installed_rules,
      evaluator_installed_seeds,
      evaluator_installed_lower_rows,
      evaluator_collected_closure_rows
    ]).

%% profile_main/0 is det.
%
% Command entrypoint. With `--` the argv terms are the fixture path and the
% output directory; the output directory defaults under v7/out, anchored at the
% repository root rather than the current directory. The fixture is resolved
% against the filesystem before compilation so every displayed path can be
% rendered relative to the repository root.
profile_main :-
    current_prolog_flag(argv, Arguments),
    parse_arguments(Arguments, Fixture, OutputDirectory),
    (   require_fixture(Fixture, AbsoluteFixture)
    ->  run_stages(AbsoluteFixture, OutputDirectory, ExitCode)
    ;   profile_error(source, Fixture),
        ExitCode = 2
    ),
    halt(ExitCode).

parse_arguments([Fixture, OutputDirectory | _], Fixture, OutputDirectory) :-
    !.
parse_arguments([Fixture], Fixture, OutputDirectory) :-
    !,
    default_output_directory(OutputDirectory).
parse_arguments(_, _, _) :-
    profile_error(usage, 'expected <fixture> [output-directory]'),
    halt(2).

default_output_directory(Directory) :-
    repository_root(Root),
    directory_file_path(Root, 'v7/out/compiler-profile', Directory).

%% repository_root(-Root) is det.
%
% The checkout or worktree root, derived from this module's own location
% (`<root>/v7/bench/1_compiler_profile.pl`). Used only to render displayed
% paths stably; occurrence equality never consults it.
repository_root(Root) :-
    source_file(dl7_compiler_profile:profile_main, Source),
    absolute_file_name(Source, AbsoluteSource),
    file_directory_name(AbsoluteSource, BenchDirectory),
    file_directory_name(BenchDirectory, V7Directory),
    file_directory_name(V7Directory, Root).

%% require_fixture(+Path, -Absolute) is semidet.
require_fixture(Path, Absolute) :-
    catch(absolute_file_name(Path, Absolute,
                             [access(read), file_errors(error)]),
          _,
          fail).

run_stages(Fixture, OutputDirectory, ExitCode) :-
    catch(
        run_stages_(Fixture, OutputDirectory, ExitCode),
        Error,
        ( profile_error(compile, Error),
          ExitCode = 3 )).

run_stages_(Fixture, OutputDirectory, ExitCode) :-
    (   compile_stage(Fixture, Rows, Diagnostics)
    ->  run_report_stage(Fixture, OutputDirectory, Rows, Diagnostics, ExitCode)
    ;   profile_error(compile, compiler_returned_failure),
        ExitCode = 3
    ).

%% run_report_stage(+Fixture, +OutputDirectory, +Rows, +Diagnostics, -Exit) is det.
%
% A throw or failure inside report staging is labeled `report`, not `compile`,
% so the shell can preserve the precise stage without inventing one.
run_report_stage(Fixture, OutputDirectory, Rows, Diagnostics, ExitCode) :-
    catch(
        (   report_stage(Fixture, OutputDirectory, Rows, Diagnostics)
        ->  ExitCode = 0
        ;   profile_error(report, report_failed),
            ExitCode = 4
        ),
        ReportError,
        ( profile_error(report, ReportError),
          ExitCode = 4 )).

%% compile_stage(+Fixture, -Rows, -Diagnostics) is semidet.
compile_stage(Fixture, Rows, Diagnostics) :-
    clear_compiler_caches,
    setenv('DL7_TRACE', debug),
    with_profile_scope(
        compile_dl7(Fixture, Rows, _Runtime, Diagnostics)).

%% report_stage(+Fixture, +OutputDirectory, +Rows, +Diagnostics) is semidet.
%
% All five artifacts are deterministic: wall is reported only to stderr, and
% every displayed path is rendered against the repository root so a different
% checkout prefix yields identical bytes.
report_stage(Fixture, OutputDirectory, Rows, Diagnostics) :-
    latest_compile_trace(Program, _Phases, _Steps, TotalMeasurement),
    TotalMeasurement = measurement(TotalWall, _, TotalInferences, _, _, _, _,
                                   _, _, _, _, _),
    repository_root(RepoRoot),
    render_fixture_text(RepoRoot, Fixture, DisplayFixture),
    collected_profile_debug_events(Events),
    collected_profile_occurrences(Occurrences),
    build_spans(Events, TotalInferences, TotalWall, Spans),
    length(Rows, CompilerRows),
    length(Diagnostics, DiagnosticCount),
    duplicate_occurrence_summary(RepoRoot, Occurrences, DuplicateReport),
    make_directory_path(OutputDirectory),
    profile_dict(DisplayFixture, Program, TotalInferences,
                 CompilerRows, DiagnosticCount, Spans, DuplicateReport,
                 ProfileDict),
    profile_json_text(ProfileDict, JsonText),
    write_text_file(OutputDirectory, '0_profile.json', JsonText),
    folded_text(Spans, FoldedText),
    write_text_file(OutputDirectory, '1_folded.txt', FoldedText),
    duplicate_tsv_text(DuplicateReport, TsvText),
    write_text_file(OutputDirectory, '2_duplicates.tsv', TsvText),
    summary_text(DisplayFixture, TotalInferences, CompilerRows, DuplicateReport,
                 SummaryText),
    write_text_file(OutputDirectory, '4_summary.txt', SummaryText),
    html_text(DisplayFixture, JsonText, Spans, HtmlText),
    write_text_file(OutputDirectory, '3_flamechart.html', HtmlText),
    format(user_error, 'DL7-PROFILE-WALL total_wall_ms=~w~n', [TotalWall]).

profile_error(Stage, Message) :-
    format(user_error, 'DL7-PROFILE-ERROR stage=~w ~q~n', [Stage, Message]).

write_text_file(Directory, Name, Text) :-
    directory_file_path(Directory, Name, Path),
    setup_call_cleanup(
        open(Path, write, Stream, [encoding(utf8)]),
        format(Stream, '~s', [Text]),
        close(Stream)).

%% ------------------------------------------------------------------
%% Span construction
%% ------------------------------------------------------------------
%
% Frames carry a running child cursor so each opened child gets a deterministic
% start offset. Phase and step widths come from the inference field the tracer
% already measures; a stratum span sums its three child step widths. Wall is
% carried alongside and never affects ordering or identity.

%% build_spans(+Events, +TotalInferences, +TotalWallMs, -Spans) is det.
build_spans(Events, TotalInferences, TotalWall, Spans) :-
    Root = frame(compile, none, compile, compile, 0, 0, 0, 0, -1),
    fold_events(Events, [Root], Closed),
    RootSpan = span(-1, compile, none, compile, compile, 0, 0,
                    TotalInferences, TotalWall),
    preorder_spans([RootSpan | Closed], Spans).

fold_events([], _Stack, []).

fold_events([Fields | Events], Stack0, Closed) :-
    process_event(Fields, Stack0, Stack, NewSpans),
    fold_events(Events, Stack, Rest),
    append(NewSpans, Rest, Closed).

process_event(Fields, Stack0, Stack, NewSpans) :-
    event_name(Fields, Name),
    (   Name == phase_begin
    ->  event_field(Fields, phase, Phase),
        event_field(Fields, seq, Seq),
        make_id(phase_, Seq, Id),
        open_span(Stack0, Id, Phase, phase, Seq, Stack),
        NewSpans = []
    ;   Name == phase_end
    ->  event_inferences(Fields, Inf),
        event_wall(Fields, Wall),
        close_top(Stack0, Inf, Wall, Span, Stack),
        NewSpans = [Span]
    ;   Name == step_begin
    ->  step_begin(Fields, Stack0, Stack),
        NewSpans = []
    ;   Name == step_end
    ->  step_end(Fields, Stack0, Stack, NewSpans)
    ;   Stack = Stack0,
        NewSpans = []
    ).

step_begin(Fields, Stack0, Stack) :-
    event_field(Fields, step, Step),
    event_field(Fields, seq, Seq),
    (   Step = evaluate_round(Round)
    ->  round_label(Round, Label),
        make_id(round_, Seq, Id),
        open_span(Stack0, Id, Label, round, Seq, Stack)
    ;   Step = evaluate_install(Level)
    ->  make_id(stratum_, Seq, StratumId),
        stratum_label(Level, Slabel),
        open_span(Stack0, StratumId, Slabel, stratum, Seq, Stack1),
        make_id(install_, Seq, Id),
        open_span(Stack1, Id, install, install, Seq, Stack)
    ;   Step = evaluate_collect(_)
    ->  make_id(collect_, Seq, Id),
        open_span(Stack0, Id, collect, collect, Seq, Stack)
    ;   Step = evaluate_cleanup(_)
    ->  make_id(cleanup_, Seq, Id),
        open_span(Stack0, Id, cleanup, cleanup, Seq, Stack)
    ;   Stack = Stack0
    ).

step_end(Fields, Stack0, Stack, NewSpans) :-
    event_field(Fields, step, Step),
    event_inferences(Fields, Inf),
    event_wall(Fields, Wall),
    (   Step = evaluate_round(_)
    ->  close_top(Stack0, Inf, Wall, Span, Stack),
        NewSpans = [Span]
    ;   Step = evaluate_install(_)
    ->  close_top(Stack0, Inf, Wall, Span, Stack),
        NewSpans = [Span]
    ;   Step = evaluate_collect(_)
    ->  close_top(Stack0, Inf, Wall, Span, Stack),
        NewSpans = [Span]
    ;   Step = evaluate_cleanup(_)
    ->  close_top(Stack0, Inf, Wall, Cleanup, Stack1),
        close_stratum(Stack1, Stratum, Stack),
        NewSpans = [Stratum, Cleanup]
    ;   Stack = Stack0,
        NewSpans = []
    ).

open_span([Parent | Rest], Id, Label, Category, Seq,
          [New, Parent | Rest]) :-
    Parent = frame(ParentId, _, _, _, Depth, Start, Cursor, _, _),
    NewDepth is Depth + 1,
    NewStart is Start + Cursor,
    New = frame(Id, ParentId, Label, Category, NewDepth, NewStart, 0, 0, Seq).

close_top([Top, Parent0 | Rest], Width, Wall, Span, [Parent | Rest]) :-
    Top = frame(Id, ParentId, Label, Category, Depth, Start, _, _, Seq),
    Span = span(Seq, Id, ParentId, Label, Category, Depth, Start, Width, Wall),
    Parent0 = frame(ParentId, GrandParent, PLabel, PCategory, PDepth, PStart,
                    PIn, PWall, PSeq),
    NewIn is PIn + Width,
    NewWall is PWall + Wall,
    Parent = frame(ParentId, GrandParent, PLabel, PCategory, PDepth, PStart,
                   NewIn, NewWall, PSeq).

close_stratum([Stratum | Rest], Span, Stack) :-
    Stratum = frame(_, _, _, _, _, _, InCursor, WallCursor, _),
    close_top([Stratum | Rest], InCursor, WallCursor, Span, Stack).

event_name(Fields, Name) :-
    memberchk(event=Name, Fields).

event_field(Fields, Name, Value) :-
    memberchk(Name=Value, Fields).

event_inferences(Fields, Inferences) :-
    memberchk(inferences=Inferences, Fields).

event_wall(Fields, Wall) :-
    (   memberchk(wall_ms=Wall, Fields)
    ->  true
    ;   Wall = 0
    ).

make_id(Prefix, Seq, Id) :-
    atomic_list_concat([Prefix, Seq], Id).

round_label(Round, Label) :-
    atomic_list_concat([round_, Round], Label).

stratum_label(Level, Label) :-
    atomic_list_concat([stratum_, Level], Label).

%% preorder_spans(+Closed, -Spans) is det.
%
% Closed holds spans with a stable OpenSeq. Sorting by OpenSeq yields DFS
% pre-order, the flamechart reading order, independent of wall.
preorder_spans(Closed, Spans) :-
    findall(Seq-Span,
            ( member(Span, Closed),
              Span = span(Seq, _, _, _, _, _, _, _, _) ),
            Keyed),
    keysort(Keyed, Sorted),
    findall(Span, member(_-Span, Sorted), Spans).

span_id(span(_, Id, _, _, _, _, _, _, _), Id).
span_parent(span(_, _, Parent, _, _, _, _, _, _), Parent).
span_label(span(_, _, _, Label, _, _, _, _, _), Label).
span_width(span(_, _, _, _, _, _, _, Width, _), Width).
span_wall(span(_, _, _, _, _, _, _, _, Wall), Wall).

%% ------------------------------------------------------------------
%% Folded stacks
%% ------------------------------------------------------------------

folded_text(Spans, Text) :-
    span_self_weight_pairs(Spans, Pairs),
    aggregate_folded(Pairs, Aggregated),
    with_output_to(string(Text), emit_folded(Aggregated)).

span_self_weight_pairs(Spans, Pairs) :-
    findall(Path-Self,
            ( member(Span, Spans),
              span_path(Spans, Span, Path),
              span_self_width(Spans, Span, Self) ),
            Pairs).

span_path(Spans, Span, Path) :-
    span_id(Span, Id),
    ancestor_labels(Spans, Id, Labels),
    atomic_list_concat(Labels, ';', Path).

ancestor_labels(Spans, Id, Labels) :-
    span_by_id(Spans, Id, Span),
    span_parent(Span, Parent),
    (   Parent == none
    ->  Labels = [compile]
    ;   span_label(Span, Label),
        ancestor_labels(Spans, Parent, Up),
        append(Up, [Label], Labels)
    ).

span_by_id([Span | _], Id, Span) :-
    span_id(Span, SpanId),
    SpanId == Id,
    !.
span_by_id([_ | Spans], Id, Span) :-
    span_by_id(Spans, Id, Span).

span_self_width(Spans, Span, Self) :-
    span_id(Span, Id),
    span_width(Span, Width),
    findall(ChildWidth,
            ( member(Child, Spans),
              span_parent(Child, Id),
              span_width(Child, ChildWidth) ),
            ChildWidths),
    sum_list(ChildWidths, ChildSum),
    Raw is Width - ChildSum,
    (   Raw < 0
    ->  Self = 0
    ;   Self = Raw
    ).

aggregate_folded(Pairs, Aggregated) :-
    keysort(Pairs, Sorted),
    group_pairs_by_key(Sorted, Groups),
    findall(Path-Total,
            ( member(Path-Values, Groups),
              sum_list(Values, Total) ),
            Aggregated0),
    sort(Aggregated0, Aggregated).

%% emit_folded(+Pairs) is det.
%
% Standard folded-stack output carries only positive self-weight; a span whose
% width is fully attributed to children contributes no standalone line.
emit_folded([]).
emit_folded([Path-Weight | Rest]) :-
    (   Weight =:= 0
    ->  true
    ;   format(current_output, '~w ~w~n', [Path, Weight])
    ),
    emit_folded(Rest).

%% ------------------------------------------------------------------
%% Duplicate-work report
%% ------------------------------------------------------------------

%% duplicate_occurrence_summary(+Occurrences, -Report) is det.
%
% Convenience form with no path normalization; used by tests that pin duplicate
% arithmetic and category separation on synthetic occurrences.
duplicate_occurrence_summary(Occurrences, Report) :-
    duplicate_occurrence_summary('', Occurrences, Report).

%% duplicate_occurrence_summary(+RepoRoot, +Occurrences, -Report) is det.
%
% Occurrences is the recorded Category-Identity list. The report keeps each
% category separate so the same term under a different semantic role is never
% merged, and equality inside a category is verified with == after hash
% bucketing. RepoRoot normalizes only the rendered identity text; the terms
% themselves stay exact for ==.
duplicate_occurrence_summary(RepoRoot, Occurrences, Report) :-
    group_occurrence_categories(Occurrences, Grouped),
    profile_categories(Categories),
    findall(Entry,
            ( member(Category, Categories),
              category_duplicate_entry(RepoRoot, Grouped, Category, Entry) ),
            Entries),
    overall_duplicate_entry(Entries, Overall),
    Report = _{ denominator:
                    "sum of occurrence counts across categories",
                formula:
                    "duplicate_percent = duplicate_occurrences / total_occurrences * 100",
                duplicate_inference_percent: unavailable,
                categories: Entries,
                overall: Overall }.

group_occurrence_categories(Occurrences, Grouped) :-
    findall(Category-Identity,
            member(Category-Identity, Occurrences),
            Pairs),
    keysort(Pairs, Sorted),
    group_pairs_by_key(Sorted, Grouped).

category_duplicate_entry(RepoRoot, Grouped, Category, Entry) :-
    (   memberchk(Category-Identities, Grouped)
    ->  true
    ;   Identities = []
    ),
    findall(Hash-Identity,
            ( member(Identity, Identities),
              term_hash(Identity, Hash) ),
            HashPairs),
    duplicate_groups_from_hashes(HashPairs, Counts),
    length(Counts, Unique),
    sum_counts(Counts, Total),
    Duplicate is Total - Unique,
    duplicate_percent(Duplicate, Total, Percent),
    top_repeated(RepoRoot, Counts, 5, Top),
    Entry = _{ category: Category,
               total: Total,
               unique: Unique,
               duplicate: Duplicate,
               percent: Percent,
               top_repeated: Top }.

%% duplicate_groups(+Identities, -Counts) is det.
duplicate_groups(Identities, Counts) :-
    findall(Hash-Identity,
            ( member(Identity, Identities),
              term_hash(Identity, Hash) ),
            HashPairs),
    duplicate_groups_from_hashes(HashPairs, Counts).

%% duplicate_groups_from_hashes(+HashPairs, -Counts) is det.
%
% HashPairs is Hash-Identity. Equal hashes select one bucket; identities inside
% a bucket are split only by ==, so a hash collision is never treated as a
% duplicate. Exposed so tests can pin collision rejection with a forced hash.
duplicate_groups_from_hashes(HashPairs, Counts) :-
    keysort(HashPairs, Sorted),
    hash_buckets(Sorted, Counts0),
    sort(Counts0, Counts1),
    reverse(Counts1, Counts).

hash_buckets([], []).
hash_buckets([Hash-Identity | Rest], Counts) :-
    same_hash(Hash, Rest, Same, RestOut),
    count_exact([Identity | Same], Pairs),
    hash_buckets(RestOut, RestCounts),
    append(Pairs, RestCounts, Counts).

same_hash(_, [], [], []).
same_hash(Hash, [Hash1-Identity | Rest], Same, RestOut) :-
    (   Hash1 == Hash
    ->  Same = [Identity | Same1],
        same_hash(Hash, Rest, Same1, RestOut)
    ;   Same = [],
        RestOut = [Hash1-Identity | Rest]
    ).

count_exact([], []).
count_exact([Identity | Identities], [Count-Identity | Pairs]) :-
    partition_exact(Identity, Identities, Same, Rest),
    length(Same, SameCount),
    Count is SameCount + 1,
    count_exact(Rest, Pairs).

partition_exact(_, [], [], []).
partition_exact(Identity, [Other | Others], Same, Rest) :-
    (   Identity == Other
    ->  Same = [Other | Same1],
        partition_exact(Identity, Others, Same1, Rest)
    ;   Rest = [Other | Rest1],
        partition_exact(Identity, Others, Same, Rest1)
    ).

sum_counts([], 0).
sum_counts([Count-_ | Counts], Total) :-
    sum_counts(Counts, Rest),
    Total is Count + Rest.

overall_duplicate_entry(Entries, Overall) :-
    sum_entry_field(Entries, total, Total),
    sum_entry_field(Entries, unique, Unique),
    sum_entry_field(Entries, duplicate, Duplicate),
    duplicate_percent(Duplicate, Total, Percent),
    Overall = _{ total: Total, unique: Unique,
                 duplicate: Duplicate, percent: Percent }.

sum_entry_field([], _, 0).
sum_entry_field([Entry | Entries], Field, Sum) :-
    get_dict(Field, Entry, Value),
    sum_entry_field(Entries, Field, Rest),
    Sum is Value + Rest.

duplicate_percent(_, 0, 0.0) :-
    !.
duplicate_percent(Duplicate, Total, Percent) :-
    Raw is Duplicate * 100.0 / Total,
    Percent is round(Raw * 100) / 100.0.

top_repeated(RepoRoot, Counts, Limit, Top) :-
    include(count_above_one, Counts, Repeated),
    take_first(Repeated, Limit, Taken),
    maplist(repeated_entry(RepoRoot), Taken, Top).

count_above_one(Count-_) :-
    Count > 1.

repeated_entry(RepoRoot, Count-Identity,
               _{count: Count, identity: Text}) :-
    render_identity_text(RepoRoot, Identity, Text).

%% render_identity_text(+RepoRoot, +Identity, -Text) is det.
%
% Structural rendering capped so a large checker input cannot flood the TSV or
% the JSON. Equality was already decided on the full term with ==; RepoRoot only
% rewrites checkout-absolute prefixes in the rendered text to `$REPO/...`, so
% two worktrees of the same source render identically.
render_identity_text(RepoRoot, Identity, Text) :-
    with_output_to(string(Raw), write_canonical(Identity)),
    normalize_rendered_paths(RepoRoot, Raw, Normalized),
    string_length(Normalized, Length),
    (   Length =< 160
    ->  Text = Normalized
    ;   sub_string(Normalized, 0, 157, _, Prefix),
        string_concat(Prefix, "...", Text)
    ).

%% render_fixture_text(+RepoRoot, +Fixture, -Text) is det.
render_fixture_text(RepoRoot, Fixture, Text) :-
    atom_string(Fixture, FixtureText),
    normalize_rendered_paths(RepoRoot, FixtureText, Text).

%% normalize_rendered_paths(+RepoRoot, +Text, -Normalized) is det.
%
% Replaces every occurrence of the absolute repository root (with its trailing
% separator) with the stable token `$REPO/`. An empty root is the identity.
normalize_rendered_paths('', Text, Text) :-
    !.
normalize_rendered_paths(RepoRoot, Text, Normalized) :-
    atom_string(RepoRoot, RootText0),
    strip_trailing_slash(RootText0, RootText),
    string_concat(RootText, "/", Prefix),
    replace_all(Text, Prefix, "$REPO/", Normalized).

strip_trailing_slash(Text, Stripped) :-
    (   sub_string(Text, _, 1, 0, "/")
    ->  sub_string(Text, 0, _, 1, Stripped)
    ;   Stripped = Text
    ).

replace_all(Text, Needle, Replacement, Result) :-
    string_length(Needle, NeedleLength),
    (   sub_string(Text, Before, NeedleLength, _, Needle)
    ->  sub_string(Text, 0, Before, _, Prefix),
        Start is Before + NeedleLength,
        sub_string(Text, Start, _, 0, Suffix),
        replace_all(Suffix, Needle, Replacement, SuffixOut),
        string_concat(Prefix, Replacement, Step),
        string_concat(Step, SuffixOut, Result)
    ;   Result = Text
    ).

take_first(_, 0, []) :-
    !.
take_first([], _, []).
take_first([Head | Tail], Limit, [Head | First]) :-
    Next is Limit - 1,
    take_first(Tail, Next, First).

%% ------------------------------------------------------------------
%% JSON, TSV, summary, HTML
%% ------------------------------------------------------------------

profile_dict(Fixture, Program, TotalInferences,
             CompilerRows, DiagnosticCount, Spans, DuplicateReport, Dict) :-
    findall(SpanDict,
            ( member(Span, Spans), span_dict(Span, SpanDict) ),
            SpanDicts),
    length(SpanDicts, SpanCount),
    Dict = _{ fixture: Fixture,
              program: Program,
              width_metric: inferences,
              deterministic: true,
              total_inferences: TotalInferences,
              compiler_rows: CompilerRows,
              diagnostics: DiagnosticCount,
              span_count: SpanCount,
              spans: SpanDicts,
              duplicate_work: DuplicateReport }.

span_dict(Span, Dict) :-
    span_id(Span, Id),
    span_parent(Span, Parent),
    span_label(Span, Label),
    span_width(Span, Width),
    Span = span(_, _, _, _, Category, Depth, Start, _, _),
    json_parent(Parent, JsonParent),
    Dict = _{ id: Id,
              parent: JsonParent,
              label: Label,
              category: Category,
              depth: Depth,
              start: Start,
              width: Width,
              inferences: Width }.

json_parent(none, null) :-
    !.
json_parent(Parent, Parent).

profile_json_text(Dict, Text) :-
    with_output_to(string(Body),
                   json_write_dict(current_output, Dict, [width(0)])),
    string_concat(Body, "\n", Text).

%% deterministic_profile_text(+Dict, -Text) is det.
%
% Canonical JSON projection. The profile dict no longer carries wall fields;
% the defensive exclusion keeps this predicate usable if a legacy dict with
% `total_wall_ms` or `wall_observations` is passed.
deterministic_profile_text(Dict, Text) :-
    dict_pairs(Dict, Tag, Pairs0),
    exclude(wall_pair, Pairs0, Pairs),
    dict_pairs(Clean, Tag, Pairs),
    with_output_to(string(Text),
                   json_write_dict(current_output, Clean, [width(0)])).

wall_pair(Key-_) :-
    memberchk(Key, [wall_observations, total_wall_ms]).

duplicate_tsv_text(Report, Text) :-
    get_dict(categories, Report, Entries),
    get_dict(overall, Report, Overall),
    with_output_to(
        string(Text),
        ( format('category\ttotal\tunique\tduplicate\tpercent\ttop_repeated~n', []),
          forall(member(Entry, Entries), tsv_entry(Entry)),
          tsv_overall(Overall) )).

tsv_entry(Entry) :-
    get_dict(category, Entry, Category),
    get_dict(total, Entry, Total),
    get_dict(unique, Entry, Unique),
    get_dict(duplicate, Entry, Duplicate),
    get_dict(percent, Entry, Percent),
    get_dict(top_repeated, Entry, Top),
    top_repeated_text(Top, TopText),
    format('~w\t~w\t~w\t~w\t~2f\t~s~n',
           [Category, Total, Unique, Duplicate, Percent, TopText]).

tsv_overall(Overall) :-
    get_dict(total, Overall, Total),
    get_dict(unique, Overall, Unique),
    get_dict(duplicate, Overall, Duplicate),
    get_dict(percent, Overall, Percent),
    format('overall\t~w\t~w\t~w\t~2f\t~s~n',
           [Total, Unique, Duplicate, Percent, "sum of per-category counts"]).

top_repeated_text([], "") :-
    !.
top_repeated_text(Top, Text) :-
    findall(Piece,
            ( member(Entry, Top),
              get_dict(count, Entry, Count),
              get_dict(identity, Entry, Identity),
              format(string(Piece), "~wx ~s", [Count, Identity]) ),
            Pieces),
    atomic_list_concat(Pieces, ' | ', Text0),
    tsv_safe(Text0, Text).

tsv_safe(Text0, Text) :-
    with_output_to(string(Text),
                   replace_text_chars(Text0)).

replace_text_chars(Text) :-
    current_output(Stream),
    string_codes(Text, Codes),
    maplist(safe_code, Codes, SafeCodes),
    string_codes(Safe, SafeCodes),
    format(Stream, '~s', [Safe]).

safe_code(9, 32) :-
    !.
safe_code(10, 32) :-
    !.
safe_code(13, 32) :-
    !.
safe_code(Code, Code).

summary_text(Fixture, TotalInferences, CompilerRows, Report, Text) :-
    get_dict(overall, Report, Overall),
    get_dict(categories, Report, Entries),
    get_dict(total, Overall, Total),
    get_dict(unique, Overall, Unique),
    get_dict(duplicate, Overall, Duplicate),
    get_dict(percent, Overall, Percent),
    with_output_to(
        string(Text),
        ( format('DL7 compiler profile summary~n', []),
          format('fixture: ~w~n', [Fixture]),
          format('deterministic width metric: inferences~n', []),
          format('compiler rows: ~w~n', [CompilerRows]),
          format('total inferences: ~w~n', [TotalInferences]),
          nl,
          format('duplicate-work denominator: sum of occurrence counts across categories~n', []),
          format('duplicate-work formula: duplicate_percent = duplicate_occurrences / total_occurrences * 100~n', []),
          format('overall: total=~w unique=~w duplicate=~w percent=~2f~n',
                 [Total, Unique, Duplicate, Percent]),
          format('duplicate_inference_percent: unavailable~n', []),
          nl,
          format('per category:~n', []),
          forall(member(Entry, Entries), summary_entry(Entry)) )).

summary_entry(Entry) :-
    get_dict(category, Entry, Category),
    get_dict(total, Entry, Total),
    get_dict(unique, Entry, Unique),
    get_dict(duplicate, Entry, Duplicate),
    get_dict(percent, Entry, Percent),
    get_dict(top_repeated, Entry, Top),
    format('  ~w: total=~w unique=~w duplicate=~w percent=~2f~n',
           [Category, Total, Unique, Duplicate, Percent]),
    forall(member(TopEntry, Top), summary_top(TopEntry)).

summary_top(Entry) :-
    get_dict(count, Entry, Count),
    get_dict(identity, Entry, Identity),
    format('    ~wx ~s~n', [Count, Identity]).

html_text(Fixture, JsonText, Spans, Text) :-
    findall(Depth, ( member(S, Spans),
                     S = span(_, _, _, _, _, Depth, _, _, _) ), Depths),
    max_list(Depths, MaxDepth),
    Height is (MaxDepth + 1) * 22,
    with_output_to(
        string(Text),
        ( format('<!DOCTYPE html>~n', []),
          format('<html lang="en">~n<head>~n', []),
          format('<meta charset="utf-8">~n', []),
          format('<title>DL7 compiler flamechart - ~w</title>~n', [Fixture]),
          format('<style>~n', []),
          format('body{font-family:system-ui,sans-serif;margin:1rem;color:#1b1b1b}~n', []),
          format('#flamechart{position:relative;height:~wpx;border:1px solid #ccc;background:#fafafa;overflow:auto}~n', [Height]),
          format('.span{position:absolute;height:20px;box-sizing:border-box;border:1px solid #4a6fa5;background:#c9dcf5;font-size:11px;line-height:18px;padding:0 3px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}~n', []),
          format('.span:hover{background:#9fc2ec}~n', []),
          format('ul#span-list{font-family:ui-monospace,monospace;font-size:12px}~n', []),
          format('</style>~n</head>~n<body>~n', []),
          format('<h1>DL7 compiler profile: ~w</h1>~n', [Fixture]),
          format('<p>Deterministic width metric: inferences. Wall time is metadata only and does not affect ordering.</p>~n', []),
          format('<div id="flamechart" role="img" aria-label="DL7 compiler flamechart for ~w"></div>~n', [Fixture]),
          format('<h2>Spans</h2>~n<ul id="span-list">~n', []),
          forall(member(Span, Spans), html_span(Span)),
          format('</ul>~n', []),
          format('<script id="profile-data" type="application/json">~s</script>~n', [JsonText]),
          format('~s~n', [html_script]),
          format('</body>~n</html>~n', []) )).

html_span(Span) :-
    span_id(Span, Id),
    span_parent(Span, Parent),
    span_label(Span, Label),
    span_width(Span, Width),
    Span = span(_, _, _, _, Category, Depth, Start, _, _),
    format('<li><span class="label" data-id="~w" data-parent="~w" data-category="~w" data-depth="~w" data-start="~w" data-width="~w">~w</span></li>~n',
           [Id, Parent, Category, Depth, Start, Width, Label]).

html_script(
    "<script>\n(function(){\n  var node = document.getElementById('profile-data');\n  var data = JSON.parse(node.textContent);\n  var spans = data.spans || [];\n  var root = spans[0];\n  var scale = root && root.width ? 100 / root.width : 1;\n  var host = document.getElementById('flamechart');\n  spans.forEach(function(s){\n    var bar = document.createElement('div');\n    bar.className = 'span';\n    bar.style.left = (s.start * scale) + '%';\n    bar.style.width = Math.max(0.15, s.width * scale) + '%';\n    bar.style.top = (s.depth * 22) + 'px';\n    bar.title = s.id + ' ' + s.label + ': ' + s.width + ' inferences (' + s.category + ')';\n    bar.textContent = s.label;\n    host.appendChild(bar);\n  });\n})();\n</script>").
