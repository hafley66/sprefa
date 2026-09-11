:- module(dl7_checker,
          [ check_datalog/4,
            check_goal_sequence/4,
            check_resolved_rules/5,
            open_checker_origin_arena/1,
            close_checker_origin_arena/0
          ]).

:- use_module(library(assoc), [get_assoc/3, list_to_assoc/2]).
:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module(library(error), [must_be/2]).
:- use_module(library(gensym), [gensym/2]).
:- use_module(library(pairs), [group_pairs_by_key/2]).
:- use_module('../1_libtime/0_evaluator',
              [integer_comparison/3, stratify_rules/3]).
:- use_module('0_lowerer', [kernel_relation/2]).
:- use_module('1b_compiler_tracer',
              [debug_trace_on/0,
                debug_event/2,
                debug_histogram_fields/3,
                profile_occurrence/2
              ]).

:- dynamic arena_edge_origin/6.
:- dynamic arena_seed_origin/4.
:- dynamic arena_rule_origin/4.
:- dynamic arena_goal_origin/5.

:- thread_local checker_origin_arena_scope/1.
:- thread_local checker_origin_lookup_scope/1.

:- meta_predicate with_checker_origin_lookup(+, 0).

%% open_checker_origin_arena(+ModuleOrigins) is det.
%
% Materialize one owning compile's checker-keyed origins in source-list order.
% The explicit arena id keeps nested compiles and compiles in other threads
% disjoint while the four flattened dynamic predicates remain JITI-indexed.
open_checker_origin_arena(ModuleOrigins) :-
    gensym(dl7_checker_origin_arena_, ArenaId),
    asserta(checker_origin_arena_scope(ArenaId)),
    catch(
        (   install_module_origin_facts(ModuleOrigins, ArenaId, 0, _)
        ->  true
        ;   close_checker_origin_arena,
            fail
        ),
        Error,
        ( close_checker_origin_arena,
          throw(Error)
        )).

install_module_origin_facts([], _, Sequence, Sequence).
install_module_origin_facts(
    [module_origins(_, Origins) | ModuleOrigins], ArenaId,
    Sequence0, Sequence) :-
    install_checker_origin_facts(
        Origins, ArenaId, Sequence0, Sequence1),
    install_module_origin_facts(
        ModuleOrigins, ArenaId, Sequence1, Sequence).

install_checker_origin_facts([], _, Sequence, Sequence).
install_checker_origin_facts(
    [Origin | Origins], ArenaId, Sequence0, Sequence) :-
    install_checker_origin_fact(Origin, ArenaId, Sequence0, Sequence1),
    install_checker_origin_facts(
        Origins, ArenaId, Sequence1, Sequence).

install_checker_origin_fact(
    origin(edge(Owner, Name, Index), NodeId), ArenaId,
    Sequence0, Sequence) :-
    !,
    assertz(arena_edge_origin(
                Owner, Name, Index, ArenaId, Sequence0, NodeId)),
    Sequence is Sequence0 + 1.
install_checker_origin_fact(
    origin(seed(SeedIndex), NodeId), ArenaId, Sequence0, Sequence) :-
    !,
    assertz(arena_seed_origin(SeedIndex, ArenaId, Sequence0, NodeId)),
    Sequence is Sequence0 + 1.
install_checker_origin_fact(
    origin(rule(RuleIndex), NodeId), ArenaId, Sequence0, Sequence) :-
    !,
    assertz(arena_rule_origin(RuleIndex, ArenaId, Sequence0, NodeId)),
    Sequence is Sequence0 + 1.
install_checker_origin_fact(
    origin(goal(RuleIndex, GoalIndex), NodeId), ArenaId,
    Sequence0, Sequence) :-
    !,
    assertz(arena_goal_origin(
                RuleIndex, GoalIndex, ArenaId, Sequence0, NodeId)),
    Sequence is Sequence0 + 1.
install_checker_origin_fact(
    origin(node(_), _), _, Sequence, Sequence) :- !.
install_checker_origin_fact(
    origin(relation(_), _), _, Sequence, Sequence) :- !.

%% close_checker_origin_arena is det.
%
% Pop and erase exactly the innermost compile arena. The owning compiler wraps
% its checker/comptime lifetime in setup_call_cleanup/3.
close_checker_origin_arena :-
    (   retract(checker_origin_arena_scope(ArenaId))
    ->  retractall(arena_edge_origin(_, _, _, ArenaId, _, _)),
        retractall(arena_seed_origin(_, ArenaId, _, _)),
        retractall(arena_rule_origin(_, ArenaId, _, _)),
        retractall(arena_goal_origin(_, _, ArenaId, _, _))
    ;   true
    ).

with_checker_origin_lookup(Origins, Goal) :-
    checker_origin_lookup(Origins, Lookup),
    setup_call_cleanup(
        asserta(checker_origin_lookup_scope(Lookup)),
        call(Goal),
        retract(checker_origin_lookup_scope(Lookup))).

checker_origin_lookup(Origins, arena(ArenaId)) :-
    checker_origin_arena_scope(ArenaId),
    checker_origin_sequence(Origins, ArenaId, 0, Sequence),
    \+ checker_origin_at_sequence(ArenaId, Sequence),
    !.
checker_origin_lookup(_, list).

checker_origin_sequence([], _, Sequence, Sequence).
checker_origin_sequence(
    [origin(edge(Owner, Name, Index), NodeId) | Origins],
    ArenaId, Sequence0, Sequence) :-
    !,
    arena_edge_origin(
        Owner, Name, Index, ArenaId, Sequence0, NodeId),
    Sequence1 is Sequence0 + 1,
    checker_origin_sequence(Origins, ArenaId, Sequence1, Sequence).
checker_origin_sequence(
    [origin(seed(SeedIndex), NodeId) | Origins],
    ArenaId, Sequence0, Sequence) :-
    !,
    arena_seed_origin(SeedIndex, ArenaId, Sequence0, NodeId),
    Sequence1 is Sequence0 + 1,
    checker_origin_sequence(Origins, ArenaId, Sequence1, Sequence).
checker_origin_sequence(
    [origin(rule(RuleIndex), NodeId) | Origins],
    ArenaId, Sequence0, Sequence) :-
    !,
    arena_rule_origin(RuleIndex, ArenaId, Sequence0, NodeId),
    Sequence1 is Sequence0 + 1,
    checker_origin_sequence(Origins, ArenaId, Sequence1, Sequence).
