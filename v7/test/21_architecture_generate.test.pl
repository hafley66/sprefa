% SABOTAGE RECEIPTS: renaming a milestone component leaves the edge validation
% green but drifts the generated files; removing dangling-edge validation lets
% an edge point at an undeclared component; accepting an unknown status turns an
% undeclared denominator from `unknown` into a fabricated percentage. The
% generated-file parity case fails the moment 2_system.d2 or 2_progress.md is
% edited by hand.
:- begin_tests(dl7_architecture_generate).

:- use_module('../applications/architecture/1_generate.pl',
              [ load_graph/3,
                validate_graph/2,
                component_fraction/5,
                fraction_percent/3,
                render_d2/2,
                render_progress/2,
                check_text/3
              ]).

:- dynamic architecture_graph/1.
:- dynamic test_directory/1.

:- prolog_load_context(directory, TestDirectory),
   assertz(test_directory(TestDirectory)).

load_architecture :-
    v7_directory(V7),
    load_graph(V7, Graph, Diagnostics),
    Diagnostics == [],
    assertz(architecture_graph(Graph)).

v7_directory(V7) :-
    test_directory(TestDirectory),
    directory_file_path(TestDirectory, '..', Unresolved),
    absolute_file_name(Unresolved, V7,
                       [file_type(directory), access(read)]).

:- load_architecture.

test(exact_artifact_rows_for_representative_subset,
     []) :-
    architecture_graph(graph(Components, Edges, Milestones, AcceptanceItems,
                             Evidence, PhaseAxis, FlowSteps)),
    length(Components, 72),
    length(Edges, 75),
    length(Milestones, 72),
    length(AcceptanceItems, 7),
    length(Evidence, 179),
    length(PhaseAxis, 3),
    length(FlowSteps, 19),
    memberchk(component('dl7.kernel',
                        'DL7 compiler kernel, lowering, driver',
                        implemented, comptime, core),
              Components),
    memberchk(component('dl6.conformance',
                        'conformance corpus and battery of record',
                        reference, runtime, userland),
              Components),
    memberchk(edge('dl7.emit.sqlite:lowers_to:ivm.sqlite',
                   'dl7.emit.sqlite', lowers_to, 'ivm.sqlite'),
              Edges),
    memberchk(edge('rt.v7:depends_on:rt.executor',
                   'rt.v7', depends_on, 'rt.executor'),
              Edges),
    memberchk(milestone('m.dl7.kernel', 'dl7.kernel',
                        'v7/tasks/00_PROGRESS.md:24-34'),
              Milestones),
    memberchk(acceptance_item('ai.dl7.kernel.done', 'm.dl7.kernel',
                              completed, 10),
              AcceptanceItems),
    memberchk(acceptance_item('ai.dl6.conformance.done', 'm.dl6.conformance',
                              completed, 163),
              AcceptanceItems),
    memberchk(evidence('ev.dl7.kernel.1', 'dl7.kernel',
                       'v7/src/2_comptime/0_lowerer.pl:1,25'),
              Evidence),
    memberchk(phase_axis(macrotime, syntax_graph, expanded_syntax), PhaseAxis),
    memberchk(phase_axis(runtime, whole_language_facts_deltas,
                         maintained_query_effect_relations),
              PhaseAxis),
    memberchk(flow_step(runtime_tick, 7, 'ext.tsi', writes, 'ivm.sqlite'),
              FlowSteps),
    memberchk(flow_step(dl6_reference, 2, 'dl6.lower', emits, 'ProgramJson'),
              FlowSteps).

test(dangling_edge_rejected) :-
    Graph = graph(
                [component(a, 'A', implemented, comptime, core)],
                [edge('a:reads:ghost', a, reads, ghost)],
                [], [], [], [], []),
    validate_graph(Graph, Diagnostics),
    memberchk(dangling_edge_target('a:reads:ghost', ghost), Diagnostics).

test(dangling_edge_source_rejected) :-
    Graph = graph(
                [component(a, 'A', implemented, comptime, core)],
                [edge('ghost:reads:a', ghost, reads, a)],
                [], [], [], [], []),
    validate_graph(Graph, Diagnostics),
    memberchk(dangling_edge_source('ghost:reads:a', ghost), Diagnostics).

test(invalid_acceptance_status_rejected) :-
    Graph = graph(
                [component(a, 'A', implemented, comptime, core)],
                [],
                [milestone('m.a', a, 'src.md')],
                [acceptance_item('i.1', 'm.a', maybe, 1)],
                [], [], []),
    validate_graph(Graph, Diagnostics),
    memberchk(invalid_acceptance_status('i.1', maybe), Diagnostics).

test(invalid_component_state_rejected) :-
    Graph = graph(
                [component(a, 'A', sort_of_done, comptime, core)],
                [], [], [], [], [], []),
    validate_graph(Graph, Diagnostics),
    memberchk(invalid_component_state(a, sort_of_done), Diagnostics).

test(percentage_arithmetic) :-
    Milestones = [milestone('m.a', a, 'src.md')],
    AcceptanceItems = [ acceptance_item('i.1', 'm.a', completed, 3),
                        acceptance_item('i.2', 'm.a', pending, 2)
                      ],
    component_fraction(a, [], Milestones, AcceptanceItems, Fraction),
    Fraction == completed(3, 5),
    fraction_percent(Fraction, Percent, _),
    Percent =:= 60.0.

test(unknown_denominator) :-
    component_fraction(a, [], [milestone('m.a', a, 'src.md')], [],
                       NoItems),
    NoItems == unknown,
    component_fraction(b, [], [], [], NoMilestone),
    NoMilestone == unknown,
    component_fraction(c, [], [milestone('m.c', c, 'src.md')],
                       [acceptance_item('i.1', 'm.c', completed, 0)],
                       ZeroDenominator),
    ZeroDenominator == unknown.

test(deterministic_output, []) :-
    architecture_graph(Graph),
    render_d2(Graph, D2a),
    render_d2(Graph, D2b),
    D2a == D2b,
    render_progress(Graph, ProgressA),
    render_progress(Graph, ProgressB),
    ProgressA == ProgressB.

test(tracked_outputs_match_generation, []) :-
    architecture_graph(Graph),
    render_d2(Graph, D2),
    render_progress(Graph, Progress),
    v7_directory(V7),
    directory_file_path(V7, 'applications/architecture/2_system.d2', D2Path),
    directory_file_path(V7, 'applications/architecture/2_progress.md',
                        ProgressPath),
    check_text(D2Path, D2, ok),
    check_text(ProgressPath, Progress, ok).

test(generated_file_drift_detected, []) :-
    v7_directory(V7),
    directory_file_path(V7, 'applications/architecture/2_system.d2', D2Path),
    check_text(D2Path, "tampered\n", D2Drift),
    D2Drift == drift(generated_file_drift(D2Path)),
    check_text('/nonexistent/architecture.d2', "x", Missing),
    Missing = drift(generated_file_missing('/nonexistent/architecture.d2')).

:- end_tests(dl7_architecture_generate).
