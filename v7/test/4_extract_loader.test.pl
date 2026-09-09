% SABOTAGE RECEIPT: on the base sha `0c_extract_loader.pl` does not exist and
% `load_files/2` over this file fails before a single test runs.

:- begin_tests(dl7_extract_loader).

:- use_module('../src/2_comptime/0c_extract_loader',
              [ load_tsi_stream/3,
                accepted_rows/2,
                install_tsi_graph/6
              ]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project/5]).

% The loader reads primitive classes out of the prelude basement, so the unit
% cases hand it the one class the fixtures name.
prelude_stub(
    [module_basement(
         module(prelude),
         basement_program(
             root_graph([ node(module(prelude)), module(module(prelude)),
                          product(module(prelude)),
                          node(prelude_string), product(prelude_string)
                        ],
                        [pending_edge(module(prelude), string,
                                      target(prelude_string), 0)]),
             datalog_program([], [], [])))]).

prelude_stub_with_empty_tuple(
    [module_basement(
         module(prelude),
         basement_program(
             root_graph([ node(module(prelude)), module(module(prelude)),
                          product(module(prelude)),
                          node(prelude_empty), product(prelude_empty)
                        ],
                        [pending_edge(module(prelude), '()',
                                      target(prelude_empty), 0)]),
             datalog_program([], [], [])))]).

test(unit_wire_class_reuses_the_empty_tuple_prelude_node) :-
    Rows = [ extract_run(1, syntax, rust, test, ['unit']),
             extract_fact(1, 'tsi.type', [id(7)]),
             extract_fact(2, 'tsi.primitive', [id(7), atom(unit)]),
             extract_fact(3, 'tsi.name', [id(7), text("()")]),
             extract_witness(1, 1, parse),
             extract_witness(2, 1, parse),
             extract_witness(3, 1, parse)
           ],
    prelude_stub_with_empty_tuple(Basements0),
    install_tsi_graph(Rows, Basements0, [], Basements, _, Diagnostics),
    Owner = module(tsi(rust, ['unit'])),
    Basements = [module_basement(Owner,
                                 basement_program(root_graph(Nodes, _), _))
                | Basements0],
    \+ memberchk(node(tsi_node(Owner, 7)), Nodes),
    Diagnostics == [].

stream_path(Name, Path) :-
    atomic_list_concat(['v7/test/fixtures/', Name, '.jsonl'], Path).

load_streams([], [], []).
load_streams([Name | Names], Rows, Diagnostics) :-
    stream_path(Name, Path),
    load_tsi_stream(Path, StreamRows, StreamDiagnostics),
    load_streams(Names, RestRows, RestDiagnostics),
    append(StreamRows, RestRows, Rows),
    append(StreamDiagnostics, RestDiagnostics, Diagnostics).

install_streams(Names, Basement, Origins, Diagnostics) :-
    load_streams(Names, Rows, []),
    prelude_stub(Basements0),
    install_tsi_graph(Rows, Basements0, [], Basements, Origins, Diagnostics),
    Basements = [module_basement(_, Basement) | Basements0].

edge_labels(Edges, Owner, Labels) :-
    findall(Label-Index,
            member(pending_edge(Owner, Label, _, Index), Edges),
            Labels0),
    sort(Labels0, Labels).

accepted_edge_labels(Names, Labels) :-
    load_streams(Names, Rows, []),
    accepted_rows(Rows, Accepted),
    findall(Label,
            member(extract_fact(_, 'tsi.edge',
                                [_, _, text(Label), _, _]),
                   Accepted),
            Labels0),
    sort(Labels0, Labels).

syntax_owner(module(tsi('tree-sitter', ['blake3:user']))).
semantic_owner(module(tsi(tsc, ['blake3:user']))).

