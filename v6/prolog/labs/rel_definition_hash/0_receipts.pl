% 0_receipts.pl : canonical relation-definition and specialization-cache lab.
%
% This file is isolated from production.  It reads the current expanded
% prog/2 + plan/6 terms and proves which identity layers are required.
%
% Run:
%   swipl -q -l v6/prolog/labs/rel_definition_hash/0_receipts.pl -g go -g halt

:- use_module('../../compile',
              [read_fixture_term/4, program_plan/2]).
:- use_module('../../lower', [lower_program/2]).
:- use_module('../../analyze',
              [body_ref_uses/2, rule_head_ref/2,
               rule_is_edge/1, rule_is_level/1]).
:- use_module('../../compile/registry',
              [body_surface_for_term/6]).
:- use_module('../../1_host_expand', [prepare_program/5]).
:- use_module('../../0_graph',
              [graph_from_edges/3, graph_components/2, graph_component_of/3]).
:- use_module(library(crypto), [crypto_data_hash/3]).
:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

go :-
    alpha_equivalence_receipt,
    relation_rename_receipt,
    column_name_receipt,
    ordered_rule_receipt,
    ordered_body_receipt,
    match_expansion_receipt,
    host_contract_receipt,
    recursive_scc_receipt,
    identity_layer_receipt,
    specialization_cache_receipt,
    lowered_sql_template_receipt,
    format("11 PASS~n").

% ── current-world model ────────────────────────────────────────────────────

make_model(Name, SurfaceProgram, Initial, Schedule, Bindings,
           model(Program, RelPlans, ArrivalTargets, HostPlans)) :-
    prepare_program(SurfaceProgram, _, HostPlans, _, _),
    program_plan(
        fixture(Name, SurfaceProgram, Initial, Schedule, [])-Bindings,
        plan(_, Program, RelPlans, ArrivalTargets, _, _, _)).

fixture_model(File, Name, Model) :-
    read_fixture_term(File, Name, Term, Bindings),
    Term = fixture(Name, SurfaceProgram, Initial, Schedule, _),
    make_model(Name, SurfaceProgram, Initial, Schedule, Bindings, Model).

model_refs(model(_, RelPlans, _, _), Refs) :-
    findall(Ref, member(relplan(Ref, _, _, _, _), RelPlans), Refs0),
    sort(Refs0, Refs).

relation_rules(model(prog(_, Rules), _, _, _), Ref, RefRules) :-
    include(rule_heads(Ref), Rules, RefRules).

rule_heads(Ref, Rule) :- rule_head_ref(Rule, Ref).

relation_plane(Model, Ref, Plane) :-
    relation_rules(Model, Ref, Rules),
    findall(Mode,
            ( member(Rule, Rules),
              ( rule_is_level(Rule) -> Mode = level
              ; rule_is_edge(Rule)  -> Mode = edge
              )
            ),
            Modes0),
    sort(Modes0, Modes),
    ( Modes = [] ->
        host_role(Model, Ref, HostRole),
        ( HostRole == none -> Plane = input ; Plane = HostRole )
    ; Modes = [Only] -> Plane = Only
    ; Plane = mixed(Modes)
    ).

relation_clock(Model, Ref, Clock) :-
    relation_plane(Model, Ref, Plane),
    plane_clock(Plane, Clock).

plane_clock(input, arrival(0)).
plane_clock(host_response, async_arrival(at_least(1))).
plane_clock(host_demand, derive(0)).
plane_clock(level, derive(0)).
plane_clock(edge, boundary_write(observed_at(1))).
plane_clock(mixed(Modes), mixed(Clocks)) :-
    maplist(plane_clock, Modes, Clocks).

relation_shape_term(
    Model, Ref,
    shape(kind(Kind), columns(Columns, Types), key(Key),
          plane(Plane), clock(Clock))) :-
    Model = model(_, RelPlans, _, _),
    memberchk(relplan(Ref, Kind, Columns, Key, Types), RelPlans),
    relation_plane(Model, Ref, Plane),
    relation_clock(Model, Ref, Clock).

shape_hash(Model, Ref, Hash) :-
    relation_shape_term(Model, Ref, Shape),
    hash_term(Shape, Hash).

