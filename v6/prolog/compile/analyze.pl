% analyze.pl : structural analysis of a prog(Decls, Rules) term form. Reads
% relation kind (mirrors engine.pl:88-93 rel_kind/4, reimplemented here since
% that predicate is not exported and depends only on Decls), which refs are
% EDB (an arrival schedule may write them: never a rule head) vs derived
% (headed by a level or edge rule), and per-ref column names mined from the
% ORIGINAL surface variable names the caller recovers via
% read_term(Stream, Term, [variable_names(Bindings)]) -- column identity comes
% from the fixture source text, never invented here.
%
% Compiles the SUBSET engine.pl semantics that the two Phase B target
% fixtures use: edge rule bodies are `TriggerAtom` alone (no extra
% joined goal, no pre/1, no departed/1, no guard); level rules are acyclic
% (no self-recursion), non-aggregate, with `not/1` allowed. Anything wider
% throws unsupported_construct(What) at analysis time -- a compiler finding,
% not a silent guess.

:- module(analyze,
          [ rel_kind/3, decl_key/3, decl_keep/3, declared_refs/2,
            program_refs/2, arrival_target_refs/2, derived_refs/2,
            edge_headed_refs/2, level_headed_refs/2,
            rule_head_ref/2, rule_is_edge/1, rule_is_level/1,
            body_ref_uses/2, rel_columns/4, rel_column_types/5, snake_name/2,
            check_supported_subset/1, edge_trigger_shape/2,
            conjunction_goals/2, check_edge_head_column_types/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
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

comparison_goal(Goal) :-
    body_surface_for_term(Goal, _, guard, no_refs,
                          infix(refuse(comparison)), refused).

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
              ( Decl = kind(Ref, _) ; Decl = keyed(Ref, _) ; Decl = keep(Ref, _) )
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
edge_goal_refusal(Goal, Body, 6, edge_body_needs_bind(Body)) :-
    body_surface_for_term(Goal, _, bind, no_refs, infix(refuse(goal)), refused).
edge_goal_refusal(Goal, Body, 7, edge_body_needs_comparison(Body)) :-
    body_surface_for_term(Goal, _, guard, no_refs,
                          infix(refuse(comparison)), refused).
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

check_supported_subset(prog(Decls, Rules)) :-
    forall(( member(Rule, Rules), rule_reserved_construct(Rule, Construct) ),
           throw(unsupported_construct(Construct))),
    forall(( member(Rule, Rules), rule_is_edge(Rule) ), check_edge_rule_shape(Rule)),
    forall(( member(Rule, Rules), rule_is_level(Rule) ), check_level_rule_shape(Rule)),
    derived_refs(Rules, DerivedRefs),
    forall(( member(Rule, Rules), rule_is_edge(Rule) ), check_edge_body_refs_not_derived(Rule, DerivedRefs)),
    check_no_edge_head_conflict_risk(Decls, Rules),
    forall(( member(keyed(Ref, Positions), Decls), rel_kind(Decls, Ref, log) ),
           throw(unsupported_construct(keyed_log_rel(Ref, Positions)))).

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

% A trigger firing off a DERIVED ref (edge-headed OR level-headed --
% analyze.pl:derived_refs/2 unions both) is a genuine gap in BOTH shapes,
% not attempted, for two INDEPENDENT reasons this compiler's pipeline
% cannot currently close:
%   (a) level-headed trigger: a newly-true level row is ALSO a valid
%       trigger occurrence in engine.pl (LevelOccs, tick/7:286-290), and a
%       level-headed ref's CURRENT table state during edge-write resolution
%       (which this compiler's pipeline runs BEFORE recomputeLevels --
%       run_tick_fn_lines) reflects the PREVIOUS tick's level rows, not
%       MidLevel (the post-arrival, pre-write snapshot engine.pl's Visible
%       actually reads).
%   (b) edge-headed trigger: engine.pl threads a THIS-tick edge write
%       forward as a genuine CarryIn occurrence for T+1 (tick/7:299-312,
%       "carry-out is boundary-observable writes only"; process_occurrences
%       fires it exactly like any other occurrence next tick). This
%       compiler's `triggerOccurrences` (emit_ts.pl) only ever reads the
%       tick's OWN `arrivals` parameter -- IGenProgram's `tick(seam,
%       arrivals)` carries no slot for "rows an edge rule wrote last tick"
%       at all (round 2's own note: no tick number, no carry value, reaches
%       tick() in the real seam). A drain tick's `arrivals` is always `[]`,
%       so a marked_single or unmarked_conjunction trigger firing off
%       ANOTHER edge rule's head can never see it -- confirmed WRONG, not
%       theorized: engine_core.pl's edge_chain_hops_tick_per_stage
%       (`stage_two(Item) <+ stage_one(Item)`, stage_one itself
%       `<+ source_ev(Item)`) compiled clean and produced an empty tick 2
%       where the oracle shows `+stage_two(alpha)`, once PHASE C2 RULING 2
%       lifted the unmarked-shape refusal that had masked this the whole
%       time (no fixture with an all-marked_single/unmarked, no-extra-guard
%       edge CHAIN had ever reached compilation before). Fixing this is a
%       real IGenProgram/tickLoop.ts change (threading carry occurrences
%       into the next tick call) -- STOP-AND-REPORT per the phase C2
%       contract, not attempted here; refused by name.
check_edge_body_refs_not_derived((_Head <+ Body), DerivedRefs) :-
    edge_trigger_shape(Body, Shape),
    ( Shape = unsupported(_)
    -> true  % already refused earlier in check_edge_rule_shape
    ;  shape_trigger_refs(Shape, TriggerRefs),
       forall(( member(Ref, TriggerRefs), memberchk(Ref, DerivedRefs) ),
              throw(unsupported_construct(edge_trigger_is_derived(Ref))))
    ).

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
    ( aggregate_head_shape(Head)
    -> throw(unsupported_construct(aggregate_head(Head)))
    ; true ),
    ( head_arithmetic_shape(Head, ArithExpr)
    -> throw(unsupported_construct(head_arithmetic(Head, ArithExpr)))
    ; true ),
    ( body_forbidden_goal(Body, Forbidden)
    -> throw(unsupported_construct(level_body_goal(Head, Forbidden)))
    ; true ).

aggregate_head_shape(Head) :-
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
