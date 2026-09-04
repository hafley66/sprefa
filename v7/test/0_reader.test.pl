:- begin_tests(dl7_reader_foundation).

:- use_module('../src/0_reader/0_parser', [read_dl7/5]).
:- use_module('../src/0_reader/3_file_loader', [load_dl7/3]).
:- use_module('../src/0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../src/2_comptime/2_compiler', [compile_unit/3]).

test(standalone_fixture_has_canonical_reader_snapshot) :-
    load_dl7('v7/test/fixtures/0_minimal.dl7',
             Unit, Diagnostics),
    unit_snapshot(Unit, Snapshot),
    expected_snapshot(Expected),
    Observed = reader_result(Diagnostics, Snapshot),
    Observed == reader_result([], Expected).

test(malformed_symbol_returns_one_positioned_diagnostic) :-
    read_dl7('broken.dl7', "'\n", Forms, SourceRows, Diagnostics),
    Observed = reader_result(Forms, SourceRows, Diagnostics),
    Observed ==
        reader_result(
            [], [],
            [ diagnostic(reader, 'broken.dl7',
                         reader_node('broken.dl7', 0),
                         expected_symbol_name,
                         position(0, 1, 1))
            ]).

test(bad_symbol_name_returns_one_positioned_diagnostic) :-
    read_dl7('broken.dl7', "'a<b", Forms, SourceRows, Diagnostics),
    Observed = reader_result(Forms, SourceRows, Diagnostics),
    Observed ==
        reader_result(
            [], [],
            [ diagnostic(reader, 'broken.dl7',
                         reader_node('broken.dl7', 0),
                         invalid_symbol_name('a<b'),
                         position(0, 1, 1))
            ]).

test(malformed_form_returns_one_positioned_diagnostic) :-
    read_dl7('broken.dl7', "(\n", Forms, SourceRows, Diagnostics),
    Observed = reader_result(Forms, SourceRows, Diagnostics),
    Observed ==
        reader_result(
            [], [],
            [ diagnostic(reader, 'broken.dl7',
                         reader_node('broken.dl7', 0),
                         unterminated_form,
                         position(2, 2, 1))
            ]).

test(tree_sitter_query_literal_preserves_exact_inner_text) :-
    Text = "(cst ?path { (identifier) @name (#eq? @name \"}\") })",
    read_dl7(query_source, Text, Forms, SourceRows, Diagnostics),
    Forms = [node(_, form([
                 node(_, atom(cst)),
                 node(_, variable(_, path)),
                 node(_, literal(tree_sitter_query(Query)))
             ]))],
    Query == " (identifier) @name (#eq? @name \"}\") ",
    length(SourceRows, 4),
    Diagnostics == [],
    !.

test(dotted_names_are_one_opaque_atom) :-
    read_dl7(dotted_name_source,
             "rel.with.dot field.with.dot impl.with.dot",
             Forms, SourceRows, Diagnostics),
    maplist(payload_tree, Forms, Names),
    length(SourceRows, SourceRowCount),
    Observed = dotted_names(Diagnostics, Names, SourceRowCount),
    Observed == dotted_names(
                    [],
                    [ atom('rel.with.dot'),
                      atom('field.with.dot'),
                      atom('impl.with.dot')
                    ],
                    3).

test(unterminated_tree_sitter_query_is_positioned) :-
    read_dl7(query_source, "{ (identifier)", Forms, SourceRows,
             Diagnostics),
    Observed = reader_result(Forms, SourceRows, Diagnostics),
    Observed ==
        reader_result(
            [], [],
            [ diagnostic(reader, query_source,
                         reader_node(query_source, 0),
                         unterminated_query,
                         position(14, 1, 15))
            ]).

test(empty_form_names_the_empty_product_in_type_positions) :-
    Text = "(: () (* ))\n(: Cell (* (: value ())))\n",
    dl7_text_unit(empty_form, empty_form_source, Text, Unit, ReadDiagnostics),
    ReadDiagnostics == [],
    compile_unit(Unit, Compiled, CompileDiagnostics),
    CompileDiagnostics == [],
    Compiled = compiled_unit(
                   _, checked_datalog(root_graph(Nodes, Edges), _, _, _), _),
    memberchk(':'(_, '()', ref(Empty), _), Edges),
    memberchk(product(Empty), Nodes),
    memberchk(':'(Cell, value, ref(Empty), 0), Edges),
    memberchk(':'(_, 'Cell', ref(Cell), _), Edges).