checker_origin_sequence(
    [origin(goal(RuleIndex, GoalIndex), NodeId) | Origins],
    ArenaId, Sequence0, Sequence) :-
    !,
    arena_goal_origin(
        RuleIndex, GoalIndex, ArenaId, Sequence0, NodeId),
    Sequence1 is Sequence0 + 1,
    checker_origin_sequence(Origins, ArenaId, Sequence1, Sequence).
checker_origin_sequence([_ | Origins], ArenaId, Sequence0, Sequence) :-
    checker_origin_sequence(Origins, ArenaId, Sequence0, Sequence).

checker_origin_at_sequence(ArenaId, Sequence) :-
    arena_edge_origin(_, _, _, ArenaId, Sequence, _).
checker_origin_at_sequence(ArenaId, Sequence) :-
    arena_seed_origin(_, ArenaId, Sequence, _).
checker_origin_at_sequence(ArenaId, Sequence) :-
    arena_rule_origin(_, ArenaId, Sequence, _).
checker_origin_at_sequence(ArenaId, Sequence) :-
    arena_goal_origin(_, _, ArenaId, Sequence, _).

%% check_datalog(+BasementProgram, +Origins, -Checked, -Diagnostics) is det.
%
% Resolve every pending name through owner edges and reverse binding edges,
% check binds, indices, relation use, arities, ground seeds, and positive-rule
% safety, then emit canonical colon edges, the positive dependency graph, and
% SCC strata. Diagnostics are sorted by origin; no Checked value survives a
% diagnostic.
check_datalog(Basement, Origins, Checked, Diagnostics) :-
    profile_occurrence(checker_input, check_datalog(Basement, Origins)),
    debug_checker_input(Basement, Origins),
    with_checker_origin_lookup(
        Origins,
        check_datalog_body(Basement, Origins, Checked, Diagnostics)),
    debug_checker_output(Checked, Diagnostics).

check_datalog_body(basement_program(root_graph(Nodes, PendingEdges),
                                    datalog_program(Relations0, Seeds0, Rules0)),
                   Origins, Checked, Diagnostics) :-
    !,
    must_be(ground, Origins),
    relations_refs(Relations0, SourceRelations),
    kernel_relation_rows(KernelRelations),
    append(SourceRelations, KernelRelations, AllRelations),
    sort(AllRelations, Relations),
    bind_diagnostics(PendingEdges, Origins, BindDiags),
    resolve_edges(PendingEdges, PendingEdges, Nodes, Origins, ColonEdges,
                  EdgeDiags),
    resolve_seeds(Seeds0, 0, PendingEdges, Nodes, Relations, Origins,
                  Seeds, SeedDiags),
    resolve_rules(Rules0, 0, PendingEdges, Nodes, Relations, Origins,
                  Rules, RuleDiags),
    append([BindDiags, EdgeDiags, SeedDiags, RuleDiags], Diags),
    (   Diags == []
    ->  stratify_rules(Rules, DerivedStrata, StrataDiagnostics),
        locate_strata_diagnostics(StrataDiagnostics, Rules, Origins,
                                  LocatedStrataDiagnostics),
        finish_checked(LocatedStrataDiagnostics, DerivedStrata,
                       Nodes, ColonEdges,
                       Relations, Seeds, Rules, Checked, Diagnostics)
    ;   Checked = [],
        sort(Diags, Diagnostics)
    ).
check_datalog_body(Program, _, [], Diagnostics) :-
    must_be(ground, Program),
    Diagnostics = [diagnostic(check, none, invalid_basement_program)].

%% check_resolved_rules(+Relations, +Rules, -Depends, -Strata,
%%                      -Diagnostics) is det.
%
% Check compiler-generated checked-IR candidates. Their relation references
% are already canonical, so this entrypoint performs declaration, arity,
% mode, safety, and stratification checks without source-name resolution.
check_resolved_rules(Relations, Rules, Depends, Strata, Diagnostics) :-
    profile_occurrence(checker_input,
                       check_resolved_rules(Relations, Rules)),
    debug_resolved_input(Relations, Rules),
    check_resolved_rules_body(Relations, Rules, Depends, Strata,
                              Diagnostics),
    debug_resolved_output(Depends, Strata, Diagnostics).

check_resolved_rules_body(Relations, Rules, Depends, Strata, Diagnostics) :-
    must_be(ground, Relations),
    must_be(ground, Rules),
    resolved_rule_diagnostics(Rules, Relations, RuleDiagnostics),
    (   RuleDiagnostics == []
    ->  stratify_rules(Rules, DerivedStrata, StrataDiagnostics),
        (   StrataDiagnostics == []
        ->  depends_rows(Rules, Depends0),
            sort(Depends0, Depends),
            strata_rows(Relations, DerivedStrata, Strata),
            Diagnostics = []
        ;   Depends = [],
            Strata = [],
            Diagnostics = StrataDiagnostics
        )
    ;   Depends = [],
        Strata = [],
        sort(RuleDiagnostics, Diagnostics)
    ).

%% Checker debug events. Counts derive only while debug tracing is active.

debug_checker_input(
    basement_program(root_graph(Nodes, PendingEdges),
                     datalog_program(Relations, Seeds, Rules)),
    Origins) :-
    !,
    (   debug_trace_on
    ->  length(Nodes, NodeCount),
        length(PendingEdges, EdgeCount),
        length(Relations, RelationCount),
        length(Seeds, SeedCount),
        length(Rules, RuleCount),
        length(Origins, OriginCount),
        debug_event(checker_input,
                    [phase=check, nodes=NodeCount, pending_edges=EdgeCount,
                     relations=RelationCount, seeds=SeedCount,
                     rules=RuleCount, origins=OriginCount])
    ;   true
    ).
debug_checker_input(_, _).

debug_checker_output(Checked, Diagnostics) :-
    (   debug_trace_on
    ->  checked_counts(Checked, Nodes, Edges, Relations, Seeds, Rules),
        length(Diagnostics, DiagnosticCount),
        checker_reason_histogram(Diagnostics, Histogram),
        debug_histogram_fields(diagnostic_reasons, Histogram, HistFields),
        append([phase=check, nodes=Nodes, edges=Edges,
                relations=Relations, seeds=Seeds, rules=Rules,
                diagnostics=DiagnosticCount], HistFields, Fields),
        debug_event(checker_output, Fields)
    ;   true
    ).

