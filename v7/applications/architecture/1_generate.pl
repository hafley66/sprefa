% Generate the deterministic D2 system map and progress report from the one
% authored DL7 architecture source. Run through `just architecture-generate`
% (writes), `just architecture-check` (drift gate, no write), or
% `just architecture-render` (D2 render into a temporary directory).
%
% Modes:
%   generate  compile 0_system.dl7, emit artifacts, write 2_system.d2 and
%             2_progress.md
%   check     the same derivation, then byte-compare against the tracked files
%             and exit non-zero on drift without rewriting them

:- module(architecture_generate,
          [ generate_main/0,
            load_graph/3,
            validate_graph/2,
            component_fraction/5,
            fraction_percent/3,
            render_d2/2,
            render_progress/2,
            check_text/3
          ]).

:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module(library(filesex),
              [ directory_file_path/3,
                make_directory_path/1 ]).

:- use_module('../../src/2_comptime/2_compiler',
              [compile_dl7_project/5]).
:- use_module('../../src/3_emit/1_artifact_emitter',
              [emit_compiled/4]).

%% generate_main/0 is the command-line entry point.
generate_main :-
    current_prolog_flag(argv, Argv),
    (   Argv = [ModeArg, V7Arg],
        atom_string(Mode, ModeArg),
        memberchk(Mode, [generate, check])
    ->  once(absolute_file_name(
                 V7Arg, V7,
                 [file_type(directory), access(read), file_errors(error)])),
        run_mode(Mode, V7)
    ;   format(user_error,
               "usage: 1_generate.pl -- <generate|check> <v7-directory>~n",
               []),
        halt(2)
    ).

run_mode(Mode, V7) :-
    load_graph(V7, Graph, LoadDiagnostics),
    (   LoadDiagnostics == []
    ->  validate_graph(Graph, Diagnostics),
        (   Diagnostics == []
        ->  render_d2(Graph, D2Text),
            render_progress(Graph, ProgressText),
            apply_mode(Mode, V7, D2Text, ProgressText)
        ;   report_diagnostics(Diagnostics),
            halt(1)
        )
    ;   report_diagnostics(LoadDiagnostics),
        halt(1)
    ).

apply_mode(generate, V7, D2Text, ProgressText) :-
    architecture_paths(V7, _SystemPath, D2Path, ProgressPath),
    write_text(D2Path, D2Text),
    write_text(ProgressPath, ProgressText),
    format("architecture: wrote ~w~n", [D2Path]),
    format("architecture: wrote ~w~n", [ProgressPath]).

apply_mode(check, V7, D2Text, ProgressText) :-
    architecture_paths(V7, _SystemPath, D2Path, ProgressPath),
    check_text(D2Path, D2Text, D2Status),
    check_text(ProgressPath, ProgressText, ProgressStatus),
    (   D2Status == ok, ProgressStatus == ok
    ->  format("architecture: no drift~n", [])
    ;   ( D2Status = drift(Reason1) -> format("architecture: ~w~n", [Reason1]) ; true ),
        ( ProgressStatus = drift(Reason2) -> format("architecture: ~w~n", [Reason2]) ; true ),
        halt(1)
    ).

architecture_paths(V7, SystemPath, D2Path, ProgressPath) :-
    directory_file_path(V7, 'applications/architecture/0_system.dl7', SystemPath),
    directory_file_path(V7, 'applications/architecture/2_system.d2', D2Path),
    directory_file_path(V7, 'applications/architecture/2_progress.md', ProgressPath).

report_diagnostics(Diagnostics) :-
    forall(member(Diagnostic, Diagnostics),
           format(user_error, "architecture diagnostic ~q~n", [Diagnostic])).

write_text(Path, Text) :-
    file_directory_name(Path, Directory),
    make_directory_path(Directory),
    setup_call_cleanup(
        open(Path, write, Stream, [encoding(utf8)]),
        format(Stream, "~s", [Text]),
        close(Stream)).

check_text(Path, Expected, Status) :-
    (   exists_file(Path)
    ->  read_file_to_string(Path, Actual, [encoding(utf8)]),
        (   Actual == Expected
        ->  Status = ok
        ;   Status = drift(generated_file_drift(Path))
        )
    ;   Status = drift(generated_file_missing(Path))
    ).