test(infix_colon_rotates_to_the_canonical_prefix_tree_at_every_depth) :-
    Text = "(User: (* (id: int) (name: text)))\n((Key \"account\" Options): int)\n",
    dl7_text_unit(infix_colon, infix_colon_source, Text, Unit, Diagnostics),
    Unit = dl7_unit(_, _, Forms, _, ExpansionRows),
    maplist(payload_tree, Forms, Trees),
    Observed = infix_colon(Diagnostics, Trees, ExpansionRows),
    Observed = infix_colon(
                   [],
                   [ form([atom(':'), atom('User'),
                           form([atom('*'),
                                 form([atom(':'), atom(id), atom(int)]),
                                 form([atom(':'), atom(name), atom(text)])])]),
                     form([atom(':'),
                           form([atom('Key'), literal("account"),
                                 atom('Options')]),
                           atom(int)])
                   ],
                   [_ | _]).

payload_tree(node(_, Payload), Tree) :-
    payload_tree_value(Payload, Tree).

payload_tree_value(atom(Name), atom(Name)).
payload_tree_value(literal(Value), literal(Value)).
payload_tree_value(variable(_, Name), variable(Name)).
payload_tree_value(form(Nodes), form(Trees)) :-
    maplist(payload_tree, Nodes, Trees).

unit_snapshot(
    dl7_unit(file(_), content_sha256(Digest),
             Forms, SourceRows, ExpansionRows),
    Snapshot) :-
    maplist(snapshot_node, Forms, FormSnapshot),
    maplist(snapshot_source, SourceRows, SourceSnapshot),
    Snapshot = reader_snapshot(Digest, FormSnapshot,
                               SourceSnapshot, ExpansionRows).

snapshot_node(node(reader_node(_, Index), Payload),
              node(Index, Snapshot)) :-
    snapshot_payload(Payload, Snapshot).

snapshot_payload(atom(Name), atom(Name)).
snapshot_payload(literal(Value), literal(Value)).
snapshot_payload(variable(VariableId, Name),
                 variable(SnapshotId, Name)) :-
    snapshot_variable_id(VariableId, SnapshotId).
snapshot_payload(form(Nodes), form(Snapshots)) :-
    maplist(snapshot_node, Nodes, Snapshots).

snapshot_variable_id(variable(reader_node(_, Index), Name),
                     variable(Index, Name)).

snapshot_source(
    source(reader_node(_, Index), _, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn),
    source(Index, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn)).

