:- begin_tests(dl7_entrypoints).

:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../src/0_reader/3_file_loader', [load_dl7/3]).
:- use_module('../src/0_reader/4_module_loader', [load_dl7_units/3]).
:- use_module('../src/2_comptime/2_compiler',
              [ compile_dl7/4,
                compile_unit/3,
                type_prelude_paths/1
              ]).
:- use_module('../src/2_comptime/0_lowerer', [lower_datalog/4]).
:- use_module('../src/2_comptime/0a_module_lowerer',
              [ lower_units/4,
                merge_module_basements/4,
                install_module_aliases/6
              ]).
:- use_module('../src/2_comptime/1_checker',
              [ check_datalog/4,
                check_goal_sequence/4
              ]).
:- use_module('../src/2_comptime/1a_generated_program_assembler',
              [assemble_generated_program/5]).
:- use_module('../src/3_emit/0_artifact_emitter', [emit_compiled/4]).
:- use_module('../src/1_libtime/0_evaluator',
              [ evaluate/4,
                stratify_rules/3,
                validate_functional_rows/3
              ]).
:- use_module('fixtures/1_embedded', []).

test(module_loader_preserves_separate_file_units) :-
    load_dl7_units(
        [ 'v7/test/fixtures/0_minimal.dl7',
          'v7/test/fixtures/2_partial.dl7'
        ],
        Units, Diagnostics),
    maplist(unit_file_name, Units, FileNames),
    Observed = module_units(Diagnostics, FileNames),
    Observed == module_units([], ['0_minimal.dl7', '2_partial.dl7']).

test(module_owner_is_stable_across_content_revisions) :-
    Origin = file('/virtual/module.dl7'),
    UnitA = dl7_unit(Origin, content_sha256(first), [], [], []),
    UnitB = dl7_unit(Origin, content_sha256(second), [], [], []),
    lower_datalog(UnitA, BasementA, _, []),
    lower_datalog(UnitB, BasementB, _, []),
    basement_module_owner(BasementA, OwnerA),
    basement_module_owner(BasementB, OwnerB),
    Observed = module_owners(OwnerA, OwnerB),
    Observed ==
        module_owners(module(file('/virtual/module.dl7')),
                      module(file('/virtual/module.dl7'))).

test(separate_module_basements_merge_without_local_name_collapse) :-
    dl7_text_unit(file('/virtual/a.dl7'), '/virtual/a.dl7',
                  "(: Shared (* (: id int)))", UnitA, []),
    dl7_text_unit(file('/virtual/b.dl7'), '/virtual/b.dl7',
                  "(: Shared (* (: name text)))", UnitB, []),
    lower_units([UnitA, UnitB], ModuleBasements, ModuleOrigins, []),
    merge_module_basements(ModuleBasements, ModuleOrigins,
                           Basement, Origins),
    Basement = basement_program(root_graph(_, Edges),
                                datalog_program(Relations, _, _)),
    findall(Owner,
            member(pending_edge(Owner, 'Shared', _, 0), Edges),
            SharedOwners),
    maplist(module_origin_count, ModuleOrigins, OriginCounts),
    length(Relations, RelationCount),
    length(Origins, OriginCount),
    Observed = module_merge(SharedOwners, OriginCounts,
                            RelationCount, OriginCount),
    Observed ==
        module_merge(
            [ module(file('/virtual/a.dl7')),
              module(file('/virtual/b.dl7'))
            ],
            [ module(file('/virtual/a.dl7'))-4,
              module(file('/virtual/b.dl7'))-4
            ],
            2, 8).

test(module_alias_edges_preserve_exporting_type_identity) :-
    PreludeOwner = module(prelude),
    ProgramOwner = module(file('/virtual/program.dl7')),
    dl7_text_unit(prelude, prelude,
                  "(: Box (* (: value int)))", PreludeUnit, []),
    dl7_text_unit(file('/virtual/program.dl7'), '/virtual/program.dl7',
                  "(: Use (* (: field Box)))", ProgramUnit, []),
    lower_units([PreludeUnit, ProgramUnit],
                Basements0, Origins0, []),
    install_module_aliases(PreludeOwner, [ProgramOwner],
                           Basements0, Origins0, Basements, ModuleOrigins),
    merge_module_basements(Basements, ModuleOrigins, Basement, Origins),
    check_datalog(Basement, Origins, Checked, Diagnostics),
    Checked = checked_datalog(root_graph(_, Edges), _, _, _),
    memberchk(':'(PreludeOwner, 'Box', ref(Box), 0), Edges),
    memberchk(':'(ProgramOwner, 'Box', ref(Box), 1), Edges),
    memberchk(':'(ProgramOwner, 'Use', ref(Use), 0), Edges),
    memberchk(':'(Use, field, ref(Box), 0), Edges),
    Observed = module_alias(Diagnostics, same_target, alias_index(1)),
    Observed == module_alias([], same_target, alias_index(1)).

unit_file_name(dl7_unit(file(Path), _, _, _, _), FileName) :-
    file_base_name(Path, FileName).

basement_module_owner(
    basement_program(root_graph(Nodes, _), _), Owner) :-
    memberchk(module(Owner), Nodes).

module_origin_count(module_origins(Module, Origins), Module-Count) :-
    length(Origins, Count).

test(numbered_prelude_files_are_loaded_in_lexical_order) :-
    type_prelude_paths(Paths),
    maplist(file_base_name, Paths, Names),
    Names == ['0_constructors.dl7', '1_declarations.dl7',
              '2_constructor_rules.dl7', '3_derived_rules.dl7',
              '4_type_algebra.dl7'],
    maplist(file_exists, Paths, Exists),
    Exists == [true, true, true, true, true].

test(split_prelude_loads_all_existing_type_algebra_declarations) :-
    compile_dl7('v7/test/fixtures/2_partial.dl7', Rows, _Runtime, Diagnostics),
    once(type_operator_snapshot(Rows, Snapshot)),
    Observed = prelude_load(Diagnostics, Snapshot),
    Observed = prelude_load([], _),
    compound(Snapshot),
    !.

file_exists(Path, true) :-
    exists_file(Path),
    !.
file_exists(_, false).

test(file_and_bare_quasi_share_reader_and_expansion_pipeline) :-
    load_dl7('v7/test/fixtures/0_minimal.dl7',
             FileUnit, FileDiagnostics),
    FileUnit = dl7_unit(FileOrigin, content_sha256(FileDigest),
                        FileForms, FileRows, FileExpansions),
    once(dl7_embedded_fixture:dl7_unit(
             EmbeddedOrigin, content_sha256(EmbeddedDigest),
             EmbeddedForms, EmbeddedRows, EmbeddedExpansions)),
    content_snapshot(FileForms, FileRows, FileContent),
    content_snapshot(EmbeddedForms, EmbeddedRows, EmbeddedContent),
    origin_kinds(FileOrigin, EmbeddedOrigin, OriginKinds),
    equality(FileDigest, EmbeddedDigest, DigestEqual),
    equality(FileContent, EmbeddedContent, ContentEqual),
    Observed = entrypoint_result(
                   OriginKinds, DigestEqual, ContentEqual,
                   FileDiagnostics, FileExpansions, EmbeddedExpansions),
    Observed == entrypoint_result(true, true, true, [], [], []).

test(driver_is_canonical_on_two_consecutive_runs) :-
    load_dl7('v7/test/fixtures/0_minimal.dl7',
             ExpectedUnit, []),
    driver_run(Status1, Stdout1, Stderr1),
    driver_run(Status2, Stdout2, Stderr2),
    term_string(Unit1, Stdout1),
    term_string(Unit2, Stdout2),
    equality(Stdout1, Stdout2, OutputEqual),
    equality(Unit1, ExpectedUnit, FirstUnitEqual),
    equality(Unit2, ExpectedUnit, SecondUnitEqual),
    Observed = driver_result(Status1, Status2, OutputEqual,
                             FirstUnitEqual, SecondUnitEqual,
                             Stderr1, Stderr2),
    Observed == driver_result(exit(0), exit(0), true,
                              true, true, "", "").

test(expression_carrier_keeps_values_and_rejects_unresolved_forms) :-
    Owner = owner(expression_fixture),
    Environment = expression_environment([], [], []),
    dl7_lowerer:lower_expression(
        node(variable_node, variable(value, "?Value")),
        Owner, Environment,
        Variable, VariableGoals, VariableOrigins, VariableDiagnostics),
    dl7_lowerer:lower_expression(
        node(literal_node, literal("Ada")),
        Owner, Environment,
        Literal, LiteralGoals, LiteralOrigins, LiteralDiagnostics),
    dl7_lowerer:lower_expression(
        node(atom_node, atom('User')),
        Owner, Environment,
        Atom, AtomGoals, AtomOrigins, AtomDiagnostics),
    dl7_lowerer:lower_expression(
        node(form_node, form([])),
        Owner, Environment,
        Form, FormGoals, FormOrigins, FormDiagnostics),
    Observed = expression_carrier(
                   variable(Variable, VariableGoals, VariableOrigins,
                            VariableDiagnostics),
                   literal(Literal, LiteralGoals, LiteralOrigins,
                           LiteralDiagnostics),
                   atom(Atom, AtomGoals, AtomOrigins, AtomDiagnostics),
                   unresolved(Form, FormGoals, FormOrigins,
                              FormDiagnostics)),
    Observed == expression_carrier(
                    variable(var(value), [], [], []),
                    literal(const("Ada"), [], [], []),
                    atom(name(Owner, 'User'), [], [], []),
                    unresolved(
                        none, [], [],
                        [diagnostic(lower, form_node,
                                    unresolved_expression_form)])),
    !.