%% load_graph(+V7, -Graph, -Diagnostics) is det.
%
% Compile the one authored source, emit the architecture emitter, and
% canonicalize every artifact into a graph term whose rows are sorted by
% stable id.
load_graph(V7, Graph, Diagnostics) :-
    architecture_paths(V7, SystemPath, _D2Path, _ProgressPath),
    compile_dl7_project(V7, [SystemPath], Rows, RuntimeProgram, CompileDiagnostics),
    (   CompileDiagnostics == []
    ->  (   architecture_emitter(Rows, Emitter)
        ->  emit_compiled(
                dl7(Emitter),
                compiled_unit([], RuntimeProgram, Rows),
                Artifact, EmitDiagnostics),
            (   EmitDiagnostics == [], Artifact = artifacts(Artifacts)
            ->  graph_from_artifacts(Artifacts, Graph),
                Diagnostics = []
            ;   Diagnostics = EmitDiagnostics
            )
        ;   Graph = graph([], [], [], [], [], [], []),
            Diagnostics = [architecture_emitter_missing]
        )
    ;   Graph = graph([], [], [], [], [], [], []),
        Diagnostics = CompileDiagnostics
    ).

architecture_emitter(Rows, Emitter) :-
    member(call(ref(kernel(':')),
                [_, const('ArchEmitter'), ref(Emitter), _]),
           Rows),
    !.

%% graph_from_artifacts(+Artifacts, -Graph) is det.
graph_from_artifacts(Artifacts,
                     graph(Components, Edges, Milestones,
                           AcceptanceItems, Evidence, PhaseAxis, FlowSteps)) :-
    artifact_rows(Artifacts, "components", ComponentRows),
    artifact_rows(Artifacts, "edges", EdgeRows),
    artifact_rows(Artifacts, "milestones", MilestoneRows),
    artifact_rows(Artifacts, "acceptance_items", AcceptanceRows),
    artifact_rows(Artifacts, "evidence", EvidenceRows),
    artifact_rows(Artifacts, "phase_axis", AxisRows),
    artifact_rows(Artifacts, "flow_steps", FlowRows),

    findall(component(Id, Label, State, Binding, Plane),
            ( member(Row, ComponentRows),
              row_values(Row, [Id, Label, State, Binding, Plane]) ),
            Components0),
    sort_by_arg1(Components0, Components),

    findall(edge(Id, Src, Relation, Dst),
            ( member(Row, EdgeRows),
              row_values(Row, [Id, Src, Relation, Dst]) ),
            Edges0),
    sort_by_arg1(Edges0, Edges),

    findall(milestone(Id, Component, Src),
            ( member(Row, MilestoneRows),
              row_values(Row, [Id, Component, Src]) ),
            Milestones0),
    sort_by_arg1(Milestones0, Milestones),

    findall(acceptance_item(Id, Milestone, Status, Count),
            ( member(Row, AcceptanceRows),
              row_values(Row, [Id, Milestone, Status, Count]) ),
            Acceptance0),
    sort_by_arg1(Acceptance0, AcceptanceItems),

    findall(evidence(Id, Component, Path),
            ( member(Row, EvidenceRows),
              row_values(Row, [Id, Component, Path]) ),
            Evidence0),
    sort_by_arg1(Evidence0, Evidence),

    findall(phase_axis(Binding, Consumes, Produces),
            ( member(Row, AxisRows),
              row_values(Row, [Binding, Consumes, Produces]) ),
            PhaseAxis0),
    sort_by_arg1(PhaseAxis0, PhaseAxis),

    findall(flow_step(Flow, Step, Src, Relation, Dst),
            ( member(Row, FlowRows),
              row_values(Row, [Flow, Step, Src, Relation, Dst]) ),
            Flow0),
    predsort(by_flow_step, Flow0, FlowSteps).

artifact_rows(Artifacts, Name, Rows) :-
    memberchk(artifact(Name, _, Rows), Artifacts).

row_values(Row, Values) :-
    maplist(unconst, Row, Values).

unconst(const(Value), Atom) :- !, value_atom(Value, Atom).
unconst(Value, Atom) :- value_atom(Value, Atom).

value_atom(Value, Value) :- atom(Value), !.
value_atom(Value, Atom) :- string(Value), !, atom_string(Atom, Value).
value_atom(Value, Value).