checked_counts(
    checked_datalog(root_graph(Nodes0, Edges0),
                    datalog_program(Relations0, Seeds0, Rules0), _, _),
    Nodes, Edges, Relations, Seeds, Rules) :-
    !,
    length(Nodes0, Nodes),
    length(Edges0, Edges),
    length(Relations0, Relations),
    length(Seeds0, Seeds),
    length(Rules0, Rules).
checked_counts(_, 0, 0, 0, 0, 0).

checker_reason_histogram(Diagnostics, Histogram) :-
    findall(Name,
            ( member(diagnostic(_, _, Reason), Diagnostics),
              checker_reason_name(Reason, Name)
            ),
            Names0),
    msort(Names0, Names),
    sort(Names, Distinct),
    histogram_counts(Distinct, Names, Histogram).

checker_reason_name(Reason, Name) :-
    (   compound(Reason)
    ->  functor(Reason, Name, _)
    ;   Name = Reason
    ).

histogram_counts([], _, []).
histogram_counts([Key | Keys], Rows, [Key-Count | Rest]) :-
    aggregate_all(count, member(Key, Rows), Count),
    histogram_counts(Keys, Rows, Rest).

debug_resolved_input(Relations, Rules) :-
    (   debug_trace_on
    ->  length(Relations, RelationCount),
        length(Rules, RuleCount),
        debug_event(checker_input,
                    [phase=check, resolved=true,
                     relations=RelationCount, rules=RuleCount])
    ;   true
    ).

debug_resolved_output(Depends, Strata, Diagnostics) :-
    (   debug_trace_on
    ->  length(Depends, DependCount),
        length(Strata, StrataCount),
        length(Diagnostics, DiagnosticCount),
        debug_event(checker_output,
                    [phase=check, resolved=true, depends=DependCount,
                     strata=StrataCount, diagnostics=DiagnosticCount])
    ;   true
    ).

resolved_rule_diagnostics([], _, []).
resolved_rule_diagnostics([Rule | Rules], Relations, Diagnostics) :-
    resolved_rule_diagnostic(Rule, Relations, OwnDiagnostics),
    resolved_rule_diagnostics(Rules, Relations, RestDiagnostics),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

resolved_rule_diagnostic(rule(Head, Body), Relations, Diagnostics) :-
    !,
    resolved_call_diagnostics(Head, Relations, HeadDiagnostics),
    resolved_goal_diagnostics(Body, Relations, BodyDiagnostics),
    head_variables(Head, HeadVariables),
    check_goal_sequence_failures(Body, 0, HeadVariables, [],
                                 _, _, ModeFailures),
    maplist(unlocated_mode_diagnostic, ModeFailures, ModeDiagnostics),
    head_safety_diagnostics_with_variables(HeadVariables, Body, [], 0,
                                           SafetyDiagnostics),
    append([HeadDiagnostics, BodyDiagnostics,
            ModeDiagnostics, SafetyDiagnostics], Diagnostics).
resolved_rule_diagnostic(Rule, _,
                         [diagnostic(check, none,
                                     invalid_generated_rule(Rule))]).

resolved_goal_diagnostics([], _, []).
resolved_goal_diagnostics([checked_goal(Polarity, Call) | Goals],
                          Relations, Diagnostics) :-
    !,
    (   memberchk(Polarity, [positive, negative])
    ->  PolarityDiagnostics = []
    ;   PolarityDiagnostics =
            [diagnostic(check, none,
                        invalid_generated_polarity(Polarity))]
    ),
    resolved_call_diagnostics(Call, Relations, CallDiagnostics),
    resolved_goal_diagnostics(Goals, Relations, RestDiagnostics),
    append([PolarityDiagnostics, CallDiagnostics, RestDiagnostics],
           Diagnostics).
resolved_goal_diagnostics([Goal | Goals], Relations,
                          [diagnostic(check, none,
                                      invalid_generated_goal(Goal))
                           | Diagnostics]) :-
    resolved_goal_diagnostics(Goals, Relations, Diagnostics).

resolved_call_diagnostics(call(Relation, Arguments), Relations,
                          Diagnostics) :-
    !,
    (   memberchk(relation(Relation, Arity, _), Relations)
    ->  length(Arguments, ObservedArity),
        (   ObservedArity =:= Arity
        ->  Diagnostics = []
        ;   Diagnostics =
                [diagnostic(check, none,
                            generated_arity_mismatch(
                                Relation, Arity, ObservedArity))]
        )
    ;   Diagnostics =
            [diagnostic(check, none,
                        generated_undeclared_relation(Relation))]
    ).
resolved_call_diagnostics(Call, _,
                          [diagnostic(check, none,
                                      invalid_generated_call(Call))]).

finish_checked([], DerivedStrata, Nodes, ColonEdges, Relations, Seeds, Rules,
               Checked, []) :-
    !,
    depends_rows(Rules, Depends0),
    sort(Depends0, Depends),
    strata_rows(Relations, DerivedStrata, Strata),
    kernel_graph(KernelNodes, KernelEdges),
    append(Nodes, KernelNodes, CheckedNodes),
    append(ColonEdges, KernelEdges, AllEdges),
    msort(AllEdges, SortedEdges),
    msort(Relations, SortedRelations),
    Checked = checked_datalog(root_graph(CheckedNodes, SortedEdges),
                              datalog_program(SortedRelations, Seeds,
                                              Rules),
                              Depends, Strata).
finish_checked(Diagnostics, _, _, _, _, _, _, [], Diagnostics).

locate_strata_diagnostics([], _, _, []).
locate_strata_diagnostics(
    [diagnostic(stratify, none,
                strict_dependency_cycle(Relations)) | Diagnostics0],
    Rules, Origins,
    [diagnostic(stratify, NodeId,
                strict_dependency_cycle(Relations)) | Diagnostics]) :-
    !,
    cycle_origin(Rules, Relations, Origins, NodeId),
    locate_strata_diagnostics(Diagnostics0, Rules, Origins, Diagnostics).