test(expression_return_position_is_declared_projection_metadata) :-
    Callable = owner(return_fixture),
    Missing = owner(missing_return_fixture),
    Multiple = owner(multiple_return_fixture),
    Environment = expression_environment(
                      [], [],
                      [ pending_edge(Callable, source,
                                     name(Callable, type), 0),
                        pending_edge(Callable, return,
                                     name(Callable, type), 1),
                        pending_edge(Missing, value,
                                     name(Missing, type), 0),
                        pending_edge(Multiple, return,
                                     name(Multiple, type), 0),
                        pending_edge(Multiple, return,
                                     name(Multiple, type), 1)
                      ]),
    dl7_lowerer:expression_return_position(
        target(Callable), Environment, declared_node,
        DeclaredPosition, DeclaredDiagnostics),
    dl7_lowerer:expression_return_position(
        target(Missing), Environment, missing_node,
        MissingPosition, MissingDiagnostics),
    dl7_lowerer:expression_return_position(
        target(Multiple), Environment, multiple_node,
        MultiplePosition, MultipleDiagnostics),
    dl7_lowerer:expression_return_position(
        kernel(cons), Environment, kernel_node,
        KernelPosition, KernelDiagnostics),
    Observed = return_positions(
                   declared(DeclaredPosition, DeclaredDiagnostics),
                   missing(MissingPosition, MissingDiagnostics),
                   multiple(MultiplePosition, MultipleDiagnostics),
                   kernel(KernelPosition, KernelDiagnostics)),
    Observed == return_positions(
                    declared(1, []),
                    missing(
                        none,
                        [diagnostic(
                             lower, missing_node,
                             expression_without_return(target(Missing)))]),
                    multiple(
                        none,
                        [diagnostic(
                             lower, multiple_node,
                             expression_multiple_returns(
                                 target(Multiple), [0, 1]))]),
                    kernel(2, [])),
    !.

test(expression_modes_use_return_keys_without_restricting_full_calls) :-
    Text = "(: Source (*))\n(: Left (*))\n(: Right (*))\n(: Choice (* (: source type) (: return type)))\n(: many (* (: source type) (: result type)))\n(many Source Left)\n(many Source Right)\n",
    dl7_text_unit(expression_modes, expression_modes_source,
                  Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 Diagnostics),
    named_owner(CompilerRows, 'Source', Source),
    named_owner(CompilerRows, 'Choice', Choice),
    named_owner(CompilerRows, many, Many),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(Relations, _, _), _, _),
    memberchk(relation(ref(Choice), 2, ChoiceKeys), Relations),
    findall(Result,
            member(call(ref(Many), [ref(Source), Result]), CompilerRows),
            Results),
    sort(Results, DistinctResults),
    length(DistinctResults, ResultCount),
    ModeOwner = owner(mode_fixture),
    ModeCallable = owner(mode_callable),
    ModeEnvironment = expression_environment(
                          [reservation(
                               ModeOwner, 'Choice', target(ModeCallable),
                               product)],
                          [relation(ModeCallable, 2, [[1]])],
                          [ pending_edge(ModeCallable, source,
                                         name(ModeCallable, type), 0),
                            pending_edge(ModeCallable, return,
                                         name(ModeCallable, type), 1)
                          ]),
    dl7_lowerer:lower_expression(
        node(mode_node,
             form([ node(mode_operator, atom('Choice')),
                    node(mode_argument, literal("value"))
                  ])),
        ModeOwner, ModeEnvironment,
        none, [], [], ModeDiagnostics),
    Observed = expression_modes(
                   Diagnostics,
                   inferred_keys(ChoiceKeys),
                   ambiguous(ModeDiagnostics),
                   explicit_full_call_answers(ResultCount)),
    Observed == expression_modes(
                    [],
                    inferred_keys([[0]]),
                    ambiguous(
                        [diagnostic(
                             lower, mode_node,
                             ambiguous_expression_projection(
                                 'Choice', supplied([0]), keys([[1]]),
                                 return(1)))]),
                    explicit_full_call_answers(2)),
    !.

test(rhs_relation_application_derives_the_bound_edge) :-
    Text = "(: User (*))\n(: Echo (* (: source type) (: return type)))\n(Echo User User)\n(: Alias (Echo User))\n",
    dl7_text_unit(rhs_application, rhs_application_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(TypeGraph, RuntimeProgram, CompilerRows),
                 Diagnostics),
    named_owner(CompilerRows, 'User', User),
    named_owner(CompilerRows, 'Alias', AliasTarget),
    named_owner(CompilerRows, 'Echo', Echo),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    memberchk(
        rule(call(ref(kernel(':')),
                  [ref(Module), const('Alias'), var(Result), const(2)]),
             [checked_goal(
                  positive,
                  call(ref(Echo), [ref(User), var(Result)]))]),
        Rules),
    memberchk(':'(Module, 'Alias', ref(User), 2), TypeGraph),
    memberchk(call(ref(kernel(':')),
                   [ ref(Module), const('Alias'), ref(User), const(2)
                   ]),
              CompilerRows),
    equality(AliasTarget, User, AliasMatches),
    missing_rhs_application_receipt(MissingReceipt),
    Observed = rhs_application(
                   success(Diagnostics, alias_matches(AliasMatches)),
                   MissingReceipt),
    Observed == rhs_application(
                    success([], alias_matches(true)),
                    missing(node(18), name('Alias'), index(2))),
    !.

test(nested_and_chained_bind_applications_flatten_in_dependency_order) :-
    Text = "(: User (*))\n(: Echo (* (: source type) (: return type)))\n(: Wrap (* (: source type) (: return type)))\n(Echo User User)\n(Wrap User User)\n(: Nested (Wrap (Echo User)))\n(: First (Echo User))\n(: Chained (Wrap First))\n",
    dl7_text_unit(nested_applications, nested_applications_source,
                  Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 Diagnostics),
    named_owner(CompilerRows, 'User', User),
    named_owner(CompilerRows, 'Echo', Echo),
    named_owner(CompilerRows, 'Wrap', Wrap),
    named_owner(CompilerRows, 'Nested', NestedTarget),
    named_owner(CompilerRows, 'First', FirstTarget),
    named_owner(CompilerRows, 'Chained', ChainedTarget),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    memberchk(
        rule(call(ref(kernel(':')),
                  [ref(Module), const('Nested'), var(NestedResult), const(3)]),
             [ checked_goal(
                   positive,
                   call(ref(Echo),
                        [ref(User), var(InnerResult)])),
               checked_goal(
                   positive,
                   call(ref(Wrap),
                        [var(InnerResult), var(NestedResult)]))
             ]),
        Rules),
    memberchk(
        rule(call(ref(kernel(':')),
                  [ref(Module), const('Chained'), var(ChainedResult),
                   const(5)]),
             [ checked_goal(
                   positive,
                   call(ref(kernel(':')),
                        [ ref(Module), const('First'), var(FirstValue),
                          const(4)
                        ])),
               checked_goal(
                   positive,
                   call(ref(Wrap),
                        [var(FirstValue), var(ChainedResult)]))
             ]),
        Rules),
    maplist(equality(User),
            [NestedTarget, FirstTarget, ChainedTarget],
            TargetMatches),
    nested_expression_diagnostic_receipt(NestedDiagnostic),
    Observed = nested_applications(
                   Diagnostics, TargetMatches, NestedDiagnostic),
    Observed == nested_applications(
                    [], [true, true, true],
                    diagnostic(node(23), undeclared_relation('Missing'))),
    !.

test(rule_heads_and_bodies_share_expression_lowering) :-
    Text = "(: User (*))\n(: source (* (: value type)))\n(: Wrap (* (: source type) (: return type)))\n(: sink (* (: value type)))\n(: projected (* (: value type)))\n(: accepted (* (: value type)))\n(source User)\n(Wrap User User)\n(sink User)\n(<- (projected (Wrap ?Value))\n    (source ?Value))\n(<- (accepted ?Value)\n    (source ?Value)\n    (sink (Wrap ?Value)))\n",
    dl7_text_unit(uniform_expressions, uniform_expressions_source,
                  Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 Diagnostics),
    named_owner(CompilerRows, 'User', User),
    named_owner(CompilerRows, source, Source),
    named_owner(CompilerRows, 'Wrap', Wrap),
    named_owner(CompilerRows, sink, Sink),
    named_owner(CompilerRows, projected, Projected),
    named_owner(CompilerRows, accepted, Accepted),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    memberchk(
        rule(call(ref(Projected), [var(ProjectedValue)]),
             [ checked_goal(
                   positive,
                   call(ref(Source), [var(SourceValue)])),
               checked_goal(
                   positive,
                   call(ref(Wrap),
                        [var(SourceValue), var(ProjectedValue)]))
             ]),
        Rules),
    memberchk(
        rule(call(ref(Accepted), [var(AcceptedValue)]),
             [ checked_goal(
                   positive,
                   call(ref(Source), [var(AcceptedValue)])),
               checked_goal(
                   positive,
                   call(ref(Wrap),
                        [var(AcceptedValue), var(WrappedValue)])),
               checked_goal(
                   positive,
                   call(ref(Sink), [var(WrappedValue)]))
             ]),
        Rules),
    row_presence(CompilerRows,
                 call(ref(Projected), [ref(User)]), ProjectedPresent),
    row_presence(CompilerRows,
                 call(ref(Accepted), [ref(User)]), AcceptedPresent),
    Observed = uniform_expressions(
                   Diagnostics,
                   outputs(projected(ProjectedPresent),
                           accepted(AcceptedPresent))),
    Observed == uniform_expressions(
                    [], outputs(projected(true), accepted(true))),
    !.