%% validate_graph(+Graph, -Diagnostics) is det.
validate_graph(
    graph(Components, Edges, Milestones, AcceptanceItems, Evidence,
          PhaseAxis, FlowSteps),
    Diagnostics) :-
    component_ids(Components, IdSet),
    component_diagnostics(Components, ComponentDiagnostics),
    edge_diagnostics(Edges, IdSet, EdgeDiagnostics),
    milestone_diagnostics(Milestones, IdSet, MilestoneDiagnostics),
    milestone_ids(Milestones, MilestoneSet),
    acceptance_diagnostics(AcceptanceItems, MilestoneSet, AcceptanceDiagnostics),
    evidence_diagnostics(Evidence, IdSet, EvidenceDiagnostics),
    axis_diagnostics(PhaseAxis, AxisDiagnostics),
    flow_diagnostics(FlowSteps, FlowDiagnostics),
    append([ ComponentDiagnostics, EdgeDiagnostics, MilestoneDiagnostics,
             AcceptanceDiagnostics, EvidenceDiagnostics, AxisDiagnostics,
             FlowDiagnostics
           ], Diagnostics0),
    sort(Diagnostics0, Diagnostics).

component_ids(Components, IdSet) :-
    findall(Id, member(component(Id, _, _, _, _), Components), Ids),
    sort(Ids, IdSet).

component_diagnostics(Components, Diagnostics) :-
    findall(Diagnostic,
            ( member(component(Id, _, State, Binding, Plane), Components),
              component_diagnostic(Id, State, Binding, Plane, Diagnostic) ),
            Diagnostics0),
    findall(Diagnostic,
            duplicate_component_id(Components, Id, Diagnostic),
            Duplicates),
    append(Diagnostics0, Duplicates, Diagnostics).

component_diagnostic(Id, State, Binding, Plane, Diagnostic) :-
    (   \+ valid_state(State)
    ->  Diagnostic = invalid_component_state(Id, State)
    ;   \+ valid_binding(Binding)
    ->  Diagnostic = invalid_component_binding(Id, Binding)
    ;   \+ valid_plane(Plane)
    ->  Diagnostic = invalid_component_plane(Id, Plane)
    ;   fail
    ).

duplicate_component_id(Components, Id, Diagnostic) :-
    findall(Id, member(component(Id, _, _, _, _), Components), Ids),
    msort(Ids, Sorted),
    append(_, [Id, Id | _], Sorted),
    Diagnostic = duplicate_component_id(Id).

edge_diagnostics(Edges, IdSet, Diagnostics) :-
    findall(Diagnostic,
            ( member(edge(Id, Src, Relation, Dst), Edges),
              edge_diagnostic(Id, Src, Relation, Dst, IdSet, Diagnostic) ),
            Diagnostics0),
    findall(duplicate_edge_id(Id), duplicate_edge_id(Edges, Id), Duplicates),
    append(Diagnostics0, Duplicates, Diagnostics).

edge_diagnostic(Id, Src, Relation, Dst, IdSet, Diagnostic) :-
    (   \+ valid_relation(Relation)
    ->  Diagnostic = invalid_edge_relation(Id, Relation)
    ;   \+ memberchk(Src, IdSet)
    ->  Diagnostic = dangling_edge_source(Id, Src)
    ;   Dst \== none, \+ memberchk(Dst, IdSet)
    ->  Diagnostic = dangling_edge_target(Id, Dst)
    ;   fail
    ).

duplicate_edge_id(Edges, Id) :-
    findall(Id, member(edge(Id, _, _, _), Edges), Ids),
    msort(Ids, Sorted),
    append(_, [Id, Id | _], Sorted).

milestone_diagnostics(Milestones, IdSet, Diagnostics) :-
    findall(milestone_unknown_component(Component),
            ( member(milestone(_, Component, _), Milestones),
              \+ memberchk(Component, IdSet) ),
            Diagnostics0),
    findall(duplicate_milestone_id(Id), duplicate_milestone_id(Milestones, Id),
            Duplicates),
    append(Diagnostics0, Duplicates, Diagnostics).

duplicate_milestone_id(Milestones, Id) :-
    findall(Id, member(milestone(Id, _, _), Milestones), Ids),
    msort(Ids, Sorted),
    append(_, [Id, Id | _], Sorted).

milestone_ids(Milestones, MilestoneSet) :-
    findall(Id, member(milestone(Id, _, _), Milestones), Ids),
    sort(Ids, MilestoneSet).