host_role(model(_, RelPlans, _, HostPlans), Ref, Role) :-
    member(relplan(Ref, _, _, _, _), RelPlans),
    Ref = Name/_,
    ( member(host_plan(_, _, _, _, demand_ref(Name), _, _), HostPlans)
    -> Role = host_demand
    ; member(host_plan(_, _, _, _, _, response_ref(Name), _), HostPlans)
    -> Role = host_response
    ; Role = none
    ).

host_contract(Model, Ref, Contract) :-
    Model = model(_, _, _, HostPlans),
    Ref = Name/_,
    ( member(host_plan(_, Inputs, Outputs, template(Template),
                       demand_ref(Name), _, _), HostPlans)
    -> Contract = host_contract(demand, Inputs, Outputs, template(Template))
    ; member(host_plan(_, Inputs, Outputs, template(Template),
                       _, response_ref(Name), _), HostPlans)
    -> Contract = host_contract(response, Inputs, Outputs, template(Template))
    ; Contract = none
    ).

host_name(model(_, _, _, HostPlans), Name) :-
    member(host_plan(Name, _, _, _, _, _, _), HostPlans).

% ── exact canonical term encoding ──────────────────────────────────────────

hash_term(Term, Hash) :-
    canonical_text(Term, Text),
    crypto_data_hash(Text, Hash, [algorithm(sha256), encoding(utf8)]).

canonical_text(Term, Text) :-
    copy_term(Term, Copy),
    numbervars(Copy, 0, _, [singletons(true)]),
    with_output_to(
        string(Text),
        write_term(Copy,
                   [ quoted(true),
                     numbervars(true),
                     ignore_ops(true),
                     spacing(standard)
                   ])).

% Direct definition identity keeps current rule and conjunction order.
% Relation symbols are slots.  Variable spelling disappears through
% numbervars/4.  Dependency implementation enters only the closure hash.
direct_definition_term(Model, Ref,
                       direct(shape(ShapeHash),
                              rules(NormalizedRules),
                              dependencies(DependencyShapes),
                              host(HostContract))) :-
    shape_hash(Model, Ref, ShapeHash),
    relation_rules(Model, Ref, Rules),
    ordered_dependency_refs(Rules, DependencyRefs),
    dependency_slots(DependencyRefs, 1, DependencySlots),
    append([Ref-self], DependencySlots, RefSlots),
    maplist(normalize_rule(Model, RefSlots), Rules, NormalizedRules),
    maplist(dependency_shape(Model), DependencySlots, DependencyShapes),
    host_contract(Model, Ref, HostContract).

direct_definition_hash(Model, Ref, Hash) :-
    direct_definition_term(Model, Ref, Term),
    hash_term(Term, Hash).

dependency_shape(Model, Ref-Slot, dep(Slot, ShapeHash)) :-
    shape_hash(Model, Ref, ShapeHash).

ordered_dependency_refs(Rules, Refs) :-
    foldl(add_rule_dependencies, Rules, [], Refs).

add_rule_dependencies(Rule, Refs0, Refs) :-
    rule_body(Rule, Body),
    body_ref_uses(Body, Uses),
    foldl(add_use_ref, Uses, Refs0, Refs).

add_use_ref(use(Ref, _, _, _), Refs0, Refs) :-
    ( memberchk(Ref, Refs0) -> Refs = Refs0 ; append(Refs0, [Ref], Refs) ).

dependency_slots([], _, []).
dependency_slots([Ref | Rest], Index, [Ref-dep(Index) | Slots]) :-
    Next is Index + 1,
    dependency_slots(Rest, Next, Slots).

rule_body((_ <- Body), Body).
rule_body((_ <+ Body), Body).

normalize_rule(Model, RefSlots, Rule,
               rule(Arrow, head(HeadSlot, HeadArgs), BodyNorm)) :-
    ( Rule = (Head <- Body) -> Arrow = level
    ; Rule = (Head <+ Body) -> Arrow = edge
    ),
    normalize_relation_atom(Model, RefSlots, Head, HeadSlot, HeadArgs),
    normalize_body(Model, RefSlots, Body, BodyNorm0),
    copy_term(rule(Arrow, head(HeadSlot, HeadArgs), BodyNorm0), Normalized),
    numbervars(Normalized, 0, _, [singletons(true)]),
    Normalized = rule(Arrow, head(HeadSlot, HeadArgs), BodyNorm).