test(full_tuple_calls_preserve_reverse_relational_queries) :-
    Text = "(: User (*))\n(: Partial (* (: source type) (: return type)))\n(Partial User User)\n(: UserPatch (Partial User))\n(: reversed (* (: source type)))\n(<- (reversed ?Source)\n    (Partial ?Source UserPatch))\n",
    dl7_text_unit(reverse_query, reverse_query_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 Diagnostics),
    named_owner(CompilerRows, 'User', User),
    named_owner(CompilerRows, 'Partial', Partial),
    named_owner(CompilerRows, 'UserPatch', UserPatch),
    named_owner(CompilerRows, reversed, Reversed),
    equality(UserPatch, User, PatchMatches),
    row_presence(CompilerRows,
                 call(ref(Reversed), [ref(User)]), ReversePresent),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    memberchk(
        rule(call(ref(Reversed), [var(Source)]),
             [ checked_goal(
                   positive,
                          call(ref(kernel(':')),
                        [ ref(Module), const('UserPatch'), var(KnownResult),
                          const(2)
                        ])),
               checked_goal(
                   positive,
                   call(ref(Partial), [var(Source), var(KnownResult)]))
             ]),
        Rules),
    memberchk(call(ref(kernel(':')),
                   [ref(Module), const('UserPatch'), ref(User), const(2)]),
              CompilerRows),
    Observed = reverse_query(
                   Diagnostics,
                   patch_matches(PatchMatches),
                   source_derived(ReversePresent)),
    Observed == reverse_query(
                    [], patch_matches(true), source_derived(true)),
    !.

test(named_punned_mixed_and_omitted_slots_lower_to_one_relation_tuple) :-
    Text = "(: User (* (: id int) (: name text) (: email text)))\n(: selected (* (: name text) (: id int)))\n(: explicit (* (: name text)))\n(: mixed (* (: name text)))\n(User 7 \"Ada\" \"ada@example.test\")\n(<- (selected ?name ?id) (User ?name ?id))\n(<- (explicit ?name) (User (name: ?name) (id: 7)))\n(<- (mixed ?name) (User (name: ?name) 7))\n",
    dl7_text_unit(named_calls, named_calls_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 Diagnostics),
    named_owner(CompilerRows, 'User', User),
    named_owner(CompilerRows, selected, Selected),
    named_owner(CompilerRows, explicit, Explicit),
    named_owner(CompilerRows, mixed, Mixed),
    row_presence(CompilerRows,
                 call(ref(Selected), [const("Ada"), const(7)]),
                 SelectedPresent),
    row_presence(CompilerRows,
                 call(ref(Explicit), [const("Ada")]), ExplicitPresent),
    row_presence(CompilerRows,
                 call(ref(Mixed), [const("Ada")]), MixedPresent),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    memberchk(
        rule(call(ref(Selected), [var(Name), var(Id)]),
             [checked_goal(
                  positive,
                  call(ref(User),
                       [var(Id), var(Name), var(_OmittedEmail)]))]),
        Rules),
    named_call_diagnostics(DuplicateDiagnostic, UnknownDiagnostic),
    Observed = named_calls(
                   Diagnostics,
                   rows(SelectedPresent, ExplicitPresent, MixedPresent),
                   diagnostics(DuplicateDiagnostic, UnknownDiagnostic)),
    Observed == named_calls(
                    [], rows(true, true, true),
                    diagnostics(
                        duplicate_argument_slot(1),
                        unknown_argument_label(missing))),
    !.

named_call_diagnostics(DuplicateReason, UnknownReason) :-
    DuplicateText = "(: User (* (: id int) (: name text)))\n(<- (User (name: ?name) ?name) (User 1 \"Ada\"))\n",
    dl7_text_unit(named_duplicate, named_duplicate_source,
                  DuplicateText, DuplicateUnit, []),
    compile_unit(
        DuplicateUnit, [],
        [diagnostic(lower, _, DuplicateReason)]),
    UnknownText = "(: User (* (: id int)))\n(<- (User (missing: ?value)) (User 1))\n",
    dl7_text_unit(named_unknown, named_unknown_source,
                  UnknownText, UnknownUnit, []),
    compile_unit(
        UnknownUnit, [],
        [diagnostic(lower, _, UnknownReason)]).

test(compile_known_partial_application_erases_to_one_direct_call) :-
    Text = "(: User (*))\n(: Order (*))\n(: PairResult (*))\n(: Pair (* (: left type) (: right type) (: return type)))\n(Pair User Order PairResult)\n(: Curried ((Pair User) Order))\n",
    dl7_text_unit(partial_application, partial_application_source,
                  Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 Diagnostics),
    named_owner(CompilerRows, 'User', User),
    named_owner(CompilerRows, 'Order', Order),
    named_owner(CompilerRows, 'PairResult', PairResult),
    named_owner(CompilerRows, 'Pair', Pair),
    named_owner(CompilerRows, 'Curried', Curried),
    equality(Curried, PairResult, CurriedMatches),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    memberchk(
        rule(call(ref(kernel(':')),
                  [ref(Module), const('Curried'), var(Result), const(4)]),
             [checked_goal(
                  positive,
                  call(ref(Pair),
                       [ref(User), ref(Order), var(Result)]))]),
        Rules),
    memberchk(call(ref(kernel(':')),
                   [ ref(Module), const('Curried'), ref(PairResult), const(4)
                   ]),
              CompilerRows),
    residual_partial_terms(RuntimeProgram, ResidualPartials),
    partial_application_escape_receipt(EscapeReceipt),
    Observed = partial_application(
                   Diagnostics,
                   result_matches(CurriedMatches),
                   residual_partial_terms(ResidualPartials),
                   EscapeReceipt),
    Observed == partial_application(
                    [], result_matches(true), residual_partial_terms(0),
                    unsaturated(
                        node(22),
                        partial_application_requires_more_arguments(
                            'PairUser'))),
    !.

test(named_and_punned_expression_slots_complete_direct_and_partial_calls) :-
    Text = "(: User (*))\n(: Order (*))\n(: PairResult (*))\n(: Pair (* (: left type) (: right type) (: return type)))\n(Pair User Order PairResult)\n(: Direct (Pair (right: Order) (left: User)))\n(: Curried ((Pair (right: Order)) (left: User)))\n(: Punned (Pair ?right ?left))\n",
    dl7_text_unit(named_expressions, named_expressions_source,
                  Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 Diagnostics),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram2, CompilerRows2),
                 Diagnostics2),
    named_owner(CompilerRows, 'PairResult', PairResult),
    named_owner(CompilerRows, 'Direct', Direct),
    named_owner(CompilerRows, 'Curried', Curried),
    named_owner(CompilerRows, 'Punned', Punned),
    maplist(equality(PairResult), [Direct, Curried, Punned], Equalities),
    residual_partial_terms(RuntimeProgram, ResidualPartials),
    equality(CompilerRows, CompilerRows2, RowsRepeat),
    equality(RuntimeProgram, RuntimeProgram2, RuntimeRepeat),
    Observed = named_expressions(
                   diagnostics(Diagnostics, Diagnostics2), Equalities,
                   residual_partial_terms(ResidualPartials),
                   repeat(rows(RowsRepeat), runtime(RuntimeRepeat))),
    Observed == named_expressions(
                    diagnostics([], []), [true, true, true],
                    residual_partial_terms(0),
                    repeat(rows(true), runtime(true))),
    !.

partial_application_escape_receipt(unsaturated(node(NodeIndex), Reason)) :-
    Text = "(: User (*))\n(: Pair (* (: left type) (: right type) (: return type)))\n(: PairUser (Pair User))\n",
    dl7_text_unit(partial_escape, partial_escape_source, Text, Unit, []),
    compile_unit(
        Unit, [],
        [diagnostic(lower,
                    reader_node(partial_escape_source, NodeIndex),
                    Reason)]).

residual_partial_terms(Term, Count) :-
    findall(Partial,
            ( sub_term(Partial, Term),
              Partial = partial_application(_, _)
            ),
            Partials),
    length(Partials, Count).

nested_expression_diagnostic_receipt(diagnostic(node(NodeIndex), Reason)) :-
    Text = "(: User (*))\n(: Wrap (* (: source type) (: return type)))\n(: Bad (Wrap (Missing User)))\n",
    dl7_text_unit(nested_diagnostic, nested_diagnostic_source,
                  Text, Unit, []),
    compile_unit(
        Unit, [],
        [diagnostic(lower,
                    reader_node(nested_diagnostic_source, NodeIndex),
                    Reason)]).

missing_rhs_application_receipt(
    missing(node(NodeIndex), name(Name), index(Index))) :-
    Text = "(: User (*))\n(: Empty (* (: source type) (: return type)))\n(: Alias (Empty User))\n",
    dl7_text_unit(missing_rhs_application, missing_rhs_application_source,
                  Text, Unit, []),
    compile_unit(
        Unit, [],
        [diagnostic(
             compile,
             reader_node(missing_rhs_application_source, NodeIndex),
             missing_derived_bind(_, Name, Index))]).

test(userland_type_operators_chain_across_compiler_rounds) :-
    compile_dl7('v7/test/fixtures/2_partial.dl7',
                Rows1, Runtime1, Diagnostics1),
    compile_dl7('v7/test/fixtures/2_partial.dl7',
                Rows2, Runtime2, Diagnostics2),
    once(type_operator_snapshot(Rows1, Snapshot)),
    runtime_snapshot(Runtime1, RuntimeSnapshot),
    runtime_key_snapshot(Runtime1, KeySnapshot),
    compound_key_snapshot(Rows1, CompoundKeySnapshot),
    history_v1_snapshot(Rows1, Runtime1, HistorySnapshot),
    evaluator_snapshot(EvaluatorSnapshot),
    equality(Rows1, Rows2, RowsEqual),
    equality(Runtime1, Runtime2, RuntimeEqual),
    length(Rows1, CompilerRowCount),
    Observed = partial_result(Diagnostics1, Diagnostics2,
                              CompilerRowCount, Snapshot,
                              RuntimeSnapshot, KeySnapshot,
                              CompoundKeySnapshot, HistorySnapshot,
                              EvaluatorSnapshot,
                              RowsEqual, RuntimeEqual),
    Observed == partial_result(
                    [], [], 5456,
                    type_operators(
                        partial([mapped(id, option(int), 0),
                                 mapped(name, option(text), 1)]),
                        pick([mapped(id, option(int), 0),
                              mapped(name, option(text), 1)]),
                        exclude([mapped(name, option(text), 0)])),
                    runtime(counts(178, 382, 85, 298, 116, 197, 85),
                            normalized(true)),
                    keys(colon([[0, 1], [0, 3]]),
                         edge_snapshot([[0, 1], [0, 3]]),
                         nil([[0]]),
                         cons([[0, 1], [2]]),
                         intern([[0, 1]]),
                         intern_snapshot([[0, 1]]),
                         predecessor([[0, 1], [0, 2]]),
                         def([[0]]), head([[0]]),
                         head_arg([[0, 1]]), body([[0, 1]]),
                         body_arg([[0, 1, 2]])),
                    compound_key(
                        edges([key("account", int, 0),
                               field(payload, text, 1),
                               key("revision", int, 2)]),
                        rows([key(0, "account"),
                              key(1, "revision")]),
                        options_preserved(true),
                        labels_are_nodes(true)),
                    history_v1(
                        specialization(copy,
                                       [edge(id, int, 0),
                                        edge(name, text, 1)]),
                        carrier(definition(2), head(2), body(1)),
                        compiler_output([7, "Ada"]),
                        runtime(relation(2, []), rule(copy),
                                dependency(positive), stratum(0))),
                    evaluator(temporary_rules(0), temporary_seeds(0),
                              temporary_lower_rows(0), temporary_requests(0)),
                    true, true),
    !.