locate_strata_diagnostics(
    [diagnostic(stratify, none,
                aggregate_dependency_cycle(Relations)) | Diagnostics0],
    Rules, Origins,
    [diagnostic(stratify, NodeId,
                aggregate_dependency_cycle(Relations)) | Diagnostics]) :-
    !,
    aggregate_cycle_origin(Rules, Relations, Origins, NodeId),
    locate_strata_diagnostics(Diagnostics0, Rules, Origins, Diagnostics).
locate_strata_diagnostics([Diagnostic | Diagnostics0], Rules, Origins,
                          [Diagnostic | Diagnostics]) :-
    locate_strata_diagnostics(Diagnostics0, Rules, Origins, Diagnostics).

cycle_origin(Rules, Relations, Origins, NodeId) :-
    nth0(RuleIndex, Rules,
         rule(call(HeadRelation, _), Goals)),
    memberchk(HeadRelation, Relations),
    nth0(GoalIndex, Goals,
         checked_goal(negative, call(BodyRelation, _))),
    memberchk(BodyRelation, Relations),
    goal_origin(Origins, RuleIndex, GoalIndex, NodeId),
    !.
cycle_origin(_, _, _, none).

aggregate_cycle_origin(Rules, Relations, Origins, NodeId) :-
    nth0(RuleIndex, Rules,
         rule(call(HeadRelation, HeadArguments), _)),
    memberchk(HeadRelation, Relations),
    memberchk(aggregate(count, _), HeadArguments),
    rule_origin(Origins, RuleIndex, NodeId),
    !.
aggregate_cycle_origin(_, _, _, none).

%% Bind checks: one unique name per owner and dense zero-based indices.
bind_diagnostics(Edges, Origins, Diags) :-
    duplicate_bind_diagnostics(Edges, Origins, [], Diags0),
    duplicate_index_diagnostics(Edges, Origins, [], Diags1),
    dense_index_diagnostics(Edges, Edges, Origins, Diags2),
    append([Diags0, Diags1, Diags2], Diags).

duplicate_bind_diagnostics([], _, _, []).
duplicate_bind_diagnostics([pending_edge(Owner, Name, _, Index) | Rest],
                           Origins, Seen, Diags) :-
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   memberchk(seen(Owner, Name), Seen)
    ->  Diags = [diagnostic(check, NodeId, duplicate_bind(Owner, Name))
                 | RestDiags]
    ;   Diags = RestDiags
    ),
    duplicate_bind_diagnostics(Rest, Origins, [seen(Owner, Name) | Seen],
                               RestDiags).

duplicate_index_diagnostics([], _, _, []).
duplicate_index_diagnostics([pending_edge(Owner, Name, _, Index) | Rest],
                            Origins, Seen, Diags) :-
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   memberchk(seen(Owner, Index), Seen)
    ->  Diags = [diagnostic(check, NodeId,
                            duplicate_bind_index(Owner, Index)) | RestDiags]
    ;   Diags = RestDiags
    ),
    duplicate_index_diagnostics(Rest, Origins,
                                [seen(Owner, Index) | Seen], RestDiags).

dense_index_diagnostics(Edges, All, Origins, Diags) :-
    (   ground(Edges),
        ground(All)
    ->  owner_edge_count_index(All, CountIndex),
        dense_index_diagnostics_indexed(Edges, CountIndex, Origins, Diags)
    ;   dense_index_diagnostics_scanned(Edges, All, Origins, Diags)
    ).

dense_index_diagnostics_indexed([], _, _, []).
dense_index_diagnostics_indexed(
    [pending_edge(Owner, Name, _, Index) | Rest], CountIndex, Origins, Diags) :-
    (   get_assoc(Owner, CountIndex, Count)
    ->  true
    ;   Count = 0
    ),
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   (   Index < 0
        ;   Index >= Count
        )
    ->  Diags = [diagnostic(check, NodeId, non_dense_index(Owner, Index))
                 | RestDiags]
    ;   Diags = RestDiags
    ),
    dense_index_diagnostics_indexed(Rest, CountIndex, Origins, RestDiags).

dense_index_diagnostics_scanned([], _, _, []).
dense_index_diagnostics_scanned(
    [pending_edge(Owner, Name, _, Index) | Rest], All, Origins, Diags) :-
    count_owner_edges(All, Owner, Count),
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   (   Index < 0
        ;   Index >= Count
        )
    ->  Diags = [diagnostic(check, NodeId, non_dense_index(Owner, Index))
                 | RestDiags]
    ;   Diags = RestDiags
    ),
    dense_index_diagnostics_scanned(Rest, All, Origins, RestDiags).

owner_edge_count_index(Edges, CountIndex) :-
    findall(Owner-1,
            member(pending_edge(Owner, _, _, _), Edges),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    owner_edge_count_groups(Groups, Counts),
    list_to_assoc(Counts, CountIndex).

owner_edge_count_groups([], []).
owner_edge_count_groups([Owner-Occurrences | Groups],
                        [Owner-Count | Counts]) :-
    length(Occurrences, Count),
    owner_edge_count_groups(Groups, Counts).

count_owner_edges([], _, 0).
count_owner_edges([pending_edge(Owner, _, _, _) | Rest], Owner, Count) :-
    !,
    count_owner_edges(Rest, Owner, RestCount),
    Count is RestCount + 1.
count_owner_edges([_|Rest], Owner, Count) :-
    count_owner_edges(Rest, Owner, Count).

%% Pending edges become canonical ':'(Owner, Name, Target, Index) edges.
resolve_edges([], _, _, _, [], []).
resolve_edges([pending_edge(_, _, deferred_expression(_), _) | Rest],
              All, Nodes, Origins, Edges, Diags) :-
    !,
    resolve_edges(Rest, All, Nodes, Origins, Edges, Diags).
resolve_edges([pending_edge(_, _, deferred_compound_edge(_, _), _) | Rest],
              All, Nodes, Origins, Edges, Diags) :-
    !,
    resolve_edges(Rest, All, Nodes, Origins, Edges, Diags).
resolve_edges([pending_edge(Owner, Name, Target, Index) | Rest], All, Nodes,
              Origins, [':'(Owner, Name, Resolved, Index) | Edges], Diags) :-
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   resolve_target(Target, All, Nodes, [], Resolved)
    ->  Diags = RestDiags
    ;   Resolved = Target,
        Diags = [diagnostic(check, NodeId, unresolved_name(Name)) | RestDiags]
    ),
    resolve_edges(Rest, All, Nodes, Origins, Edges, RestDiags).