acceptance_diagnostics(AcceptanceItems, MilestoneSet, Diagnostics) :-
    findall(Diagnostic,
            ( member(acceptance_item(Id, Milestone, Status, Count),
                     AcceptanceItems),
              acceptance_diagnostic(Id, Milestone, Status, Count,
                                    MilestoneSet, Diagnostic) ),
            Diagnostics0),
    findall(duplicate_acceptance_item_id(Id),
            duplicate_acceptance_item_id(AcceptanceItems, Id),
            Duplicates),
    append(Diagnostics0, Duplicates, Diagnostics).

acceptance_diagnostic(Id, Milestone, Status, Count, MilestoneSet, Diagnostic) :-
    (   \+ valid_acceptance_status(Status)
    ->  Diagnostic = invalid_acceptance_status(Id, Status)
    ;   \+ ( integer(Count), Count >= 0 )
    ->  Diagnostic = invalid_acceptance_count(Id, Count)
    ;   \+ memberchk(Milestone, MilestoneSet)
    ->  Diagnostic = acceptance_unknown_milestone(Id, Milestone)
    ;   fail
    ).

duplicate_acceptance_item_id(AcceptanceItems, Id) :-
    findall(Id, member(acceptance_item(Id, _, _, _), AcceptanceItems), Ids),
    msort(Ids, Sorted),
    append(_, [Id, Id | _], Sorted).

evidence_diagnostics(Evidence, IdSet, Diagnostics) :-
    findall(evidence_unknown_component(Id, Component),
            ( member(evidence(Id, Component, _Path), Evidence),
              \+ memberchk(Component, IdSet) ),
            Diagnostics0),
    findall(duplicate_evidence_id(Id), duplicate_evidence_id(Evidence, Id),
            Duplicates),
    append(Diagnostics0, Duplicates, Diagnostics).

duplicate_evidence_id(Evidence, Id) :-
    findall(Id, member(evidence(Id, _, _), Evidence), Ids),
    msort(Ids, Sorted),
    append(_, [Id, Id | _], Sorted).

axis_diagnostics(PhaseAxis, Diagnostics) :-
    findall(invalid_axis_binding(Binding),
            ( member(phase_axis(Binding, _, _), PhaseAxis),
              \+ valid_binding(Binding) ),
            Diagnostics).

flow_diagnostics(FlowSteps, Diagnostics) :-
    findall(invalid_flow_relation(Flow, Step, Relation),
            ( member(flow_step(Flow, Step, _, Relation, _), FlowSteps),
              \+ valid_relation(Relation) ),
            Diagnostics).

valid_state(State) :-
    memberchk(State, [implemented, partial, planned, absent, reference]).
valid_binding(Binding) :-
    memberchk(Binding, [macrotime, comptime, runtime, shared]).
valid_plane(Plane) :-
    memberchk(Plane, [core, userland, 'host-runtime']).
valid_relation(Relation) :-
    memberchk(Relation,
              [depends_on, emits, lowers_to, hosts, reads, writes, watches,
               reloads, effects, later_target]).
valid_acceptance_status(Status) :-
    memberchk(Status, [completed, pending, failed, blocked]).

%% component_fraction(+Component, +Components, +Milestones, +AcceptanceItems,
%%                    -Fraction) is det.
%
% Fraction is completed(Completed, Total) when at least one acceptance item
% is declared for the component's milestone, and unknown when the denominator
% is not declared. Completion is never inferred from file existence.
component_fraction(Component, _Components, Milestones, AcceptanceItems,
                   Fraction) :-
    (   memberchk(milestone(MilestoneId, Component, _), Milestones)
    ->  findall(Status-Count,
                member(acceptance_item(_, MilestoneId, Status, Count),
                       AcceptanceItems),
                Items),
        (   Items == []
        ->  Fraction = unknown
        ;   sum_items(Items, Completed, Total),
            (   Total =:= 0
            ->  Fraction = unknown
            ;   Fraction = completed(Completed, Total)
            )
        )
    ;   Fraction = unknown
    ).

sum_items(Items, Completed, Total) :-
    findall(Count, member(_-Count, Items), Counts),
    sum_numbers(Counts, Total),
    findall(Count, member(completed-Count, Items), CompletedCounts),
    sum_numbers(CompletedCounts, Completed).