expected_snapshot(
    reader_snapshot(
        f2ae0a30fb13178923b4c1e40077cd4d71bec93c74d360e5a6754b7ba54ce26c,
        [ node(0,
               form(
                   [ node(1, atom(':')),
                     node(2, atom('User')),
                     node(3,
                          form(
                              [ node(4, atom('*')),
                                node(5,
                                     form(
                                         [ node(6, atom(':')),
                                           node(7, atom(id)),
                                           node(8, atom(int))
                                         ])),
                                node(9,
                                     form(
                                         [ node(10, atom(':')),
                                           node(11, atom(name)),
                                           node(12, atom(text))
                                         ])),
                                node(13,
                                     form(
                                         [ node(14, atom(':')),
                                           node(15, atom(note)),
                                           node(16,
                                                literal("hello\nworld"))
                                         ]))
                              ]))
                   ])),
          node(17,
               form(
                   [ node(18, atom('<-')),
                     node(19,
                          form(
                              [ node(20, atom(copy)),
                                node(21,
                                     variable(variable(17, 'Value'),
                                              'Value')),
                                node(22,
                                     variable(variable(17, 'Value'),
                                              'Value')),
                                node(23,
                                     variable(variable(23, '_'), '_')),
                                node(24,
                                     variable(variable(24, '_'), '_'))
                              ])),
                     node(25,
                          form(
                              [ node(26, atom(source)),
                                node(27,
                                     variable(variable(17, 'Value'),
                                              'Value'))
                               ]))
                    ])),
          node(28, form([])),
          node(29,
               form(
                   [ node(30, atom(':')),
                     node(31, atom('Wrapper')),
                     node(32,
                          form(
                              [ node(33, atom('*')),
                                node(34,
                                     form(
                                         [ node(35, atom(':')),
                                           node(36, atom(inner)),
                                           node(37,
                                                form(
                                                    [ node(38, atom('*')),
                                                      node(39,
                                                           form(
                                                               [ node(40,
                                                                      atom(':')),
                                                                 node(41,
                                                                      atom(tag)),
                                                                 node(42,
                                                                      literal(symbol(kind)))
                                                               ]))
                                                    ]))
                                         ])),
                                node(43,
                                     form(
                                         [ node(44, atom(':')),
                                           node(45, atom(bare)),
                                           node(46, atom(atom))
                                         ]))
                              ]))
                   ])),
          node(47, literal(symbol(spot)))
        ],
        [ source(0, 27, 103, 3, 1, 6, 32),
          source(1, 28, 29, 3, 2, 3, 3),
          source(2, 30, 34, 3, 4, 3, 8),
          source(3, 38, 102, 4, 4, 6, 31),
          source(4, 39, 40, 4, 5, 4, 6),
          source(5, 41, 51, 4, 7, 4, 17),
          source(6, 42, 43, 4, 8, 4, 9),
          source(7, 44, 46, 4, 10, 4, 12),
          source(8, 47, 50, 4, 13, 4, 16),
          source(9, 58, 71, 5, 7, 5, 20),
          source(10, 59, 60, 5, 8, 5, 9),
          source(11, 61, 65, 5, 10, 5, 14),
          source(12, 66, 70, 5, 15, 5, 19),
          source(13, 78, 101, 6, 7, 6, 30),
          source(14, 79, 80, 6, 8, 6, 9),
          source(15, 81, 85, 6, 10, 6, 14),
          source(16, 86, 100, 6, 15, 6, 29),
          source(17, 104, 155, 7, 1, 8, 21),
          source(18, 105, 107, 7, 2, 7, 4),
          source(19, 108, 134, 7, 5, 7, 31),
          source(20, 109, 113, 7, 6, 7, 10),
          source(21, 114, 120, 7, 11, 7, 17),
          source(22, 121, 127, 7, 18, 7, 24),
          source(23, 128, 130, 7, 25, 7, 27),
          source(24, 131, 133, 7, 28, 7, 30),
          source(25, 139, 154, 8, 5, 8, 20),
          source(26, 140, 146, 8, 6, 8, 12),
          source(27, 147, 153, 8, 13, 8, 19),
          source(28, 156, 158, 9, 1, 9, 3),
          source(29, 211, 286, 11, 1, 14, 22),
          source(30, 212, 213, 11, 2, 11, 3),
          source(31, 214, 221, 11, 4, 11, 11),
          source(32, 225, 285, 12, 4, 14, 21),
          source(33, 226, 227, 12, 5, 12, 6),
          source(34, 228, 264, 12, 7, 13, 28),
          source(35, 229, 230, 12, 8, 12, 9),
          source(36, 231, 236, 12, 10, 12, 15),
          source(37, 246, 263, 13, 10, 13, 27),
          source(38, 247, 248, 13, 11, 13, 12),
          source(39, 249, 262, 13, 13, 13, 26),
          source(40, 250, 251, 13, 14, 13, 15),
          source(41, 252, 255, 13, 16, 13, 19),
          source(42, 256, 261, 13, 20, 13, 25),
          source(43, 271, 284, 14, 7, 14, 20),
          source(44, 272, 273, 14, 8, 14, 9),
          source(45, 274, 278, 14, 10, 14, 14),
          source(46, 279, 283, 14, 15, 14, 19),
          source(47, 287, 292, 15, 1, 15, 6)
        ],
        [])).

:- end_tests(dl7_reader_foundation).