%% resolve_target(+TargetTerm, +Edges, +Nodes, +Visited, -Resolved) is semidet.
resolve_target(target(Target), _, _, _, ref(Target)).
resolve_target(const(Value), _, _, _, const(Value)).
resolve_target(name(Owner, Name), Edges, Nodes, Visited, Resolved) :-
    resolve_name(Owner, Name, Edges, Nodes, Visited, Resolved).

%% resolve_name(+Owner, +Name, +Edges, +Nodes, +Visited, -Resolved) is semidet.
%
% Local owner edge first, then the reverse binding edge to the containing
% owner; a module owner resolves the four pinned primitive names.
resolve_name(Owner, Name, Edges, Nodes, Visited, Resolved) :-
    \+ memberchk(Owner-Name, Visited),
    (   memberchk(pending_edge(Owner, Name, Target, _), Edges)
    ->  resolve_target(Target, Edges, Nodes, [Owner-Name | Visited], Resolved)
    ;   parent_owner(Owner, Edges, Parent),
        resolve_name(Parent, Name, Edges, Nodes, [Owner-Name | Visited],
                     Resolved)
    ;   memberchk(module(Owner), Nodes),
        kernel_relation(Name, _),
        Resolved = ref(kernel(Name))
    ;   memberchk(module(Owner), Nodes),
        primitive_name(Name),
        Resolved = ref(primitive(Name))
    ).

parent_owner(Owner, Edges, Parent) :-
    memberchk(pending_edge(Parent, _, target(Owner), _), Edges).

primitive_name(int).
primitive_name(text).
primitive_name(any).
primitive_name(type).

kernel_relation_rows(Relations) :-
    findall(relation(ref(kernel(Name)), Arity, KeySets),
            ( kernel_relation(Name, Arity),
              kernel_relation_keys(Name, KeySets)
            ),
            Relations).

kernel_relation_keys(':', [[0, 1], [0, 3]]).
kernel_relation_keys(edge_snapshot, [[0, 1], [0, 3]]).
kernel_relation_keys(nil, [[0]]).
kernel_relation_keys(cons, [[0, 1], [2]]).
kernel_relation_keys(edge_ref, [[0, 1]]).
kernel_relation_keys(intern, [[0, 1]]).
kernel_relation_keys(intern_snapshot, [[0, 1]]).
kernel_relation_keys(Name, [[0, 1]]) :-
    integer_comparison(Name, _, _),
    !.
kernel_relation_keys(def, [[0]]).
kernel_relation_keys(head, [[0]]).
kernel_relation_keys(body, [[0, 1]]).
kernel_relation_keys(node, []).
kernel_relation_keys(module, []).
kernel_relation_keys(product, []).
kernel_relation_keys(sum, []).

kernel_graph(Nodes, Edges) :-
    integer_comparison_graph(ComparisonNodes, ComparisonEdges),
    append(
    [ node(primitive(int)),
      node(primitive(text)),
      node(primitive(any)),
      node(primitive(type)),
      node(kernel(node)), product(kernel(node)),
      node(kernel(module)), product(kernel(module)),
      node(kernel(product)), product(kernel(product)),
      node(kernel(sum)), product(kernel(sum)),
      node(kernel(':')), product(kernel(':')),
      node(kernel(edge_snapshot)), product(kernel(edge_snapshot)),
      node(kernel(nil)), product(kernel(nil)),
      node(kernel(cons)), product(kernel(cons)),
      node(kernel(edge_ref)), product(kernel(edge_ref)),
      node(kernel(intern)), product(kernel(intern)),
      node(kernel(intern_snapshot)), product(kernel(intern_snapshot))
    ], ComparisonNodes, Nodes0),
    append(Nodes0,
    [
      node(kernel(def)), product(kernel(def)),
      node(kernel(head)), product(kernel(head)),
      node(kernel(body)), product(kernel(body))
    ], Nodes),
    append(
    [ ':'(kernel(node), id, ref(primitive(type)), 0),
      ':'(kernel(module), id, ref(primitive(type)), 0),
      ':'(kernel(product), id, ref(primitive(type)), 0),
      ':'(kernel(sum), id, ref(primitive(type)), 0),
      ':'(kernel(':'), owner, ref(primitive(type)), 0),
      ':'(kernel(':'), name, ref(primitive(any)), 1),
      ':'(kernel(':'), target, ref(primitive(any)), 2),
      ':'(kernel(':'), index, ref(primitive(int)), 3),
      ':'(kernel(edge_snapshot), owner, ref(primitive(type)), 0),
      ':'(kernel(edge_snapshot), name, ref(primitive(any)), 1),
      ':'(kernel(edge_snapshot), target, ref(primitive(any)), 2),
      ':'(kernel(edge_snapshot), index, ref(primitive(int)), 3),
      ':'(kernel(nil), return, ref(primitive(any)), 0),
      ':'(kernel(cons), head, ref(primitive(any)), 0),
      ':'(kernel(cons), tail, ref(primitive(any)), 1),
      ':'(kernel(cons), return, ref(primitive(any)), 2),
      ':'(kernel(edge_ref), owner, ref(primitive(type)), 0),
      ':'(kernel(edge_ref), label, ref(primitive(any)), 1),
      ':'(kernel(edge_ref), return, ref(primitive(type)), 2),
      ':'(kernel(intern), constructor, ref(primitive(type)), 0),
      ':'(kernel(intern), arguments, ref(primitive(any)), 1),
      ':'(kernel(intern), return, ref(primitive(type)), 2),
      ':'(kernel(intern_snapshot), constructor, ref(primitive(type)), 0),
      ':'(kernel(intern_snapshot), arguments, ref(primitive(any)), 1),
      ':'(kernel(intern_snapshot), return, ref(primitive(type)), 2)
    ], ComparisonEdges, Edges0),
    append(Edges0,
    [
      ':'(kernel(def), relation, ref(primitive(type)), 0),
      ':'(kernel(def), arity, ref(primitive(int)), 1),
      ':'(kernel(head), rule, ref(primitive(type)), 0),
      ':'(kernel(head), application, ref(primitive(type)), 1),
      ':'(kernel(body), rule, ref(primitive(type)), 0),
      ':'(kernel(body), goal, ref(primitive(int)), 1),
      ':'(kernel(body), polarity, ref(primitive(text)), 2),
      ':'(kernel(body), application, ref(primitive(type)), 3)
    ], Edges).