normalize_body(_, _, true, true) :- !.
normalize_body(Model, Slots, (Left, Right), and(LeftNorm, RightNorm)) :-
    !,
    normalize_body(Model, Slots, Left, LeftNorm),
    normalize_body(Model, Slots, Right, RightNorm).
normalize_body(Model, Slots, not(Body), not(BodyNorm)) :-
    !,
    normalize_body(Model, Slots, Body, BodyNorm).
normalize_body(Model, Slots, Term, surface(Functor, ArgsNorm)) :-
    nonvar(Term),
    body_surface_for_term(Term, _, _, AnalyzeRole, _, _),
    !,
    functor(Term, Functor, _),
    Term =.. [_ | Args],
    normalize_surface_args(Model, Slots, AnalyzeRole, Args, ArgsNorm).
normalize_body(Model, Slots, Atom, rel(Slot, Args)) :-
    normalize_relation_atom(Model, Slots, Atom, Slot, Args).

normalize_surface_args(Model, Slots, refs_of_arg(Index, _, _),
                       Args, Normalized) :-
    !,
    normalize_arg_at(Index, Model, Slots, Args, Normalized).
normalize_surface_args(Model, Slots, splice_bare, Args, Normalized) :-
    !,
    maplist(normalize_maybe_relation(Model, Slots), Args, Normalized).
normalize_surface_args(Model, Slots, arm(neg), Args, Normalized) :-
    !,
    maplist(normalize_maybe_body(Model, Slots), Args, Normalized).
normalize_surface_args(Model, _Slots, _, Args, Normalized) :-
    maplist(normalize_expression(Model), Args, Normalized).

normalize_arg_at(Index, Model, Slots, Args, Normalized) :-
    same_length(Args, Normalized),
    nth1(Index, Args, RelArg),
    nth1(Index, Normalized, RelNorm),
    normalize_maybe_relation(Model, Slots, RelArg, RelNorm),
    normalize_other_args(Args, Normalized, Index, 1, Model).

normalize_other_args([], [], _, _, _).
normalize_other_args([_ | Args], [_ | Norms], Skip, Skip, Model) :-
    !,
    Next is Skip + 1,
    normalize_other_args(Args, Norms, Skip, Next, Model).
normalize_other_args([Arg | Args], [Norm | Norms], Skip, Index, Model) :-
    normalize_expression(Model, Arg, Norm),
    Next is Index + 1,
    normalize_other_args(Args, Norms, Skip, Next, Model).

normalize_maybe_body(Model, Slots, Arg, body(Norm)) :-
    nonvar(Arg),
    ( Arg = (_, _) ; Arg = not(_) ),
    !,
    normalize_body(Model, Slots, Arg, Norm).
normalize_maybe_body(Model, _, Arg, expr(Norm)) :-
    normalize_expression(Model, Arg, Norm).

normalize_maybe_relation(Model, Slots, Atom, rel(Slot, Args)) :-
    callable(Atom),
    functor(Atom, Name, Arity),
    memberchk(Name/Arity-Slot, Slots),
    !,
    normalize_relation_atom(Model, Slots, Atom, Slot, Args).
normalize_maybe_relation(Model, _, Arg, expr(Norm)) :-
    normalize_expression(Model, Arg, Norm).

normalize_relation_atom(Model, Slots, Atom, Slot, ArgsNorm) :-
    Atom =.. [Name | Args],
    length(Args, Arity),
    Ref = Name/Arity,
    memberchk(Ref-Slot, Slots),
    maplist(normalize_expression(Model), Args, ArgsNorm).

normalize_expression(_, Value, Value) :- var(Value), !.
normalize_expression(Model, Value, host_digest(Role)) :-
    string(Value),
    host_name(Model, HostName),
    format(string(Value), "~w|~w", [Role, HostName]),
    memberchk(Role, [identity, witness]),
    !.
normalize_expression(_, Value, literal(Value)) :- atomic(Value), !.
normalize_expression(Model, Term, term(Functor, ArgsNorm)) :-
    Term =.. [Functor | Args],
    maplist(normalize_expression(Model), Args, ArgsNorm).

