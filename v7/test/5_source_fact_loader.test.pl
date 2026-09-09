:- begin_tests(dl7_source_fact_loader).

:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module('../src/2_comptime/0d_source_fact_loader',
              [ load_source_fact_files/3,
                install_source_fact_graph/6
              ]).

source_fact_fixture(
    'v6/sprefa-extract/tests/fixtures/source_facts/1_expected.json').

seed_counts(Seeds, Counts) :-
    findall(Name,
            member(call(name(module(source_intelligence), Name), _), Seeds),
            Names0),
    sort(Names0, Names),
    findall(Name-Count,
            ( member(Name, Names),
              aggregate_all(
                  count,
                  member(call(name(module(source_intelligence), Name), _),
                         Seeds),
                  Count)
            ),
            Counts).

test(three_query_engines_share_one_source_content_and_span_graph) :-
    source_fact_fixture(Path),
    load_source_fact_files([Path], Rows, LoadDiagnostics),
    install_source_fact_graph(Rows, [], [],
                              [module_basement(Owner, Basement)],
                              [module_origins(Owner, [])],
                              InstallDiagnostics),
    Owner == module(source_intelligence),
    Basement = basement_program(
                   root_graph([node(Owner), module(Owner), product(Owner)],
                              Edges),
                   datalog_program(Relations, Seeds, [])),
    length(Rows, 31),
    maplist(ground, Rows),
    LoadDiagnostics == [],
    InstallDiagnostics == [],
    Relations ==
        [ relation(source_relation(source), 1, []),
          relation(source_relation(source_directory), 3, []),
          relation(source_relation(source_git), 4, []),
          relation(source_relation(source_revision), 1, []),
          relation(source_relation(source_content), 2, []),
          relation(source_relation(content), 2, []),
          relation(source_relation(git_blob), 3, []),
          relation(source_relation(parse), 6, []),
          relation(source_relation(source_query), 3, []),
          relation(source_relation(content_span), 4, []),
          relation(source_relation(located), 3, []),
          relation(source_relation(source_match), 4, []),
          relation(source_relation(source_capture), 4, []),
          relation(source_relation(source_replacement), 4, [])
        ],
    seed_counts(Seeds, Counts),
    Counts ==
        [ content-1,
          content_span-6,
          located-6,
          parse-1,
          source-1,
          source_capture-6,
          source_content-1,
          source_directory-1,
          source_match-4,
          source_query-3,
          source_replacement-1
        ],
    findall(Label-Index,
            member(pending_edge(Owner, Label, _, Index), Edges),
            EdgeLabels),
    EdgeLabels ==
        [ source-0,
          source_directory-1,
          source_git-2,
          source_revision-3,
          source_content-4,
          content-5,
          git_blob-6,
          parse-7,
          source_query-8,
          content_span-9,
          located-10,
          source_match-11,
          source_capture-12,
          source_replacement-13
        ],
    Content = content(
                  'blake3:142f929b0b63f663a552591e948463449bcceaf2b8dcf8d6cbafa5941ef4fab0'),
    Source = directory_source("<directory>", "0_subject.rs"),
    memberchk(call(name(Owner, source_content),
                   [ref(Source), ref(Content)]), Seeds),
    ReplacementSpan = content_span(Content, 31, 55),
    Occurrence = located(Source, ReplacementSpan),
    memberchk(call(name(Owner, source_replacement),
                   [ ref(source_replacement(_, Occurrence, replace_print)),
                     ref(Occurrence),
                     const("eprintln!(\"hello {name}\")"),
                     const(replace_print)
                   ]),
              Seeds).

:- end_tests(dl7_source_fact_loader).