test(generated_relations_are_callable_after_declarations_freeze) :-
    Path = 'v7/test/fixtures/4_generated_call.dl7',
    compile_dl7(Path, Rows1, Runtime1, Diagnostics1),
    compile_dl7(Path, Rows2, Runtime2, Diagnostics2),
    named_owner(Rows1, 'UserHistory', UserHistory),
    Runtime1 = checked_datalog(
                   _, datalog_program(Relations, Seeds, Rules), _, _),
    named_owner(Rows1, 'SourceCopy', SourceCopy),
    named_owner(Rows1, names, Names),
    memberchk(relation(ref(UserHistory), 2, []), Relations),
    memberchk(call(ref(UserHistory), [const(7), const("Ada")]), Seeds),
    memberchk(rule(call(_, [var(Id), var(Name)]),
                   [checked_goal(
                        positive,
                        call(ref(UserHistory), [var(Id), var(Name)]))]),
              Rules),
    memberchk(rule(call(ref(UserHistory), [var(HeadId), var(HeadName)]),
                   [checked_goal(
                        positive,
                        call(ref(SourceCopy),
                             [var(HeadId), var(HeadName)]))]),
              Rules),
    memberchk(rule(call(ref(Names), [var(PunnedName)]),
                   [checked_goal(
                        positive,
                        call(ref(UserHistory),
                             [var(omitted(_, 0)), var(PunnedName)]))]),
              Rules),
    residual_partial_terms(Runtime1, ResidualPartials),
    equality(Rows1, Rows2, RowsEqual),
    equality(Runtime1, Runtime2, RuntimeEqual),
    Observed = generated_callable(
                   diagnostics(Diagnostics1, Diagnostics2),
                   repeatable(RowsEqual, RuntimeEqual),
                   residual_partial_terms(ResidualPartials)),
    Observed == generated_callable(
                    diagnostics([], []),
                    repeatable(true, true),
                    residual_partial_terms(0)).

test(escaping_partial_bind_generates_a_callable_forwarding_relation) :-
    Path = 'v7/test/fixtures/5_curry.dl7',
    compile_dl7(Path, Rows1, Runtime1, Diagnostics1),
    compile_dl7(Path, Rows2, Runtime2, Diagnostics2),
    named_owner(Rows1, 'User', User),
    named_owner(Rows1, 'Order', Order),
    named_owner(Rows1, 'PairResult', PairResult),
    named_owner(Rows1, 'Pair', Pair),
    named_owner(Rows1, 'PairUser', PairUser),
    named_owner(Rows1, 'CallableFactory', CallableFactory),
    named_owner(Rows1, 'ReturnedPair', ReturnedPair),
    named_owner(Rows1, 'ReturnedResult', ReturnedResult),
    named_owner(Rows1, 'ReturnedDone', ReturnedDone),
    named_owner(Rows1, 'Region', Region),
    named_owner(Rows1, 'TripleResult', TripleResult),
    named_owner(Rows1, 'Triple', Triple),
    named_owner(Rows1, 'TripleUser', TripleUser),
    named_owner(Rows1, 'TripleUserOrder', TripleUserOrder),
    named_owner(Rows1, 'TripleDone', TripleDone),
    Runtime1 = checked_datalog(
                   _, datalog_program(Relations, Seeds, Rules), _, _),
    memberchk(relation(ref(PairUser), 2, _), Relations),
    memberchk(relation(ref(TripleUser), 3, _), Relations),
    memberchk(relation(ref(TripleUserOrder), 2, _), Relations),
    memberchk(call(ref(PairUser),
                   [ref(Order), ref(PairResult)]), Seeds),
    memberchk(call(ref(TripleUserOrder),
                   [ref(Region), ref(TripleResult)]), Seeds),
    memberchk(rule(call(ref(PairUser), ForwardArguments),
                   [checked_goal(
                        positive,
                        call(ref(Pair),
                             [ref(User) | ForwardArguments]))]),
              Rules),
    memberchk(rule(call(ref(TripleUser), TripleUserArguments),
                   [checked_goal(
                        positive,
                        call(ref(Triple),
                             [ref(User) | TripleUserArguments]))]),
              Rules),
    memberchk(rule(call(ref(TripleUserOrder), TripleUserOrderArguments),
                   [checked_goal(
                        positive,
                        call(ref(TripleUser),
                             [ref(Order) | TripleUserOrderArguments]))]),
              Rules),
    residual_partial_terms(Runtime1, ResidualPartials),
    equality(Rows1, Rows2, RowsRepeat),
    equality(Runtime1, Runtime2, RuntimeRepeat),
    equality(ReturnedPair, Pair, ReturnedCallable),
    equality(ReturnedResult, PairUser, ReturnedPartial),
    equality(ReturnedDone, PairResult, ReturnedValue),
    equality(TripleDone, TripleResult, TripleReturn),
    Observed = returned_callable(
                   diagnostics(Diagnostics1, Diagnostics2),
                   arity(2), capture(left, User),
                   returned_callable(
                       factory(CallableFactory),
                       callable(ReturnedCallable),
                       partial(ReturnedPartial), value(ReturnedValue)),
                   chained(arity(3), arity(2), return(TripleReturn)),
                   repeat(rows(RowsRepeat), runtime(RuntimeRepeat)),
                   residual_partial_terms(ResidualPartials)),
    Observed == returned_callable(
                    diagnostics([], []),
                    arity(2), capture(left, User),
                    returned_callable(
                        factory(CallableFactory),
                        callable(true), partial(true), value(true)),
                    chained(arity(3), arity(2), return(true)),
                    repeat(rows(true), runtime(true)),
                    residual_partial_terms(0)).

test(prolog_and_dl7_emitters_share_the_closed_compiler_view) :-
    Path = 'v7/test/fixtures/5_curry.dl7',
    compile_dl7(Path, CompilerRows, RuntimeProgram, Diagnostics),
    named_owner(CompilerRows, 'CurryEmitter', CurryEmitter),
    named_owner(CompilerRows, curry_artifact, CurryArtifact),
    Compiled = compiled_unit([], RuntimeProgram, CompilerRows),
    emit_compiled(
        dl7(CurryEmitter), Compiled,
        Dl7Artifact, Dl7Diagnostics),
    emit_compiled(
        prolog(plunit_dl7_entrypoints:compiler_row_count), Compiled,
        PrologArtifact, PrologDiagnostics),
    emit_compiled(
        monomorphic_datalog, Compiled,
        DatalogArtifact, DatalogDiagnostics),
    findall(
        [ref(Source), ref(Specialization)],
        member(call(ref(CurryArtifact),
                    [ref(Source), ref(Specialization)]),
               CompilerRows),
        ExpectedRows0),
    sort(ExpectedRows0, ExpectedRows),
    length(CompilerRows, RowCount),
    Observed = emitter_protocol(
                   compile(Diagnostics),
                   dl7(Dl7Diagnostics, Dl7Artifact),
                   prolog(PrologDiagnostics, PrologArtifact),
                   datalog(DatalogDiagnostics, DatalogArtifact)),
    Observed == emitter_protocol(
                    compile([]),
                    dl7([], artifacts([
                        artifact("specializations", CurryArtifact,
                                 ExpectedRows)
                    ])),
                    prolog([], compiler_rows(RowCount)),
                    datalog([], artifact(monomorphic_datalog,
                                         RuntimeProgram))).

test(userland_type_algebra_proves_contracts_and_constructs_products) :-
    compile_dl7('v7/test/fixtures/3_type_algebra.dl7',
                Rows, Runtime, Diagnostics),
    type_algebra_snapshot(Rows, Runtime, Snapshot),
    Observed = type_algebra_result(Diagnostics, Snapshot),
    Observed == type_algebra_result(
                    [],
                    type_algebra(
                        conformance(
                            proof(canonical, node),
                            missing(name, text, 0),
                            all([identified, named])),
                        relation_edge(hash, relation_type),
                        generic(user_box, rejected_missing_name),
                        intersection(
                            [edge(0, id, int),
                             edge(1, name, text),
                             edge(2, updated, int)],
                            extend_same_identity,
                            conflict(name),
                            rejected_conflict),
                        impl(valid_user_hash,
                             invalid(hash, relation_type, 0)),
                        history(contract(user_contract),
                                runtime_row([7, "Ada", 1])))),
    !.

test(final_closure_rejects_declared_functional_key_conflicts) :-
    Relation = ref(kernel(':')),
    Relations = [relation(Relation, 4, [[0, 1], [0, 3]])],
    Rows = [ call(Relation,
                  [ref(owner), const(name), ref(first), const(0)]),
             call(Relation,
                  [ref(owner), const(name), ref(second), const(1)])
           ],
    validate_functional_rows(Relations, Rows, Diagnostics),
    Diagnostics ==
        [diagnostic(evaluate, none,
                    functional_key_conflict(
                        Relation, [0, 1], [ref(owner), const(name)],
                        call(Relation,
                             [ref(owner), const(name), ref(first), const(0)]),
                        call(Relation,
                             [ref(owner), const(name), ref(second), const(1)]))
                   )].