% ── dependency closure and recursive SCCs ──────────────────────────────────

relation_dependencies(Model, Ref, Dependencies) :-
    relation_rules(Model, Ref, Rules),
    ordered_dependency_refs(Rules, Dependencies0),
    model_refs(Model, ModelRefs),
    include(member_of(ModelRefs), Dependencies0, Dependencies).

member_of(List, Item) :- memberchk(Item, List).

% This lab used to carry its own copy of the all-pairs mutual-reachability
% search that cost the compiler's plan phase 255 s (the copy in
% compile/3_clock_check.pl). Both now read 0_graph.pl. The semantics kept:
% the old reachable/3's first clause was reflexive, so every ref landed in an
% SCC even with no cycle, which is graph_components/2's partition rather than
% graph_cyclic_components/2's cyclic-only subset.
model_graph(Model, Graph) :-
    model_refs(Model, Refs),
    findall(Ref-Dependency,
            ( member(Ref, Refs),
              relation_dependencies(Model, Ref, Dependencies),
              member(Dependency, Dependencies) ),
            Edges),
    graph_from_edges(Refs, Edges, Graph).

relation_scc(Model, Ref, Scc) :-
    model_graph(Model, Graph),
    graph_components(Graph, Components),
    graph_component_of(Components, Ref, Scc).

closure_hash(Model, Ref, Hash) :-
    relation_scc(Model, Ref, Scc),
    scc_hash(Model, Scc, Hash).

scc_hash(Model, Scc, Hash) :-
    findall(Text-Term,
            ( permutation(Scc, OrderedMembers),
              scc_representation(Model, OrderedMembers, Term),
              canonical_text(Term, Text)
            ),
            Candidates),
    keysort(Candidates, [_-Canonical | _]),
    hash_term(Canonical, Hash).

scc_representation(Model, OrderedMembers, scc(NodeTerms)) :-
    node_slots(OrderedMembers, 1, NodeSlots),
    maplist(scc_node_term(Model, NodeSlots), OrderedMembers, NodeTerms).

node_slots([], _, []).
node_slots([Ref | Rest], Index, [Ref-node(Index) | Slots]) :-
    Next is Index + 1,
    node_slots(Rest, Next, Slots).

scc_node_term(Model, InternalSlots, Ref,
              node(Node,
                   shape(Shape),
                   rules(NormalizedRules),
                   host(HostContract))) :-
    memberchk(Ref-Node, InternalSlots),
    relation_shape_term(Model, Ref, Shape),
    relation_rules(Model, Ref, Rules),
    relation_dependencies(Model, Ref, Dependencies),
    external_slots(Model, InternalSlots, Dependencies, ExternalSlots),
    append(InternalSlots, ExternalSlots, Slots),
    maplist(normalize_rule(Model, Slots), Rules, NormalizedRules),
    host_contract(Model, Ref, HostContract).

external_slots(Model, InternalSlots, Dependencies, ExternalSlots) :-
    exclude(internal_ref(InternalSlots), Dependencies, ExternalRefs0),
    sort(ExternalRefs0, ExternalRefs),
    maplist(external_slot(Model), ExternalRefs, ExternalSlots).

internal_ref(Slots, Ref) :- memberchk(Ref-_, Slots).

external_slot(Model, Ref, Ref-external(ClosureHash)) :-
    closure_hash(Model, Ref, ClosureHash).

% ── specialization keys ────────────────────────────────────────────────────

specialization_code_key(Algorithm, Version, ArgumentHashes, Key) :-
    hash_term(code(Algorithm, Version, ArgumentHashes), Key).

specialization_instance_key(CodeKey, StorageBindings, HostBindings, Key) :-
    hash_term(instance(CodeKey, StorageBindings, HostBindings), Key).

% ── receipts ───────────────────────────────────────────────────────────────