%% fraction_percent(+Fraction, -Percent, -Text) is det.
fraction_percent(unknown, unknown, "unknown") :- !.
fraction_percent(completed(Completed, Total), Percent, Text) :-
    Percent is truncate(1000 * Completed / Total) / 10,
    format(string(Text), "~w/~w (~w%)", [Completed, Total, Percent]).

% --- D2 rendering ---

%% render_d2(+Graph, -Text) is det.
render_d2(Graph, Text) :-
    Graph = graph(Components, Edges, _, _, _, _, FlowSteps),
    d2_header(HeaderLines),
    d2_classes(ClassLines),
    d2_overview(Components, Edges, OverviewLines),
    d2_layers(Components, Edges, FlowSteps, LayerLines),
    append([HeaderLines, [""], ClassLines, [""], OverviewLines, [""],
            LayerLines], Lines),
    atomics_to_string(Lines, "\n", Text).

d2_header(Lines) :-
    Lines = [
        "# DL7 shared architecture graph",
        "# Generated from 0_system.dl7 by 1_generate.pl. Do not edit by hand.",
        "",
        "title: \"DL7 shared architecture graph\"",
        "",
        "direction: right"
    ].

relation_classes([
    rel_depends_on, rel_emits, rel_lowers_to, rel_hosts, rel_reads, rel_writes,
    rel_watches, rel_reloads, rel_effects, rel_later_target
]).

relation_style(rel_depends_on, "stroke: \"#868e96\"; stroke-width: 1").
relation_style(rel_emits, "stroke: \"#2f9e44\"; stroke-width: 3").
relation_style(rel_lowers_to, "stroke: \"#7048e8\"; stroke-width: 3; stroke-dash: 4").
relation_style(rel_hosts, "stroke: \"#e8590c\"; stroke-width: 3").
relation_style(rel_reads, "stroke: \"#1971c2\"; stroke-width: 3").
relation_style(rel_writes, "stroke: \"#0c8599\"; stroke-width: 3").
relation_style(rel_watches, "stroke: \"#f08c00\"; stroke-width: 3; stroke-dash: 4").
relation_style(rel_reloads, "stroke: \"#c2255c\"; stroke-width: 3; stroke-dash: 4").
relation_style(rel_effects, "stroke: \"#ae3ec9\"; stroke-width: 4").
relation_style(rel_later_target, "stroke: \"#495057\"; stroke-width: 2; stroke-dash: 6").

d2_classes(Lines) :-
    findall(Line,
            ( member(Class,
                     [ rel_depends_on, rel_emits, rel_lowers_to, rel_hosts,
                       rel_reads, rel_writes, rel_watches, rel_reloads,
                       rel_effects, rel_later_target
                     ]),
              relation_style(Class, Style),
              format(string(Line), "  ~s: { style: { ~s } }", [Class, Style]) ),
            ClassLines),
    append(["classes: {"], ClassLines, T1),
    append(T1, ["}"], Lines).

d2_overview(Components, Edges, Lines) :-
    cluster_order(Clusters),
    findall(Block,
            ( member(Cluster, Clusters),
              cluster_block(Cluster, Components, Block) ),
            Blocks0),
    append(Blocks0, Blocks),
    findall(EdgeLine, d2_attachment_edge(Edges, EdgeLine), EdgeLines),
    append(["overview: {"], Blocks, T1),
    append(T1, EdgeLines, T2),
    append(T2, ["}"], Lines).

cluster_order([compile, algebra, generated, runtime, hosts, effects]).

cluster_block(Cluster, Components, Lines) :-
    findall(Member,
            ( member(Component, Components),
              cluster_of(Component, Cluster),
              Component = component(Id, Label, _, _, _),
              d2_node(Id, Label, "    ", Member) ),
            Members),
    (   Members == []
    ->  Lines = []
    ;   quote_id(Cluster, Quoted),
        format(string(Open), "  ~s: {", [Quoted]),
        append([Open], Members, T1),
        append(T1, ["  }"], Lines)
    ).

cluster_of(component(Id, _, _, _, Plane), Cluster) :-
    (   ( atom_prefix(Id, 'dl7.emit')
        ; Id == 'dl7.emitter_protocol'
        )
    ->  Cluster = generated
    ;   atom_prefix(Id, 'host.')
    ->  Cluster = hosts
    ;   atom_prefix(Id, 'lsp.')
    ->  Cluster = effects
    ;   Plane == core
    ->  Cluster = compile
    ;   Plane == userland
    ->  Cluster = algebra
    ;   Cluster = runtime
    ).