test(generated_program_rejects_identity_collisions_and_orphan_arguments) :-
    Existing = ref(existing),
    BaseRelations = [relation(Existing, 1, [])],
    assemble_generated_program(
        [call(ref(kernel(def)), [Existing, const(1)])],
        BaseRelations, CollisionRelations, CollisionRules,
        CollisionDiagnostics),
    assemble_generated_program(
        [call(ref(kernel(body_arg)),
              [ ref(orphan), const(0), const(0),
                const("variable"), const(value)
              ])],
        BaseRelations, OrphanRelations, OrphanRules, OrphanDiagnostics),
    Observed = generated_program_rejections(
                   collision(CollisionRelations, CollisionRules,
                             CollisionDiagnostics),
                   orphan(OrphanRelations, OrphanRules,
                          OrphanDiagnostics)),
    Observed == generated_program_rejections(
                    collision(
                        [], [],
                        [diagnostic(
                             assemble, none,
                             generated_relation_already_declared(
                                 Existing))]),
                    orphan(
                        [], [],
                        [ diagnostic(
                              assemble, none,
                              orphan_generated_rule_fragment(orphan)),
                          diagnostic(
                              assemble, none,
                              orphan_generated_body_arguments(orphan, 0))
                        ])).

test(authored_order_kernel_modes_are_checked_left_to_right) :-
    Construct = checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [var(element), const([]), var(arguments)])),
    Intern = checked_goal(
                 positive,
                 call(ref(kernel(intern)),
                      [ref(option), var(arguments), var(result)])),
    Deconstruct = checked_goal(
                      positive,
                      call(ref(kernel(cons)),
                           [var(head), var(tail), var(list)])),
    check_goal_sequence([Construct, Intern], [element], Bound,
                        AcceptedDiagnostics),
    check_goal_sequence([Deconstruct], [list], DeconstructedBound,
                        DeconstructedDiagnostics),
    check_goal_sequence([Construct], [], _, RejectedDiagnostics),
    sort(Bound, SortedBound),
    sort(DeconstructedBound, SortedDeconstructedBound),
    Observed = authored_order(
                   accepted(SortedBound, AcceptedDiagnostics),
                   deconstructed(SortedDeconstructedBound,
                                   DeconstructedDiagnostics),
                   rejected(RejectedDiagnostics)),
    Observed == authored_order(
                    accepted([arguments, element, result], []),
                    deconstructed([head, list, tail], []),
                    rejected(
                        [diagnostic(
                             check, none,
                             underconstrained_kernel_goal(
                                 cons, [[2], [0, 1]]))])).

test(stratification_is_pure_deterministic_and_strict_cycle_checked) :-
    Source = ref(source),
    Left = ref(left),
    Right = ref(right),
    AcyclicRules =
        [rule(call(Left, [var(value)]),
              [checked_goal(negative,
                            call(Source, [var(value)]))])],
    CycleRules =
        [ rule(call(Left, [var(value)]),
               [checked_goal(negative,
                             call(Right, [var(value)]))]),
          rule(call(Right, [var(value)]),
               [checked_goal(positive,
                             call(Left, [var(value)]))])
        ],
    stratify_rules(AcyclicRules, AcyclicStrata, AcyclicDiagnostics),
    stratify_rules(CycleRules, CycleStrata, CycleDiagnostics),
    evaluate(CycleRules, [], CycleClosure, EvaluationDiagnostics),
    evaluator_snapshot(EvaluatorSnapshot),
    Observed = stratification(
                   acyclic(AcyclicStrata, AcyclicDiagnostics),
                   strict_cycle(CycleStrata, CycleDiagnostics,
                                evaluation(CycleClosure,
                                           EvaluationDiagnostics,
                                           EvaluatorSnapshot))),
    Observed == stratification(
                    acyclic([stratum(Left, 1)], []),
                    strict_cycle(
                        [],
                        [diagnostic(
                             stratify, none,
                             strict_dependency_cycle([Left, Right]))],
                        evaluation(
                            [],
                            [diagnostic(
                                 stratify, none,
                                 strict_dependency_cycle([Left, Right]))],
                            evaluator(temporary_rules(0), temporary_seeds(0),
                                      temporary_lower_rows(0),
                                      temporary_requests(0))))).

test(cons_constructs_deconstructs_and_stops_at_the_empty_tail) :-
    Rules =
        [ rule(call(ref(singleton), [var(list)]),
               [checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [const(one), const([]), var(list)]))]),
          rule(call(ref(pair), [var(list)]),
               [ checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [const(two), const([]), var(tail)])),
                 checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [const(one), var(tail), var(list)]))
               ]),
          rule(call(ref(suffix), [var(list)]),
               [checked_goal(positive,
                             call(ref(source), [var(list)]))]),
          rule(call(ref(suffix), [var(tail)]),
               [ checked_goal(positive,
                              call(ref(suffix), [var(list)])),
                 checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [var(head), var(tail), var(list)]))
               ]),
          rule(call(ref(item), [var(head)]),
               [ checked_goal(positive,
                              call(ref(suffix), [var(list)])),
                 checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [var(head), var(tail), var(list)]))
               ]),
          rule(call(ref(empty_witness), []),
               [checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [var(head), var(tail), const([])]))]),
          rule(call(ref(improper_witness), []),
               [checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [var(head), var(tail), const([one | improper])]))])
        ],
    Seeds = [call(ref(source),
                  [const([const(one), const(two), const(three)])])],
    evaluate(Rules, Seeds, Closure, EvaluationDiagnostics),
    evaluator_snapshot(EvaluatorSnapshot),
    findall(Item,
            member(call(ref(item), [const(Item)]), Closure),
            Items),
    findall(List,
            member(call(ref(suffix), [const(List)]), Closure),
            Suffixes0),
    sort(Suffixes0, Suffixes),
    findall(List,
            member(call(ref(singleton), [const(List)]), Closure),
            Singletons),
    findall(List,
            member(call(ref(pair), [const(List)]), Closure),
            Pairs),
    witness_presence(Closure, empty_witness, EmptyWitness),
    witness_presence(Closure, improper_witness, ImproperWitness),
    underconstrained_cons_diagnostic(SourceDiagnostics),
    Observed = cons_result(
                   evaluation(EvaluationDiagnostics, EvaluatorSnapshot),
                   traversal(Items, Suffixes),
                   construction(Singletons, Pairs),
                   absent(EmptyWitness, ImproperWitness),
                   source_check(SourceDiagnostics)),
    Observed == cons_result(
                    evaluation(
                        [],
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0),
                                  temporary_requests(0))),
                    traversal(
                        [one, three, two],
                        [ [],
                          [const(one), const(two), const(three)],
                          [const(three)],
                          [const(two), const(three)]
                        ]),
                    construction([[const(one)]],
                                 [[const(one), const(two)]]),
                    absent(false, false),
                    source_check(
                        [diagnostic(
                             check, reader_node(cons_mode_source, 26),
                             underconstrained_kernel_goal(
                                 cons, [[2], [0, 1]]))])),
    !.

test(checked_edge_indices_expose_adjacent_and_strict_order) :-
    Text = "(: Empty (*))\n(: Singleton (* (: only int)))\n(: Triple (* (: first int) (: second int) (: third int)))\n(: before (* (: owner type) (: earlier int) (: later int)))\n(<- (before ?Owner ?Earlier ?Later)\n    (predecessor ?Owner ?Earlier ?Later))\n(<- (before ?Owner ?Earlier ?Later)\n    (predecessor ?Owner ?Earlier ?Middle)\n    (before ?Owner ?Middle ?Later))\n",
    dl7_text_unit(ordered_index, ordered_index_source, Text, Unit,
                  ReaderDiagnostics),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 CompilerDiagnostics),
    named_owner(CompilerRows, 'Empty', Empty),
    named_owner(CompilerRows, 'Singleton', Singleton),
    named_owner(CompilerRows, 'Triple', Triple),
    named_owner(CompilerRows, before, Before),
    relation_pairs(CompilerRows, ref(kernel(predecessor)), Empty,
                   EmptyPairs),
    relation_pairs(CompilerRows, ref(kernel(predecessor)), Singleton,
                   SingletonPairs),
    relation_pairs(CompilerRows, ref(kernel(predecessor)), Triple,
                   AdjacentPairs),
    relation_pairs(CompilerRows, ref(Before), Triple, StrictPairs),
    runtime_predecessor_snapshot(RuntimeProgram, Triple, RuntimeSnapshot),
    Observed = ordered_index_result(
                   diagnostics(ReaderDiagnostics, CompilerDiagnostics),
                   empty(EmptyPairs),
                   singleton(SingletonPairs),
                   triple(adjacent(AdjacentPairs), strict(StrictPairs)),
                   RuntimeSnapshot),
    Observed == ordered_index_result(
                    diagnostics([], []),
                    empty([]),
                    singleton([]),
                    triple(
                        adjacent([0-1, 1-2]),
                        strict([0-1, 0-2, 1-2])),
                    runtime(
                        keys([[0, 1], [0, 2]]),
                        ordered_seeds([0-1, 1-2]))),
    !.

test(prefix_negation_is_safe_stratified_and_cleanup_scoped) :-
    anti_join_receipt(AntiJoin),
    unsafe_negation_receipt(Unsafe),
    negative_cycle_receipt(Cycle),
    negative_kernel_receipt(Kernel),
    evaluator_exception_receipt(Exception),
    Observed = negation_result(AntiJoin, Unsafe, Cycle, Kernel, Exception),
    Observed == negation_result(
                    anti_join(
                        values(["a"]),
                        body([positive(candidate), negative(blocked)]),
                        dependencies(positive, negative),
                        strata(candidate(0), blocked(0), allowed(1)),
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0),
                                  temporary_requests(0))),
                    unsafe(
                        goal_node(32),
                        variable_node(27)),
                    cycle(
                        goal_node(38),
                        relations([left, right])),
                    kernel(
                        goal_node(32),
                        negative_constructive_kernel_goal(cons)),
                    exception(
                        caught,
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0),
                                  temporary_requests(0)))),
    !.

