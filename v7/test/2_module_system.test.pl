:- begin_tests(dl7_module_system).

:- use_module('../src/0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../src/2_comptime/0_lowerer', [lower_datalog/4]).
:- use_module('../src/2_comptime/0b_filesystem_grapher',
              [install_project_graph/6]).
:- use_module('../src/2_comptime/0c_module_resolver',
              [ resolve_path/6,
                check_visible_name_collisions/3,
                check_module_cycles/2
              ]).
:- use_module('../src/2_comptime/1_checker', [check_datalog/4]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project/5]).

test(numbered_filesystem_segments_form_dense_product_edges) :-
    Root = '/virtual/project',
    Path = '/virtual/project/0_src/1_models/0_users.dl7',
    Unit = dl7_unit(file(Path), content_sha256(test), [], [], []),
    install_project_graph(dl7_project(Root, [Unit]), [], [],
                          Basements, ModuleOrigins, Diagnostics),
    RootOwner = module(directory(Root)),
    SrcOwner = module(directory('/virtual/project/0_src')),
    ModelsOwner = module(directory('/virtual/project/0_src/1_models')),
    UsersOwner = module(file(Path)),
    Basements =
        [module_basement(
             RootOwner,
             basement_program(
                 root_graph(
                     [ node(RootOwner), module(RootOwner), product(RootOwner),
                       node(SrcOwner), module(SrcOwner), product(SrcOwner),
                       node(ModelsOwner), module(ModelsOwner),
                       product(ModelsOwner), product(UsersOwner)
                     ],
                     [ pending_edge(RootOwner, src, target(SrcOwner), 0),
                       pending_edge(SrcOwner, models,
                                    target(ModelsOwner), 0),
                       pending_edge(ModelsOwner, users,
                                    target(UsersOwner), 0)
                     ]),
                 datalog_program([], [], [])))],
    ModuleOrigins = [module_origins(RootOwner, EdgeOrigins)],
    length(EdgeOrigins, 3),
    Diagnostics == [].

test(filesystem_products_are_traversed_with_colon_goals) :-
    Root = 'v7/test/fixtures/modules',
    AccountsPath = 'v7/test/fixtures/modules/0_accounts.dl7',
    ConsumerPath = 'v7/test/fixtures/modules/1_consumer.dl7',
    compile_dl7_project(Root, [AccountsPath, ConsumerPath],
                        Rows, Runtime, Diagnostics),
    absolute_file_name(Root, CanonicalRoot,
                       [file_type(directory), access(read)]),
    absolute_file_name(AccountsPath, CanonicalAccounts, [access(read)]),
    absolute_file_name(ConsumerPath, CanonicalConsumer, [access(read)]),
    RootOwner = module(directory(CanonicalRoot)),
    AccountsOwner = module(file(CanonicalAccounts)),
    ConsumerOwner = module(file(CanonicalConsumer)),
    Runtime = checked_datalog(
                  root_graph(Nodes, Edges),
                  datalog_program(_, _, _), _, _),
    memberchk(product(RootOwner), Nodes),
    memberchk(product(AccountsOwner), Nodes),
    memberchk(product(ConsumerOwner), Nodes),
    memberchk(':'(RootOwner, accounts, ref(AccountsOwner), 0), Edges),
    memberchk(':'(RootOwner, consumer, ref(ConsumerOwner), 1), Edges),
    memberchk(':'(AccountsOwner, 'User', ref(UserType), 0), Edges),
    memberchk(':'(ConsumerOwner, found_user, ref(FoundUser), 0), Edges),
    memberchk(call(ref(FoundUser), [ref(UserType)]), Rows),
    Diagnostics == [].

test(sum_and_product_edges_share_one_path_traversal) :-
    Text = "(: Option (+ (: none (*)) (: some (* (: value int)))))",
    dl7_text_unit(path_fixture, path_fixture_source, Text, Unit, []),
    lower_datalog(Unit, Basement, Origins, []),
    check_datalog(Basement, Origins, Checked, []),
    Checked = checked_datalog(root_graph(_, Edges), _, _, _),
    resolve_path(module(path_fixture),
                 ['Option', some, value], Edges,
                 Target, Proof, Diagnostics),
    Target = primitive(int),
    Proof = [ path_step(module(path_fixture), 'Option', Sum, 0),
              path_step(Sum, some, Some, 1),
              path_step(Some, value, primitive(int), 0)
            ],
    Diagnostics == [].

test(path_resolution_reports_first_missing_and_ambiguous_segment) :-
    Edges = [ ':'(root, item, ref(left), 0),
              ':'(root, item, ref(right), 1)
            ],
    resolve_path(root, [item, value], Edges,
                 AmbiguousTarget, AmbiguousProof, AmbiguousDiagnostics),
    resolve_path(root, [missing], Edges,
                 MissingTarget, MissingProof, MissingDiagnostics),
    Observed = path_failures(
                   ambiguous(AmbiguousTarget, AmbiguousProof,
                             AmbiguousDiagnostics),
                   missing(MissingTarget, MissingProof,
                           MissingDiagnostics)),
    Observed == path_failures(
                    ambiguous(
                        none, [],
                        [diagnostic(
                             resolve_path, segment(0),
                             ambiguous_path_segment(
                                 root, item,
                                 [ path_step(root, item, left, 0),
                                   path_step(root, item, right, 1)
                                 ]))]),
                    missing(
                        none, [],
                        [diagnostic(
                             resolve_path, segment(0),
                             missing_path_segment(root, missing))])).

test(visible_name_checks_separate_local_duplicates_from_import_aliases) :-
    Edges = [ pending_edge(module(a), local, target(left), 0),
              pending_edge(module(a), local, target(right), 1)
            ],
    Imports = [ module_import(module(a), module(b), dep),
                module_import(module(a), module(c), dep),
                module_import(module(a), module(d), same),
                module_import(module(a), module(d), same)
              ],
    check_visible_name_collisions(Edges, Imports, Diagnostics),
    Diagnostics ==
        [ diagnostic(
              module, module(a),
              ambiguous_import_alias(
                  module(a), dep, [module(b), module(c)])),
          diagnostic(
              module, module(a),
              duplicate_local_name(
                  module(a), local,
                  [edge(target(left), 0), edge(target(right), 1)])),
          diagnostic(
              module, module(a),
              duplicate_module_import(module(a), module(d), same))
        ].

test(module_cycles_report_closed_canonical_paths) :-
    Imports = [ module_import(c, a, a),
                module_import(a, b, b),
                module_import(b, c, c),
                module_import(x, x, self)
              ],
    check_module_cycles(Imports, Diagnostics),
    Diagnostics ==
        [ diagnostic(module, none, module_dependency_cycle([a, b, c, a])),
          diagnostic(module, none, module_dependency_cycle([x, x]))
        ].

:- end_tests(dl7_module_system).