atom_prefix(Atom, Prefix) :-
    sub_atom(Atom, 0, _, _, Prefix).

d2_attachment_edge(Edges, Line) :-
    member(edge(_, Src, Relation, Dst), Edges),
    Dst \== none,
    d2_edge(Src, Relation, Dst, "  ", Line).

d2_edge(Src, Relation, Dst, Indent, Line) :-
    quote_id(Src, QuotedSrc),
    quote_id(Dst, QuotedDst),
    quote_text(Relation, QuotedRelation),
    relation_class(Relation, Class),
    format(string(Line), "~s~s -> ~s: ~s { class: ~s }",
           [Indent, QuotedSrc, QuotedDst, QuotedRelation, Class]).

relation_class(Relation, Class) :-
    (   relation_class_direct(Relation, Class)
    ->  true
    ;   Class = rel_depends_on
    ).

relation_class_direct(Relation, Class) :-
    format(atom(Class), "rel_~w", [Relation]).

d2_layers(Components, Edges, FlowSteps, Lines) :-
    d2_compiler_layer(Components, Edges, CompilerLines),
    d2_flow_layer(runtime_tick, Components, FlowSteps, TickLines),
    d2_reload_layer(Components, Edges, ReloadLines),
    d2_reference_layer(Components, FlowSteps, ReferenceLines),
    layer_block("compiler", CompilerLines, CompilerBlock),
    layer_block("runtime_tick", TickLines, TickBlock),
    layer_block("reload", ReloadLines, ReloadBlock),
    layer_block("reference", ReferenceLines, ReferenceBlock),
    append(["layers: {"], CompilerBlock, T1),
    append(T1, TickBlock, T2),
    append(T2, ReloadBlock, T3),
    append(T3, ReferenceBlock, T4),
    append(T4, ["}"], Lines).

layer_block(Name, BodyLines, Lines) :-
    quote_id(Name, Quoted),
    format(string(Open), "  ~s: {", [Quoted]),
    append([Open], BodyLines, T1),
    append(T1, ["  }"], Lines).

d2_compiler_layer(Components, Edges, Lines) :-
    compiler_ids(Components, Ids),
    sort(Ids, IdSet),
    findall(NodeLine,
            ( member(Id, Ids),
              component_label(Components, Id, Label),
              d2_node(Id, Label, "    ", NodeLine) ),
            NodeLines),
    findall(EdgeLine,
            ( member(edge(_, Src, Relation, Dst), Edges),
              Dst \== none,
              memberchk(Src, IdSet),
              memberchk(Dst, IdSet),
              d2_edge(Src, Relation, Dst, "    ", EdgeLine) ),
            EdgeLines),
    append(NodeLines, EdgeLines, Lines).

compiler_ids(Components, Ids) :-
    findall(Id,
            ( member(component(Id, _, _, _, _), Components),
              compiler_cluster(Id) ),
            Ids).

compiler_cluster(Id) :-
    memberchk(Id,
              [ 'dl7.reader', 'dl7.macrotime', 'dl7.kernel', 'dl7.evaluator',
                'dl7.checker', 'dl7.module_graph', 'dl7.prelude', 'dl7.schema',
                'dl7.extract_loader', 'dl7.source_fact_loader',
                'dl7.host_planner', 'dl7.compiler_tracer',
                'dl7.compiler_cacher', 'dl7.tool_cli', 'dl7.bench',
                'dl7.userland', 'dl7.emitters', 'dl7.emitter_protocol',
                'dl7.emit.dbsp', 'dl7.emit.sqlite', 'dl7.emit.rust'
              ]).

d2_flow_layer(Flow, Components, FlowSteps, Lines) :-
    findall(flow_step(F, Step, Src, Relation, Dst),
            ( member(flow_step(F, Step, Src, Relation, Dst), FlowSteps),
              F == Flow ),
            Steps),
    findall(Id,
            ( member(flow_step(_, _, Src, _, Dst), Steps),
              ( Id = Src ; Id = Dst ) ),
            Ids0),
    sort(Ids0, Ids),
    findall(NodeLine,
            ( member(Id, Ids),
              flow_label(Components, Id, Label),
              d2_node(Id, Label, "    ", NodeLine) ),
            NodeLines),
    findall(EdgeLine,
            ( member(flow_step(_, _, Src, Relation, Dst), Steps),
              d2_edge(Src, Relation, Dst, "    ", EdgeLine) ),
            EdgeLines),
    append(NodeLines, EdgeLines, Lines).

