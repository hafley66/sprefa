% analyze.pl : structural analysis of a prog(Decls, Rules) term form. Reads
% relation kind (mirrors engine.pl:88-93 rel_kind/4, reimplemented here since
% that predicate is not exported and depends only on Decls), which refs are
% EDB (an arrival schedule may write them: never a rule head) vs derived
% (headed by a level or edge rule), and per-ref column names mined from typed
% declaration entries or the ORIGINAL surface variable names the caller recovers via
% read_term(Stream, Term, [variable_names(Bindings)]) -- column identity comes
% from the fixture source text, never invented here.
%
% Compiles the SUBSET engine.pl semantics that the two Phase B target
% fixtures use: edge rule bodies are `TriggerAtom` alone (no extra
% joined goal, no pre/1, no departed/1, no guard); level rules are
% non-aggregate, with `not/1` and single-reference self recursion allowed.
% Anything wider
% throws unsupported_construct(What) at analysis time -- a compiler finding,
% not a silent guess.

:- module(analyze,
          [ rel_kind/3, decl_key/3, decl_keep/3, declared_refs/2,
            program_refs/2, arrival_target_refs/2, derived_refs/2,
            edge_headed_refs/2, level_headed_refs/2,
            rule_head_ref/2, rule_is_edge/1, rule_is_level/1,
            body_ref_uses/2, rel_columns/4, rel_columns/5,
            rel_column_types/5, rel_column_types/7, snake_name/2,
            check_supported_subset/1, edge_trigger_shape/2,
            conjunction_goals/2, check_edge_head_column_types/2,
            aggregate_head_template/2, rule_is_aggregate/1,
            body_guard_goals/2, guard_goal/1, bind_goal/3,
            program_column_types/7 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module('../0_match_expand', [expand_match_program/2]).
:- use_module('../conformance/body', [rel_ref/2]).
:- use_module(registry,
              [ surface_for_term/6,
                body_surface_for_term/6
              ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ rel kind (mirrors engine.pl declared_kind/rel_kind exactly) ════════════

declared_kind(Decls, Ref, Kind) :- memberchk(kind(Ref, Kind), Decls).

rel_kind(Decls, Ref, log) :- declared_kind(Decls, Ref, log), !.
rel_kind(Decls, Ref, set) :- declared_kind(Decls, Ref, set), !.
rel_kind(Decls, Ref, set) :- memberchk(keyed(Ref, _), Decls), !.
rel_kind(_, _, set).

decl_key(Decls, Ref, Positions) :- memberchk(keyed(Ref, Positions), Decls).

decl_keep(Decls, Ref, Bound) :- memberchk(keep(Ref, Bound), Decls), !.
decl_keep(_, _, all).

% ═══ rule shape ═════════════════════════════════════════════════════════════

rule_is_edge((_ <+ _)).
rule_is_level((_ <- _)).

rule_head_ref((Head <- _), Ref) :- rel_ref(Head, Ref).
rule_head_ref((Head <+ _), Ref) :- rel_ref(Head, Ref).

rule_head((Head <- _), Head).
rule_head((Head <+ _), Head).

rule_body((_ <- Body), Body).
rule_body((_ <+ Body), Body).

edge_headed_refs(Rules, Refs) :-
    findall(Ref, ( member(Rule, Rules), rule_is_edge(Rule), rule_head_ref(Rule, Ref) ), Refs0),
    sort(Refs0, Refs).

level_headed_refs(Rules, Refs) :-
    findall(Ref, ( member(Rule, Rules), rule_is_level(Rule), rule_head_ref(Rule, Ref) ), Refs0),
    sort(Refs0, Refs).

derived_refs(Rules, Refs) :-
    edge_headed_refs(Rules, EdgeRefs), level_headed_refs(Rules, LevelRefs),
    append(EdgeRefs, LevelRefs, All), sort(All, Refs).

% ═══ body walking ════════════════════════════════════════════════════════════
% body_ref_uses(Body, Uses): Uses = list of use(Ref, Args, Sign, Marking) for
% every relation atom the body reaches, Sign = pos|neg (neg = under not/1,
% which is a strictly-lower-stratum read, never a trigger source), Marking =
% trigger|sampled. Recurses into not/1 (unlike
% body.pl's body_atoms/2, which the engine deliberately keeps shallow there
% because a negated atom is never a trigger candidate; here we want it for
% stratification and column-name mining, both of which DO care about a
% negated read).

body_ref_uses((Left, Right), Uses) :- !,
    body_ref_uses(Left, LeftUses), body_ref_uses(Right, RightUses),
    append(LeftUses, RightUses, Uses).
body_ref_uses(Term, Uses) :-
    body_surface_for_term(Term, _, _, AnalyzeRole, _, _), !,
    analyze_role_uses(AnalyzeRole, Term, Uses).
body_ref_uses(Atom, [use(Ref, Args, pos, trigger)]) :-
    atom_ref_args(Atom, Ref, Args).

analyze_role_uses(refs_of_arg(Index, Sign, Marking), Term,
                  [use(Ref, Args, Sign, Marking)]) :-
    arg(Index, Term, Atom),
    atom_ref_args(Atom, Ref, Args).
analyze_role_uses(splice_bare, Term, Uses) :-
    Term =.. [_ | Bodies],
    body_ref_uses_list(Bodies, Uses).
analyze_role_uses(arm(neg), Term, Uses) :-
    arg(1, Term, Goal),
    body_ref_uses(Goal, InnerUses),
    maplist(flip_to_neg, InnerUses, Uses).
analyze_role_uses(no_refs, _, []).

flip_to_neg(use(Ref, Args, _, Marked), use(Ref, Args, neg, Marked)).

body_ref_uses_list([], []).
body_ref_uses_list([Body | Rest], Uses) :-
    body_ref_uses(Body, BodyUses), body_ref_uses_list(Rest, RestUses),
    append(BodyUses, RestUses, Uses).

atom_ref_args(Atom, Ref, Args) :- rel_ref(Atom, Ref), Atom =.. [_ | Args].

% ═══ guard / bind goal classification (EXPRESSION LIFT) ═════════════════════
% Both keyed off the registry axis, never a local functor list: `guard` is
% the comparison family, `bind` is := / is. A goal in either family
% contributes zero rel uses (body_ref_uses/2's no_refs role above) and is
% compiled by lower.pl into a WHERE condition or a SELECT-expression binding.

guard_goal(Goal) :-
    body_surface_for_term(Goal, _, guard, no_refs, infix(_), _).

bind_goal(Goal, Variable, Expr) :-
    body_surface_for_term(Goal, _, bind, no_refs, infix(_), _),
    arg(1, Goal, Variable),
    arg(2, Goal, Expr).

% Every guard/bind goal a rule body reaches, LEFT TO RIGHT (engine.pl
% solve/2 resolves a conjunction left to right, so a bind must be able to
% read a variable an earlier bind introduced -- lower.pl folds the same
% order). not/1 is NOT descended into: a comparison under negation is
% refused by name in check_level_rule_shape/1 below, since compile_negative_
% uses/4 emits a bare NOT EXISTS over rel atoms and would silently drop the
% condition.
body_guard_goals(Body, Goals) :-
    conjunction_goals(Body, AllGoals),
    include(guard_or_bind_goal, AllGoals, Goals).

guard_or_bind_goal(Goal) :- guard_goal(Goal), !.
guard_or_bind_goal(Goal) :- bind_goal(Goal, _, _).

% ═══ program-wide ref inventory ═════════════════════════════════════════════
% declared_refs/2: every kind(Ref, _) declaration, regardless of whether any
% rule ever mentions Ref. Sweeping the full fixture corpus (phase C) turned
% up EDB-only fixtures with an empty Rules list entirely (e.g.
% engine_core.pl's retention_count_prunes_oldest: `kind(event/1, log),
% keep(event/1, count(2))`, no rules at all) -- program_refs/2 alone walks
% only Rules, so a rel that is purely an arrival target with zero readers
% was silently absent from RelPlans, dropping its DDL/arrival handling from
% the emitted program entirely (a genuine compiler gap, not a
% supported-subset refusal: the program "compiled" into something that could
% never accept its own schedule). compile.pl:program_plan/2 unions this with
% program_refs/2's rule-derived set.

% A ref can be declared via kind/2, keyed/2, or keep/2 independently -- a
% fixture may declare ONLY keyed(Ref, Positions) with no kind/2 at all
% (rel_kind/3's own fallback already handles that: keyed implies Set), and
% the corpus has a real case with zero rule readers too
% (scopes.pl:zombie_scope_negative_case_a2b's `keyed(open_pane/2, [1])`,
% intentionally never read by any rule -- comment: "REJECTED READING
% dropped on purpose"). Scanning only kind/2 missed it, and its Schedule
% arrival then hit the emitted program's "arrival for undeclared rel" guard.
declared_refs(Decls, Refs) :-
    findall(Ref,
            ( member(Decl, Decls),
              ( Decl = kind(Ref, _)
              ; Decl = keyed(Ref, _)
              ; Decl = keep(Ref, _)
              ; Decl = col_type(Ref, _, _)
              )
            ), Refs0),
    sort(Refs0, Refs).

program_refs(Rules, Refs) :-
    findall(Ref,
            ( member(Rule, Rules),
              ( rule_head_ref(Rule, Ref)
              ; rule_body(Rule, Body), body_ref_uses(Body, Uses), member(use(Ref, _, _, _), Uses) )
            ), Refs0),
    sort(Refs0, Refs).

% Arrival targets = every referenced ref that is NEVER a rule head. A schedule
% (or the fixture's Initial rows) is the only source that can write these.
arrival_target_refs(Rules, ArrivalRefs) :-
    program_refs(Rules, AllRefs),
    derived_refs(Rules, DerivedRefs),
    subtract(AllRefs, DerivedRefs, ArrivalRefs).

% ═══ column naming from surface variable identity ═══════════════════════════
% rel_columns(Rules, Bindings, Ref, Columns): for each argument position of
% Ref (1..Arity), scan every occurrence of Ref across all rule heads and body
% atoms; the first occurrence whose argument at that position is, by ==/2
% identity, one of Bindings' bound variables gives the column its name
% (snake_case of the surface Prolog variable name). A position that is never
% a plain named variable anywhere falls back to col<N>.

% findall/bagof would COPY_TERM every solution's Args, which severs the
% shared variable identity this whole scheme depends on (the same hazard
% engine.pl's trigger_items comment calls out: "findall copies its template,
% which would sever the trigger atom from the body"). column_name_at/4
% therefore drives ref_occurrence_args/3 directly inside an if-then,
% backtracking over the ORIGINAL term without ever collecting it into a list.

rel_columns(Rules, Bindings, Name/Arity, Columns) :-
    numlist(1, Arity, Positions),
    maplist(column_name_at(Rules, Bindings, Name/Arity), Positions, Columns).

rel_columns(Decls, Rules, Bindings, Ref, Columns) :-
    Ref = _Name/Arity,
    rel_columns(Rules, Bindings, Ref, InferredColumns),
    findall(Column,
            member(col_type(Ref, Column, _), Decls),
            TypedColumns),
    ( length(TypedColumns, Arity)
    -> Columns = TypedColumns
    ;  replace_declared_column_names(InferredColumns, TypedColumns, Columns)
    ).

replace_declared_column_names([], _, []).
replace_declared_column_names([Column | Rest], TypedColumns, [Column | More]) :-
    memberchk(Column, TypedColumns), !,
    replace_declared_column_names(Rest, TypedColumns, More).
replace_declared_column_names([Column | Rest], TypedColumns, [Column | More]) :-
    replace_declared_column_names(Rest, TypedColumns, More).

ref_occurrence_args(Rules, Ref, Args) :-
    member(Rule, Rules),
    ( rule_head(Rule, Head), rel_ref(Head, Ref), Head =.. [_ | Args]
    ; rule_body(Rule, Body), body_ref_uses(Body, Uses), member(use(Ref, Args, _, _), Uses) ).

column_name_at(Rules, Bindings, Ref, Position, ColumnName) :-
    ( ref_occurrence_args(Rules, Ref, Args),
      nth1(Position, Args, Arg),
      member(SurfaceName = BoundVar, Bindings),
      Arg == BoundVar
    -> snake_name(SurfaceName, ColumnName)
    ; format(atom(ColumnName), 'col~w', [Position]) ).

% CamelCase Prolog variable name -> snake_case column identifier.
snake_name(VarName, ColumnName) :-
    atom_codes(VarName, Codes),
    snake_codes(Codes, SnakeCodes0),
    ( SnakeCodes0 = [0'_ | Rest] -> SnakeCodes = Rest ; SnakeCodes = SnakeCodes0 ),
    atom_codes(ColumnName, SnakeCodes).

snake_codes([], []).
snake_codes([Code | Rest], Out) :-
    ( code_type(Code, upper)
    -> code_type(Lower, to_lower(Code)), Out = [0'_, Lower | More]
    ;  Out = [Code | More]
    ),
    snake_codes(Rest, More).

% ═══ column type inference from concrete literal values (PHASE C2 RULING 1)
% ═══════════════════════════════════════════════════════════════════════════
% Decls carries no per-column type syntax at all (no fixture in the corpus
% declares one -- expressions.pl's own header comment: "no HM/enum type
% checker... no rel_decl/column type", SCOREBOARD.md Finding 3), so a
% column's SQL storage type is inferred from every literal atomic value ever
% observed at that argument position, across three sources: the fixture's
% own Rules (head/body atom occurrences, reusing ref_occurrence_args/3 --
% var and compound arguments there are not literals and are filtered by
% atomic/1 below), its Initial seed rows, and its Schedule arrivals (either
% sign; a retraction's row is as real a literal witness as an addition's).
% A position is INTEGER only when EVERY literal witness found is a Prolog
% integer/1; TEXT otherwise, including "zero literal witnesses at all" (a
% column reached only through variables or nested inside a compound
% argument -- compound-term columns stay inline-flat text per the ruling's
% punt, and this default is exactly how that stays true with no special
% case: a compound occurrence is never atomic/1, so it never contributes a
% witness, and the column falls through to text). Matches the corpus
% observation behind the heuristic (Finding 3): no fixture here quotes a
% digit string, so Prolog's own reader never hands this scan an ambiguous
% integer-vs-atom token.
rel_column_types(Rules, Initial, Schedule, Name/Arity, Types) :-
    numlist(1, Arity, Positions),
    maplist(column_type_at(Rules, Initial, Schedule, Name/Arity), Positions, Types).

rel_column_types(Decls, Rules, Initial, Schedule, Bindings, Ref, Types) :-
    rel_columns(Decls, Rules, Bindings, Ref, Columns),
    Ref = _Name/Arity,
    numlist(1, Arity, Positions),
    maplist(column_type_at_decl(Decls, Rules, Initial, Schedule, Ref, Columns),
            Positions, Types).

column_type_at_decl(Decls, Rules, Initial, Schedule, Ref, Columns, Position, Type) :-
    nth1(Position, Columns, Column),
    ( memberchk(col_type(Ref, Column, DeclaredType), Decls)
    -> findall(WitnessType,
                ( column_source_args(Rules, Initial, Schedule, Ref, Args),
                  nth1(Position, Args, Witness),
                  atomic(Witness),
                  literal_witness_type(Witness, WitnessType)
                ), WitnessTypes),
       ( member(WitnessType, WitnessTypes), WitnessType \== DeclaredType
       -> throw(unsupported_construct(
                    decl_type_conflicts_witness(Ref, Position, DeclaredType, WitnessType)))
       ; Type = DeclaredType
       )
    ; column_type_at(Rules, Initial, Schedule, Ref, Position, Type)
    ).

literal_witness_type(Witness, int) :- integer(Witness), !.
literal_witness_type(_, text).

column_type_at(Rules, Initial, Schedule, Ref, Position, Type) :-
    findall(Witness,
            ( column_source_args(Rules, Initial, Schedule, Ref, Args),
              nth1(Position, Args, Witness),
              atomic(Witness)
            ), AtomicWitnesses),
    ( AtomicWitnesses \== [], forall(member(Witness, AtomicWitnesses), integer(Witness))
    -> Type = int
    ;  Type = text
    ).

% ═══ program-wide column typing (EXPRESSION + AGGREGATE LIFT) ══════════════
% PHASE C2 RULING 1 types each column from ITS OWN literal witnesses alone,
% which is exactly right while every value in a column arrives as a literal
% somewhere. An expression column does not: union_size/3's third column is
% only ever written by the head expression `LeftSize + RightSize - Shared`
% and only ever read into jaccard's body variable `Union`, so it has ZERO
% literal witnesses and falls to the "no witness -> text" default while the
% value crossing it is the integer 12. Stored TEXT, the tick-log and
% final-state encoders print "12" where the oracle prints 12, and `Union > 0`
% compares TEXT affinity against an integer literal. That is fail-first check
% (a), the TEXT-collapse class, and this pass is the fix.
%
% One fixpoint over LEVEL rule heads only (edge heads keep the literal-
% witness-only rule this arc did not touch, so check_edge_head_column_types/2
% keeps refusing the same two fixtures):
%   1. seed every column from its literal witnesses, keeping "no witness"
%      DISTINCT from "text witness" (contribution `none` vs `text`);
%   2. for each level rule, build a variable -> type environment from its
%      positive body atoms (using the CURRENT type map) plus its left-to-
%      right binds, then type each head argument expression;
%   3. a column's type is `text` if ANY contributor says text, else `int` if
%      any says int, else `none`;
%   4. iterate until nothing moves (types flow producer -> consumer, e.g.
%      union_size col3 -> jaccard's Union -> jaccard col3), then `none`
%      becomes `text`, the unchanged default.
% A DECLARED col_type/3 always wins and is never revised, so the declaration
% stays the authority the wave-2 spelling ruling made it.
program_column_types(Decls, Rules, Initial, Schedule, Bindings, Refs, RefTypes) :-
    findall(Ref-Columns,
            ( member(Ref, Refs), rel_columns(Decls, Rules, Bindings, Ref, Columns) ),
            RefColumns),
    findall(Ref-Seeds,
            ( member(Ref, Refs),
              memberchk(Ref-Columns, RefColumns),
              seed_column_contributions(Decls, Rules, Initial, Schedule, Ref,
                                        Columns, Seeds) ),
            SeedMap),
    include(rule_is_level, Rules, LevelRules),
    column_type_fixpoint(LevelRules, RefColumns, SeedMap, SeedMap, Settled),
    findall(Ref-Types,
            ( member(Ref-Contributions, Settled),
              maplist(contribution_to_type, Contributions, Types) ),
            RefTypes).

contribution_to_type(frozen(Type), Type) :- !.
contribution_to_type(open(none), text) :- !.
contribution_to_type(open(Type), Type).

raw_contribution(frozen(Type), Type) :- !.
raw_contribution(open(Type), Type).

% frozen(Type) = a declared col_type/3, the authority no rule contribution
% may revise (the wave-2 spelling ruling). open(Contribution) = inferred,
% still widenable.
seed_column_contributions(Decls, Rules, Initial, Schedule, Ref, Columns, Seeds) :-
    Ref = _Name/Arity,
    numlist(1, Arity, Positions),
    maplist(seed_column_contribution(Decls, Rules, Initial, Schedule, Ref, Columns),
            Positions, Seeds).

seed_column_contribution(Decls, Rules, Initial, Schedule, Ref, Columns, Position, Seed) :-
    nth1(Position, Columns, Column),
    ( memberchk(col_type(Ref, Column, _), Decls)
    -> column_type_at_decl(Decls, Rules, Initial, Schedule, Ref, Columns,
                           Position, DeclaredType),
       Seed = frozen(DeclaredType)
    ;  findall(Witness,
               ( column_source_args(Rules, Initial, Schedule, Ref, Args),
                 nth1(Position, Args, Witness),
                 atomic(Witness)
               ), AtomicWitnesses),
       ( AtomicWitnesses == []
       -> Seed = open(none)
       ;  forall(member(Witness, AtomicWitnesses), integer(Witness))
       -> Seed = open(int)
       ;  Seed = open(text)
       )
    ).

% TypeMap carries the RAW contribution per position (int | text | none), NOT
% the resolved storage type. Resolving `none` to its TEXT default before the
% fixpoint settles is wrong and was measured wrong: a rel with no literal
% witness anywhere (waiver_block_comment/2, present in timeless_rail's rules
% but with zero Initial rows in most of its fixtures) would enter the
% environment as `text`, text dominates the merge, and the genuinely-integer
% column it feeds (eprintln_waiver_line/2's line number, which a SIBLING
% clause types int from a real witness) collapsed to text -- taking all nine
% timeless_rail fixtures out with comparison_operand_not_int. `none` means
% "contributes nothing" all the way to the end; only contribution_to_type/2,
% after the fixpoint, turns a still-unknown column into TEXT.
column_type_fixpoint(LevelRules, RefColumns, SeedMap, Current, Settled) :-
    findall(Ref-TypeList,
            ( member(Ref-Contributions, Current),
              maplist(raw_contribution, Contributions, TypeList) ),
            TypeMap),
    findall(Ref-Merged,
            ( member(Ref-Seeds, SeedMap),
              rule_head_contributions(LevelRules, RefColumns, TypeMap, Ref,
                                      RuleContributions),
              merge_contribution_lists(Seeds, RuleContributions, Merged) ),
            Next),
    ( Next == Current
    -> Settled = Current
    ;  column_type_fixpoint(LevelRules, RefColumns, SeedMap, Next, Settled)
    ).

rule_head_contributions(LevelRules, RefColumns, TypeMap, Ref, Contributions) :-
    findall(HeadTypes,
            ( member(Rule, LevelRules),
              rule_head_ref(Rule, Ref),
              rule_head_contribution(RefColumns, TypeMap, Rule, HeadTypes) ),
            Contributions).

rule_head_contribution(RefColumns, TypeMap, (Head <- Body), HeadTypes) :-
    body_type_environment(RefColumns, TypeMap, Body, Environment),
    Head =.. [_ | Args],
    maplist(head_arg_contribution(Environment), Args, HeadTypes).

head_arg_contribution(Environment, Arg, Type) :-
    ( aggregate_arg_contribution(Environment, Arg, AggType)
    -> Type = AggType
    ;  expression_type(Arg, Environment, Type)
    ).

% count is always an integer count; sum/min/max carry their argument's type
% (min_list/max_list in the oracle only accept numbers, so a non-int there is
% refused at lowering time, not silently retyped).
aggregate_arg_contribution(_, Arg, int) :-
    nonvar(Arg), surface_for_term(Arg, count/1, aggregate, _, _, _), !.
aggregate_arg_contribution(Environment, Arg, Type) :-
    nonvar(Arg),
    surface_for_term(Arg, Kind/1, aggregate, _, _, _),
    memberchk(Kind, [sum, min, max]), !,
    arg(1, Arg, Inner),
    expression_type(Inner, Environment, Type).

% Variable -> type, from the rule's positive body atoms then its binds, in
% body order. A variable an atom already bound is NOT rebound by a later
% bind (that is an equality check, not a fresh binding -- same rule
% lower.pl's guard walk applies).
body_type_environment(RefColumns, Current, Body, Environment) :-
    body_ref_uses(Body, Uses),
    include(use_is_positive, Uses, PositiveUses),
    foldl(atom_use_bindings(RefColumns, Current), PositiveUses, [], AtomEnvironment),
    body_guard_goals(Body, GuardGoals),
    foldl(bind_goal_binding, GuardGoals, AtomEnvironment, Environment).

use_is_positive(use(_, _, pos, _)).

atom_use_bindings(RefColumns, Current, use(Ref, Args, _, _), Environment0, Environment) :-
    ( memberchk(Ref-Columns, RefColumns), memberchk(Ref-Types, Current)
    -> length(Columns, Arity),
       numlist(1, Arity, Positions),
       foldl(atom_arg_binding(Args, Types), Positions, Environment0, Environment)
    ;  Environment = Environment0
    ).

% A variable already bound to `none` (an as-yet-untyped column) is UPGRADED
% when a later atom in the same body binds it to a real type: `p(X), q(X)`
% where p's column has no witness yet and q's is int must give X int, not
% none. Only none is overwritten; a real type stays put, so the FIRST typed
% binding wins and the environment is order-stable.
atom_arg_binding(Args, Types, Position, Environment0, Environment) :-
    ( nth1(Position, Args, Arg), var(Arg), nth1(Position, Types, Type)
    -> ( environment_lookup(Environment0, Arg, Existing)
       -> ( Existing == none, Type \== none
          -> environment_replace(Environment0, Arg, Type, Environment)
          ;  Environment = Environment0 )
       ;  Environment = [Arg-Type | Environment0]
       )
    ;  Environment = Environment0
    ).

environment_replace([Variable-Existing | Rest], Target, Type, Out) :-
    ( Variable == Target
    -> Out = [Variable-Type | Rest]
    ;  Out = [Variable-Existing | More], environment_replace(Rest, Target, Type, More)
    ).

bind_goal_binding(Goal, Environment0, Environment) :-
    ( bind_goal(Goal, Variable, Expr), var(Variable),
      \+ environment_lookup(Environment0, Variable, _)
    -> expression_type(Expr, Environment0, Type),
       Environment = [Variable-Type | Environment0]
    ;  Environment = Environment0
    ).

environment_lookup([Variable-Type | Rest], Target, Found) :-
    ( Variable == Target -> Found = Type ; environment_lookup(Rest, Target, Found) ).

% expression_type/3 mirrors engine.pl:eval_expr/2's own result kinds:
% arithmetic is Int-only, concat produces text, a braces/compound value is
% stored as text, and an unknown variable contributes nothing (`none`).
expression_type(Expr, Environment, Type) :-
    var(Expr), !,
    ( environment_lookup(Environment, Expr, Type) -> true ; Type = none ).
expression_type(Expr, _, int) :- integer(Expr), !.
expression_type(Expr, _, text) :- atomic(Expr), !.
expression_type(concat(_), _, text) :- !.
expression_type(Expr, Environment, Type) :-
    arithmetic_expression(Expr, Left, Right), !,
    expression_type(Left, Environment, LeftType),
    expression_type(Right, Environment, RightType),
    ( ( LeftType == text ; RightType == text ) -> Type = text ; Type = int ).
expression_type(_, _, text).

arithmetic_expression(Expr, Left, Right) :-
    compound(Expr), Expr =.. [Functor, Left, Right],
    memberchk(Functor, ['+', '-', '*', '/', mod]).

% Positionwise combine, only where the seed is open/1: text dominates, then
% int, then none. A frozen(Type) position is returned unchanged.
merge_contribution_lists(Seeds, RuleContributions, Merged) :-
    foldl(merge_one_rule_contribution, RuleContributions, Seeds, Merged).

merge_one_rule_contribution(Types, Acc0, Acc) :- maplist(merge_contribution, Types, Acc0, Acc).

merge_contribution(_, frozen(Type), frozen(Type)) :- !.
merge_contribution(Contribution, open(Existing), open(Merged)) :-
    merge_type(Contribution, Existing, Merged).

merge_type(text, _, text) :- !.
merge_type(_, text, text) :- !.
merge_type(int, _, int) :- !.
merge_type(_, int, int) :- !.
merge_type(_, _, none).

% Every place a ground literal for Ref can appear: a rule head/body atom
% occurrence, an Initial seed row, or one Schedule tick's arrival row.
column_source_args(Rules, _Initial, _Schedule, Ref, Args) :- ref_occurrence_args(Rules, Ref, Args).
column_source_args(_Rules, Initial, _Schedule, Ref, Args) :-
    member(Row, Initial), rel_ref(Row, Ref), Row =.. [_ | Args].
column_source_args(_Rules, _Initial, Schedule, Ref, Args) :-
    member(Batch, Schedule), member(Signed, Batch),
    ( Signed = +Atom ; Signed = -Atom ),
    rel_ref(Atom, Ref), Atom =.. [_ | Args].

% ═══ edge trigger shape (bare trigger atoms) ════════════════════════════════
% Grounded in engine.pl's trigger_items/2: bare positive atoms are triggers,
% latest/1 and the special body forms are sampled or non-trigger reads.
% (:112-126, the exact goal classification engine.pl's unmarked fallback
% walks). Two ACCEPTED shapes, everything else a NAMED refusal:
%   marked_single(Atom)      -- Body = Atom, the single trigger source.
%   unmarked_conjunction(Atoms) -- Body is a plain
%     comma-conjunction of one or more ordinary positive rel atoms (arity
%     >= 1), nothing else: engine.pl's unmarked_items/2 wraps EVERY body
%     atom as its own arrival(...) trigger item when no only/1 is present,
%     so ANY of them arriving is an independent trigger occurrence,
%     evaluated by solving the WHOLE body (occurrence_trigger/4 binds only
%     the firing atom's own arguments via unification with the arrived row;
%     solve/2 then re-checks every other atom against the CURRENT store --
%     body.pl:96-110). A single atom (N=1) is the degenerate case: no other
%     atom to join, so lowering it is unchanged from marked_single except
%     the trigger no longer needs the only/1 wrapper.
% Everything else names the SPECIFIC blocking construct instead of a
% blanket edge_body_shape reason, so the scoreboard's per-construct tally
% stays precise as this widens further.
edge_trigger_shape(Body, unsupported(Reason)) :-
    conjunction_goals(Body, Goals),
    edge_registered_refusal(Body, Goals, Reason), !.
edge_trigger_shape(Body, unmarked_conjunction(Atoms)) :-
    conjunction_goals(Body, Goals),
    maplist(plain_positive_atom, Goals),
    !, Atoms = Goals.
edge_trigger_shape(Body, unsupported(edge_body_shape(Body))).

edge_registered_refusal(Body, Goals, Reason) :-
    findall(Priority-Candidate,
            ( member(Goal, Goals),
              edge_goal_refusal(Goal, Body, Priority, Candidate)
            ),
            Candidates),
    keysort(Candidates, [_-Reason | _]).

edge_goal_refusal(Goal, Body, 1, edge_body_with_latest(Body)) :-
    body_surface_for_term(Goal, _, sample, refs_of_arg(_, pos, sampled),
                          wrapper(rel_atom, lower), live).
edge_goal_refusal(Goal, Body, 2, edge_body_needs_finalize(Body)) :-
    body_surface_for_term(Goal, _, time, refs_of_arg(_, pos, trigger),
                          wrapper(rel_atom, refuse(goal)), refused).
edge_goal_refusal(Goal, Body, 3, edge_body_needs_pre(Body)) :-
    body_surface_for_term(Goal, _, sample, refs_of_arg(_, pos, sampled),
                          wrapper(rel_atom, refuse(goal)), refused).
edge_goal_refusal(Goal, Body, 4, edge_body_needs_now(Body)) :-
    body_surface_for_term(Goal, _, time, no_refs,
                          wrapper(expr, refuse(goal)), refused).
edge_goal_refusal(Goal, Body, 5, edge_body_needs_negation(Body)) :-
    body_surface_for_term(Goal, _, sign, arm(neg),
                          wrapper(body_item, lower), live).
% Binds and comparisons are LIVE in a level body as of the expression lift,
% and still refused in an EDGE body: an edge arm projects ONE arrival row
% through numbered placeholders and joins the other atoms against their
% current tables (edge_statement_single/5), a shape the guard walk has no
% seam in yet. Refused by the precise name rather than falling through to the
% blanket edge_body_shape, so the scoreboard tally stays readable.
edge_goal_refusal(Goal, Body, 6, edge_body_needs_bind(Body)) :-
    bind_goal(Goal, _, _).
edge_goal_refusal(Goal, Body, 7, edge_body_needs_comparison(Body)) :-
    guard_goal(Goal).
edge_goal_refusal(Goal, Body, 8, edge_body_needs_json_destructure(Body)) :-
    body_surface_for_term(Goal, _, guard, no_refs,
                          wrapper(expr_pair, refuse(goal)), refused).

conjunction_goals((Left, Right), Goals) :- !,
    conjunction_goals(Left, LeftGoals), conjunction_goals(Right, RightGoals),
    append(LeftGoals, RightGoals, Goals).
conjunction_goals(Term, Goals) :-
    compound(Term), Term =.. [combine | Atoms], Atoms \== [], !,
    maplist(conjunction_goals, Atoms, AtomGoals), append(AtomGoals, Goals).
conjunction_goals(next(Atom), [Atom]) :- !.
conjunction_goals(Goal, [Goal]).

plain_positive_atom(Goal) :-
    compound(Goal),
    \+ body_surface_for_term(Goal, _, _, _, _, _).

% Trigger refs a shape can fire from -- used by the same-key conflict-risk
% check below. marked_single fires from exactly one ref; unmarked_conjunction
% fires from ANY of its atoms' refs (engine.pl unmarked_items/2 wraps every
% one independently).
shape_trigger_refs(marked_single(Atom), [Ref]) :- rel_ref(Atom, Ref).
shape_trigger_refs(unmarked_conjunction(Atoms), Refs) :-
    findall(Ref, ( member(Atom, Atoms), rel_ref(Atom, Ref) ), Refs0), sort(Refs0, Refs).

% ═══ edge head column-type consistency (PHASE C2 RULING 1 x RULING 2) ═══════
% A real gap the unmarked-trigger widening surfaced, not present in the
% marked_single-only corpus before this ruling: an edge rule's HEAD column
% inherits its VALUE from a body atom via a shared variable (e.g.
% spine_semantics.pl's `xref(FromSpanId, ...) <+ pin_extracted(FromSpanId,
% ...)`), but analyze.pl:rel_column_types/5 infers each ref's column types
% from ITS OWN literal occurrences alone -- xref/6 never appears as a raw
% Schedule arrival (it is edge-headed), so its own from_span_id position
% never sees the literal integer values that only ever arrive via
% pin_extracted's arguments, and defaults to text (PHASE C2 RULING 1's own
% "zero witnesses -> text" rule). The stored column is TEXT while the
% flowing value is a genuine integer, so the tick log prints the quoted
% string form the oracle prints as a bare number -- WRONG, not a silent
% pass. This is called AFTER RelPlans exists (compile.pl:program_plan/2,
% past check_supported_subset/1, which runs before RelPlans is built) and
% refuses the specific mismatch by name rather than attempting general
% cross-rule type propagation (a real fix, out of this ruling's scope --
% Item 1 only ever reasons about one ref's OWN literal occurrences).
check_edge_head_column_types(RelPlans, Rules) :-
    forall(( member(Rule, Rules), rule_is_edge(Rule) ),
           check_edge_head_column_types_for_rule(RelPlans, Rule)).

check_edge_head_column_types_for_rule(RelPlans, (Head <+ Body)) :-
    edge_trigger_shape(Body, Shape),
    ( Shape = marked_single(TriggerAtom) -> BodyAtoms = [TriggerAtom]
    ; Shape = unmarked_conjunction(Atoms) -> BodyAtoms = Atoms
    ; BodyAtoms = []
    ),
    rel_ref(Head, HeadRef),
    memberchk(relplan(HeadRef, _, _, _, HeadColumnTypes), RelPlans),
    Head =.. [_ | HeadArgs],
    forall(( nth1(HeadPosition, HeadArgs, HeadArg), var(HeadArg),
             nth1(HeadPosition, HeadColumnTypes, HeadColumnType),
             member(BodyAtom, BodyAtoms),
             rel_ref(BodyAtom, BodyRef),
             BodyAtom =.. [_ | BodyArgs],
             nth1(BodyPosition, BodyArgs, BodyArg), BodyArg == HeadArg,
             memberchk(relplan(BodyRef, _, _, _, BodyColumnTypes), RelPlans),
             nth1(BodyPosition, BodyColumnTypes, BodyColumnType),
             BodyColumnType \== HeadColumnType ),
           throw(unsupported_construct(edge_head_column_type_mismatch(HeadRef, HeadPosition, BodyColumnType, HeadColumnType)))).

% ═══ supported-subset gate ═══════════════════════════════════════════════════
% Refuses (with a specific term, not a generic failure) any construct wider
% than what lower.pl knows how to emit: an edge rule body must classify as
% marked_single or unmarked_conjunction (edge_trigger_shape/2 above); a level
% rule must not carry an aggregate head (aggregate_head, level_eval.pl) and
% must not reference pre/1, now/1, decode/2 or json_each/2 (none of the two
% target fixtures need them; lower.pl has no SQL shape for them yet).
%
% FORMAL MODEL: TICK-MODEL.md (same directory) is the semiring/grading
% semantics behind this gate. The cross-plane refusals below
% (log_on_level_headed_rel, latest_in_level_rule, pre_in_level_rule, and
% engine.pl's finalize_in_level_rule / keyed_level_head) are hand-proven
% instances of its ring/grade discipline; the planned clock checker
% (TICK-MODEL.md section 6) generalizes them and lives here when built.

check_supported_subset(SugaredProg) :-
    expand_match_program(SugaredProg, ExpandedProg),
    check_supported_subset_expanded(ExpandedProg).

check_supported_subset_expanded(prog(Decls, Rules)) :-
    forall(( member(Rule, Rules), rule_reserved_construct(Rule, Construct) ),
           throw(unsupported_construct(Construct))),
    forall(( member(kind(Ref, log), Decls), member(LevelRule, Rules),
             rule_is_level(LevelRule), rule_head_ref(LevelRule, Ref) ),
           throw(unsupported_construct(log_on_level_headed_rel(Ref)))),
    forall(( member(LevelRule, Rules), rule_is_level(LevelRule),
             rule_body(LevelRule, Body), level_body_latest_ref(Body, Ref) ),
           throw(unsupported_construct(latest_in_level_rule(Ref)))),
    forall(( member(LevelRule, Rules), rule_is_level(LevelRule),
             rule_body(LevelRule, Body), level_body_pre_ref(Body, Ref) ),
           throw(unsupported_construct(pre_in_level_rule(Ref)))),
    forall(( member(keep(Ref, _), Decls), rel_kind(Decls, Ref, Kind), Kind \== log ),
           throw(unsupported_construct(keep_on_non_log_rel(Ref)))),
    forall(( member(Rule, Rules), rule_is_edge(Rule) ), check_edge_rule_shape(Rule)),
    forall(( member(Rule, Rules), rule_is_level(Rule) ), check_level_rule_shape(Rule)),
    check_no_edge_head_conflict_risk(Decls, Rules),
    forall(( member(keyed(Ref, _), Decls), member(LevelRule, Rules),
             rule_is_level(LevelRule), rule_head_ref(LevelRule, Ref) ),
           throw(unsupported_construct(keyed_level_head(Ref)))),
    forall(( member(keyed(Ref, Positions), Decls), rel_kind(Decls, Ref, log) ),
           throw(unsupported_construct(keyed_log_rel(Ref, Positions)))).

level_body_latest_ref((Left, Right), Ref) :-
    ( level_body_latest_ref(Left, Ref) ; level_body_latest_ref(Right, Ref) ).
level_body_latest_ref(not(Body), Ref) :- level_body_latest_ref(Body, Ref).
level_body_latest_ref(latest(Atom), Ref) :- rel_ref(Atom, Ref).

level_body_pre_ref((Left, Right), Ref) :-
    ( level_body_pre_ref(Left, Ref) ; level_body_pre_ref(Right, Ref) ).
level_body_pre_ref(not(Body), Ref) :- level_body_pre_ref(Body, Ref).
level_body_pre_ref(pre(Atom), Ref) :- rel_ref(Atom, Ref).

check_edge_rule_shape((Head <+ Body)) :-
    edge_trigger_shape(Body, Shape),
    ( Shape = unsupported(Reason) -> throw(unsupported_construct(Reason)) ; true ),
    % Same hazard as a level head (head_arithmetic_shape/2's comment):
    % compile_head_expr renders every compound argument as a json1 tagged
    % term unconditionally, arithmetic functor or not. No fixture in the
    % current corpus hits this on an edge head, checked defensively anyway.
    ( head_arithmetic_shape(Head, ArithExpr)
    -> throw(unsupported_construct(head_arithmetic(Head, ArithExpr)))
    ; true ).

rule_reserved_construct(Rule, Construct) :-
    rule_body(Rule, Body),
    reserved_construct_in_body(Body, Construct).

reserved_construct_in_body((Left, Right), Construct) :- !,
    ( reserved_construct_in_body(Left, Construct)
    ; reserved_construct_in_body(Right, Construct)
    ).
reserved_construct_in_body(Term, Construct) :-
    body_surface_for_term(Term, Functor/_, _, _, LowerRole, reserved),
    reserved_construct_name(LowerRole, Functor, Construct).

reserved_construct_name(wrapper(_, refuse(lifecycle)), Functor,
                        lifecycle_arm(Functor)) :- !.
reserved_construct_name(_, Functor, Functor).

% engine.pl's check_occurrence_conflicts (called once per OCCURRENCE, across
% every rule in the program) throws keyed_conflict/3 when the SAME occurrence
% satisfies two rules heading the same keyed rel with two DIFFERENT derived
% rows for the same key. This compiler's lowering resolves each edge rule/arm
% independently and has no equivalent per-occurrence validation, so it would
% silently let the LAST-running arm's write win instead of throwing -- the
% exact shape merge_family.pl:one_occurrence_two_rows_still_conflicts exists
% to catch (`(latest(cli,a) <+ ping(_)), (latest(cli,b) <+ ping(_))`, both
% rules triggered by the SAME ping/1 occurrence). The conflict can only arise
% when two edge rules/arms sharing a KEYED head also share a trigger ref
% (shape_trigger_refs/2 above) -- refused by name rather than left to
% silently miscompile. key_last_write_wins and its siblings stay clean: each
% rule there is triggered by a DIFFERENT ref (from_poll vs from_push), so no
% single occurrence can ever satisfy both.
check_no_edge_head_conflict_risk(Decls, Rules) :-
    include(rule_is_edge, Rules, EdgeRules),
    findall(HeadRef-TriggerRefs,
            ( member(Rule, EdgeRules), rule_head_ref(Rule, HeadRef),
              rule_body(Rule, Body), edge_trigger_shape(Body, Shape),
              shape_trigger_refs(Shape, TriggerRefs) ),
            HeadTriggerPairs),
    forall(( decl_key(Decls, HeadRef, _),
             findall(Refs, member(HeadRef-Refs, HeadTriggerPairs), AllRefsForHead),
             nth0(IndexA, AllRefsForHead, RefsA),
             nth0(IndexB, AllRefsForHead, RefsB), IndexA < IndexB,
             intersection(RefsA, RefsB, Shared), Shared \== [] ),
           throw(unsupported_construct(edge_head_conflict_risk(HeadRef, Shared)))).

check_level_rule_shape((Head <- Body)) :-
    ( refused_aggregate_head_shape(Head, RefusedAgg)
    -> throw(unsupported_construct(aggregate_head(RefusedAgg)))
    ; true ),
    ( body_forbidden_goal(Body, Forbidden)
    -> throw(unsupported_construct(level_body_goal(Head, Forbidden)))
    ; true ),
    ( negated_guard_goal(Body, NegatedGuard)
    -> throw(unsupported_construct(negated_guard_goal(Head, NegatedGuard)))
    ; true ),
    ( aggregate_head_template(Head, _)
    -> check_aggregate_rule_shape(Head, Body)
    ; true ).

% A comparison or bind nested under not/1. compile_negative_uses/4 renders a
% negated atom as a bare NOT EXISTS over rel columns and never sees the
% conjunction's other goals, so a guard inside the negation would be SILENTLY
% DROPPED (the exact silent-filter-loss class the phase-C sweep found for
% un-negated comparisons). No corpus fixture writes one; refused by name
% rather than left to miscompile.
negated_guard_goal(Body, Goal) :-
    conjunction_goals(Body, Goals),
    member(NegatedGoal, Goals),
    NegatedGoal = not(Inner),
    body_guard_goals(Inner, InnerGuards),
    InnerGuards = [Goal | _].

% An aggregate head column whose body reads the head's OWN rel: engine.pl
% stratifies an aggregate strictly above every rel its body reads
% (level_eval.pl rule_body_constraint/4, Gap=1 for EVERY body ref of an
% aggregate head), so a self-reading aggregate is not_stratified there and
% has no SQL shape here either.
check_aggregate_rule_shape(Head, Body) :-
    rel_ref(Head, HeadRef),
    body_ref_uses(Body, Uses),
    ( member(use(HeadRef, _, _, _), Uses)
    -> throw(unsupported_construct(aggregate_head_reads_itself(HeadRef)))
    ; true ),
    ( member(use(_, _, pos, _), Uses)
    -> true
    ; throw(unsupported_construct(aggregate_head_no_positive_body(HeadRef)))
    ).

% aggregate_head_template(+Head, -Template): the same classification
% level_eval.pl:aggregate_head/3 performs, projected off the registry's
% `aggregate` axis instead of a local functor list. Template is one entry per
% head argument, plain(Expr) or agg(Kind, Expr); it is an aggregate head only
% when at least one entry is agg(_, _), matching the oracle exactly.
aggregate_head_template(Head, Template) :-
    compound(Head),
    Head =.. [_ | Args],
    Args \== [],
    maplist(classify_head_arg, Args, Template),
    memberchk(agg(_, _), Template).

classify_head_arg(Arg, agg(json_object, KeyExpr-ValueExpr)) :-
    nonvar(Arg), Arg = json_object(KeyExpr, ValueExpr), !.
classify_head_arg(Arg, agg(Kind, Expr)) :-
    nonvar(Arg),
    surface_for_term(Arg, Kind/1, aggregate, no_refs, head(_), _), !,
    arg(1, Arg, Expr).
classify_head_arg(Arg, plain(Arg)).

rule_is_aggregate((Head <- _)) :- aggregate_head_template(Head, _).

% Only the aggregate kinds whose registry row still says refuse(aggregate)
% block the rule (json_array/json_object -- see registry.pl for the cons-text
% encoding crack). count/sum/min/max lower.
refused_aggregate_head_shape(Head, Arg) :-
    compound(Head),
    Head =.. [_ | Args],
    member(Arg, Args),
    surface_for_term(Arg, _, aggregate, no_refs,
                     head(refuse(aggregate)), refused).

% compile_head_expr (lower.pl) renders EVERY compound head argument as a
% json1 "construct a tagged term" expression (json_object('fn', Functor,
% 'args', json_array(...))) -- correct for a genuine domain compound like
% route_data(RouteId), silently WRONG for an arithmetic expression like
% `LeftSize + RightSize - Shared`: nothing evaluates it, so the stored value
% is a json1-encoded expression TREE, never the computed number. Phase C
% sweep finding (fixtures/expressions.pl:head_expression_evaluates_derived_column,
% comparison_filters_rows): both "compiled" clean under the pre-existing
% gate and produced a wrong stored value, invisible to the tick-log diff
% here because both fixtures also have an empty Schedule (all grading is via
% `final(...)`, which this compiler's sweep does not check -- see
% SCOREBOARD.md findings). Refusing head arithmetic turns that silent wrong
% output into a named refusal until real arithmetic lowering lands (widening
% priority: "arithmetic + bind :=" in the phase C contract's own ordering).
head_arithmetic_shape(Head, ArithExpr) :-
    Head =.. [_ | Args],
    member(Arg, Args),
    contains_arithmetic_functor(Arg, ArithExpr).

contains_arithmetic_functor(Arg, Arg) :-
    compound(Arg), Arg =.. [Functor | SubArgs], SubArgs \== [],
    memberchk(Functor, ['+', '-', '*', '/', mod]), !.
contains_arithmetic_functor(Arg, Found) :-
    compound(Arg), Arg =.. [_ | SubArgs], member(SubArg, SubArgs), contains_arithmetic_functor(SubArg, Found).

body_forbidden_goal((Left, Right), Forbidden) :- !,
    ( body_forbidden_goal(Left, Forbidden) ; body_forbidden_goal(Right, Forbidden) ).
% Comparison operators (body_ref_uses/2 already returns zero Uses for these
% -- comparison_goal/1 -- meaning nothing downstream ever compiled them into
% a WHERE clause; a level rule that filters on one silently lost the filter)
% and `:=`/`is` binds (body_ref_uses/2 returns zero Uses for these too, and
% lower.pl's compile_pattern_arg never learns the bound variable, so any
% head reference to it already fails as unbound_head_var -- but a `:=`/`is`
% goal that is NEVER read by the head, only used to gate a later comparison,
% reached no failure at all and just silently vanished). Both are phase C
% sweep findings (fixtures/expressions.pl:comparison_filters_rows,
% range_join_over_arithmetic, bind_computes_derived_value_then_comparison_filters);
% refusing them cleanly until real lowering lands, rather than leaving the
% silent-drop behavior.
body_forbidden_goal(Term, Forbidden) :-
    body_surface_for_term(Term, _, _, _, LowerRole, refused),
    refused_goal_term(LowerRole, Term, Forbidden).

refused_goal_term(infix(refuse(comparison)), Term, comparison(Term)) :- !.
refused_goal_term(infix(refuse(goal)), Term, bind(Term)) :- !.
refused_goal_term(wrapper(_, refuse(goal)), Term, Term).