test(a_syntax_stream_alone_becomes_a_product_with_dense_edges) :-
    install_streams(['tsi/0_syntax_user'], Basement, Origins, Diagnostics),
    syntax_owner(Owner),
    Basement = basement_program(root_graph(Nodes, Edges),
                                datalog_program(Relations, Seeds, [])),
    UserNode = tsi_node(Owner, 0),
    memberchk(product(UserNode), Nodes),
    edge_labels(Edges, UserNode, Labels),
    Labels == [id-0, name-1],
    Relations == [relation(tsi_relation(Owner, 'ts.readonly'), 1, [])],
    Seeds == [call(name(Owner, 'ts.readonly'),
                   [ref(tsi_edge(Owner, 2))])],
    memberchk(pending_edge(Owner, 'ts.readonly',
                           target(tsi_relation(Owner, 'ts.readonly')), 0),
              Edges),
    Origins = [module_origins(Owner, NodeOrigins) | _],
    NodeOrigins == [origin(node(UserNode),
                           extract(typescript, 'blake3:user', 10, 14))],
    Diagnostics == [].

test(an_edge_to_a_primitive_class_targets_the_prelude_product) :-
    install_streams(['tsi/0_syntax_user'], Basement, _, _),
    syntax_owner(Owner),
    Basement = basement_program(root_graph(Nodes, Edges), _),
    memberchk(pending_edge(tsi_node(Owner, 0), id,
                           target(prelude_string), 0),
              Edges),
    \+ memberchk(node(tsi_node(Owner, 1)), Nodes),
    \+ memberchk(product(tsi_node(Owner, 1)), Nodes).

test(a_complete_semantic_run_replaces_the_syntax_edges) :-
    accepted_edge_labels(['tsi/0_syntax_user', 'tsi/1_semantic_user'],
                         Labels),
    Labels == ["id", "name"],
    load_streams(['tsi/0_syntax_user', 'tsi/1_semantic_user'], Rows, []),
    accepted_rows(Rows, Accepted),
    \+ memberchk(extract_fact(4, 'tsi.edge', _), Accepted),
    \+ memberchk(extract_fact(5, 'tsi.edge', _), Accepted),
    memberchk(extract_fact(14, 'tsi.edge', _), Accepted),
    memberchk(extract_fact(15, 'tsi.edge', _), Accepted),
    memberchk(extract_fact(6, 'ts.readonly', [id(2)]), Accepted).

test(the_newest_complete_semantic_run_wins) :-
    accepted_edge_labels(['tsi/1_semantic_user', 'tsi/2_semantic_user_v2'],
                         Labels),
    Labels == ["id", "label"],
    load_streams(['tsi/1_semantic_user', 'tsi/2_semantic_user_v2'],
                 Rows, []),
    accepted_rows(Rows, Accepted),
    \+ memberchk(extract_fact(14, 'tsi.edge', _), Accepted),
    memberchk(extract_fact(24, 'tsi.edge', _), Accepted),
    memberchk(extract_fact(25, 'tsi.edge', _), Accepted).

test(a_recursive_edge_closes_through_its_own_owner, [timeout(10)]) :-
    install_streams(['tsi/3_recursive'], Basement, _, Diagnostics),
    Owner = module(tsi(tsc, ['blake3:node'])),
    NodeType = tsi_node(Owner, 10),
    Basement = basement_program(root_graph(Nodes, Edges), _),
    memberchk(product(NodeType), Nodes),
    findall(Edge, member(Edge, Edges), AllEdges),
    AllEdges == [pending_edge(NodeType, next, target(NodeType), 0)],
    Diagnostics == [].

test(a_value_argument_carries_no_literal_and_is_reported) :-
    install_streams(['tsi/4_value'], _, _, Diagnostics),
    memberchk(diagnostic(extract, none, tsi_value_lacks_literal(23)),
              Diagnostics),
    memberchk(diagnostic(extract, none, tsi_called_unresolved(21)),
              Diagnostics).

test(a_protocol_this_door_does_not_speak_voids_the_stream) :-
    stream_path('tsi_invalid/0_protocol_2', Path),
    load_tsi_stream(Path, Rows, Diagnostics),
    Rows == [],
    Diagnostics == [diagnostic(extract, stream(Path), tsi_protocol(2))].

test(a_relation_outside_the_registry_is_named_and_skipped) :-
    install_streams(['tsi_invalid/1_unknown_relation'], Basement,
                    _, Diagnostics),
    Diagnostics == [diagnostic(extract, none,
                               tsi_unknown_relation('tsi.frobnicate'))],
    Basement = basement_program(_, datalog_program(Relations, Seeds, [])),
    Relations == [],
    Seeds == [].

