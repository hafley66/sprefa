% Emit lowered compiler plans as Rust source modules for the
% v6/sprefa-engine-rs runtime crate.
%
% emit_program/5 is substitutable for emit_ts:emit_program/5 with no call-site
% special case: same lowered/8 destructure, same output contract. The Text it
% produces is a Rust file carrying the program's IGenProgram data as one
% ProgramJson document (library(http/json_write) serializes a built dict), the
% same "compiler emits JSON, runtime parses it" seam dd-runner already buys.
% Only the tick log is byte-diffed; program JSON whitespace is irrelevant.
:- module(emit_rust,
          [ emit_program/5 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(json)).
:- use_module(library(pcre)).
:- use_module(lower, [ departure_frontier_table_name/2,
                       program_text_intern_plan/3,
                       struct_type_plans/3, fixpoint_round_cap/1 ]).
:- use_module(strat, [cyclic_head_groups/2]).
:- use_module('0_rel_record').
:- use_module(analyze, [ body_ref_uses/2, level_body_pre_ref/2, rule_head_ref/2,
                         listened_departure_refs/2, program_uses_tick/2 ]).
:- use_module('1_host_expand', [compile_host_decl/2,
                                host_plan_contract/2]).
:- use_module('compile/registry', [host_execution/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ helpers ═════════════════════════════════════════════════════════════════

ref_name(Ref, Name) :- ( Ref = _/_ -> arg(1, Ref, Name) ; atom_string(Name, Ref) ).

lines_block(Lines, Text) :- atomic_list_concat(Lines, '\n', Text).

position_index(Position, Index) :- Index is Position - 1.

json_write_string(Value, Atom) :-
    with_output_to(string(S), json_write_dict(current_output, Value, [width(0)])),
    atom_string(Atom, S).

raw_string_hashes(Payload, Hashes) :-
    re_foldl(longest_hash_match, '"(?<hashes>#+)', Payload, 0, Longest,
             [capture_type(range)]),
    HashCount is Longest + 1,
    length(HashCodes, HashCount),
    maplist(=(0'#), HashCodes),
    atom_codes(Hashes, HashCodes).

longest_hash_match(Match, Longest0, Longest) :-
    Match.hashes = _-RunLength,
    Longest is max(Longest0, RunLength).

% Name-Value pairs to a dict (JSON object), for the map-shaped fields.
pairs_to_dict([], _{}) :- !.
pairs_to_dict(Pairs, Dict) :-
    foldl(add_pair, Pairs, _{}, Dict).
add_pair(Name-Value, Acc, Out) :- Out = Acc.put(Name, Value).

% ═══ plan accessors ══════════════════════════════════════════════════════════

plan_intern_mode(plan(_, _, _, _, _, _, _, _, InternMode), InternMode).

reconcile_every_tick(plan(_, prog(_, Rules), _, _, _, _, _, _, _), Reconcile) :-
    ( member(Rule, Rules),
      Rule = (_ <- Body),
      body_ref_uses(Body, Uses),
      member(use(_, _, neg, _), Uses)
    -> Reconcile = true
    ;  Reconcile = false
    ).

% ═══ section builders: each returns a JSON-able value (dict / list / atom) ═══

map_from(RelPlans, Getter, Map) :-
    findall(Name-Value,
            ( member(Rel, RelPlans),
              relplan_parts(Rel, Ref, _K, _Cols, _Key, _Types),
              ref_name(Ref, Name),
              call(Getter, Rel, Value) ),
            Pairs),
    pairs_to_dict(Pairs, Map).

rel_columns_of(Rel, Columns) :- relplan_parts(Rel, _Ref, _K, Columns, _Key, _T).
rel_column_types_of(Rel, Types) :-
    relplan_parts(Rel, _Ref, _K, _Cols, _Key, ColumnTypes),
    maplist(boundary_type_name, ColumnTypes, Types).

boundary_type_name(ref(_), ref) :- !.
boundary_type_name(json, json) :- !.
boundary_type_name(json_list(_), json) :- !.
% F3 mirror: Vec<Value> at the row seam, the same name on both doors.
boundary_type_name(list(_), list) :- !.
boundary_type_name(bytes, bytes) :- !.
boundary_type_name(T, T).

boot_dict(bootstmt(Rel, Sql, Params), _{rel: Rel, sql: Sql, params: JsonParams}) :-
    maplist(boot_param, Params, JsonParams).

% A text param stays a JSON string even when it spells `true`/`false`/`null`,
% which json_write_dict would otherwise emit as the bare JSON literal.
boot_param(bool_lit(Boolean), Boolean) :- !.
boot_param(Param, Param) :- number(Param), !.
boot_param(Param, Text) :- atom_string(Param, Text).

final_select_entry(deltastmt(Ref, Sql, _, _, _), Name-Sql) :- ref_name(Ref, Name).

final_select_map(DeltaStatements, Map) :-
    maplist(final_select_entry, DeltaStatements, Pairs),
    pairs_to_dict(Pairs, Map).

arrival_tpl(Ref, ArrivalStatements, Tpl) :-
    memberchk(arrivalstmt(Ref, Kind, AddSql, DelSql, _, _), ArrivalStatements),
    ( DelSql == none -> DelText = null ; DelText = DelSql ),
    Tpl = _{kind: Kind, add_sql: AddSql, del_sql: DelText}.

arrival_templates_map(ArrivalStatements, Map) :-
    findall(Name-Tpl,
            ( member(arrivalstmt(Ref, _, _, _, _, _), ArrivalStatements),
              ref_name(Ref, Name),
              arrival_tpl(Ref, ArrivalStatements, Tpl) ),
            Pairs),
    pairs_to_dict(Pairs, Map).

relation_dict(RelPlans, ArrivalStatements, DepartureRefs,
              deltastmt(Ref, _Sel, DeltaTable, BoundarySql, _Stored), Dict) :-
    ref_name(Ref, Name),
    relplan_shape(RelPlans, Ref, Kind, Columns, KeyOrNone, RawColumnTypes),
    maplist(boundary_type_name, RawColumnTypes, ColumnTypes),
    ( KeyOrNone = key(KeyPositions) -> maplist(position_index, KeyPositions, KeyIndices)
    ; KeyIndices = []
    ),
    ( memberchk(arrivalstmt(Ref, _, _, _, AddSql, _), ArrivalStatements)
    -> AddText = AddSql ; AddText = null ),
    ( memberchk(arrivalstmt(Ref, _, _, _, _, DelSql), ArrivalStatements),
      DelSql \== none
    -> DelText = DelSql ; DelText = null ),
    format(atom(FrontierTable), '__frontier_~w', [Name]),
    format(atom(NextFrontierTable), '__next_frontier_~w', [Name]),
    ( memberchk(Ref, DepartureRefs)
    -> departure_frontier_table_name(Ref, DepartureTable), DepField = DepartureTable
    ; DepField = null ),
    Dict = _{ rel: Name, kind: Kind, table_name: Name,
              delta_table_name: DeltaTable,
              frontier_table_name: FrontierTable,
              next_frontier_table_name: NextFrontierTable,
              departure_frontier_table_name: DepField,
              columns: Columns, column_types: ColumnTypes,
              key_indices: KeyIndices,
              arrival_add_sql: AddText, arrival_del_sql: DelText,
              boundary_sql: BoundarySql }.

relations_list(RelPlans, ArrivalStatements, DepartureRefs, DeltaStatements, Dicts) :-
    maplist(relation_dict(RelPlans, ArrivalStatements, DepartureRefs),
            DeltaStatements, Dicts).

edge_dict(RelPlans,
          edgestmt(HeadRef, _Trigger, HeadColumns, KeyColumns, _Proj, _Write,
                   DeltaProjectSql, _Kind, edgeinterns(_, DeltaInternSqls)),
          Dict) :-
    ref_name(HeadRef, HeadName),
    relplan_shape(RelPlans, HeadRef, HeadKind, _Columns, _Key, _Types),
    format(atom(DeltaTable), '__delta_~w', [HeadName]),
    head_to_key_indices(HeadColumns, KeyColumns, KeyIndices),
    intern_field(DeltaInternSqls, InternField),
    Dict = _{ head_rel: HeadName, head_columns: HeadColumns,
              head_table_name: HeadName, head_delta_table_name: DeltaTable, head_kind: HeadKind,
              key_indices: KeyIndices, project_sql: DeltaProjectSql,
              intern_sql: InternField }.

intern_field([], null) :- !.
intern_field(InternSqls, InternSqls).
head_to_key_indices(HeadColumns, KeyColumns, Indices) :-
    maplist(key_index(HeadColumns), KeyColumns, Indices).
key_index(Columns, Col, Index) :- nth0(Index, Columns, Col).

edges_list(RelPlans, EdgeStatements, Dicts) :-
    maplist(edge_dict(RelPlans), EdgeStatements, Dicts).

ordered_edge_statement(edgestmt(_, _, _, _, _, _, _, ordered_arrival, _)).
ordered_edge_statement(edgestmt(_, _, _, _, _, _, _, ordered_departure, _)).

ordered_program(EdgeStatements) :-
    member(Statement, EdgeStatements),
    ordered_edge_statement(Statement),
    !.

ordered_trigger_kind(ordered_departure, departure) :- !.
ordered_trigger_kind(departure, departure) :- !.
ordered_trigger_kind(_, arrival).

plan_pre_refs(Rules, Refs) :-
    findall(Ref,
            ( member((_ <+ Body), Rules),
              level_body_pre_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

ordered_arm_dict(RelPlans, PreRefs,
        edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql,
                 WriteSql, _, EdgeTriggerKind,
                 edgeinterns(ProjectInternSqls, _)), Dict) :-
    ref_name(HeadRef, HeadName),
    ref_name(TriggerRef, TriggerName),
    relplan_shape(RelPlans, HeadRef, HeadKind, _, _, _),
    ordered_trigger_kind(EdgeTriggerKind, TriggerKind),
    head_to_key_indices(HeadColumns, KeyColumns, KeyIndices),
    ( memberchk(HeadRef, PreRefs) -> EvolvesPre = true ; EvolvesPre = false ),
    intern_field(ProjectInternSqls, InternField),
    Dict = _{ trigger_rel: TriggerName, trigger_kind: TriggerKind,
              head_rel: HeadName, head_kind: HeadKind,
              head_columns: HeadColumns, key_indices: KeyIndices,
              project_sql: ProjectSql, write_sql: WriteSql,
              evolves_pre: EvolvesPre, intern_sql: InternField }.

ordered_recursive_levels(Rules, Recursive) :-
    ( member(Rule, Rules), Rule = (_ <- Body),
      rule_head_ref(Rule, HeadRef),
      body_ref_uses(Body, Uses),
      memberchk(use(HeadRef, _, pos, _), Uses)
    -> Recursive = true
    ;  Recursive = false
    ).

ordered_fields(EdgeStatements, RelPlans, Rules, Ordered, Arms, PreNames,
               RecursiveLevels) :-
    ( ordered_program(EdgeStatements)
    -> Ordered = true,
       plan_pre_refs(Rules, PreRefs),
       maplist(ordered_arm_dict(RelPlans, PreRefs), EdgeStatements, Arms),
       maplist(ref_name, PreRefs, PreNames),
       ordered_recursive_levels(Rules, RecursiveLevels)
    ;  Ordered = false, Arms = [], PreNames = [], RecursiveLevels = false
    ).

level_dict(HeadTable, CyclicHeadGroups,
           levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql,
                     RefCountSql, AggregateSql, DeltaInternSqls),
           Dict) :-
    ref_name(HeadRef, HeadName),
    recursion_group_field(CyclicHeadGroups, HeadRef, RecursionGroupField),
    format(atom(DeltaTable), '__delta_~w', [HeadName]),
    memberchk(HeadName-[HeadColumns, RawHeadTypes], HeadTable),
    maplist(boundary_type_name, RawHeadTypes, HeadTypes),
    ( DeltaInsertSql = none -> InsertField = null ; InsertField = DeltaInsertSql ),
    atomic_list_concat([DeleteSql | InsertSqls], ';\n', RecomputeSql),
    select_sql_text(HeadName, HeadColumns, SelectSql),
    refcount_fields(RefCountSql, SupportField, ExpandField, DredField,
                    SupportInternField),
    aggregate_field(AggregateSql, AggregateField),
    intern_field(DeltaInternSqls, InternField),
    Dict = _{ head_rel: HeadName,
              intern_sql: InternField,
              head_delta_table_name: DeltaTable,
              head_columns: HeadColumns,
              head_column_types: HeadTypes,
              insert_sql: InsertField,
              select_sql: SelectSql,
              recompute_delete_sql: DeleteSql,
              recompute_insert_sqls: InsertSqls,
              recompute_sql: RecomputeSql,
              support_sql: SupportField,
              support_intern_sql: SupportInternField,
              expand_sql: ExpandField,
              dred_sql: DredField,
              recursion_group: RecursionGroupField,
              aggregate_sql: AggregateField }.

% The mirror of emit_ts.pl:recursion_group_field/3, spelled as the dict this
% door serializes; `heads` is what a tripped cap names on BOTH doors.
recursion_group_field(CyclicHeadGroups, HeadRef, null) :-
    \+ memberchk(HeadRef-_, CyclicHeadGroups), !.
recursion_group_field(CyclicHeadGroups, HeadRef, Dict) :-
    memberchk(HeadRef-GroupIndex, CyclicHeadGroups),
    fixpoint_round_cap(RoundCap),
    findall(Name,
            ( member(Ref-GroupIndex, CyclicHeadGroups), ref_name(Ref, Name) ),
            Names),
    atomic_list_concat(Names, ',', JoinedNames),
    format(atom(Heads), '[~w]', [JoinedNames]),
    Dict = _{ group: GroupIndex, round_cap: RoundCap, heads: Heads }.

select_sql_text(HeadName, HeadColumns, SelectSql) :-
    maplist(quote_ident_local, HeadColumns, Quoted),
    atomic_list_concat(Quoted, ', ', HeadSql),
    format(atom(SelectSql), 'SELECT ~w FROM "~w"', [HeadSql, HeadName]).
quote_ident_local(Col, Quoted) :- format(atom(Quoted), '"~w"', [Col]).

levels_list(LevelStatements, HeadTable, CyclicHeadGroups, Dicts) :-
    maplist(level_dict(HeadTable, CyclicHeadGroups), LevelStatements, Dicts).

retention_dict(retentionstmt(Ref, _Limit, DeleteSql), Dict) :-
    ref_name(Ref, Name),
    Dict = _{ rel: Name, delete_sql: DeleteSql }.
retentions_list(RetentionStatements, Dicts) :-
    maplist(retention_dict, RetentionStatements, Dicts).

refcount_fields(none, null, null, null, null) :- !.
refcount_fields(refcountsql(ClearSql, SeedSql, UpdateSql, StageRetractSql,
                            CollectZeroSql, ClearNewSql, FillNewSql,
                            StageAddSql, StageFrontierSql, StageNextFrontierSql,
                            InsertNewSql, ExpandPlan, DredPlan, _FixpointIr,
                            SupportInternSqls),
                SupportText, ExpandText, DredText, SupportInternText) :-
    SupportText =
    [ ClearSql, SeedSql, UpdateSql, StageRetractSql, CollectZeroSql,
      ClearNewSql, FillNewSql, StageAddSql, StageFrontierSql,
      StageNextFrontierSql, InsertNewSql ],
    expand_field(ExpandPlan, ExpandText),
    dred_field(DredPlan, DredText),
    ( SupportInternSqls == [] -> SupportInternText = null
    ; SupportInternText = SupportInternSqls ).

expand_field(none, null) :- !.
expand_field(expandplan(ClearASql, ClearBSql, SeedSqls, HopABSql, HopBASql,
                        AbsorbASql, AbsorbBSql, RoundCap),
             Dict) :-
    Dict = _{ clear_a_sql: ClearASql, clear_b_sql: ClearBSql,
              seed_sqls: SeedSqls, hop_ab_sql: HopABSql, hop_ba_sql: HopBASql,
              absorb_a_sql: AbsorbASql, absorb_b_sql: AbsorbBSql,
              round_cap: RoundCap }.

dred_field(none, null) :- !.
dred_field(dredplan(ClearPing, ClearPong, ClearCone, AssertSeeds,
                    AssertAB, AssertBA, CommitA, CommitB, ArrivalA, ArrivalB,
                    DredSeeds, DredAB, DredBA, ConeAbsorbA, ConeAbsorbB,
                    ConeTrim, HeadDelete, RederiveSeeds, ReviveAB, ReviveBA,
                    ConeDropA, ConeDropB, StageRetract, HeadCount),
            Dict) :-
    Dict = _{ clear_ping_sql: ClearPing, clear_pong_sql: ClearPong,
              clear_cone_sql: ClearCone, assert_seed_sqls: AssertSeeds,
              assert_hop_ab_sql: AssertAB, assert_hop_ba_sql: AssertBA,
              commit_a_sql: CommitA, commit_b_sql: CommitB,
              arrival_a_sql: ArrivalA, arrival_b_sql: ArrivalB,
              dred_seed_sqls: DredSeeds, dred_hop_ab_sql: DredAB,
              dred_hop_ba_sql: DredBA, cone_absorb_a_sql: ConeAbsorbA,
              cone_absorb_b_sql: ConeAbsorbB, cone_trim_sql: ConeTrim,
              head_delete_sql: HeadDelete, rederive_seed_sqls: RederiveSeeds,
              revive_hop_ab_sql: ReviveAB, revive_hop_ba_sql: ReviveBA,
              cone_drop_a_sql: ConeDropA, cone_drop_b_sql: ConeDropB,
              stage_retract_sql: StageRetract, head_count_sql: HeadCount }.

aggregate_field(none, null) :- !.
aggregate_field(aggsql(_ScopeCols, _ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                       DeleteScopedSql, InsertScopedSqls, InternSqls),
                Dict) :-
    ( InternSqls == [] -> InternField = null ; InternField = InternSqls ),
    Dict = _{ scope_clear_sql: ScopeClearSql, scope_seed_sql: ScopeSeedSqls,
              delete_scoped_sql: DeleteScopedSql,
              insert_scoped_sql: InsertScopedSqls,
              intern_sql: InternField, delta_maintained: false }.
aggregate_field(avgsql(_ScopeCols, _ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                       DeleteScopedSql, InsertScopedSqls, _BootSqls),
                Dict) :-
    Dict = _{ scope_clear_sql: ScopeClearSql, scope_seed_sql: ScopeSeedSqls,
              delete_scoped_sql: DeleteScopedSql,
              insert_scoped_sql: InsertScopedSqls,
              intern_sql: null, delta_maintained: true }.

% The same plan emit_ts.pl renders as TEXT_INTERN_PLAN. `none` at intern(direct)
% and at any program whose columns are all unencoded.
text_intern_field(none, null) :- !.
text_intern_field(textintern(InternSql, LookupSql, RelColumns), Dict) :-
    pairs_to_dict(RelColumns, ColumnsDict),
    Dict = _{ intern_sql: InternSql, lookup_sql: LookupSql,
              rel_columns: ColumnsDict }.

struct_type_dict(structtype(TypeName, Columns, RefTypes, KeyIndices,
                            ConflictSql, InternSql, LookupSql), Dict) :-
    maplist(struct_ref_field, RefTypes, Refs),
    Dict = _{ name: TypeName, columns: Columns, refs: Refs,
              key_indices: KeyIndices, conflict_sql: ConflictSql,
              intern_sql: InternSql, lookup_sql: LookupSql }.

struct_ref_field(none, null) :- !.
struct_ref_field(TypeName, TypeName).

% The same rows emit_ts.pl renders as host_plans, so the two runtimes read one
% executor contract: name, columns, template, demand/response rels, execution.
host_plan_dict(host_plan(Name, Inputs, Outputs, template(Template),
                         demand_ref(DemandName), response_ref(ResponseName), _),
               Dict) :-
    maplist(host_column_dict, Inputs, InputDicts),
    maplist(host_column_dict, Outputs, OutputDicts),
    host_execution(Name, Template, Executor),
    Base = _{ name: Name, inputs: InputDicts, outputs: OutputDicts,
              template: Template, demand_rel: DemandName,
              response_rel: ResponseName, execution: Executor },
    HostPlan = host_plan(Name, Inputs, Outputs, template(Template),
                         demand_ref(DemandName), response_ref(ResponseName), _),
    host_plan_contract(HostPlan,
                       host_contract(RequestType, ResponseType)),
    (   host_contract_is_structured(RequestType, ResponseType)
    ->  host_type_descriptor_dict(RequestType, RequestDict),
        host_type_descriptor_dict(ResponseType, ResponseDict),
        Dict = Base.put(_{request_type: RequestDict,
                          response_type: ResponseDict})
    ;   Dict = Base
    ).

host_contract_is_structured(type_descriptor(_, RequestFields),
                            type_descriptor(_, ResponseFields)) :-
    ( member(field(_, Type), RequestFields), structured_host_type(Type)
    ; member(field(_, Type), ResponseFields), structured_host_type(Type)
    ),
    !.

structured_host_type(Type) :-
    \+ memberchk(Type, [text, int, float, bool]).

host_type_descriptor_dict(type_descriptor(TypeName/Arity, Fields), Dict) :-
    format(atom(Ref), '~w/~w', [TypeName, Arity]),
    maplist(host_field_dict, Fields, FieldDicts),
    Dict = _{ ref: Ref, fields: FieldDicts }.

host_field_dict(field(Name, Type), _{ name: Name, type: Type }).

host_column_dict(col(Name, Type), _{ name: Name, type: Type }).

struct_ref_columns_map(RelPlans, Map) :-
    findall(Name-Refs,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Ref, _, _, _, ColumnTypes),
              memberchk(ref(_), ColumnTypes),
              ref_name(Ref, Name),
              maplist(column_type_ref_field, ColumnTypes, Refs) ),
            Pairs),
    pairs_to_dict(Pairs, Map).

column_type_ref_field(ref(TypeName), TypeName) :- !.
column_type_ref_field(_, null).

% ═══ assemble the Rust source ════════════════════════════════════════════════

emit_program(Name, Plan, Lowered, BootStatements, Text) :-
    Lowered = lowered(Name, Ddl, ArrivalStatements, EdgeStatements,
                      LevelStatements, DeltaStatements, RelPlans, ArrivalTargets),
    plan_intern_mode(Plan, InternMode),
    include(is_level_statement, LevelStatements, RuleLevelStatements),
    include(is_retention_statement, LevelStatements, RetentionStatements),
    Plan = plan(_, TickProg, LoweringTypes, _, _, _, _, _, _),
    TickProg = prog(PlanDecls, PlanRules),
    program_uses_tick(TickProg, UsesTick),
    listened_departure_refs(PlanRules, DepartureRefs),
    reconcile_every_tick(Plan, ReconcileEveryTick),

    findall(HeadName-[Cols, Types],
            ( member(deltastmt(Ref, _, _, _, _), DeltaStatements),
              ref_name(Ref, HeadName),
              relplan_shape(RelPlans, Ref, _K, Cols, _Key, Types) ),
            HeadTable),

    map_from(RelPlans, rel_columns_of, RelColumns),
    map_from(RelPlans, rel_column_types_of, RelColumnTypes),
    maplist(ref_name, ArrivalTargets, ArrivalTargetNames),
    maplist(boot_dict, BootStatements, BootDicts),
    final_select_map(DeltaStatements, FinalSelect),
    arrival_templates_map(ArrivalStatements, ArrivalTemplates),
    relations_list(RelPlans, ArrivalStatements, DepartureRefs, DeltaStatements,
                   Relations),
    edges_list(RelPlans, EdgeStatements, Edges),
    cyclic_head_groups(PlanRules, CyclicHeadGroups),
    levels_list(RuleLevelStatements, HeadTable, CyclicHeadGroups, Levels),
    retentions_list(RetentionStatements, Retentions),
    program_text_intern_plan(InternMode, RelPlans, TextInternPlan),
    text_intern_field(TextInternPlan, TextInternField),
    struct_type_plans(PlanDecls, LoweringTypes, StructPlans),
    maplist(struct_type_dict, StructPlans, StructTypes),
    struct_ref_columns_map(RelPlans, StructRefColumns),
    ordered_fields(EdgeStatements, RelPlans, PlanRules,
                   OrderedProgram, OrderedArms, OrderedPreRefs,
                   OrderedRecursiveLevels),
    findall(HostDict,
            ( member(Decl, PlanDecls),
              Decl = sh_decl(_, _, _, _),
              compile_host_decl(Decl, HostPlan),
              host_plan_dict(HostPlan, HostDict) ),
            HostPlanDicts),

    ProgramDict =
    _{ name: Name,
       intern_mode: InternMode,
       ddl: Ddl,
       rel_columns: RelColumns,
       rel_column_types: RelColumnTypes,
       arrival_targets: ArrivalTargetNames,
       boot: BootDicts,
       final_select: FinalSelect,
       arrival_templates: ArrivalTemplates,
       text_intern_plan: TextInternField,
       struct_types: StructTypes,
       struct_ref_columns: StructRefColumns,
       ordered_program: OrderedProgram,
       ordered_arms: OrderedArms,
       ordered_pre_refs: OrderedPreRefs,
       ordered_recursive_levels: OrderedRecursiveLevels,
       relations: Relations,
       edges: Edges,
       levels: Levels,
       retentions: Retentions,
       uses_tick: UsesTick,
       reconcile_every_tick: ReconcileEveryTick,
       % Constant true: no fallback tick path exists on either door; the field
       % stays only because engine-rs program.rs deserializes it.
       incremental_safe: true,
       host_plans: HostPlanDicts },
    json_write_string(ProgramDict, ProgramJson),
    raw_string_hashes(ProgramJson, RawStringHashes),
    format(atom(HeadLine), '// Program: ~w', [Name]),
    atomic_list_concat(['pub const PROGRAM_JSON: &str = r', RawStringHashes, '"'],
                       ProgramJsonOpen),
    atomic_list_concat(['"', RawStringHashes, ';'], ProgramJsonClose),
    HeaderLines =
    [ '// GENERATED by v6/prolog/emit_rust.pl. Do not hand-edit; recompile.',
      HeadLine,
      '',
      'use sprefa_engine_rs::types::ProgramJson;',
      '',
      ProgramJsonOpen,
      ProgramJson,
      ProgramJsonClose,
      '',
      'pub fn program() -> ProgramJson {',
      '    serde_json::from_str(PROGRAM_JSON).expect("emitted program json")',
      '}'
    ],
    lines_block(HeaderLines, Body),
    format(atom(Text), '~w\n', [Body]).

is_level_statement(levelstmt(_, _, _, _, _, _, _)).
is_retention_statement(retentionstmt(_, _, _)).
