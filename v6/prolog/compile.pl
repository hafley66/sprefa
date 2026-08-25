% Phase B entry point. Fixture terms are read with read_term/3 and
% variable_names/1 so source variable identity remains available to analysis.
%
% Emit:  swipl -q -l v6/prolog/compile/compile.pl \
%          -g "compile_fixture(switch_as_keyed_replace, 'v6/prolog/conformance/fixtures/scopes.pl', 'v6/prolog/compile/out/switch_as_keyed_replace.ts')" \
%          -g halt
%
:- module(compile,
          [ read_fixture_term/4,
            compile_fixture/3,
            compile_fixture/4,
            compile_fixture/5,
            compile_dl6/2,
            compile_dl6/3,
            compile_program/6,
            compile_program/7,
            default_intern_mode/1,
            dl6_seeded_form/3,
            measure_phase/3,
            restore_phase_outcome/1,
            run_compile_step/4,
            write_compile_trace/2,
            throw_text_door_error/2,
            program_plan/2,
            program_plan/3,
            compiler_owned_contract/1,
            dd_compile_context/2
          ]).

% Out-of-band channel for the emitter seam. compile_program_phases/8 carries
% Initial as an argument and Schedule inside the fixture term, but the seam
% call emits _5 (Name, Plan, Lowered, BootStatements, Text); an emitter that
% also needs Initial and Schedule (isolated_compiler_dd:compile_program) reads them from
% this thread-local, set immediately before the seam call and cleared after.
:- thread_local dd_compile_context/2.

% Without this, open/3 inherits the ambient locale: under LC_ALL=C the default
% is `text` (ASCII) and any non-ASCII fixture throws on write.
:- set_prolog_flag(encoding, utf8).

:- use_module(library(lists)).
:- use_module('0_unsupported_messages', []).
:- use_module('0_dot_expand/0_dot_expand', [resolve_relation_paths/3]).
:- use_module('1_expansion/1_expansion',
              [ expand_program/3, expand_program_with_bindings/4 ]).
:- use_module('1_host_expand', [prepare_program/5, query_decl/3]).
:- use_module(analyze).
:- use_module('3_clock_check', [check_clock_program/1]).
:- use_module('2_subscribe', [subscribed_rels/4]).
:- use_module(strat).
:- use_module(lower).
:- use_module(emit_ts).
:- use_module(library(tableutil), [table_statistics/2]).
:- use_module('compile/parse_dl_dcg', [ parse_dl_line_for_reason/2 ]).
:- use_module('compile/scripts/0_json_arrival',
              [ arrival_column_types/4, schedule_value/5 ]).
:- use_module('use_resolve', [expand_uses/8, short_hash/2]).
:- use_module(library(http/json), [json_read_dict/3]).
:- use_module('diag', [emit_diag_file/2]).
:- use_module('0_dot_expand/0_type_plane',
              [world_row_shape_violation/3, type_definitions/2]).
:- use_module('0_rel_record', [rel_cols/4]).
:- use_module('compile/0_trace',
              [ dl6_trace_on/0, reset_step_trace/0, record_step/3,
                write_step_trace/2, run_compile_step/4,
                capture_phase_measurement/2, statistics_snapshot/1,
                zero_phase_measurement/1 ]).