% SABOTAGE RECEIPT: on the base sha each of the nine foreign rows files its own
% malformed_record diagnostic, Diagnostics holds 11 and length/2 fails at 2.
test(foreign_extract_records_are_skipped_without_a_diagnostic) :-
    stream_path('tsi_invalid/2_foreign_records', Path),
    load_tsi_stream(Path, Rows, Diagnostics),
    length(Diagnostics, 2),
    memberchk(diagnostic(extract, stream(Path),
                         tsi_line(Path, 26, malformed_record(frobnicate))),
              Diagnostics),
    memberchk(diagnostic(extract, stream(Path),
                         tsi_line(Path, 27, malformed_record(fact))),
              Diagnostics),
    memberchk(extract_fact(231, 'tsi.type', [id(0)]), Rows),
    memberchk(extract_witness(231, 0, parse), Rows),
    memberchk(extract_coverage(0, 'tsi.type', partial), Rows),
    findall(Name,
            ( member(Row, Rows),
              functor(Row, Name, _),
              memberchk(Name, [node, edge, sig, site, param, arg,
                               resolved_type_edge])
            ),
            ForeignNames),
    ForeignNames == [].

% SABOTAGE RECEIPT: listing fact in foreign_record/1, or reaching that test
% before decode_record/3, drops this diagnostic and the test fails.
test(a_malformed_tsi_record_is_still_malformed) :-
    stream_path('tsi_invalid/2_foreign_records', Path),
    load_tsi_stream(Path, Rows, Diagnostics),
    memberchk(diagnostic(extract, stream(Path),
                         tsi_line(Path, 27, malformed_record(fact))),
              Diagnostics),
    \+ memberchk(extract_fact(9001, _, _), Rows).

test(a_loaded_product_proves_conformance_to_an_authored_contract) :-
    compile_dl7_project(
        'v7/test/fixtures/tsi_project',
        [ 'v7/test/fixtures/tsi_project/0_contract.dl7',
          tsi_streams(['v7/test/fixtures/tsi/1_semantic_user.jsonl'])
        ],
        Rows, Runtime, Diagnostics),
    Diagnostics == [],
    Runtime = checked_datalog(root_graph(_, Edges), _, _, _),
    memberchk(':'(_, tsi_conforms_probe, ref(ProbeRelation), _), Edges),
    memberchk(':'(_, 'Mapper', ref(Mapper), _), Edges),
    semantic_owner(Owner),
    UserNode = tsi_node(Owner, 0),
    findall(Proof,
            member(call(ref(ProbeRelation), [ref(UserNode), ref(Proof)]),
                   Rows),
            Proofs),
    Proofs = [application(_, [UserNode, Mapper])],
    length(Proofs, 1).

test(authored_rules_can_join_dotted_tsi_relations) :-
    compile_dl7_project(
        'v7/examples',
        [ 'v7/examples/0_rust_traits.dl7',
          tsi_streams(['v7/test/fixtures/tsi/5_rust_graph.jsonl'])
        ],
        Rows, Runtime, Diagnostics),
    Diagnostics == [],
    absolute_file_name('v7/examples/0_rust_traits.dl7',
                       ProgramPath, [access(read)]),
    ProgramOwner = module(file(ProgramPath)),
    Runtime = checked_datalog(root_graph(_, Edges), _, _, _),
    memberchk(':'(ProgramOwner, source_name, ref(SourceName), _), Edges),
    findall(Name,
            member(call(ref(SourceName), [const(Name)]), Rows),
            Names),
    Names == ["Box", "Circle", "Mapper", "Option", "Self", "Shape",
              "Square", "T", "User", "View", "map", "std", "str"],
    memberchk(':'(ProgramOwner, source_trait, ref(SourceTrait), _), Edges),
    findall(Trait,
            member(call(ref(SourceTrait), [const(Trait)]), Rows),
            Traits),
    Traits == ["Mapper"].

:- end_tests(dl7_extract_loader).
