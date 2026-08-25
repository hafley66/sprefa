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
                       struct_type_plans/3, struct_type_plans/4, fixpoint_round_cap/1,
                       query_order_by_map/3 ]).
:- use_module(strat, [cyclic_head_groups/2]).
:- use_module('0_rel_record').
:- use_module('3_analyze/analyze', [ body_ref_uses/2, level_body_pre_ref/2, rule_head_ref/2,
                         listened_departure_refs/2, program_uses_tick/2 ]).
:- use_module('2_host_expand/1_host_expand', [compile_host_decl/3, query_decl/3,
                                host_plan_contract/2]).
:- use_module('0_dot_expand/registry', [host_execution/3]).
% bind_executor/2 left the registry with the bind surface; pinned here so the
% term-door bind_decl path the resident runtime still walks keeps its executor.
bind_executor(interval, live_interval).
bind_executor(watch,    live_watch).
:- use_module('1_expansion/0_option_expand', [option_enum_name/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ IR version ══════════════════════════════════════════════════════════════
% emit_ts.pl carries the same number under the same spelling, and both runtimes
% refuse a program document whose value is not the one they interpret.
ir_version(1).

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
% Keep endpoint identity distinct from a followed row at the ProgramJson seam.
boundary_type_name(idref(_), relation_id) :- !.
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

final_select_entry(OrderByMap, deltastmt(Ref, Sql, _, _, _), Name-Ordered) :-
    ref_name(Ref, Name),
    (   memberchk(Name-OrderBySql, OrderByMap)
    ->  atom_concat(Sql, OrderBySql, Ordered)
    ;   Ordered = Sql
    ).

query_names(PlanDecls, Names) :-
    findall(Name,
            ( member(QueryDecl, PlanDecls), query_decl(QueryDecl, Atom, _),
              functor(Atom, Name, _Arity) ),
            Names).

final_select_map(OrderByMap, DeltaStatements, Map) :-
    maplist(final_select_entry(OrderByMap), DeltaStatements, Pairs),
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
    relplan_storage_name(RelPlans, Ref, StorageName),
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
    format(atom(FrontierTable), '__frontier_~w', [StorageName]),
    format(atom(NextFrontierTable), '__next_frontier_~w', [StorageName]),
    ( memberchk(Ref, DepartureRefs)
    -> format(atom(DepartureTable), '__departure_frontier_~w', [StorageName]), DepField = DepartureTable
    ; DepField = null ),
    Dict0 = _{ rel: Name, kind: Kind, table_name: StorageName,
               delta_table_name: DeltaTable,
               frontier_table_name: FrontierTable,
               next_frontier_table_name: NextFrontierTable,
               departure_frontier_table_name: DepField,
               columns: Columns, column_types: ColumnTypes,
               key_indices: KeyIndices,
               arrival_add_sql: AddText, arrival_del_sql: DelText,
               boundary_sql: BoundarySql },
    % Keyed only under frontier(shared); per_rel JSON stays byte-identical.
    (   lower:frontier_mode(shared),
        lower:shared_frontier_relation_id(RelPlans, Ref, RelationId)
    ->  put_dict(shared_frontier, Dict0, _{ relation_id: RelationId }, Dict)
    ;   Dict = Dict0
    ).

relations_list(RelPlans, ArrivalStatements, DepartureRefs, DeltaStatements, Dicts) :-
    maplist(relation_dict(RelPlans, ArrivalStatements, DepartureRefs),
            DeltaStatements, Dicts).

edge_dict(RelPlans, PreRefs,
          edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql, _Write,
                   DeltaProjectSql, Kind, edgeinterns(ProjectInternSqls, DeltaInternSqls)),
          Dict) :-
    ref_name(HeadRef, HeadName),
    ref_name(TriggerRef, TriggerName),
    relplan_storage_name(RelPlans, HeadRef, HeadStorageName),
    relplan_shape(RelPlans, HeadRef, HeadKind, _Columns, _Key, _Types),
    format(atom(DeltaTable), '__delta_~w', [HeadStorageName]),
    head_to_key_indices(HeadColumns, KeyColumns, KeyIndices),
    intern_field(DeltaInternSqls, InternField),
    arm_schedule(Kind, Schedule),
    ordered_trigger_kind(Kind, TriggerKind),
    (   Schedule == sequenced
    ->  OccurrenceProject = ProjectSql,
        intern_field(ProjectInternSqls, OccurrenceIntern)
    ;   OccurrenceProject = null, OccurrenceIntern = null
    ),
    ( memberchk(HeadRef, PreRefs) -> EvolvesPre = true ; EvolvesPre = false ),
    Dict = _{ head_rel: HeadName, head_columns: HeadColumns,
              head_table_name: HeadStorageName, head_delta_table_name: DeltaTable, head_kind: HeadKind,
              key_indices: KeyIndices, project_sql: DeltaProjectSql,
              intern_sql: InternField,
              schedule: Schedule,
              trigger_rel: TriggerName, trigger_kind: TriggerKind,
              occurrence_project_sql: OccurrenceProject,
              occurrence_intern_sql: OccurrenceIntern,
              evolves_pre: EvolvesPre }.

intern_field([], null) :- !.
intern_field(InternSqls, InternSqls).
head_to_key_indices(HeadColumns, KeyColumns, Indices) :-
    maplist(key_index(HeadColumns), KeyColumns, Indices).
key_index(Columns, Col, Index) :- nth0(Index, Columns, Col).

edges_list(RelPlans, PreRefs, EdgeStatements, Dicts) :-
    maplist(edge_dict(RelPlans, PreRefs), EdgeStatements, Dicts).

% arrival_trigger_kind/4 reaches these two exactly when the body reads the
% store this tick is still writing: pre/1, or a negation over an edge head.
arm_schedule(ordered_arrival, sequenced) :- !.
arm_schedule(ordered_departure, sequenced) :- !.
arm_schedule(_, set_at_once).

ordered_trigger_kind(ordered_departure, departure) :- !.
ordered_trigger_kind(departure, departure) :- !.
ordered_trigger_kind(_, arrival).

plan_pre_refs(Rules, Refs) :-
    findall(Ref,
            ( member((_ <+ Body), Rules),
              level_body_pre_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

level_dict(RelPlans, HeadTable, CyclicHeadGroups,
           levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql,
                     RefCountSql, AggregateSql, DeltaInternSqls),
           Dict) :-
    ref_name(HeadRef, HeadName),
    relplan_storage_name(RelPlans, HeadRef, HeadStorageName),
    recursion_group_field(CyclicHeadGroups, HeadRef, RecursionGroupField),
    format(atom(DeltaTable), '__delta_~w', [HeadStorageName]),
    memberchk(HeadName-[HeadColumns, RawHeadTypes], HeadTable),
    maplist(boundary_type_name, RawHeadTypes, HeadTypes),
    ( DeltaInsertSql = none -> InsertField = null ; InsertField = DeltaInsertSql ),
    atomic_list_concat([DeleteSql | InsertSqls], ';\n', RecomputeSql),
    select_sql_text(HeadStorageName, HeadColumns, SelectSql),
    refcount_fields(RefCountSql, SupportField, ExpandField, DredField,
                    SupportInternField, SupportCountField),
    aggregate_field(AggregateSql, AggregateField),
    intern_field(DeltaInternSqls, InternField),
    Dict0 = _{ head_rel: HeadName,
              intern_sql: InternField,
              head_table_name: HeadStorageName,
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
              aggregate_sql: AggregateField },
    % Absent under frontier(per_rel): the key itself never appears, so a
    % per-rel program's JSON keeps its bytes.
    (   SupportCountField == null
    ->  Dict = Dict0
    ;   put_dict(support_count_sql, Dict0, SupportCountField, Dict)
    ).

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

levels_list(RelPlans, LevelStatements, HeadTable, CyclicHeadGroups, Dicts) :-
    maplist(level_dict(RelPlans, HeadTable, CyclicHeadGroups), LevelStatements, Dicts).

retention_dict(retentionstmt(Ref, _Limit, DeleteSql), Dict) :-
    ref_name(Ref, Name),
    Dict = _{ rel: Name, delete_sql: DeleteSql }.
retentions_list(RetentionStatements, Dicts) :-
    maplist(retention_dict, RetentionStatements, Dicts).

refcount_fields(none, null, null, null, null, null) :- !.
refcount_fields(refcountsql(ClearSql, SeedSql, UpdateSql, StageRetractSql,
                            CollectZeroSql, ClearNewSql, FillNewSql,
                            StageAddSql, StageFrontierSql, StageNextFrontierSql,
                            InsertNewSql, ExpandPlan, DredPlan, _FixpointIr,
                            SupportInternSqls, SupportCountPlan),
                SupportText, ExpandText, DredText, SupportInternText,
                SupportCountText) :-
    SupportText =
    [ ClearSql, SeedSql, UpdateSql, StageRetractSql, CollectZeroSql,
      ClearNewSql, FillNewSql, StageAddSql, StageFrontierSql,
      StageNextFrontierSql, InsertNewSql ],
    expand_field(ExpandPlan, ExpandText),
    dred_field(DredPlan, DredText),
    ( SupportInternSqls == [] -> SupportInternText = null
    ; SupportInternText = SupportInternSqls ),
    support_count_field(SupportCountPlan, SupportCountText).

support_count_field(none, null) :- !.
support_count_field(supportcount(ClearSql, WriteSqls),
                    _{ clear_sql: ClearSql, write_sqls: WriteSqls }).

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

% Column 1 is the configuration column for every bind (registry.pl:309), so the
% literals are the cadences and file sets the program's own rules name there.
bind_plan_dict(bind_decl(Name, Columns), Rules, Dict) :-
    maplist(host_column_dict, Columns, ColumnDicts),
    bind_read_literals(Rules, Name, Columns, Literals),
    (   bind_executor(Name, Executor)
    ->  true
    ;   throw(bind_mismatch(Name, Columns))
    ),
    Dict = _{ name: Name, columns: ColumnDicts, literals: Literals,
              execution: Executor }.

% The scan is over the WHOLE rule term: a rule that heads a bind rel is already
% stopped at load (bind_and_rule_head), so every occurrence reachable here reads.
bind_read_literals(Rules, Name, Columns, Literals) :-
    length(Columns, Arity),
    findall(Literal,
            ( bind_subterm(Rules, Atom),
              compound(Atom),
              functor(Atom, Name, Arity),
              arg(1, Atom, Literal),
              bind_config_literal(Literal)
            ),
            Raw),
    sort(Raw, Literals).

bind_config_literal(Literal) :-
    nonvar(Literal),
    ( integer(Literal) -> true
    ; string(Literal)  -> true
    ; atom(Literal), Literal \== []
    ).

bind_subterm(Term, Term) :-
    nonvar(Term).
bind_subterm(Term, Sub) :-
    nonvar(Term),
    compound(Term),
    arg(_, Term, Argument),
    bind_subterm(Argument, Sub).

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

% Enum values keep their endpoint INTEGER in the physical relation.  This
% emitted schema describes only the public tagged boundary and the generated
% variant payload relations used to materialize that endpoint.
enum_type_plans(Decls, RelPlans, DeltaStatements, Plans) :-
    findall(Name, enum_runtime_name(Decls, Name), Names0),
    sort(Names0, Names),
    maplist(enum_type_plan(Decls, RelPlans, DeltaStatements), Names, Plans).

enum_runtime_name(Decls, Name) :- member(enum_column(_, _, Name), Decls).
enum_runtime_name(Decls, Name) :-
    member(option_column(_, _, Element), Decls),
    option_enum_name(Element, Name).
enum_runtime_name(Decls, Name) :-
    member(enum_option_payload(_, _, _, Element), Decls),
    option_enum_name(Element, Name).

enum_type_plan(Decls, RelPlans, DeltaStatements, Name,
               enumtype(Name, Variants, Identity)) :-
    findall(Tag-Variant,
            enum_variant_plan(Decls, RelPlans, DeltaStatements, Name, Tag, Variant), Pairs0),
    keysort(Pairs0, Pairs), pairs_values(Pairs, Variants),
    enum_identity_plan(Name, Identity).

enum_variant_plan(Decls, RelPlans, DeltaStatements, EnumName, Tag,
                  enumvariant(Tag, VariantName, Fields, FieldTypes, FieldEnums, SelectSql)) :-
    atomic_list_concat([EnumName, '_'], Prefix),
    member(RelPlan, RelPlans),
    relplan_parts(RelPlan, VariantName/_, _, [id | Fields], _, [_ | FieldTypes0]),
    atom_concat(Prefix, Tag, VariantName),
    atom_concat(EnumName, '_tag', TagRelation),
    VariantName \== TagRelation,
    member(deltastmt(VariantName/_, SelectSql, _, _, _), DeltaStatements),
    maplist(boundary_type_name, FieldTypes0, FieldTypes),
    maplist(enum_variant_field(Decls, VariantName), Fields, FieldEnums).

enum_variant_field(Decls, VariantName, Field, EnumName) :-
    member(enum_column(VariantName/_, Field, EnumName), Decls), !.
enum_variant_field(Decls, VariantName, Field, EnumName) :-
    member(enum_option_payload(_, VariantName, Field, Element), Decls),
    option_enum_name(Element, EnumName), !.
enum_variant_field(_, _, _, null).

enum_type_dict(enumtype(Name, Variants, Identity),
               _{name: Name, variants: VariantDicts, identity: Identity}) :-
    maplist(enum_variant_dict, Variants, VariantDicts).
enum_variant_dict(enumvariant(Tag, Rel, Fields, FieldTypes, FieldEnums, SelectSql),
                  _{tag: Tag, rel: Rel, fields: Fields, field_types: FieldTypes,
                    field_enums: FieldEnums, select_sql: SelectSql}).

enum_ref_columns_map(Decls, RelPlans, Map) :-
    findall(Name-Refs,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Ref, _, Columns, _, _),
              ref_name(Ref, Name),
              enum_ref_fields(Decls, Ref, Columns, Columns, Refs),
              member(Field, Refs), Field \== null ),
            Pairs),
    pairs_to_dict(Pairs, Map).