typed_edge_program(Input, State, Operation, Program) :-
    InputRef = Input/2,
    StateRef = State/2,
    In =.. [Input, Key, Value],
    Out =.. [State, Key, Next],
    edge_expression(Operation, Value, Expr),
    Program = prog(
        [ col_type(InputRef, key, text),
          col_type(InputRef, value, int),
          kind(InputRef, log),
          keep(InputRef, all),
          col_type(StateRef, key, text),
          col_type(StateRef, value, int),
          keyed(StateRef, [1])
        ],
        [(Out <+ In, Next := Expr)]).

edge_expression(identity, Value, Value).
edge_expression(increment, Value, Value + 1).

alpha_equivalence_receipt :-
    ProgramA = prog(
        [ col_type(source/2, key, text),
          col_type(source/2, value, int),
          col_type(total/2, key, text),
          col_type(total/2, value, int)
        ],
        [(total(Key, Next) <- source(Key, Value), Next := Value + 1)]),
    ProgramB = prog(
        [ col_type(source/2, key, text),
          col_type(source/2, value, int),
          col_type(total/2, key, text),
          col_type(total/2, value, int)
        ],
        [(total(OtherKey, Result) <-
             source(OtherKey, Number), Result := Number + 1)]),
    make_model(alpha_a, ProgramA, [], [], [], ModelA),
    make_model(alpha_b, ProgramB, [], [], [], ModelB),
    direct_definition_hash(ModelA, total/2, Hash),
    direct_definition_hash(ModelB, total/2, Hash),
    format("PASS variable alpha-equivalence~n").

relation_rename_receipt :-
    typed_edge_program(input_a, state_a, identity, ProgramA),
    typed_edge_program(input_b, state_b, identity, ProgramB),
    make_model(rename_a, ProgramA, [], [], [], ModelA),
    make_model(rename_b, ProgramB, [], [], [], ModelB),
    shape_hash(ModelA, state_a/2, Shape),
    shape_hash(ModelB, state_b/2, Shape),
    direct_definition_hash(ModelA, state_a/2, Direct),
    direct_definition_hash(ModelB, state_b/2, Direct),
    closure_hash(ModelA, state_a/2, Closure),
    closure_hash(ModelB, state_b/2, Closure),
    format("PASS systematic relation rename preserves content hashes~n").

column_name_receipt :-
    typed_edge_program(input_a, state_a, identity, ProgramA),
    ProgramB = prog(
        [ col_type(input_b/2, owner, text),
          col_type(input_b/2, amount, int),
          kind(input_b/2, log),
          keep(input_b/2, all),
          col_type(state_b/2, owner, text),
          col_type(state_b/2, amount, int),
          keyed(state_b/2, [1])
        ],
        [(state_b(Key, Next) <+ input_b(Key, Value), Next := Value)]),
    make_model(column_a, ProgramA, [], [], [], ModelA),
    make_model(column_b, ProgramB, [], [], [], ModelB),
    shape_hash(ModelA, state_a/2, HashA),
    shape_hash(ModelB, state_b/2, HashB),
    HashA \== HashB,
    format("PASS declared column rename changes shape hash~n").

ordered_rule_receipt :-
    Decls = [ col_type(source/1, value, int),
              col_type(out/1, value, int) ],
    Rule1 = (out(Value) <- source(Value), Value > 0),
    Rule2 = (out(Value) <- source(Value), Value < 10),
    make_model(rule_order_a, prog(Decls, [Rule1, Rule2]), [], [], [], ModelA),
    make_model(rule_order_b, prog(Decls, [Rule2, Rule1]), [], [], [], ModelB),
    direct_definition_hash(ModelA, out/1, HashA),
    direct_definition_hash(ModelB, out/1, HashB),
    HashA \== HashB,
    format("PASS source rule order remains in exact-code hash~n").

ordered_body_receipt :-
    Decls = [ col_type(left/2, key, text),
              col_type(left/2, value, int),
              col_type(right/2, key, text),
              col_type(right/2, value, int),
              col_type(out/2, key, text),
              col_type(out/2, value, int) ],
    ProgramA = prog(
        Decls,
        [(out(Key, Value) <- left(Key, Value), right(Key, Value))]),
    ProgramB = prog(
        Decls,
        [(out(Key, Value) <- right(Key, Value), left(Key, Value))]),
    make_model(body_order_a, ProgramA, [], [], [], ModelA),
    make_model(body_order_b, ProgramB, [], [], [], ModelB),
    direct_definition_hash(ModelA, out/2, HashA),
    direct_definition_hash(ModelB, out/2, HashB),
    HashA \== HashB,
    format("PASS conjunction order remains in exact-code hash~n").