integer_comparison_graph(Nodes, Edges) :-
    findall(Node,
            ( integer_comparison(Name, _, _),
              member(Node, [node(kernel(Name)), product(kernel(Name))])
            ),
            Nodes),
    findall(Edge,
            ( integer_comparison(Name, _, _),
              member(Edge,
                     [ ':'(kernel(Name), left, ref(primitive(int)), 0),
                       ':'(kernel(Name), right, ref(primitive(int)), 1)
                     ])
            ),
            Edges).

%% Seeds resolve to ground calls over declared product relations.
resolve_seeds([], _, _, _, _, _, [], []).
resolve_seeds([Seed | Rest], SeedIndex, Edges, Nodes, Relations, Origins,
              [ResolvedSeed | Seeds], Diags) :-
    seed_origin(Origins, SeedIndex, NodeId),
    resolve_call(Seed, Edges, Nodes, Relations, Result),
    (   Result = ok(call(Target, Args)),
        \+ member(var(_), Args)
    ->  ResolvedSeed = call(Target, Args),
        Diags = RestDiags
    ;   Result = ok(call(_, _))
    ->  ResolvedSeed = Seed,
        Diags = [diagnostic(check, NodeId, non_ground_seed) | RestDiags]
    ;   Result = error(Reason)
    ->  ResolvedSeed = Seed,
        Diags = [diagnostic(check, NodeId, Reason) | RestDiags]
    ),
    NextSeedIndex is SeedIndex + 1,
    resolve_seeds(Rest, NextSeedIndex, Edges, Nodes, Relations, Origins,
                  Seeds, RestDiags).

%% Rules resolve head and body calls and check positive-rule safety.
resolve_rules([], _, _, _, _, _, [], []).
resolve_rules([Rule | Rest], RuleIndex, Edges, Nodes, Relations, Origins,
              [ResolvedRule | Rules], Diags) :-
    Rule = rule(Head, Body),
    rule_origin(Origins, RuleIndex, NodeId),
    resolve_call(Head, Edges, Nodes, Relations, HeadResult),
    resolve_goals(Body, RuleIndex, 0, Edges, Nodes, Relations, Origins,
                  BodyResult, GoalDiags),
    (   HeadResult = ok(ResolvedHead),
        BodyResult = ok(ResolvedBody)
    ->  ResolvedRule = rule(ResolvedHead, ResolvedBody),
        head_variables(ResolvedHead, HeadVariables),
        check_goal_sequence_failures(ResolvedBody, 0, HeadVariables, [],
                                     _, _, ModeFailures),
        mode_failure_diagnostics(ModeFailures, RuleIndex, Origins, ModeDiags),
        head_safety_diagnostics_with_variables(HeadVariables, ResolvedBody,
                                               Origins, RuleIndex,
                                               SafetyDiags),
        append([GoalDiags, ModeDiags, SafetyDiags], OwnDiags)
    ;   ResolvedRule = Rule,
        (   HeadResult = error(Reason)
        ->  OwnDiags = [diagnostic(check, NodeId, Reason) | GoalDiags]
        ;   OwnDiags = GoalDiags
        )
    ),
    NextRuleIndex is RuleIndex + 1,
    resolve_rules(Rest, NextRuleIndex, Edges, Nodes, Relations, Origins,
                  Rules, RestDiags),
    append(OwnDiags, RestDiags, Diags).

resolve_goals([], _, _, _, _, _, _, ok([]), []).
resolve_goals([pending_goal(Polarity, Goal) | Rest], RuleIndex, GoalIndex,
              Edges, Nodes, Relations,
              Origins, Result, Diags) :-
    goal_origin(Origins, RuleIndex, GoalIndex, NodeId),
    resolve_call(Goal, Edges, Nodes, Relations, GoalResult),
    NextGoalIndex is GoalIndex + 1,
    resolve_goals(Rest, RuleIndex, NextGoalIndex, Edges, Nodes, Relations,
                  Origins, RestResult, RestDiags),
    (   GoalResult = ok(ResolvedGoal),
        RestResult = ok(RestGoals)
    ->  Result = ok([checked_goal(Polarity, ResolvedGoal) | RestGoals])
    ;   Result = error(rule)
    ),
    (   GoalResult = error(Reason)
    ->  Diags = [diagnostic(check, NodeId, Reason) | RestDiags]
    ;   Diags = RestDiags
    ).

%% check_goal_sequence(+Goals, +Bound0, -Bound, -Diagnostics) is det.
%
% Fold checked goals in authored order. Bound0 represents variables supplied
% by the relation call context. Ordinary positive calls bind every variable;
% constructive kernel calls first require one of their declared input modes.
check_goal_sequence(Goals, Bound0, Bound, Diagnostics) :-
    check_goal_sequence_failures(Goals, 0, Bound0, Bound0,
                                 Bound, _, Failures),
    maplist(unlocated_mode_diagnostic, Failures, Diagnostics).

check_goal_sequence_failures([], _, Available, Produced,
                             Available, Produced, []).
check_goal_sequence_failures([Goal | Goals], GoalIndex,
                             Available0, Produced0, Available, Produced,
                             Failures) :-
    check_goal_transition(Goal, Available0, Produced0,
                          Available1, Produced1, Reason),
    NextGoalIndex is GoalIndex + 1,
    check_goal_sequence_failures(Goals, NextGoalIndex,
                                 Available1, Produced1,
                                 Available, Produced, RestFailures),
    (   Reason == none
    ->  Failures = RestFailures
    ;   Failures = [goal_failure(GoalIndex, Reason) | RestFailures]
    ).

check_goal_transition(Goal, Available, Produced,
                      Available, Produced, Reason) :-
    goal_call(Goal, negative, _),
    !,
    check_goal(Goal, Produced, _, Reason).
check_goal_transition(Goal, Available0, Produced0,
                      Available, Produced, Reason) :-
    goal_call(Goal, positive, _),
    !,
    goal_variables(Goal, Variables),
    check_goal_with_variables(Goal, Available0, Available, Reason,
                              Variables),
    (   Reason == none
    ->  add_variables(Variables, Produced0, Produced)
    ;   Produced = Produced0
    ).