enum_ref_fields(_, _, _, [], []).
enum_ref_fields(Decls, Ref, AllColumns, [Column | Rest], [Field | Fields]) :-
    ( ( member(enum_column(Ref, Column, EnumName), Decls)
      ; member(option_column(Ref, Column, Element), Decls),
        option_enum_name(Element, EnumName)
      )
    -> ( nth0(EndpointIndex, AllColumns, id),
         nth0(CurrentIndex, AllColumns, Column), EndpointIndex =\= CurrentIndex
       -> Field = _{name: EnumName, endpoint_index: EndpointIndex}
       ;  Field = _{name: EnumName, endpoint_index: null}
       )
    ; Field = null
    ),
    enum_ref_fields(Decls, Ref, AllColumns, Rest, Fields).

% Enum endpoints owned by an ordinary row keep using that row's id. A keyed
% enum-only row has no such owner, so its tagged public value is interned in a
% durable enum identity table before the generated variant arrival is staged.
% The table key is canonical tagged JSON, making none and each some payload
% ordinary, non-NULL values with stable equality across ticks and restarts.
enum_identity_plan(Name, _{intern_sql: InternSql, lookup_sql: LookupSql}) :-
    enum_identity_table(Name, Table),
    quote_ident_local(Table, QuotedTable),
    format(atom(InternSql),
           'INSERT OR IGNORE INTO ~w ("value") VALUES (?)', [QuotedTable]),
    format(atom(LookupSql),
           'SELECT "id", "value" FROM ~w WHERE "value" = ?', [QuotedTable]).