:- use_module('0_generic_expand', [generated_generic_name/1]).
:- use_module(compile_messages,
              [ dl6_debug/3, dl6_debugging/1, dl6_reset_checkpoint/0,
                dl6_last_checkpoint/1, dl6_program_sizes/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- meta_predicate measure_phase(0, -, -).

% ═══ reading ═════════════════════════════════════════════════════════════════

read_fixture_term(File, Name, Term, Bindings) :-
    open(File, read, Stream),
    call_cleanup(find_fixture(Stream, Name, Term, Bindings), close(Stream)).

% Operators declared with op/3 inside a module are local to THAT module's own
% clause parsing; they do not carry over to a raw read_term/3 stream read.
% Every fixture file opens with its own `:- op(...)` directives (FIXTURES.md:
% "copy them from fixtures/merge_family.pl"), which normal consult would
% execute in sequence, making <-/<+/:= visible for the REST of that same
% file. This reader replays that: a directive term is CALLED, exactly what
% consult does, so the file's own operator declarations take effect for
% later terms in the same scan. compile.pl's own top-of-file op/3 lines are
% therefore redundant with this replay but kept for readability/IDE parsing.
find_fixture(Stream, Name, Term, Bindings) :-
    read_term(Stream, Candidate, [variable_names(CandidateBindings)]),
    ( Candidate == end_of_file
    -> throw(fixture_not_found(Name))
    ; Candidate = (:- Directive)
    -> call(Directive), find_fixture(Stream, Name, Term, Bindings)
    ; Candidate = fixture(Name, _, _, _, _)
    -> Term = Candidate, Bindings = CandidateBindings
    ; find_fixture(Stream, Name, Term, Bindings)
    ).

% @comment-ok: plan/9 field contract, the record's single documentation site
% ═══ the compile plan : everything lower.pl and emit_ts.pl need, computed
% once so both stay pure functions of it rather than re-deriving it ═════════
%
% plan(Name, Prog, Types, RelPlans, ArrivalTargets, RuleOrder, EdgeRules,
%      SubscribedRels, InternMode)
%   Types: 0_type_plane.pl:type_definitions/2 over Prog's Decls; carried so
%          plan consumers read it instead of re-deriving it.
%   RelPlans: 0_rel_record.pl's rel/5, one per ref program_refs/2 or a typed
%             declaration finds; arrival targets and derived rels alike.
%   RuleOrder: level rules in strat.pl:sql_rule_order/2 order.
%   EdgeRules: edge rules, program order (engine.pl tries edge rules in
%              program order for each occurrence; with at most one edge rule
%              per target fixture this is a formality kept for generality).
%   SubscribedRels: 2_subscribe.pl's cone, sorted Name/Arity. Computed here
%              and threaded to emission; nothing else reads it.

% 1_host_expand.pl is SHARED with the reference engine, so its unsupported constructs are
% thrown in the oracle's vocabulary: a bare term. This door wraps them, the
% same way analyze.pl wraps every trigger it takes from 0_program_check.pl,
% because "every compiler unsupported construct is unsupported_construct/1" is what the
% unsupported construct-message umbrella and the sweep's supported/unsupported split both
% read -- scripts/text_door_receipt.pl:classify_term_door_error/3 treats
% anything else as a HARNESS failure, which is how the first host-unsupported construct
% fixtures in the corpus surfaced this. Six host unsupported constructs predate them
% (refused_host_decl, column_mismatch, probe_mismatch, bind_mismatch,
% host_executor_mismatch, query_mismatch) and none had a fixture, so nothing
% had ever put one through this door.
%
% Wrapping HERE rather than inside the pre-pass keeps the pre-pass shared and
% keeps the ORACLE's terms bare, which is the same per-door split every
% trigger in 0_program_check.pl already has. Callers reaching
% compile_host_decl/2 directly still see the bare term; only whole-program
% compilation wraps.
prepare_program_for_compiler(SugaredProg, HostProg) :-
    catch(prepare_program(SugaredProg, HostProg, _, _, _), Refusal,
          throw_as_compiler_unsupported(Refusal)).

throw_as_compiler_unsupported(unsupported_construct(Reason)) :-
    !,
    throw(unsupported_construct(Reason)).
throw_as_compiler_unsupported(Refusal) :-
    throw(unsupported_construct(Refusal)).

preserve_compiler_type_rules(prog(Decls, Rules), Bindings,
                             prog(Decls, RuntimeRules), CompilerRules,
                             CompilerBindings) :-
    partition_compiler_type_rules(Decls, Rules, SourceCompilerRules, RuntimeRules),
    copy_term(SourceCompilerRules-Bindings, CompilerRules-CompilerBindings).
preserve_compiler_type_rules(program(Decls, Rules, Queries), Bindings,
                             program(Decls, RuntimeRules, Queries), CompilerRules,
                             CompilerBindings) :-
    partition_compiler_type_rules(Decls, Rules, SourceCompilerRules, RuntimeRules),
    copy_term(SourceCompilerRules-Bindings, CompilerRules-CompilerBindings).

partition_compiler_type_rules(_, [], [], []).
partition_compiler_type_rules(Decls, [Rule | Rules], [Rule | CompilerRules], RuntimeRules) :-
    compiler_type_rule(Decls, Rule),
    !,
    partition_compiler_type_rules(Decls, Rules, CompilerRules, RuntimeRules).
partition_compiler_type_rules(Decls, [Rule | Rules], CompilerRules, [Rule | RuntimeRules]) :-
    partition_compiler_type_rules(Decls, Rules, CompilerRules, RuntimeRules).

compiler_type_rule(Decls, (Head <- _)) :-
    compound(Head), functor(Head, Name, Arity),
    memberchk(col_type(Name/Arity, return, type), Decls).

materialize_reference_target_rels(prog(Decls0, Rules), prog(Decls, Rules)) :-
    findall(col_type(Name/Arity, Column, Type),
            ( member(type_decl(Name, Specs), Decls0),
              length(Specs, Arity),
              member(col(Column, Type), Specs),
              \+ memberchk(col_type(Name/Arity, Column, Type), Decls0) ),
            MissingColumns),
    append(Decls0, MissingColumns, Decls).

% The catalog rel is compiler-known, so its columns come from lower.pl's
% contract rather than from whatever variables a caller's rule happened to use.
materialize_catalog_rel(prog(Decls0, Rules), prog(Decls, Rules)) :-
    program_uses_catalog(prog(Decls0, Rules), UsesCatalog),
    (   UsesCatalog == true
    ->  catalog_ddl_contract(CatalogName, ColumnSpecs),
        length(ColumnSpecs, Arity),
        findall(col_type(CatalogName/Arity, Column, Type),
                ( member(Column-Type, ColumnSpecs),
                  \+ memberchk(col_type(CatalogName/Arity, Column, Type), Decls0) ),
                CatalogColumns),
        append(Decls0, CatalogColumns, Decls)
    ;   Decls = Decls0
    ).

% THE BUILD DEFAULT, not the contract's. Flip attempt 2026-08-08: referee said
% NO (RUN wrong=13, FINAL wrong=17; plans/2026-08-08-flip-referee-red.md).
default_intern_mode(dict).

program_plan(Term, Plan) :-
    default_intern_mode(Mode),
    program_plan(Term, [intern(Mode)], Plan).

program_plan(fixture(Name, SugaredProg, Initial, Schedule, _Expectations)-Bindings,
             Options, Plan) :-
    % The body-use table is keyed on body terms, so it belongs to ONE program.
    run_compile_step(plan, reset_body_use_cache, reset_body_use_cache, _),
    % On the AUTHOR's text, before any expansion: `__host_demand_*` and the
    % catalog's own col_type decls are the compiler writing its own namespace.
    check_step(reserved_namespace, check_reserved_namespace(SugaredProg)),
    check_step(compiler_type_rules,
               preserve_compiler_type_rules(SugaredProg, Bindings, RuntimeProgram,
                                            CompilerRules, CompilerBindings)),
    run_compile_step(plan, prepare_program_for_compiler,
                     prepare_program_for_compiler(RuntimeProgram,
                                                  HostRuntimeProgram), _),
    HostRuntimeProgram = prog(HostDecls, HostRules),
    append(HostRules, CompilerRules, HostRulesAndCompilerRules),
    HostProg = prog(HostDecls, HostRulesAndCompilerRules),
    % Host preparation stays a PRE-PASS (see engine.pl); the sugar phases run
    % in the order 1_expansion.pl declares.
    append(Bindings, CompilerBindings, ExpansionBindings),
    run_compile_step(plan, expand_program_with_bindings,
                     expand_program_with_bindings(HostProg, ExpansionBindings,
                                                  ExpandedProg, _), _),
    run_compile_step(plan, materialize_reference_target_rels,
                     materialize_reference_target_rels(ExpandedProg,
                                                       ReferencedProg), _),
    run_compile_step(plan, materialize_catalog_rel,
                     materialize_catalog_rel(ReferencedProg, Prog), _),
    Prog = prog(Decls, Rules),
    expanded_program_debug(Decls, Rules),
    % Every later plan step and every plan consumer reads this table.
    run_compile_step(plan, type_definitions,
                     type_definitions(Decls, Types), _),
    % ..._expanded/1, not check_supported_subset/1: Prog is ALREADY expanded
    % here, and the sugared entry expands again. That second expansion was the
    % redundant order site rank R3 removes.
    check_step(supported_subset, check_supported_subset_expanded(Prog)),
    check_step(clock, check_clock_program(Prog)),
    % STRUCT-AS-ROWS, the compiler's half of SLOT-ARRIVAL-MALFORMED. The
    % oracle refuses the same rows in engine.pl:check_world_shapes/3; here it
    % runs at PLAN time so a malformed program never reaches emission and
    % lands in the sweep's `unsupported` bucket beside every other named
    % unsupported construct, rather than compiling and then throwing mid-replay.
    check_step(world_shapes, check_world_shapes(Prog, Initial, Schedule)),
    % Union rule-derived refs with EVERY declared ref (analyze.pl:
    % declared_refs/2's header comment) -- a kind(Ref, _) decl that no rule
    % ever mentions is still a real rel a schedule can write, and must still
    % get a table + arrival handling in the emitted program -- and with every
    % ref an Initial row seeds (analyze.pl:seeded_refs/2), which the oracle
    % stores whether or not anything declares it.
    run_compile_step(plan, program_refs, program_refs(Rules, RuleRefs), _),
    run_compile_step(plan, declared_refs, declared_refs(Decls, DeclaredRefs), _),
    run_compile_step(plan, seeded_refs, seeded_refs(Initial, SeededRefs), _),
    append([RuleRefs, DeclaredRefs, SeededRefs], AllRefs0), sort(AllRefs0, AllRefs),
    check_step(single_arity, check_single_arity_per_name(AllRefs)),
    run_compile_step(plan, derived_refs, derived_refs(Rules, DerivedRefs), _),
    % The catalog is seeded by DDL, so it is never an arrival target; leaving it
    % in would open the serve door to writes against a compiler-owned table.
    catalog_ddl_contract(CatalogName, CatalogSpecs),
    length(CatalogSpecs, CatalogArity),
    subtract(AllRefs, [CatalogName/CatalogArity | DerivedRefs], ArrivalTargets),
    % EXPRESSION + AGGREGATE LIFT: one program-wide typing fixpoint replaces
    % the per-ref rel_column_types/7 call. Same answer for every column that
    % has a literal witness or a declaration (so every pre-lift fixture keeps
    % its exact types); the difference is a column written ONLY by a level
    % rule's head expression, which now inherits that expression's type
    % instead of falling to the no-witness TEXT default. That default was the
    % TEXT-collapse ("12" vs 12) fail-first check (a) names.
    run_compile_step(plan, rel_columns,
        findall(Ref-Columns,
                ( member(Ref, AllRefs),
                  rel_columns(Decls, Rules, Bindings, Ref, Columns) ),
                RefColumns), _),
    run_compile_step(plan, program_column_types,
                     program_column_types(Decls, Types, Rules, Initial, Schedule,
                                          AllRefs, RefColumns, RefTypes), _),
    % Ordered after the typing fixpoint: a stored rel's physical name carries a
    % digest of the storage shape that fixpoint settles.
    run_compile_step(plan, relation_shapes,
                     relation_shapes(Decls, AllRefs, RefColumns, RefTypes,
                                     Shapes), _),
    run_compile_step(plan, relation_storage_names,
                     relation_storage_names(Name, Decls, DerivedRefs, Shapes,
                                            AllRefs, StorageNames), _),
    run_compile_step(plan, rel_plans,
        findall(rel(Ref, StorageName, Kind, Cols, KeyOrNone),
                ( member(Ref, AllRefs),
                  memberchk(Ref-StorageName, StorageNames),
                  rel_kind(Decls, Ref, Kind),
                  memberchk(Ref-Columns, RefColumns),
                  memberchk(Ref-ColumnTypes, RefTypes),
                  ( decl_key(Decls, Ref, Positions) -> KeyOrNone = key(Positions) ; KeyOrNone = none ),
                  maplist(column_origin(Decls, Ref), Columns, Origins),
                  rel_cols(Columns, Origins, ColumnTypes, Cols)
                ), RelPlans), _),
    % PHASE C2 RULING 1 x RULING 2: this needs RelPlans (ColumnTypes), so it
    % runs here rather than inside check_supported_subset/1 above (which
    % runs before RelPlans exists).
    check_step(edge_head_column_types,
               check_edge_head_column_types(RelPlans, Rules)),
    run_compile_step(plan, sql_rule_order, sql_rule_order(Rules, RuleOrder), _),
    run_compile_step(plan, edge_rules,
                     include(rule_is_edge, Rules, EdgeRules), _),
    % The query decls are the cone's only seeds, read from the POST-expansion
    % Decls for the same reason emit_ts.pl:world_plan_lines/2 reads them there.
    findall(QueryAtom,
            ( member(QueryDecl, Decls), query_decl(QueryDecl, QueryAtom, _) ),
            Queries),
    run_compile_step(plan, subscribed_rels,
                     subscribed_rels(Decls, Rules, Queries, SubscribedRels), _),
    intern_mode(Options, InternMode),
    Plan = plan(Name, Prog, Types, RelPlans, ArrivalTargets, RuleOrder,
                EdgeRules, SubscribedRels, InternMode),
    plan_debug(RelPlans, ArrivalTargets, SubscribedRels, InternMode).

:- meta_predicate check_step(+, 0).

% The step is named BEFORE it runs: a wedge or a failure inside plan then
% reports which check it stopped in rather than only the phase.
check_step(Label, Goal) :-
    dl6_debug(check, "~w", [Label]),
    run_compile_step(plan, Label, Goal, _).

expanded_program_debug(Decls, Rules) :-
    (   dl6_debugging(plan)
    ->  length(Decls, DeclCount),
        length(Rules, RuleCount),
        dl6_debug(plan, "expanded decls=~d rules=~d", [DeclCount, RuleCount])
    ;   true
    ).

plan_debug(RelPlans, ArrivalTargets, SubscribedRels, InternMode) :-
    (   dl6_debugging(plan)
    ->  length(RelPlans, RelCount),
        length(ArrivalTargets, ArrivalCount),
        length(SubscribedRels, SubscribedCount),
        dl6_debug(plan,
                  "planned rels=~d arrival targets=~d subscribed=~d intern=~w",
                  [RelCount, ArrivalCount, SubscribedCount, InternMode])
    ;   true
    ).

% The same test analyze.pl:seed_column_contribution/9 freezes a type on, so
% the record's declared slot and the fixpoint's `frozen` cannot disagree.
column_origin(Decls, Ref, Column, Origin) :-
    (   memberchk(col_type(Ref, Column, DeclaredType), Decls)
    ->  Origin = declared(DeclaredType)
    ;   Origin = inferred
    ).

% ═══ the compiler-owned `__` namespace ══════════════════════════════════════
% SQLite gives tables and views one namespace, so a user `__txt_x` collides.

% Derived from the catalog contract rather than listed twice, so a future
% contract row gets its reservation for free.
compiler_owned_contract(Name) :- catalog_ddl_contract(Name, _).

reserved_namespace_name(Name) :-
    atom(Name),
    sub_atom(Name, 0, 2, _, '__').

check_reserved_namespace(SugaredProg) :-
    (   SugaredProg = prog(Decls, _Rules),
        reserved_relation_value_violation(Decls, obj/1)
    ->  throw(unsupported_construct(reserved_relation_value_carrier(obj/1)))
    ;   SugaredProg = prog(Decls, Rules),
        reserved_namespace_violation(Decls, Rules, Name)
    ->  throw(unsupported_construct(reserved_rel_namespace(Name)))
    ;   true
    ).

% obj/1 is the canonical runtime carrier for relation-valued columns. A
% declaration with that reference would collide before contextual normalization.
reserved_relation_value_violation(Decls, Ref) :-
    declared_refs(Decls, DeclaredRefs),
    memberchk(Ref, DeclaredRefs).

% Reading a contract rel is allowed and writing one is not, which is the split
% compile.pl already enforces by subtracting the catalog from ArrivalTargets.
reserved_namespace_violation(Decls, Rules, Name) :-
    declared_refs(Decls, DeclaredRefs),
    derived_refs(Rules, DerivedRefs),
    program_refs(Rules, RuleRefs),
    append(DeclaredRefs, DerivedRefs, WrittenRefs),
    (   member(Name/_, WrittenRefs),
        reserved_namespace_name(Name)
    ;   member(Name/_, RuleRefs),
        reserved_namespace_name(Name),
        \+ compiler_owned_contract(Name),
        \+ option_enum_generated_name(Name),
        \+ generated_generic_name(Name)
    ),
    !.

% Reading a minted '__opt_<t>' rel in a body is ordinary enum consumption;
% declaring or head-writing one stays a reserved-namespace refusal.
option_enum_generated_name(Name) :-
    atom(Name),
    sub_atom(Name, 0, _, _, '__opt_').

% ═══ top level ═══════════════════════════════════════════════════════════════

compile_fixture(Name, FixtureFile, OutFile) :-
    compile_fixture(Name, FixtureFile, OutFile, emit_ts:emit_program).

compile_fixture(Name, FixtureFile, OutFile, Emitter) :-
    default_intern_mode(Mode),
    compile_fixture(Name, FixtureFile, OutFile, Emitter, [intern(Mode)]).

compile_fixture(Name, FixtureFile, OutFile, Emitter, Options) :-
    read_fixture_term(FixtureFile, Name, Term, Bindings),
    Term = fixture(Name, _Prog, Initial, _Schedule, _Expectations),
    compile_program(Name, Term, Bindings, Initial, OutFile, Emitter, Options).

check_world_shapes(prog(Decls, _), Initial, Schedule) :-
    append([Initial | Schedule], WorldRows),
    (   world_row_shape_violation(Decls, WorldRows, mismatch(Ref, Column, TypeName, Reason))
    ->  ( Reason = int_out_of_range(Value)
        -> throw(unsupported_construct(int_out_of_range(Ref, Column, Value)))
        ;  throw(unsupported_construct(
                    type_arrival_shape_mismatch(Ref, Column, TypeName, Reason)))
        )
    ;   true
    ).

% table_name/2 drops the arity, so two arities of one name collide on one table.
% Refs arrives sorted, so equal names are adjacent and one walk answers it.
check_single_arity_per_name([]).
check_single_arity_per_name([_]) :- !.
check_single_arity_per_name([Name/LowArity, Name/HighArity | _]) :-
    LowArity \== HighArity,
    !,
    throw_as_compiler_unsupported(rel_arity_collision(Name, LowArity, HighArity)).
check_single_arity_per_name([_ | Rest]) :-
    check_single_arity_per_name(Rest).

% The authored Ref remains Name/Arity throughout semantic analysis.  This
% registry gives the storage lowering a second, physical identity from the
% module that declared the relation.  A same Ref declared by separate modules
% is already one runtime relation before lowering, so refuse it rather than
% inventing two SQLite tables that the runtime cannot distinguish.
relation_storage_names(EntryStem, Decls, DerivedRefs, Shapes, Refs, Names) :-
    rel_module_hash_index(Decls, HashIndex),
    maplist(relation_storage_candidate(EntryStem, Decls, HashIndex, DerivedRefs,
                                       Shapes),
            Refs, Candidates),
    keysort(Candidates, Ordered),
    allocate_storage_names(Ordered, [], Names).

relation_storage_candidate(EntryStem, Decls, HashIndex, DerivedRefs, Shapes,
                           Ref, Key-(Ref-Base)) :-
    Ref = Name/Arity,
    relation_declaring_module(EntryStem, Decls, HashIndex, Ref, ModuleStem),
    storage_identifier(ModuleStem, ModulePart),
    storage_identifier(Name, RelationPart),
    storage_base_name(ModulePart, RelationPart, Prefixed),
    storage_shape_suffix(DerivedRefs, Shapes, Ref, Base, Prefixed),
    sqlite_ascii_fold(Base, Folded),
    Key = key(Folded, ModuleStem, Name, Arity).

% ═══ shape identity : docs/storage-name-hash.md ═════════════════════════════

% THE DERIVED SEAM: a derived rel keeps the bare prefixed spelling.
storage_shape_suffix(DerivedRefs, _Shapes, Ref, Prefixed, Prefixed) :-
    memberchk(Ref, DerivedRefs),
    !.
storage_shape_suffix(_DerivedRefs, _Shapes, Name/_Arity, Prefixed, Prefixed) :-
    reserved_namespace_name(Name),
    !.
storage_shape_suffix(_DerivedRefs, Shapes, Ref, Base, Prefixed) :-
    storage_shape_digest(Shapes, Ref, Digest),
    !,
    atomic_list_concat([Prefixed, Digest], '_', Base).
storage_shape_suffix(_DerivedRefs, _Shapes, _Ref, Prefixed, Prefixed).

%! storage_shape_digest(+Shapes, +Ref, -Digest) is semidet.
%   Fails for a Ref with no shape, the same Ref the rel record drops.
storage_shape_digest(Shapes, Ref, Digest) :-
    memberchk(Ref-OwnShape, Shapes),
    shape_closure(Shapes, [Ref], [], Reached0),
    sort(Reached0, Reached),
    findall(Target-Shape,
            ( member(Target, Reached),
              Target \== Ref,
              memberchk(Target-Shape, Shapes) ),
            Referenced),
    format(atom(Canonical), '~q', [OwnShape-Referenced]),
    short_hash(Canonical, Full),
    sub_atom(Full, 0, 12, _, Digest).

shape_closure(_Shapes, [], Reached, Reached).
shape_closure(Shapes, [Ref | Rest], Seen, Reached) :-
    memberchk(Ref, Seen),
    !,
    shape_closure(Shapes, Rest, Seen, Reached).
shape_closure(Shapes, [Ref | Rest], Seen, Reached) :-
    (   memberchk(Ref-shape(_Kind, Columns, _Key), Shapes)
    ->  shape_column_targets(Shapes, Columns, Targets)
    ;   Targets = []
    ),
    append(Targets, Rest, Pending),
    shape_closure(Shapes, Pending, [Ref | Seen], Reached).

% A target type name resolves through the shape table, so the arity comes from
% the program rather than from a second lookup.
shape_column_targets(Shapes, Columns, Targets) :-
    findall(TargetName/Arity,
            ( member(column(_, ColumnType), Columns),
              type_reference_name(ColumnType, TargetName),
              memberchk(TargetName/Arity-_, Shapes) ),
            Targets).

type_reference_name(ref(Name), Name) :- !.
type_reference_name(Type, Name) :-
    compound(Type),
    Type =.. [_ | Arguments],
    member(Argument, Arguments),
    type_reference_name(Argument, Name).

%! relation_shapes(+Decls, +Refs, +RefColumns, +RefTypes, -Shapes) is det.
%   One shape/3 per rel: what SQLite stores, never rule text or filename.
relation_shapes(Decls, Refs, RefColumns, RefTypes, Shapes) :-
    findall(Ref-shape(Kind, Columns, KeyOrNone),
            ( member(Ref, Refs),
              rel_kind(Decls, Ref, Kind),
              memberchk(Ref-Names, RefColumns),
              memberchk(Ref-ColumnTypes, RefTypes),
              maplist(shape_column, Names, ColumnTypes, Columns),
              ( decl_key(Decls, Ref, Positions)
              -> KeyOrNone = key(Positions)
              ;  KeyOrNone = none )
            ), Shapes).

shape_column(Name, ColumnType, column(Name, ColumnType)).

% EntryStem is the compilation unit's own name: the .dl6 entry file stem on
% the text path, the fixture name on the term path.  Both paths must reach
% the same physical spelling for the same program, which is what
% compile/scripts/text_door_receipt.pl compares byte for byte.
relation_declaring_module(EntryStem, Decls, HashIndex, Name/_Arity, ModuleStem) :-
    rel_module_hashes(HashIndex, Decls, Name, Hashes),
    relation_declaring_module_stem(EntryStem, Decls, Name, Hashes, ModuleStem).

% One grouping pass over the declarations rather than a findall/3 over all of
% them per reference: pokeapi ran 224 references against 1434 declarations,
% 20.0 ms. A non-ground relation name cannot be a group key without changing
% which declarations member/2's unification reaches, so that case keeps the
% scan.
rel_module_hash_index(Decls, Index) :-
    findall(Name-Hash, member(rel_module_decl(Name, Hash), Decls), Pairs),
    (   forall(member(Name-_, Pairs), ground(Name))
    ->  keysort(Pairs, Sorted),
        group_pairs_by_key(Sorted, Grouped),
        Index = rel_module_hashes(Grouped)
    ;   Index = unkeyed
    ).

rel_module_hashes(rel_module_hashes(Grouped), _Decls, Name, Hashes) :-
    ground(Name),
    !,
    (   memberchk(Name-Unsorted, Grouped)
    ->  sort(Unsorted, Hashes)
    ;   Hashes = []
    ).
rel_module_hashes(_Index, Decls, Name, Hashes) :-
    findall(Hash, member(rel_module_decl(Name, Hash), Decls), Unsorted),
    sort(Unsorted, Hashes).

relation_declaring_module_stem(_EntryStem, Decls, _Name, [Hash], ModuleStem) :-
    !,
    (   memberchk(module_storage_decl(Hash, Stem), Decls)
    ->  ModuleStem = Stem
    ;   ModuleStem = none
    ).
relation_declaring_module_stem(_EntryStem, _Decls, Name, [First, Second | Rest], _) :-
    Hashes = [First, Second | Rest],
    throw_as_compiler_unsupported(rel_module_identity_collision(Name, Hashes)).
relation_declaring_module_stem(_EntryStem, _Decls, Name, [], none) :-
    compiler_owned_contract(Name),
    !.
relation_declaring_module_stem(_EntryStem, Decls, _Name, [], ModuleStem) :-
    memberchk(entry_module_decl(Hash), Decls),
    memberchk(module_storage_decl(Hash, ModuleStem), Decls),
    !.
relation_declaring_module_stem(EntryStem, _Decls, _Name, [], EntryStem).

% A compiler-owned relation keeps its established physical spelling; every
% other relation carries the module path that declared it.
storage_identifier(none, '') :- !.
storage_identifier(Value, Identifier) :-
    atom_codes(Value, Codes),
    maplist(storage_identifier_code, Codes, SafeCodes),
    atom_codes(Identifier, SafeCodes).

storage_identifier_code(Code, Code) :-
    ( Code >= 0'a, Code =< 0'z
    ; Code >= 0'A, Code =< 0'Z
    ; Code >= 0'0, Code =< 0'9
    ; Code =:= 0'_
    ), !.
storage_identifier_code(_, 0'_).

storage_base_name('', RelationPart, RelationPart) :- !.
storage_base_name(ModulePart, RelationPart, Base) :-
    atomic_list_concat([ModulePart, RelationPart], '_', Base).

sqlite_ascii_fold(Text, Folded) :-
    atom_codes(Text, Codes),
    maplist(sqlite_ascii_fold_code, Codes, FoldedCodes),
    atom_codes(Folded, FoldedCodes).

sqlite_ascii_fold_code(Code, Folded) :-
    Code >= 0'A, Code =< 0'Z,
    !,
    Folded is Code + 32.
sqlite_ascii_fold_code(Code, Code).

allocate_storage_names([], _, []).
allocate_storage_names([_Key-(Ref-Base) | Rest], UsedFolds,
                       [Ref-StorageName | Names]) :-
    unique_storage_name(Base, UsedFolds, StorageName, StorageFold),
    allocate_storage_names(Rest, [StorageFold | UsedFolds], Names).

% A suffix minted for an ASCII-fold collision can itself equal another source
% base (person vs person_2).  Reserve every final folded spelling, not only
% each source base group, so SQLite never aliases the two tables.
unique_storage_name(Base, UsedFolds, StorageName, StorageFold) :-
    sqlite_ascii_fold(Base, BaseFold),
    (   memberchk(BaseFold, UsedFolds)
    ->  unique_storage_suffix(Base, 2, UsedFolds, StorageName, StorageFold)
    ;   StorageName = Base,
        StorageFold = BaseFold
    ).

unique_storage_suffix(Base, Suffix, UsedFolds, StorageName, StorageFold) :-
    format(atom(Candidate), '~w_~w', [Base, Suffix]),
    sqlite_ascii_fold(Candidate, CandidateFold),
    (   memberchk(CandidateFold, UsedFolds)
    ->  NextSuffix is Suffix + 1,
        unique_storage_suffix(Base, NextSuffix, UsedFolds, StorageName, StorageFold)
    ;   StorageName = Candidate,
        StorageFold = CandidateFold
    ).

compile_dl6(File, OutFile) :-
    default_intern_mode(Mode),
    compile_dl6(File, OutFile, [intern(Mode)]).

compile_dl6(File, OutFile, Options) :-
    file_base_name(File, BaseName),
    file_name_extension(Name, _Extension, BaseName),
    dl6_reset_checkpoint,
    reset_step_trace,
    dl6_debug(parse, "source ~w", [File]),
    catch(
        ( run_compile_phase(Name, parse,
                            expand_uses(File, [], [], _, Prog, _,
                                        Bindings, Findings),
                            ParseMeasurement),
          parse_debug(Prog, Findings),
          ( Findings == []
          -> true
          ; throw(unsupported_construct(surface_findings(Findings)))
          ) ),
        ParseError,
        ( emit_diag_file(File, ParseError),
          throw(ParseError) )
    ),
    run_compile_step(driver, dl6_seeded_form,
                     dl6_seeded_form(Prog, Initial, ProgOut), _),
    emitter_option(Options, Emitter),
    run_compile_step(driver, schedule_option,
                     schedule_option(Options, Prog, Bindings, Schedule), _),
    catch(
        compile_program_phases(Name, fixture(Name, ProgOut, Initial, Schedule, []),
                               Bindings, Initial, OutFile, Emitter,
                               Options, PhaseMeasurements),
        Error,
        throw_text_door_error(File, Error)
    ),
    write_compile_trace(
        Name, [phase(parse, ParseMeasurement) | PhaseMeasurements]).

% The .dl6 text door default is emit_ts:emit_program; an emitter(Module:Pred)
% option swaps it in with no call-site special case, and a caller that passes
% no emitter keeps byte-identical output.
emitter_option(Options, Emitter) :-
    ( memberchk(emitter(Emitter), Options) -> true ; Emitter = emit_ts:emit_program ).

% A .dl6 TEXT program has no spelling for an arrival schedule (Initial comes
% from dl6 facts); an external schedule is the only form that exists, so the
% schedule(File) option is the door that fills the fixture term's Schedule
% slot. Same external JSON shape sweep.pl writes and the http client posts.
schedule_option(Options, Prog, Bindings, Schedule) :-
    ( memberchk(schedule(File), Options)
    -> read_schedule_file(Prog, Bindings, File, Schedule)
    ;  Schedule = []
    ).

read_schedule_file(Prog, Bindings, ScheduleFile, Schedule) :-
    setup_call_cleanup(
        open(ScheduleFile, read, Stream),
        json_read_dict(Stream, Batches, [value_string_as(string)]),
        close(Stream)),
    maplist(schedule_batch_terms(Prog, Bindings), Batches, Schedule).

schedule_batch_terms(Prog, Bindings, Batch, Terms) :-
    maplist(arrival_term(Prog, Bindings), Batch, Terms).

% arrival_term/4 mirrors dl6_oracle.pl's read_schedule: rel-name and arity
% reset an atom's type from the program's column decls, so a json column's
% arrival is parsed into json terms rather than left as a string.
arrival_term(Prog, Bindings, Arrival, Term) :-
    atom_string(Rel, Arrival.rel),
    length(Arrival.row, Arity),
    arrival_column_types(Prog, Bindings, Rel/Arity, ColumnTypes),
    maplist(schedule_value(text_door, Rel), ColumnTypes, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; throw(unsupported_construct(bad_arrival_sign(Arrival.sign)))
    ).

% Shared by the two text-door callers that parse .dl6 themselves.
% Ground bodiless clauses become seed rows; non-ground ones stay refused.
dl6_seeded_form(Prog, Initial, ProgOut) :-
    Prog = prog(Decls, Rules),
    !,
    partition_dl6_facts(Decls, Rules, Initial, RealRules),
    ProgOut = prog(Decls, RealRules).
dl6_seeded_form(Prog, Initial, ProgOut) :-
    Prog = program(Decls, Rules, Queries),
    !,
    partition_dl6_facts(Decls, Rules, Initial, RealRules),
    ProgOut = program(Decls, RealRules, Queries).
dl6_seeded_form(Prog, [], Prog).

partition_dl6_facts(_, [], [], []).
partition_dl6_facts(Decls, [Rule | Rules], [Fact | Facts], Rest) :-
    dl6_fact_in_decls(Decls, Rule, Fact),
    \+ compiler_type_fact(Decls, Fact),
    !,
    partition_dl6_facts(Decls, Rules, Facts, Rest).
partition_dl6_facts(Decls, [Rule | Rules], Facts, [Rule | Rest]) :-
    partition_dl6_facts(Decls, Rules, Facts, Rest).

dl6_fact_in_decls(_, Rule, Fact) :-
    dl6_fact(Rule, Fact),
    !.
dl6_fact_in_decls(Decls, (Head0 <- true), Fact) :-
    ground(Head0),
    dotted_relation_head(Head0),
    resolve_relation_paths(Decls, [(Head0 <- true)], [ResolvedRule]),
    dl6_fact(ResolvedRule, Fact).

dotted_relation_head(rel_path(Segments, Args)) :-
    is_list(Segments),
    is_list(Args).
dotted_relation_head(Head) :-
    compound(Head),
    functor(Head, '.', 2).

compiler_type_fact(Decls, Fact) :-
    compound(Fact),
    functor(Fact, Name, Arity),
    memberchk(col_type(Name/Arity, return, type), Decls).

% Structured-term arguments (e.g. a ts_query value) are not seed-row shaped;
% they keep the rule path their fixtures already compile through.
dl6_fact((Head <- true), Head) :-
    !,
    ground(Head),
    fact_args_atomic(Head).
dl6_fact(Term, Term) :-
    ground(Term),
    \+ Term = match(_, _),
    fact_args_atomic(Term).

fact_args_atomic(Fact) :-
    Fact =.. [_ | Args],
    forall(member(Arg, Args), atomic(Arg)).

% EXPORTED: scripts/bop_check.pl calls compile_program/6 itself, so it needs
% the same wrapper compile_dl6/2's own catch site applies.
throw_text_door_error(File, Error) :-
    Error = unsupported_construct(at(_, _, _)),
    !,
    emit_diag_file(File, Error),
    throw(Error).
throw_text_door_error(File, unsupported_construct(Reason)) :-
    ( parse_dl_line_for_reason(Reason, Line)
    -> Emitted = unsupported_construct(at(File, Line, Reason))
    ;  Emitted = unsupported_construct(Reason)
    ),
    emit_diag_file(File, Emitted),
    throw(Emitted).
throw_text_door_error(_File, Error) :-
    throw(Error).

compile_program(Name, Term, Bindings, Initial, OutFile, Emitter) :-
    default_intern_mode(Mode),
    compile_program(Name, Term, Bindings, Initial, OutFile, Emitter,
                    [intern(Mode)]).

compile_program(Name, Term, Bindings, Initial, OutFile, Emitter, Options) :-
    reset_step_trace,
    compile_program_phases(Name, Term, Bindings, Initial, OutFile, Emitter,
                           Options, PhaseMeasurements),
    zero_phase_measurement(EmptyMeasurement),
    write_compile_trace(
        Name, [phase(parse, EmptyMeasurement) | PhaseMeasurements]).

% frontier(shared) consolidates transient frontier state; absent = per_rel,
% byte-identical output (plans/2026-08-19-shared-sqlite-frontier.md).
frontier_option(Options, Mode) :-
    (   memberchk(frontier(Mode0), Options)
    ->  ( memberchk(Mode0, [per_rel, shared])
        -> Mode = Mode0
        ;  throw(unsupported_construct(bad_frontier_option(Mode0)))
        )
    ;   Mode = per_rel
    ).

compile_program_phases(Name, Term, Bindings, Initial, OutFile, Emitter,
                       Options, PhaseMeasurements) :-
    frontier_option(Options, FrontierMode),
    with_frontier_mode(FrontierMode,
        compile_program_phases_moded(Name, Term, Bindings, Initial, OutFile,
                                     Emitter, Options, PhaseMeasurements)).

compile_program_phases_moded(Name, Term, Bindings, Initial, OutFile, Emitter,
                             Options, PhaseMeasurements) :-
    dl6_reset_checkpoint,
    run_compile_phase(Name, plan,
                      program_plan(Term-Bindings, Options, Plan),
                      PlanMeasurement),
    run_compile_phase(Name, lower,
                      lower_program(Plan, Lowered),
                      LowerMeasurement),
    Plan = plan(_, prog(Decls, _), Types, RelPlans, _, _, _, _, Mode),
    Lowered = lowered(_, _, ArrivalStatements, EdgeStatements, LevelStatements,
                      DeltaStatements, _, _),
    lower_debug(ArrivalStatements, EdgeStatements, LevelStatements,
                DeltaStatements),
    run_compile_phase(
        Name, boot,
        boot_statements(Mode, Decls, Types, RelPlans, Initial, LevelStatements,
                        BootStatements),
        BootMeasurement),
    boot_debug(Initial, BootStatements),
    run_compile_phase(
        Name, emit,
        with_emit_context(Initial, Term,
            call(Emitter, Name, Plan, Lowered, BootStatements, Text)),
        EmitMeasurement),
    emit_debug(Emitter, Text),
    run_compile_phase(
        Name, write,
        write_compiled_output(OutFile, Text),
        WriteMeasurement),
    PhaseMeasurements = [ phase(plan, PlanMeasurement),
                          phase(lower, LowerMeasurement),
                          phase(boot, BootMeasurement),
                          phase(emit, EmitMeasurement),
                          phase(write, WriteMeasurement)
                        ].

write_compiled_output(OutFile, Text) :-
    setup_call_cleanup(
        open(OutFile, write, Stream),
        format(Stream, "~s", [Text]),
        close(Stream)),
    (   dl6_debugging(write)
    ->  size_file(OutFile, Bytes),
        dl6_debug(write, "~w bytes=~d", [OutFile, Bytes])
    ;   true
    ),
    format("wrote ~w~n", [OutFile]).

% The dd emitter reads Initial and Schedule out of band; both are available
% here (Initial is the argument, Schedule is in the fixture term), so they are
% asserted around the seam call and retracted once it returns. emit_ts never
% reads the context, so the statement does not move its output.
with_emit_context(Initial, fixture(_, _, _, Schedule, _), Goal) :-
    assertz(dd_compile_context(Initial, Schedule)),
    setup_call_cleanup(true, Goal, retractall(dd_compile_context(_, _))).

% A phase that FAILS carries no ball, so the diagnosis is printed here, where
% the checkpoint is still the one the phase reached; the thrown term keeps its
% shape for the callers that classify it.
run_compile_phase(Name, Phase, Goal, Measurement) :-
    dl6_debug(Phase, "begin program=~w", [Name]),
    measure_phase(Goal, Measurement, Outcome),
    (   Outcome == failed
    ->  dl6_last_checkpoint(Checkpoint),
        print_message(error, compile_phase_failed(Phase, Name, Checkpoint)),
        throw(compile_phase_failed(Phase))
    ;   phase_wall_debug(Phase, Measurement),
        restore_phase_outcome(Outcome)
    ).

phase_wall_debug(Phase, measurement(WallMs, _, Inferences, _, _, _, _, _, _, _,
                                    _, _)) :-
    dl6_debug(Phase, "done wall=~wms inferences=~w", [WallMs, Inferences]).

% Every count below walks a list, so each one is computed only under its own
% topic; the phase begin/done lines carry the checkpoint when they are off.
parse_debug(Prog, Findings) :-
    (   dl6_debugging(parse)
    ->  dl6_program_sizes(Prog, DeclCount, RuleCount),
        length(Findings, FindingCount),
        dl6_debug(parse, "parsed decls=~d rules=~d findings=~d",
                  [DeclCount, RuleCount, FindingCount])
    ;   true
    ).

lower_debug(ArrivalStatements, EdgeStatements, LevelStatements,
            DeltaStatements) :-
    (   dl6_debugging(lower)
    ->  length(ArrivalStatements, ArrivalCount),
        length(EdgeStatements, EdgeCount),
        length(LevelStatements, LevelCount),
        length(DeltaStatements, DeltaCount),
        dl6_debug(lower, "arrival=~d edge=~d level=~d delta=~d",
                  [ArrivalCount, EdgeCount, LevelCount, DeltaCount])
    ;   true
    ).

boot_debug(Initial, BootStatements) :-
    (   dl6_debugging(boot)
    ->  length(Initial, SeedCount),
        length(BootStatements, BootCount),
        dl6_debug(boot, "seed rows=~d boot statements=~d",
                  [SeedCount, BootCount])
    ;   true
    ).

emit_debug(Emitter, Text) :-
    (   dl6_debugging(emit)
    ->  emitted_size(Text, CharCount),
        dl6_debug(emit, "emitter=~w characters=~d", [Emitter, CharCount])
    ;   true
    ).

% Emitters hand back a code list or an atom; `~s` accepts both, so the size
% probe has to as well.
emitted_size(Text, Size) :-
    is_list(Text),
    !,
    length(Text, Size).
emitted_size(Text, Size) :-
    atom_length(Text, Size).

measure_phase(Goal, Measurement, Outcome) :-
    setup_call_cleanup(
        statistics_snapshot(Before),
        phase_outcome(Goal, Outcome),
        capture_phase_measurement(Before, Measurement)).

phase_outcome(Goal, Outcome) :-
    catch(
        ( once(call(Goal))
        -> Outcome = ok
        ; Outcome = failed
        ),
        Error,
        Outcome = error(Error)).

restore_phase_outcome(ok).
restore_phase_outcome(failed) :-
    fail.
restore_phase_outcome(error(Error)) :-
    throw(Error).

write_compile_trace(Name, PhaseMeasurements) :-
    phase_trace_measurement(PhaseMeasurements, parse, ParseMeasurement),
    phase_trace_measurement(PhaseMeasurements, plan, PlanMeasurement),
    phase_trace_measurement(PhaseMeasurements, lower, LowerMeasurement),
    phase_trace_measurement(PhaseMeasurements, boot, BootMeasurement),
    phase_trace_measurement(PhaseMeasurements, emit, EmitMeasurement),
    phase_trace_measurement(PhaseMeasurements, write, WriteMeasurement),
    phase_trace_measurement_values(ParseMeasurement, ParseWall, ParseInf),
    phase_trace_measurement_values(PlanMeasurement, PlanWall, PlanInf),
    phase_trace_measurement_values(LowerMeasurement, LowerWall, LowerInf),
    phase_trace_measurement_values(BootMeasurement, BootWall, BootInf),
    phase_trace_measurement_values(EmitMeasurement, EmitWall, EmitInf),
    phase_trace_measurement_values(WriteMeasurement, WriteWall, WriteInf),
    TotalWall is ParseWall + PlanWall + LowerWall + BootWall + EmitWall + WriteWall,
    TotalInf is ParseInf + PlanInf + LowerInf + BootInf + EmitInf + WriteInf,
    format(user_error,
           "COMPILE-TRACE program=~w parse=~w/~w plan=~w/~w lower=~w/~w boot=~w/~w emit=~w/~w write=~w/~w total=~w/~w~n",
           [ Name,
             ParseWall, ParseInf,
             PlanWall, PlanInf,
             LowerWall, LowerInf,
             BootWall, BootInf,
             EmitWall, EmitInf,
             WriteWall, WriteInf,
             TotalWall, TotalInf
           ]),
    write_step_trace(Name, PhaseMeasurements).

phase_trace_measurement(PhaseMeasurements, Phase, Measurement) :-
    memberchk(phase(Phase, Measurement), PhaseMeasurements).

phase_trace_measurement_values(measurement(WallMs, _CpuMs, Inferences, _GcCount,
                                           _GcReclaimedBytes, _GcMs,
                                           _GcLeftBytes, _TableCount,
                                           _TableAnswers, _TableReuses,
                                           _TableSpaceBytes,
                                           _TableCompiledSpaceBytes),
                               WallMs, Inferences).