check_goal_with_variables(Goal, Bound0, Bound, Reason, _Variables) :-
    goal_call(Goal, positive,
              call(ref(kernel(Name)), Arguments)),
    integer_comparison(Name, _, _),
    !,
    (   maplist(argument_is_bound_in(Bound0), Arguments)
    ->  Bound = Bound0,
        integer_comparison_argument_types(Name, Arguments, Reason)
    ;   Bound = Bound0,
        Reason = underconstrained_kernel_goal(Name, [[0, 1]])
    ).
check_goal_with_variables(Goal, Bound0, Bound, Reason, Variables) :-
    goal_call(Goal, positive,
              call(ref(kernel(cons)), [Head, Tail, List])),
    !,
    (   (   argument_is_bound(List, Bound0)
        ;   argument_is_bound(Head, Bound0),
            argument_is_bound(Tail, Bound0)
        )
    ->  add_variables(Variables, Bound0, Bound),
        Reason = none
    ;   Bound = Bound0,
        Reason = underconstrained_kernel_goal(cons, [[2], [0, 1]])
    ).
check_goal_with_variables(Goal, Bound0, Bound, Reason, Variables) :-
    goal_call(Goal, positive,
              call(ref(kernel(edge_ref)), [Owner, Label, _])),
    !,
    (   argument_is_bound(Owner, Bound0),
        argument_is_bound(Label, Bound0)
    ->  add_variables(Variables, Bound0, Bound),
        Reason = none
    ;   Bound = Bound0,
        Reason = underconstrained_kernel_goal(
                     edge_ref, [[0, 1]])
    ).
check_goal_with_variables(Goal, Bound0, Bound, Reason, Variables) :-
    goal_call(Goal, positive,
              call(ref(kernel(intern)), [Constructor, Arguments, _])),
    !,
    (   argument_is_bound(Constructor, Bound0),
        argument_is_bound(Arguments, Bound0)
    ->  add_variables(Variables, Bound0, Bound),
        Reason = none
    ;   Bound = Bound0,
        Reason = underconstrained_kernel_goal(intern, [[0, 1]])
    ).
check_goal_with_variables(Goal, Bound0, Bound, none, Variables) :-
    goal_call(Goal, positive, _),
    add_variables(Variables, Bound0, Bound).

check_goal(Goal, Bound0, Bound, Reason) :-
    goal_call(Goal, positive, _),
    !,
    goal_variables(Goal, Variables),
    check_goal_with_variables(Goal, Bound0, Bound, Reason, Variables).

check_goal(Goal, Bound, Bound, negative_constructive_kernel_goal(Name)) :-
    goal_call(Goal, negative, call(ref(kernel(Name)), _)),
    memberchk(Name, [cons, edge_ref, intern, nil]),
    !.
check_goal(Goal, Bound, Bound, Reason) :-
    goal_call(Goal, negative,
              call(ref(kernel(Name)), Arguments)),
    integer_comparison(Name, _, _),
    !,
    goal_variables(Goal, Variables),
    unbound_variables(Variables, Bound, Unbound),
    (   Unbound == []
    ->  integer_comparison_argument_types(Name, Arguments, Reason)
    ;   Reason = unbound_negative_goal(Unbound)
    ).
check_goal(Goal, Bound, Bound, Reason) :-
    goal_call(Goal, negative, _),
    !,
    goal_variables(Goal, Variables),
    unbound_variables(Variables, Bound, Unbound),
    (   Unbound == []
    ->  Reason = none
    ;   Reason = unbound_negative_goal(Unbound)
    ).
argument_is_bound_in(Bound, Argument) :-
    argument_is_bound(Argument, Bound).

integer_comparison_argument_types(Name, Arguments, Reason) :-
    (   nth0(Position, Arguments, Argument),
        \+ int_argument(Argument)
    ->  Reason = kernel_argument_type_mismatch(
                     Name, Position, int, Argument)
    ;   Reason = none
    ).

int_argument(var(_)).
int_argument(const(Value)) :- integer(Value).

argument_is_bound(Argument, Bound) :-
    argument_variables(Argument, Variables),
    variables_are_bound(Variables, Bound).

argument_variables(Argument, Variables) :-
    findall(Identity,
            ( sub_term(Subterm, Argument),
              Subterm = var(Identity)
            ),
            Variables0),
    sort(Variables0, Variables).

variables_are_bound([], _).
variables_are_bound([Variable | Variables], Bound) :-
    memberchk(Variable, Bound),
    variables_are_bound(Variables, Bound).

unbound_variables([], _, []).
unbound_variables([Variable | Variables], Bound, Unbound) :-
    (   memberchk(Variable, Bound)
    ->  Unbound = Rest
    ;   Unbound = [Variable | Rest]
    ),
    unbound_variables(Variables, Bound, Rest).

add_variables([], Bound, Bound).
add_variables([Variable | Variables], Bound0, Bound) :-
    (   memberchk(Variable, Bound0)
    ->  Bound1 = Bound0
    ;   Bound1 = [Variable | Bound0]
    ),
    add_variables(Variables, Bound1, Bound).

unlocated_mode_diagnostic(goal_failure(_, Reason),
                          diagnostic(check, none, Reason)).

mode_failure_diagnostics([], _, _, []).
mode_failure_diagnostics([goal_failure(GoalIndex, Reason) | Failures],
                         RuleIndex, Origins,
                         [diagnostic(check, NodeId, Reason) | Diagnostics]) :-
    goal_origin(Origins, RuleIndex, GoalIndex, NodeId),
    mode_failure_diagnostics(Failures, RuleIndex, Origins, Diagnostics).

head_variables(call(_, Arguments), Variables) :-
    findall(Identity,
            ( member(Argument, Arguments),
              argument_variables(Argument, ArgumentVariables),
              member(Identity, ArgumentVariables)
            ),
            Variables0),
    sort(Variables0, Variables).

%% resolve_call(+Call, +Edges, +Nodes, +Relations, -Result) is det.
%
% Result is ok(call(ref(Relation), ResolvedArgs)) or error(Reason).
resolve_call(call(name(Owner, Name), Args), Edges, Nodes, Relations, Result) :-
    (   resolve_name(Owner, Name, Edges, Nodes, [], Target)
    ->  (   Target = ref(_)
        ->  (   memberchk(relation(Target, Arity, _), Relations)
            ->  length(Args, ObservedArity),
                (   ObservedArity =:= Arity
                ->  resolve_args(Args, Edges, Nodes, ArgsResult),
                    (   ArgsResult = ok(ResolvedArgs)
                    ->  Result = ok(call(Target, ResolvedArgs))
                    ;   Result = ArgsResult
                    )
                ;   Result = error(arity_mismatch(Name, Arity, ObservedArity))
                )
            ;   Result = error(undeclared_relation(Name))
            )
        ;   Result = error(not_relation(Name))
        )
    ;   Result = error(unresolved_name(Name))
    ).