enum_identity_table(Name, Table) :-
    atomic_list_concat(['__enum_identity_', Name], Table).

enum_identity_ddls(Decls, Ddls) :-
    findall(Ddl,
            ( enum_runtime_name(Decls, Name),
              enum_identity_table(Name, Table), quote_ident_local(Table, QuotedTable),
              format(atom(Ddl),
                     'CREATE TABLE ~w ("id" INTEGER PRIMARY KEY, "value" TEXT NOT NULL UNIQUE)',
                     [QuotedTable]) ),
            Ddls0),
    sort(Ddls0, Ddls).

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
    query_order_by_map(PlanDecls, RelPlans, OrderByMap),
    final_select_map(OrderByMap, DeltaStatements, FinalSelect),
    query_names(PlanDecls, QueryNames),
    arrival_templates_map(ArrivalStatements, ArrivalTemplates),
    relations_list(RelPlans, ArrivalStatements, DepartureRefs, DeltaStatements,
                   Relations),
    plan_pre_refs(PlanRules, PreRefs),
    maplist(ref_name, PreRefs, PreSnapshotRels),
    edges_list(RelPlans, PreRefs, EdgeStatements, Edges),
    cyclic_head_groups(PlanRules, CyclicHeadGroups),
    levels_list(RelPlans, RuleLevelStatements, HeadTable, CyclicHeadGroups, Levels),
    retentions_list(RetentionStatements, Retentions),
    program_text_intern_plan(InternMode, RelPlans, TextInternPlan),
    text_intern_field(TextInternPlan, TextInternField),
    struct_type_plans(PlanDecls, LoweringTypes, RelPlans, StructPlans),
    maplist(struct_type_dict, StructPlans, StructTypes),
    struct_ref_columns_map(RelPlans, StructRefColumns),
    enum_type_plans(PlanDecls, RelPlans, DeltaStatements, EnumPlans),
    maplist(enum_type_dict, EnumPlans, EnumTypes),
    enum_ref_columns_map(PlanDecls, RelPlans, EnumRefColumns),
    findall(HostDict,
            ( member(Decl, PlanDecls),
              Decl = sh_decl(_, _, _, _),
              compile_host_decl(Decl, PlanDecls, HostPlan),
              host_plan_dict(HostPlan, HostDict) ),
            HostPlanDicts),
    findall(BindDict,
            ( member(BindDecl, PlanDecls),
              BindDecl = bind_decl(_, _),
              bind_plan_dict(BindDecl, PlanRules, BindDict) ),
            BindPlanDicts),

    enum_identity_ddls(PlanDecls, EnumIdentityDdls),
    append(Ddl, EnumIdentityDdls, FullDdl),

    ir_version(IrVersion),
    ProgramDict =
    _{ name: Name,
       ir_version: IrVersion,
       intern_mode: InternMode,
       ddl: FullDdl,
       rel_columns: RelColumns,
       rel_column_types: RelColumnTypes,
       arrival_targets: ArrivalTargetNames,
       boot: BootDicts,
       final_select: FinalSelect,
       queries: QueryNames,
       arrival_templates: ArrivalTemplates,
       text_intern_plan: TextInternField,
       struct_types: StructTypes,
       struct_ref_columns: StructRefColumns,
       enum_types: EnumTypes,
       enum_ref_columns: EnumRefColumns,
       pre_snapshot_rels: PreSnapshotRels,
       relations: Relations,
       edges: Edges,
       levels: Levels,
       retentions: Retentions,
       uses_tick: UsesTick,
       reconcile_every_tick: ReconcileEveryTick,
       % Constant true: no fallback tick path exists on either door; the field
       % stays only because engine-rs program.rs deserializes it.
       incremental_safe: true,
       host_plans: HostPlanDicts,
       bind_plans: BindPlanDicts },
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