flow_node(Id, Id).

flow_label(Components, Id, Label) :-
    (   component_label(Components, Id, Label)
    ->  true
    ;   format(string(Label), "~w", [Id])
    ).

d2_reload_layer(Components, Edges, Lines) :-
    findall(Id,
            ( member(component(Id, _, _, _, _), Components),
              reload_component(Id) ),
            Ids0),
    sort(Ids0, Ids),
    sort(Ids, IdSet),
    findall(NodeLine,
            ( member(Id, Ids),
              component_label(Components, Id, Label),
              d2_node(Id, Label, "    ", NodeLine) ),
            NodeLines),
    findall(EdgeLine,
            ( member(edge(_, Src, Relation, Dst), Edges),
              Dst \== none,
              memberchk(Src, IdSet),
              memberchk(Dst, IdSet),
              d2_edge(Src, Relation, Dst, "    ", EdgeLine) ),
            EdgeLines),
    append(NodeLines, EdgeLines, Lines).

reload_component(Id) :-
    memberchk(Id,
              [hmr, 'dl6.reload_catalog', 'dl6.storage', 'dl6.retention',
               'dl6.frontier', 'rt.tsv2', 'rt.dd_ram', 'dd.backend']).

d2_reference_layer(Components, FlowSteps, Lines) :-
    findall(flow_step(F, Step, Src, Relation, Dst),
            ( member(flow_step(F, Step, Src, Relation, Dst), FlowSteps),
              memberchk(F, [dl6_reference, v7_successor]) ),
            Steps),
    findall(Id,
            ( member(flow_step(_, _, Src, _, Dst), Steps),
              ( Id = Src ; Id = Dst ) ),
            Ids0),
    sort(Ids0, Ids),
    findall(NodeLine,
            ( member(Id, Ids),
              flow_label(Components, Id, Label),
              d2_node(Id, Label, "    ", NodeLine) ),
            NodeLines),
    findall(EdgeLine,
            ( member(flow_step(_, _, Src, Relation, Dst), Steps),
              d2_edge(Src, Relation, Dst, "    ", EdgeLine) ),
            EdgeLines),
    append(NodeLines, EdgeLines, Lines).

component_label(Components, Id, Label) :-
    member(component(Id, Label, _, _, _), Components),
    !.

d2_node(Id, Label, Indent, Line) :-
    quote_id(Id, QuotedId),
    quote_label(Label, QuotedLabel),
    format(string(Line), "~s~s: ~s", [Indent, QuotedId, QuotedLabel]).

quote_id(Id, Quoted) :-
    format(string(Quoted), "\"~s\"", [Id]).

quote_text(Text, Quoted) :-
    format(string(Quoted), "\"~s\"", [Text]).

quote_label(Label, Quoted) :-
    atom_string(Label, Text),
    split_string(Text, "\"", "", Parts),
    atomic_list_concat(Parts, "\\\"", Escaped),
    format(string(Quoted), "\"~s\"", [Escaped]).

% --- progress rendering ---

%% render_progress(+Graph, -Text) is det.
render_progress(Graph, Text) :-
    Graph = graph(Components, Edges, Milestones, AcceptanceItems, Evidence,
                  PhaseAxis, FlowSteps),
    length(Components, ComponentCount),
    length(Edges, EdgeCount),
    length(Milestones, MilestoneCount),
    length(AcceptanceItems, AcceptanceCount),
    length(Evidence, EvidenceCount),
    length(FlowSteps, FlowCount),
    findall(State-Count,
            ( member(State, [implemented, partial, planned, absent, reference]),
              aggregate_all(count,
                            member(component(_, _, State, _, _), Components),
                            Count) ),
            StateCounts),
    header_lines(ComponentCount, EdgeCount, MilestoneCount, AcceptanceCount,
                 EvidenceCount, FlowCount, StateCounts, HeaderLines),
    component_lines(Components, Milestones, AcceptanceItems, ComponentLines),
    evidence_lines(Evidence, EvidenceLines),
    axis_lines(PhaseAxis, AxisLines),
    append([HeaderLines, ComponentLines, EvidenceLines, AxisLines], Lines),
    atomics_to_string(Lines, "\n", Text).