match_expansion_receipt :-
    File = 'v6/prolog/conformance/fixtures/1_match_block.pl',
    fixture_model(File, match_classify_response, Sugared),
    fixture_model(File, match_classify_response_desugared, Desugared),
    forall(
        member(Ref,
               [fetch_result_fresh/3,
                fetch_result_unchanged/1,
                fetch_result_error/2]),
        ( direct_definition_hash(Sugared, Ref, Hash),
          direct_definition_hash(Desugared, Ref, Hash),
          closure_hash(Sugared, Ref, Closure),
          closure_hash(Desugared, Ref, Closure)
        )),
    format("PASS match hashes after expansion equal handwritten rules~n").

host_program(HostName, Template, Program) :-
    Probe =.. [probe, HostName, [Endpoint], [Status], []],
    Program = program(
        [ col_type(source/1, endpoint, text),
          col_type(result/2, endpoint, text),
          col_type(result/2, status, int),
          sh_decl(HostName,
                  [col(endpoint, text)],
                  [col(status, int)],
                  template(Template))
        ],
        [(result(Endpoint, Status) <- source(Endpoint), Probe)],
        []).

host_response_ref(model(_, RelPlans, _, _), Ref) :-
    member(relplan(Ref, _, _, _, _), RelPlans),
    Ref = Name/_,
    sub_atom(Name, 0, _, _, '__host_response_').

host_contract_receipt :-
    host_program(fetch, "run {endpoint}", ProgramA),
    host_program(load, "run {endpoint}", ProgramB),
    host_program(load, "other {endpoint}", ProgramC),
    make_model(host_a, ProgramA, [], [], [], ModelA),
    make_model(host_b, ProgramB, [], [], [], ModelB),
    make_model(host_c, ProgramC, [], [], [], ModelC),
    host_response_ref(ModelA, RefA),
    host_response_ref(ModelB, RefB),
    host_response_ref(ModelC, RefC),
    direct_definition_hash(ModelA, RefA, Hash),
    direct_definition_hash(ModelB, RefB, Hash),
    direct_definition_hash(ModelC, RefC, OtherHash),
    Hash \== OtherHash,
    format("PASS generated host names normalize; template bytes invalidate~n").

recursive_program(A, B, Guarded, Program) :-
    ARef = A/1,
    BRef = B/1,
    AHead =.. [A, X],
    AFromB =.. [B, X],
    BHead =.. [B, X],
    BFromA =.. [A, X],
    Seed = seed(X),
    ( Guarded == true
    -> BBody = (BFromA, X > 0)
    ; BBody = BFromA
    ),
    Program = prog(
        [ col_type(seed/1, value, int),
          col_type(ARef, value, int),
          col_type(BRef, value, int)
        ],
        [ (AHead <- Seed),
          (AHead <- AFromB),
          (BHead <- BBody)
        ]).

recursive_scc_receipt :-
    recursive_program(a, b, false, ProgramA),
    recursive_program(x, y, false, ProgramB),
    recursive_program(x, y, true, ProgramC),
    recursive_model(a, b, ProgramA, ModelA),
    recursive_model(x, y, ProgramB, ModelB),
    recursive_model(x, y, ProgramC, ModelC),
    closure_hash(ModelA, a/1, Hash),
    closure_hash(ModelB, x/1, Hash),
    closure_hash(ModelA, b/1, Hash),
    closure_hash(ModelB, y/1, Hash),
    closure_hash(ModelC, x/1, Changed),
    Hash \== Changed,
    format("PASS recursive SCC hash survives rename and sees edge/body change~n").

recursive_model(A, B, Program,
                model(Program,
                      [ relplan(seed/1, set, [value], none, [int]),
                        relplan(A/1, set, [value], none, [int]),
                        relplan(B/1, set, [value], none, [int])
                      ],
                      [seed/1],
                      [])).