test(count_groups_completed_lower_proofs_and_rejects_bad_placement) :-
    grouped_count_receipt(Grouped),
    nested_count_receipt(NestedCount),
    multiple_count_receipt(Multiple),
    misplaced_count_receipt(Misplaced),
    nested_head_receipt(Nested),
    aggregate_cycle_receipt(Cycle),
    Observed = count_result(
                   Grouped, NestedCount, Multiple, Misplaced, Nested, Cycle),
    Observed == count_result(
                    grouped(
                        rows(["east"-2, "west"-1]),
                        checked_head(
                            [plain(region), aggregate(count, region)]),
                        dependency(positive),
                        strata(source(0), count(1)),
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0),
                                  temporary_requests(0))),
                    nested_count(
                        rows([1]),
                        body([positive(source), positive('Wrap')])),
                    multiple(
                        node(27),
                        multiple_count_aggregates(region_count)),
                    misplaced(
                        node(26),
                        aggregate_outside_rule_head),
                    nested(
                        node(25),
                        undeclared_relation(wrapper)),
                    cycle(
                        node(13),
                        aggregate_dependency_cycle([loop]))),
    !.

nested_count_receipt(nested_count(rows(Counts), body(Body))) :-
    Text = "(: User (*))\n(: source (* (: value type)))\n(: Wrap (* (: source type) (: return type)))\n(: wrapped_count (* (: total int)))\n(source User)\n(Wrap User User)\n(<- (wrapped_count (count (Wrap ?Value)))\n    (source ?Value))\n",
    dl7_text_unit(nested_count, nested_count_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows), []),
    named_owner(CompilerRows, source, Source),
    named_owner(CompilerRows, 'Wrap', Wrap),
    named_owner(CompilerRows, wrapped_count, WrappedCount),
    findall(Count,
            member(call(ref(WrappedCount), [const(Count)]), CompilerRows),
            Counts),
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    memberchk(
        rule(call(ref(WrappedCount),
                  [aggregate(count, var(WrappedValue))]),
             [ checked_goal(
                   positive,
                   call(ref(Source), [var(SourceValue)])),
               checked_goal(
                   positive,
                   call(ref(Wrap),
                        [var(SourceValue), var(WrappedValue)]))
             ]),
        Rules),
    Body = [positive(source), positive('Wrap')].

grouped_count_receipt(Receipt) :-
    Text = "(: sale (* (: region text) (: item text)))\n(: region_count (* (: region text) (: total int)))\n(sale \"east\" \"one\")\n(sale \"east\" \"two\")\n(sale \"west\" \"three\")\n(<- (region_count ?Region (count ?Region))\n    (sale ?Region ?Item))\n",
    dl7_text_unit(grouped_count, grouped_count_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows), []),
    named_owner(CompilerRows, sale, Sale),
    named_owner(CompilerRows, region_count, RegionCount),
    findall(Region-Count,
            member(call(ref(RegionCount), [const(Region), const(Count)]),
                   CompilerRows),
            Rows),
    grouped_count_runtime_snapshot(RuntimeProgram, Sale, RegionCount,
                                   RuntimeSnapshot),
    evaluator_snapshot(EvaluatorSnapshot),
    RuntimeSnapshot = runtime(CheckedHead, Dependency, Strata),
    Receipt = grouped(rows(Rows), CheckedHead, Dependency, Strata,
                      EvaluatorSnapshot).

grouped_count_runtime_snapshot(
    checked_datalog(_, datalog_program(_, _, Rules), Depends, Strata),
    Sale, RegionCount,
    runtime(checked_head(
                [plain(region), aggregate(count, region)]),
            dependency(positive),
            strata(source(SaleLevel), count(CountLevel)))) :-
    memberchk(rule(
                  call(ref(RegionCount),
                       [var(Region), aggregate(count, var(Region))]),
                  [checked_goal(
                       positive,
                       call(ref(Sale), [var(Region), var(_)]))]),
              Rules),
    memberchk(depends(ref(RegionCount), ref(Sale), positive), Depends),
    memberchk(stratum(ref(Sale), SaleLevel), Strata),
    memberchk(stratum(ref(RegionCount), CountLevel), Strata).

multiple_count_receipt(multiple(node(NodeIndex), Reason)) :-
    Text = "(: source (* (: value text)))\n(: region_count (* (: first int) (: second int)))\n(source \"x\")\n(<- (region_count (count ?Value) (count ?Value))\n    (source ?Value))\n",
    dl7_text_unit(multiple_count, multiple_count_source, Text, Unit, []),
    compile_unit(Unit, [],
                 [diagnostic(lower,
                             reader_node(multiple_count_source, NodeIndex),
                             Reason)]).

misplaced_count_receipt(misplaced(node(NodeIndex), Reason)) :-
    Text = "(: source (* (: value text)))\n(: bad (* (: value text)))\n(source \"x\")\n(<- (bad ?Value)\n    (count ?Value)\n    (source ?Value))\n",
    dl7_text_unit(misplaced_count, misplaced_count_source, Text, Unit, []),
    compile_unit(Unit, [],
                 [diagnostic(lower,
                             reader_node(misplaced_count_source, NodeIndex),
                             Reason)]).

nested_head_receipt(nested(node(NodeIndex), Reason)) :-
    Text = "(: source (* (: value text)))\n(: bad (* (: value text)))\n(source \"x\")\n(<- (bad (wrapper ?Value))\n    (source ?Value))\n",
    dl7_text_unit(nested_head, nested_head_source, Text, Unit, []),
    compile_unit(Unit, [],
                 [diagnostic(lower,
                             reader_node(nested_head_source, NodeIndex),
                             Reason)]).

aggregate_cycle_receipt(
    cycle(node(NodeIndex), aggregate_dependency_cycle([loop]))) :-
    Text = "(: loop (* (: value text) (: total int)))\n(<- (loop ?Value (count ?Value))\n    (loop ?Value ?Total))\n",
    dl7_text_unit(aggregate_cycle, aggregate_cycle_source, Text, Unit, []),
    lower_datalog(Unit, Basement, Origins, []),
    Basement = basement_program(root_graph(_, PendingEdges), _),
    memberchk(pending_edge(_, loop, target(Loop), _), PendingEdges),
    check_datalog(
        Basement, Origins, [],
        [diagnostic(stratify,
                    reader_node(aggregate_cycle_source, NodeIndex),
                    aggregate_dependency_cycle([ref(Loop)]))]).

anti_join_receipt(Receipt) :-
    Text = "(: candidate (* (: value text)))\n(: blocked (* (: value text)))\n(: allowed (* (: value text)))\n(candidate \"a\")\n(candidate \"b\")\n(blocked \"b\")\n(<- (allowed ?Value)\n    (candidate ?Value)\n    (not (blocked ?Value)))\n",
    dl7_text_unit(anti_join, anti_join_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows), []),
    named_owner(CompilerRows, candidate, Candidate),
    named_owner(CompilerRows, blocked, Blocked),
    named_owner(CompilerRows, allowed, Allowed),
    findall(Value,
            member(call(ref(Allowed), [const(Value)]), CompilerRows),
            Values),
    anti_join_runtime_snapshot(RuntimeProgram, Candidate, Blocked, Allowed,
                               RuntimeSnapshot),
    evaluator_snapshot(EvaluatorSnapshot),
    RuntimeSnapshot = runtime(Body, Dependencies, Strata),
    Receipt = anti_join(values(Values), Body, Dependencies, Strata,
                        EvaluatorSnapshot).

anti_join_runtime_snapshot(
    checked_datalog(_, datalog_program(_, _, Rules), Depends, Strata),
    Candidate, Blocked, Allowed,
    runtime(body(BodySnapshot), dependencies(Positive, Negative),
            strata(candidate(CandidateLevel), blocked(BlockedLevel),
                    allowed(AllowedLevel)))) :-
    memberchk(rule(call(ref(Allowed), [_]), Body), Rules),
    maplist(label_checked_goal(Candidate, Blocked), Body, BodySnapshot),
    dependency_presence(Depends, ref(Allowed), ref(Candidate), positive,
                        Positive),
    dependency_presence(Depends, ref(Allowed), ref(Blocked), negative,
                        Negative),
    memberchk(stratum(ref(Candidate), CandidateLevel), Strata),
    memberchk(stratum(ref(Blocked), BlockedLevel), Strata),
    memberchk(stratum(ref(Allowed), AllowedLevel), Strata).

label_checked_goal(Candidate, _,
                   checked_goal(positive, call(ref(Candidate), [_])),
                   positive(candidate)).
label_checked_goal(_, Blocked,
                   checked_goal(negative, call(ref(Blocked), [_])),
                   negative(blocked)).

dependency_presence(Depends, Head, Body, Polarity, Polarity) :-
    memberchk(depends(Head, Body, Polarity), Depends).

unsafe_negation_receipt(unsafe(goal_node(GoalIndex),
                               variable_node(VariableIndex))) :-
    Text = "(: candidate (* (: value text)))\n(: blocked (* (: value text)))\n(: allowed (* (: value text)))\n(<- (allowed ?Value)\n    (not (blocked ?Value))\n    (candidate ?Value))\n",
    dl7_text_unit(unsafe_negation, unsafe_negation_source, Text, Unit, []),
    compile_unit(
        Unit, [],
        [diagnostic(
             check, reader_node(unsafe_negation_source, GoalIndex),
             unbound_negative_goal(
                 [variable(reader_node(unsafe_negation_source, VariableIndex),
                           'Value')]))]).