header_lines(ComponentCount, EdgeCount, MilestoneCount, AcceptanceCount,
              EvidenceCount, FlowCount, StateCounts, Lines) :-
    findall(Row,
            ( member(State-Count, StateCounts),
              format(string(Row), "| state ~w | ~w |", [State, Count]) ),
            StateRows),
    format(string(TotalComponents), "| components | ~w |", [ComponentCount]),
    format(string(TotalEdges), "| typed attachment edges | ~w |", [EdgeCount]),
    format(string(TotalMilestones), "| milestones | ~w |", [MilestoneCount]),
    format(string(TotalAcceptance),
           "| acceptance item rows | ~w |", [AcceptanceCount]),
    format(string(TotalEvidence), "| evidence paths | ~w |", [EvidenceCount]),
    format(string(TotalFlow), "| concrete flow steps | ~w |", [FlowCount]),
    append([TotalComponents, TotalEdges, TotalMilestones, TotalAcceptance,
            TotalEvidence, TotalFlow], StateRows, CountRows),
    append([
        "# DL7 shared architecture progress",
        "",
        "Generated from [`0_system.dl7`](0_system.dl7) by [`1_generate.pl`](1_generate.pl). Do not edit by hand.",
        "",
        "## Source",
        "",
        "`v7/receipts/16_dl6_dl7_architecture_inventory.md` is the only authored input. One DL7 relational kernel is applied at macrotime, comptime, and runtime; DL6 is a DL7 userland application, not a second core.",
        "",
        "## Counts",
        "",
        "| metric | value |",
        "| --- | --- |"
    ], CountRows, T1),
    append(T1, [
        "",
        "## Per-component milestone",
        "",
        "Fraction is completed acceptance items over total declared acceptance items. `unknown` means no acceptance list is declared for that component; completion is never inferred from a file existing.",
        "",
        "| component | label | plane | binding | state | completed/total | percent |",
        "| --- | --- | --- | --- | --- | --- | --- |"
    ], T2),
    Lines = T2.

component_lines(Components, Milestones, AcceptanceItems, Lines) :-
    findall(Line,
            ( member(component(Id, Label, State, Binding, Plane), Components),
              component_fraction(Id, Components, Milestones, AcceptanceItems,
                                 Fraction),
              fraction_columns(Fraction, FractionText, PercentText),
              format(string(Line),
                     "| ~w | ~w | ~w | ~w | ~w | ~w | ~w |",
                     [Id, Label, Plane, Binding, State, FractionText,
                      PercentText]) ),
            Lines).

fraction_columns(unknown, "unknown", "unknown") :- !.
fraction_columns(completed(Completed, Total), FractionText, PercentText) :-
    Percent is truncate(1000 * Completed / Total) / 10,
    format(string(FractionText), "~w/~w", [Completed, Total]),
    format(string(PercentText), "~w%", [Percent]).

evidence_lines(Evidence, Lines) :-
    findall(Line,
            ( member(evidence(_, Component, Path), Evidence),
              format(string(Line), "| ~w | ~w |", [Component, Path]) ),
            Rows),
    append([
        "",
        "## Evidence paths",
        "",
        "| component | path |",
        "| --- | --- |"
    ], Rows, Lines).

axis_lines(PhaseAxis, Lines) :-
    findall(Line,
            ( member(phase_axis(Binding, Consumes, Produces), PhaseAxis),
              format(string(Line), "| ~w | ~w | ~w |",
                     [Binding, Consumes, Produces]) ),
            Rows),
    append([
        "",
        "## Phase axis",
        "",
        "| binding | consumes | produces |",
        "| --- | --- | --- |"
    ], Rows, Lines).

% --- deterministic ordering helpers ---

sort_by_arg1(Rows0, Rows) :-
    predsort(by_arg1, Rows0, Rows).

by_arg1(Order, RowA, RowB) :-
    arg(1, RowA, ValueA),
    arg(1, RowB, ValueB),
    compare(Order, ValueA, ValueB).

by_flow_step(Order, RowA, RowB) :-
    arg(1, RowA, FlowA),
    arg(2, RowA, StepA),
    arg(1, RowB, FlowB),
    arg(2, RowB, StepB),
    compare(Order, FlowA-StepA, FlowB-StepB).

sum_numbers(List, Sum) :-
    foldl(add_number, List, 0, Sum).

add_number(Number, Acc, Total) :-
    Total is Acc + Number.
