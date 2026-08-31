:- begin_tests(dl7_module_system).

:- use_module('../src/2_comptime/0b_filesystem_grapher',
              [install_project_graph/6]).
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

:- end_tests(dl7_module_system).