negative_cycle_receipt(cycle(goal_node(GoalIndex),
                             relations([left, right]))) :-
    Text = "(: domain (* (: value text)))\n(: left (* (: value text)))\n(: right (* (: value text)))\n(domain \"a\")\n(<- (left ?Value)\n    (domain ?Value)\n    (not (right ?Value)))\n(<- (right ?Value)\n    (domain ?Value)\n    (not (left ?Value)))\n",
    dl7_text_unit(negative_cycle, negative_cycle_source, Text, Unit, []),
    lower_datalog(Unit, Basement, Origins, []),
    Basement = basement_program(root_graph(_, PendingEdges), _),
    memberchk(pending_edge(_, left, target(Left), _), PendingEdges),
    memberchk(pending_edge(_, right, target(Right), _), PendingEdges),
    sort([ref(Left), ref(Right)], ExpectedRelations),
    check_datalog(
        Basement, Origins, [],
        [diagnostic(stratify,
                    reader_node(negative_cycle_source, GoalIndex),
                    strict_dependency_cycle(ExpectedRelations))]).

negative_kernel_receipt(
    kernel(goal_node(GoalIndex), negative_constructive_kernel_goal(cons))) :-
    Text = "(: source (* (: value any)))\n(: bad (* (: value any)))\n(source \"x\")\n(<- (bad ?Value)\n    (source ?Value)\n    (nil ?Empty)\n    (not (cons ?Value ?Empty ?List)))\n",
    dl7_text_unit(negative_kernel, negative_kernel_source, Text, Unit, []),
    compile_unit(
        Unit, [],
        [diagnostic(
             check, reader_node(negative_kernel_source, GoalIndex),
             negative_constructive_kernel_goal(cons))]).

evaluator_exception_receipt(exception(caught, EvaluatorSnapshot)) :-
    catch(evaluate([], [call(ref(seed), [_])], _, _),
          error(instantiation_error, _),
          true),
    evaluator_snapshot(EvaluatorSnapshot).

witness_presence(Closure, Relation, true) :-
    memberchk(call(ref(Relation), []), Closure),
    !.
witness_presence(_, _, false).

row_presence(Rows, Row, true) :-
    memberchk(Row, Rows),
    !.
row_presence(_, _, false).

underconstrained_cons_diagnostic(Diagnostics) :-
    Text = "(: Source (* (: value any)))\n(: Bad (* (: value any)))\n(Source \"ok\")\n(<- (Bad ?Value)\n    (cons ?Head ?Tail ?List)\n    (Source ?Value))\n",
    dl7_text_unit(cons_mode, cons_mode_source, Text, Unit, []),
    compile_unit(Unit, [], Diagnostics).

type_operator_snapshot(Rows, Snapshot) :-
    member(call(ref(kernel(':')),
                [ref(Module), const('User'), ref(User), const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Partial'), ref(PartialConstructor),
                 const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Option'), ref(OptionConstructor),
                 const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Pick'), ref(PickConstructor),
                 const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Exclude'), ref(ExcludeConstructor),
                 const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('UserPatch'), ref(Partial), const(_)]),
           Rows),
    member(call(ref(kernel(':')),
                [ ref(Module), const('MaybePatch'),
                  ref(application(OptionConstructor, [Partial])), const(_)
                ]),
           Rows),
    named_owner(Rows, selected_request, SelectedRequest),
    named_owner(Rows, excluded_request, ExcludedRequest),
    Partial = application(PartialConstructor, [User]),
    Picked = application(PickConstructor,
                         [Partial, [const(id), const(name)]]),
    Excluded = application(ExcludeConstructor,
                           [Picked, [const(id)]]),
    memberchk(call(ref(SelectedRequest), [ref(Picked)]), Rows),
    memberchk(call(ref(ExcludedRequest), [ref(Excluded)]), Rows),
    member(call(ref(kernel(node)), [ref(Partial)]), Rows),
    member(call(ref(kernel(product)), [ref(Partial)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Partial), const(id),
                 ref(application(OptionConstructor, [primitive(int)])),
                 const(0)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Partial), const(name),
                 ref(application(OptionConstructor, [primitive(text)])),
                 const(1)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Picked), const(id),
                 ref(application(OptionConstructor, [primitive(int)])),
                 const(0)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Picked), const(name),
                 ref(application(OptionConstructor, [primitive(text)])),
                 const(1)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Excluded), const(name),
                 ref(application(OptionConstructor, [primitive(text)])),
                 const(0)]), Rows),
    Snapshot = type_operators(
                   partial([mapped(id, option(int), 0),
                            mapped(name, option(text), 1)]),
                   pick([mapped(id, option(int), 0),
                         mapped(name, option(text), 1)]),
                   exclude([mapped(name, option(text), 0)])).

runtime_snapshot(
    checked_datalog(root_graph(Nodes, Edges),
                    datalog_program(Relations, Seeds, Rules),
                    Depends, Strata),
    runtime(counts(NodeCount, EdgeCount, RelationCount, SeedCount,
                   RuleCount, DependsCount, StrataCount),
            normalized(Normalized))) :-
    maplist(length,
            [Nodes, Edges, Relations, Seeds, Rules, Depends, Strata],
            [NodeCount, EdgeCount, RelationCount, SeedCount,
             RuleCount, DependsCount, StrataCount]),
    (   normalized_program(Relations, Seeds, Rules, Depends, Strata)
    ->  Normalized = true
    ;   Normalized = false
    ).

runtime_key_snapshot(
    checked_datalog(_, datalog_program(Relations, _, _), _, _),
    keys(colon(ColonKeys), edge_snapshot(SnapshotKeys),
         nil(NilKeys), cons(ConsKeys), intern(InternKeys),
         intern_snapshot(InternSnapshotKeys),
         predecessor(PredecessorKeys), def(DefKeys), head(HeadKeys),
         head_arg(HeadArgKeys), body(BodyKeys), body_arg(BodyArgKeys))) :-
    memberchk(relation(ref(kernel(':')), 4, ColonKeys), Relations),
    memberchk(relation(ref(kernel(edge_snapshot)), 4, SnapshotKeys),
              Relations),
    memberchk(relation(ref(kernel(nil)), 1, NilKeys), Relations),
    memberchk(relation(ref(kernel(cons)), 3, ConsKeys), Relations),
    memberchk(relation(ref(kernel(intern)), 3, InternKeys), Relations),
    memberchk(relation(ref(kernel(intern_snapshot)), 3,
                       InternSnapshotKeys), Relations),
    memberchk(relation(ref(kernel(predecessor)), 3, PredecessorKeys),
              Relations),
    memberchk(relation(ref(kernel(def)), 2, DefKeys), Relations),
    memberchk(relation(ref(kernel(head)), 2, HeadKeys), Relations),
    memberchk(relation(ref(kernel(head_arg)), 4, HeadArgKeys), Relations),
    memberchk(relation(ref(kernel(body)), 4, BodyKeys), Relations),
    memberchk(relation(ref(kernel(body_arg)), 5, BodyArgKeys), Relations).

compound_key_snapshot(
    Rows,
    compound_key(
        edges([key("account", int, 0),
               field(payload, text, 1),
               key("revision", int, 2)]),
        rows([key(0, "account"), key(1, "revision")]),
        options_preserved(OptionsPreserved),
        labels_are_nodes(LabelsAreNodes))) :-
    named_owner(Rows, 'Ledger', Ledger),
    named_owner(Rows, 'PrimaryKeyOptions', Options),
    named_owner(Rows, 'Key', KeyConstructor),
    named_owner(Rows, composite_key, CompositeKey),
    AccountLabel = application(KeyConstructor, ["account", Options]),
    RevisionLabel = application(KeyConstructor, ["revision", Options]),
    memberchk(call(ref(kernel(':')),
                   [ ref(Ledger), ref(AccountLabel), ref(primitive(int)),
                     const(0)
                   ]), Rows),
    memberchk(call(ref(kernel(':')),
                   [ ref(Ledger), const(payload), ref(primitive(text)),
                     const(1)
                   ]), Rows),
    memberchk(call(ref(kernel(':')),
                   [ ref(Ledger), ref(RevisionLabel), ref(primitive(int)),
                     const(2)
                   ]), Rows),
    memberchk(call(ref(CompositeKey),
                   [ref(Ledger), const(0), const("account"), ref(Options)]),
              Rows),
    memberchk(call(ref(CompositeKey),
                   [ref(Ledger), const(1), const("revision"), ref(Options)]),
              Rows),
    (   AccountLabel = application(KeyConstructor, ["account", Options]),
        RevisionLabel = application(KeyConstructor, ["revision", Options])
    ->  OptionsPreserved = true
    ;   OptionsPreserved = false
    ),
    (   memberchk(call(ref(kernel(node)), [ref(AccountLabel)]), Rows),
        memberchk(call(ref(kernel(node)), [ref(RevisionLabel)]), Rows)
    ->  LabelsAreNodes = true
    ;   LabelsAreNodes = false
    ).

history_v1_snapshot(
    Rows,
    checked_datalog(_, datalog_program(Relations, _, Rules),
                    Depends, Strata),
    history_v1(
        specialization(copy, Edges),
        carrier(definition(2), head(2), body(1)),
        compiler_output([7, "Ada"]),
        runtime(relation(2, []), rule(copy),
                dependency(positive), stratum(0)))) :-
    member(call(ref(kernel(':')),
                [ref(Module), const('HistoryV1'), ref(Constructor),
                 const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('User'), ref(User), const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('HistoryOptions'), ref(Options),
                 const(_)]), Rows),
    memberchk(call(ref(kernel(':')),
                   [ref(Options), const(mode), const("copy"), const(0)]),
              Rows),
    History = application(Constructor, [User, Options]),
    findall(edge(Name, Primitive, Index),
            member(call(ref(kernel(':')),
                        [ ref(History), const(Name),
                          ref(primitive(Primitive)), const(Index)
                        ]), Rows),
            Edges),
    memberchk(call(ref(kernel(def)), [ref(History), const(2)]), Rows),
    findall(Position,
            member(call(ref(kernel(head_arg)),
                        [ ref(History), const(Position),
                          const("variable"), const(_)
                        ]), Rows),
            HeadPositions),
    memberchk(call(ref(kernel(head)), [ref(History), ref(History)]), Rows),
    findall(Goal,
            member(call(ref(kernel(body)),
                        [ ref(History), const(Goal), const("positive"),
                          ref(User)
                        ]), Rows),
            BodyGoals),
    length(HeadPositions, 2),
    length(BodyGoals, 1),
    memberchk(call(ref(History), [const(7), const("Ada")]), Rows),
    memberchk(relation(ref(History), 2, []), Relations),
    GeneratedArguments =
        [var(generated(History, id)), var(generated(History, name))],
    memberchk(rule(call(ref(History), GeneratedArguments),
                   [checked_goal(
                        positive,
                        call(ref(User), GeneratedArguments))]),
              Rules),
    memberchk(depends(ref(History), ref(User), positive), Depends),
    memberchk(stratum(ref(History), 0), Strata).