identity_layer_receipt :-
    typed_edge_program(input_a, state_a, identity, ProgramA),
    typed_edge_program(input_a, state_a, increment, ProgramB),
    make_model(identity_a, ProgramA, [], [], [], ModelA),
    make_model(identity_b, ProgramB, [], [], [], ModelB),
    shape_hash(ModelA, state_a/2, LayoutHash),
    shape_hash(ModelB, state_a/2, LayoutHash),
    closure_hash(ModelA, state_a/2, SemanticA),
    closure_hash(ModelB, state_a/2, SemanticB),
    SemanticA \== SemanticB,
    StorageIdentity = state_a/2,
    nonvar(StorageIdentity),
    format("PASS layout, program semantics, and stable storage identity separate~n").

specialization_cache_receipt :-
    maplist(
        specialization_call,
        [ scan(site_a, add_v1),
          scan(site_b, add_v1),
          scan(site_c, add_v2),
          switch(site_d, fetch_v1),
          switch(site_e, fetch_v1),
          switch(site_f, fetch_v1)
        ],
        CodeKeys,
        InstanceKeys),
    sort(CodeKeys, UniqueCodeKeys),
    sort(InstanceKeys, UniqueInstanceKeys),
    length(CodeKeys, 6),
    length(UniqueCodeKeys, 3),
    length(UniqueInstanceKeys, 6),
    format("PASS 6 calls -> 3 code templates, 6 state/storage instances~n").

specialization_call(scan(Site, ReducerVersion), CodeKey, InstanceKey) :-
    specialization_code_key(
        ordered_scan, 1,
        [event_shape(key_text_value_int),
         state_shape(key_text_value_int),
         reducer(ReducerVersion)],
        CodeKey),
    specialization_instance_key(
        CodeKey, [state_table(Site)], [], InstanceKey).
specialization_call(switch(Site, HostVersion), CodeKey, InstanceKey) :-
    specialization_code_key(
        switch_map, 1,
        [outer_shape(key_text_value_text),
         scope_shape(key_text_value_text),
         host_contract(HostVersion)],
        CodeKey),
    specialization_instance_key(
        CodeKey, [scope_table(Site)], [host_binding(Site)], InstanceKey).

lowered_payload_hash(ProgramName, Program, RawHash, AbstractHash,
                     Replacements) :-
    make_model(ProgramName, Program, [], [], [], Model),
    Model = model(Prog, RelPlans, ArrivalTargets, _),
    program_plan(
        fixture(ProgramName, Program, [], [], [])-[],
        Plan),
    Plan = plan(_, Prog, RelPlans, ArrivalTargets, _, _, _),
    lower_program(Plan, Lowered),
    Lowered =.. [lowered, _ | Fields],
    Payload =.. [lowered | Fields],
    canonical_text(Payload, RawText),
    crypto_data_hash(RawText, RawHash,
                     [algorithm(sha256), encoding(utf8)]),
    foldl(replace_pair, Replacements, RawText, AbstractText),
    crypto_data_hash(AbstractText, AbstractHash,
                     [algorithm(sha256), encoding(utf8)]).

replace_pair(From-To, Input, Output) :-
    replace_all(Input, From, To, Output).

replace_all(Input, Needle, Replacement, Output) :-
    ( sub_string(Input, Before, _, After, Needle)
    -> sub_string(Input, 0, Before, _, Prefix),
       string_length(Needle, NeedleLength),
       Start is Before + NeedleLength,
       sub_string(Input, Start, After, 0, Tail),
       replace_all(Tail, Needle, Replacement, ReplacedTail),
       string_concat(Prefix, Replacement, First),
       string_concat(First, ReplacedTail, Output)
    ; Output = Input
    ).

lowered_sql_template_receipt :-
    typed_edge_program(incoming_left, state_left, identity, ProgramA),
    typed_edge_program(incoming_right, state_right, identity, ProgramB),
    lowered_payload_hash(
        sql_a, ProgramA, RawA, Abstract,
        ["incoming_left"-"$input", "state_left"-"$state"]),
    lowered_payload_hash(
        sql_b, ProgramB, RawB, Abstract,
        ["incoming_right"-"$input", "state_right"-"$state"]),
    RawA \== RawB,
    format("PASS renamed programs share abstract lowered SQL, raw SQL stays bound~n").