resolve_args([], _, _, ok([])).
resolve_args([Arg | Rest], Edges, Nodes, Result) :-
    resolve_argument(Arg, Edges, Nodes, ArgumentResult),
    (   ArgumentResult = ok(Resolved)
    ->  resolve_args(Rest, Edges, Nodes, RestResult),
        (   RestResult = ok(RestResolved)
        ->  Result = ok([Resolved | RestResolved])
        ;   Result = RestResult
        )
    ;   Result = ArgumentResult
    ).

resolve_argument(name(Owner, Name), Edges, Nodes, Result) :-
    !,
    (   resolve_name(Owner, Name, Edges, Nodes, [], Resolved)
    ->  Result = ok(Resolved)
    ;   Result = error(unresolved_name(Name))
    ).
resolve_argument(aggregate(count, Expression), Edges, Nodes, Result) :-
    !,
    resolve_argument(Expression, Edges, Nodes, ExpressionResult),
    (   ExpressionResult = ok(ResolvedExpression)
    ->  Result = ok(aggregate(count, ResolvedExpression))
    ;   Result = ExpressionResult
    ).
resolve_argument(Argument, _, _, ok(Argument)).

%% Every head var(Identity) must occur in a positive body call.
head_safety_diagnostics(call(_, HeadArgs), Body, Origins, RuleIndex, Diags) :-
    head_variables(call(_, HeadArgs), HeadVars),
    head_safety_diagnostics_with_variables(HeadVars, Body, Origins, RuleIndex,
                                           Diags).

head_safety_diagnostics_with_variables(HeadVars, Body, Origins, RuleIndex,
                                       Diags) :-
    rule_origin(Origins, RuleIndex, NodeId),
    head_safety_from_variables(HeadVars, Body, NodeId, Diags).

head_safety_from_variables(HeadVars, Body, NodeId, Diags) :-
    findall(Var, (member(Goal, Body),
                  goal_variables(Goal, GoalVars),
                  member(Var, GoalVars)), BodyVars),
    unsafe_vars(HeadVars, BodyVars, NodeId, Diags).

%% goal_call(+CheckedGoal, -Polarity, -Call) is det.
goal_call(checked_goal(Polarity, Call), Polarity, Call).

%% goal_variables(+CheckedGoal, -VariableIdentities) is det.
goal_variables(Goal, Variables) :-
    goal_call(Goal, _, call(_, Arguments)),
    findall(Identity, member(var(Identity), Arguments), Variables).

%% goal_dependency(+HeadRef, +CheckedGoal, -Dependency) is det.
goal_dependency(HeadRef, Goal, depends(HeadRef, BodyRef, Polarity)) :-
    goal_call(Goal, Polarity, call(BodyRef, _)).

unsafe_vars([], _, _, []).
unsafe_vars([Var | Rest], BodyVars, NodeId, Diags) :-
    (   memberchk(Var, BodyVars)
    ->  Diags = RestDiags
    ;   Diags = [diagnostic(check, NodeId, unsafe_head_var(Var)) | RestDiags]
    ),
    unsafe_vars(Rest, BodyVars, NodeId, RestDiags).

%% One dependency row per rule; sort/2 keeps distinct tuples.
depends_rows([], []).
depends_rows([rule(call(HeadRef, _), Body) | Rest], Depends) :-
    body_refs(Body, HeadRef, OwnDeps),
    depends_rows(Rest, RestDeps),
    append(OwnDeps, RestDeps, Depends).

body_refs([], _, []).
body_refs([Goal | Rest], HeadRef, [Dependency | More]) :-
    goal_dependency(HeadRef, Goal, Dependency),
    body_refs(Rest, HeadRef, More).

%% One deterministic stratum row per declared relation. Relations without a
%% derived head remain at zero; the shared stratifier owns derived levels.
strata_rows(Relations, DerivedStrata, Strata) :-
    maplist(relation_stratum(DerivedStrata), Relations, Strata0),
    sort(Strata0, Strata).

relation_stratum(DerivedStrata, relation(Relation, _, _),
                 stratum(Relation, Level)) :-
    (   memberchk(stratum(Relation, DerivedLevel), DerivedStrata)
    ->  Level = DerivedLevel
    ;   Level = 0
    ).

relations_refs([], []).
relations_refs([relation(Target, Arity, KeySets) | Rest],
               [relation(ref(Target), Arity, KeySets) | Refs]) :-
    relations_refs(Rest, Refs).

edge_origin(Origins, Owner, Name, Index, NodeId) :-
    (   checker_origin_lookup_scope(arena(ArenaId))
    ->  arena_edge_origin(Owner, Name, Index, ArenaId, _, NodeId)
    ;   memberchk(origin(edge(Owner, Name, Index), NodeId), Origins)
    ),
    !.
edge_origin(_, _, _, _, none).

seed_origin(Origins, SeedIndex, NodeId) :-
    (   checker_origin_lookup_scope(arena(ArenaId))
    ->  arena_seed_origin(SeedIndex, ArenaId, _, NodeId)
    ;   memberchk(origin(seed(SeedIndex), NodeId), Origins)
    ),
    !.
seed_origin(_, _, none).

rule_origin(Origins, RuleIndex, NodeId) :-
    (   checker_origin_lookup_scope(arena(ArenaId))
    ->  arena_rule_origin(RuleIndex, ArenaId, _, NodeId)
    ;   memberchk(origin(rule(RuleIndex), NodeId), Origins)
    ),
    !.
rule_origin(_, _, none).

goal_origin(Origins, RuleIndex, GoalIndex, NodeId) :-
    (   checker_origin_lookup_scope(arena(ArenaId))
    ->  arena_goal_origin(RuleIndex, GoalIndex, ArenaId, _, NodeId)
    ;   memberchk(origin(goal(RuleIndex, GoalIndex), NodeId), Origins)
    ),
    !.
goal_origin(_, _, _, none).