type_algebra_snapshot(
    Rows,
    checked_datalog(_, datalog_program(Relations, _, _), _, _),
    type_algebra(
        conformance(proof(canonical, node), missing(name, text, 0),
                    all([identified, named])),
        relation_edge(hash, relation_type),
        generic(user_box, rejected_missing_name),
        intersection(IntersectionEdges, extend_same_identity,
                     conflict(name), rejected_conflict),
        impl(valid_user_hash, invalid(hash, relation_type, 0)),
        history(contract(user_contract), runtime_row([7, "Ada", 1])))) :-
    named_owner(Rows, 'User', User),
    named_owner(Rows, 'UserContract', UserContract),
    named_owner(Rows, 'Named', Named),
    named_owner(Rows, 'Identified', Identified),
    named_owner(Rows, 'MissingName', MissingName),
    named_owner(Rows, 'Conforms', Conforms),
    named_owner(Rows, 'UserConformance', UserConformance),
    UserConformance = application(Conforms, [User, UserContract]),
    memberchk(call(ref(Conforms),
                   [ref(User), ref(UserContract), ref(UserConformance)]),
              Rows),
    memberchk(call(ref(kernel(node)), [ref(UserConformance)]), Rows),
    named_owner(Rows, missing_contract_edge, MissingContractEdge),
    memberchk(call(ref(MissingContractEdge),
                   [ ref(MissingName), ref(Named), const(name),
                     ref(primitive(text)), const(0)
                   ]), Rows),
    named_owner(Rows, user_contracts, UserContracts),
    memberchk(call(ref(UserContracts), [const(Contracts)]), Rows),
    Contracts = [ref(Identified), ref(Named)],
    named_owner(Rows, 'ConformsAll', ConformsAll),
    named_owner(Rows, all_contract_proof, AllContractProof),
    memberchk(call(ref(AllContractProof), [ref(AllProof)]), Rows),
    AllProof = application(ConformsAll, [User, Contracts]),
    memberchk(call(ref(ConformsAll),
                   [ref(User), const(Contracts), ref(AllProof)]), Rows),
    named_owner(Rows, 'HashFunction', HashFunction),
    named_owner(Rows, 'Hashable', Hashable),
    memberchk(call(ref(kernel(':')),
                   [ ref(Hashable), const(hash), ref(HashFunction), const(0)
                   ]), Rows),
    named_owner(Rows, 'NamedBox', NamedBox),
    named_owner(Rows, 'UserBox', UserBox),
    UserBox = application(NamedBox, [User]),
    named_owner(Rows, box_attempt, BoxAttempt),
    \+ member(call(ref(BoxAttempt), _), Rows),
    named_owner(Rows, 'Intersect', Intersect),
    named_owner(Rows, 'IdentifiedNamed', IdentifiedNamed),
    named_owner(Rows, 'NamedTimestamped', NamedTimestamped),
    named_owner(Rows, 'UserView', UserView),
    named_owner(Rows, 'ExtendedUserView', ExtendedUserView),
    UserView = application(Intersect, [IdentifiedNamed, NamedTimestamped]),
    ExtendedUserView = UserView,
    ordered_primitive_edges(Rows, UserView, IntersectionEdges),
    named_owner(Rows, 'NumericName', NumericName),
    named_owner(Rows, intersection_conflict, IntersectionConflict),
    memberchk(call(ref(IntersectionConflict),
                   [ref(Named), ref(NumericName), const(name)]), Rows),
    \+ member(call(ref(Intersect),
                   [ref(Named), ref(NumericName), _]), Rows),
    named_owner(Rows, 'UserHashWitness', UserHashWitness),
    named_owner(Rows, 'BrokenHashWitness', BrokenHashWitness),
    named_owner(Rows, valid_impl, ValidImpl),
    memberchk(call(ref(ValidImpl),
                   [ref(Hashable), ref(User), ref(UserHashWitness)]), Rows),
    named_owner(Rows, invalid_impl_edge, InvalidImplEdge),
    memberchk(call(ref(InvalidImplEdge),
                   [ ref(Hashable), ref(User), ref(BrokenHashWitness),
                     const(hash), ref(HashFunction), const(0)
                   ]), Rows),
    named_owner(Rows, 'HistoryOptions', HistoryOptions),
    memberchk(call(ref(kernel(':')),
                   [ ref(HistoryOptions), const(contract), ref(UserContract),
                     const(1)
                   ]), Rows),
    named_owner(Rows, 'HistoryV1', HistoryV1),
    named_owner(Rows, 'UserHistory', UserHistory),
    UserHistory = application(HistoryV1, [User, HistoryOptions]),
    memberchk(call(ref(UserHistory),
                   [const(7), const("Ada"), const(1)]), Rows),
    memberchk(relation(ref(UserHistory), 3, []), Relations).

ordered_primitive_edges(Rows, Owner, Edges) :-
    findall(Index-edge(Index, Name, Primitive),
            member(call(ref(kernel(':')),
                        [ ref(Owner), const(Name),
                          ref(primitive(Primitive)), const(Index)
                        ]), Rows),
            IndexedEdges),
    keysort(IndexedEdges, Ordered),
    indexed_edge_values(Ordered, Edges).

indexed_edge_values([], []).
indexed_edge_values([_-Edge | Indexed], [Edge | Edges]) :-
    indexed_edge_values(Indexed, Edges).

named_owner(Rows, Name, Owner) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Owner), const(_)]),
           Rows),
    !.

compiler_row_count(compiler_view(_, CompilerRows, _),
                   compiler_rows(Count)) :-
    length(CompilerRows, Count).

relation_pairs(Rows, Relation, Owner, Pairs) :-
    findall(Earlier-Later,
            member(call(Relation,
                        [ref(Owner), const(Earlier), const(Later)]),
                   Rows),
            Pairs).

runtime_predecessor_snapshot(
    checked_datalog(_, datalog_program(Relations, Seeds, _), _, _),
    Owner,
    runtime(keys(Keys), ordered_seeds(Pairs))) :-
    memberchk(relation(ref(kernel(predecessor)), 3, Keys), Relations),
    relation_pairs(Seeds, ref(kernel(predecessor)), Owner, Pairs).

normalized_program(Relations, Seeds, Rules, Depends, Strata) :-
    maplist(normalized_relation, Relations),
    maplist(normalized_call, Seeds),
    maplist(normalized_rule, Rules),
    maplist(normalized_depends, Depends),
    maplist(normalized_stratum, Strata).

normalized_relation(relation(ref(_), Arity, KeySets)) :-
    integer(Arity),
    is_list(KeySets).
normalized_call(call(ref(_), Arguments)) :- is_list(Arguments).
normalized_rule(rule(Head, Body)) :-
    normalized_call(Head),
    maplist(normalized_goal, Body).
normalized_goal(checked_goal(Polarity, Call)) :-
    memberchk(Polarity, [positive, negative]),
    normalized_call(Call).
normalized_depends(depends(ref(_), ref(_), Polarity)) :-
    memberchk(Polarity, [positive, negative]).
normalized_stratum(stratum(ref(_), Level)) :-
    integer(Level),
    Level >= 0.

evaluator_snapshot(
    evaluator(temporary_rules(RuleFacts), temporary_seeds(SeedFacts),
              temporary_lower_rows(LowerFacts),
              temporary_requests(RequestFacts))) :-
    aggregate_all(count, dl7_evaluator:evaluation_rule(_, _), RuleFacts),
    aggregate_all(count, dl7_evaluator:evaluation_seed(_, _), SeedFacts),
    aggregate_all(count, dl7_evaluator:evaluation_lower(_, _), LowerFacts),
    aggregate_all(count, dl7_evaluator:evaluation_request(_, _),
                  RequestFacts).

origin_kinds(file(_),
             embedded(_, position(_, _, _)),
             true) :-
    !.
origin_kinds(_, _, false).

equality(Left, Right, true) :-
    Left == Right,
    !.
equality(_, _, false).

content_snapshot(Forms, SourceRows,
                 content(FormSnapshot, SourceSnapshot)) :-
    maplist(content_node, Forms, FormSnapshot),
    maplist(content_source, SourceRows, SourceSnapshot).

content_node(node(reader_node(_, Index), Payload),
             node(Index, Snapshot)) :-
    content_payload(Payload, Snapshot).

content_payload(atom(Name), atom(Name)).
content_payload(literal(Value), literal(Value)).
content_payload(variable(VariableId, Name),
                variable(SnapshotId, Name)) :-
    content_variable_id(VariableId, SnapshotId).
content_payload(form(Nodes), form(Snapshots)) :-
    maplist(content_node, Nodes, Snapshots).

content_variable_id(variable(reader_node(_, Index), Name),
                    variable(Index, Name)).

content_source(
    source(reader_node(_, Index), _, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn),
    source(Index, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn)).

driver_run(Status, Stdout, Stderr) :-
    process_create(
        path(swipl),
        [ '-q',
          '-s', 'v7/src/0_reader/5_cli_mainer.pl',
          '--', 'v7/test/fixtures/0_minimal.dl7'
        ],
        [ stdout(pipe(StdoutStream)),
          stderr(pipe(StderrStream)),
          process(Process)
        ]),
    read_string(StdoutStream, _, Stdout),
    close(StdoutStream),
    read_string(StderrStream, _, Stderr),
    close(StderrStream),
    process_wait(Process, Status),
    !.

:- end_tests(dl7_entrypoints).
