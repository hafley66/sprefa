% analyze.pl : structural analysis of a prog(Decls, Rules) term form. Reads
% relation kind (0_program_check.pl:relation_kind/3, shared with the oracle
% since rank R9 -- this file used to carry its own copy of it), which refs are
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
            edge_headed_refs/2, level_headed_refs/2, seeded_refs/2,
            rule_head_ref/2, rule_is_edge/1, rule_is_level/1,
            body_ref_uses/2, rel_columns/4, rel_columns/5,
            rel_column_types/5, rel_column_types/7, snake_name/2,
            check_supported_subset/1, check_supported_subset_expanded/1,
            edge_trigger_shape/2,
            conjunction_goals/2, check_edge_head_column_types/2,
            aggregate_head_template/2, rule_is_aggregate/1,
            body_guard_goals/2, guard_goal/1, bind_goal/3, tick_goal/2,
            program_uses_tick/2,
            program_column_types/7,
            % Body traversals, exported as the characterization seam for the
            % shared-walker consolidation (rank R1). The oracle ships its own
            % copy of the latest/1 and pre/1 scans, so the test that pins them
            % equal has to reach both sides by name.
            level_body_latest_ref/2, level_body_pre_ref/2,
            % The compiler's copy of engine.pl:listened_departure_refs/2 --
            % the rels some rule binds with finalize/1, which are exactly the
            % rels that owe a departure frontier. lower.pl and emit_ts.pl both
            % read it; the departure_ref_parity plunit unit pins it equal to
            % the oracle's.
            listened_departure_refs/2,
            reserved_construct_in_body/2, body_forbidden_goal/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module('../1_expansion', [expand_program/3]).
:- use_module('../0_body_walk',
              [ walk_body/3,
                body_conjunction_goals/3, body_wrapper_refs/4 ]).
:- use_module('../0_program_check',
              [ first_violation/3, relation_kind/3, declared_key/3 ]).
:- use_module('../0_type_plane',
              [ type_definitions/2, type_definition/4, column_storage/3,
                declared_type_name/2 ]).
:- use_module('../conformance/body', [rel_ref/2]).
:- use_module(registry,
              [ surface_for_term/6,
                body_surface_for_term/6,
                expression/5
              ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ rel kind ═══════════════════════════════════════════════════════════════
% Shared with engine.pl rather than mirroring it (rank R9 of
% plans/2026-07-29-prolog-org-review.md). The header here used to say "mirrors
% engine.pl exactly", which was true and is exactly the kind of claim that
% stops being true without anything failing. Both doors now call one
% implementation, and declaration_query_parity is the receipt.

rel_kind(Decls, Ref, Kind) :- relation_kind(Decls, Ref, Kind).

decl_key(Decls, Ref, Positions) :- declared_key(Decls, Ref, Positions).

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

% This is the walk the review named as the reference semantics for the shared
% traversal, and the one whose policy is widest: it descends not/1 and splices
% next/1 and combine. The traversal itself now lives in 0_body_walk.pl (rank
% R1); what stays here is the projection from a walk event to a use/4.
%
% Polarity comes from the walker, which absorbs to neg under any descended
% not/1 rather than flipping, so a doubly negated atom reads negative. That is
% what the old flip_to_neg/2 did, applied unconditionally rather than as a
% flip; the nested_not golden pins it.
% Collected by recursion and NEVER by findall/3: findall copies its template,
% which would sever every use/4's Args from the body's own variables and leave
% lowering to join over fresh holes.
body_ref_uses(Body, Uses) :-
    walk_body(Body, walk_policy(descend_not(true), splice_bare(true)),
              Events),
    events_uses(Events, Uses).

events_uses([], []).
events_uses([Event | Rest], Uses) :-
    (   event_use(Event, Use)
    ->  Uses = [Use | RestUses]
    ;   Uses = RestUses
    ),
    events_uses(Rest, RestUses).

% A node the registry has a row for contributes according to its AnalyzeRole;
% a node it does not is a relation atom read positively as a trigger source.
event_use(event(_, Polarity, plain_atom, Atom), use(Ref, Args, Polarity,
                                                    trigger)) :-
    !,
    atom_ref_args(Atom, Ref, Args).
event_use(event(_, Polarity, surface(_, _, refs_of_arg(Index, Sign, Marking),
                                     _, _), Term),
          use(Ref, Args, Polarity, Marking)) :-
    % The registry's own Sign is positive for every current row; a negative
    % polarity can only come from an enclosing not/1, which the walker has
    % already absorbed. Reading it here keeps the row honest if one is added.
    Sign == pos,
    arg(Index, Term, Atom),
    atom_ref_args(Atom, Ref, Args).

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

% now/1 reads the kernel tick counter (engine.pl step 8: "now(Tick) is a
% kernel read of the current tick (R3), never an arrival"). It binds like a
% `:=` goal and contributes zero rel uses, which is why it rides the same
% guard fold rather than a plane of its own; the argument must be a plain
% variable, since the tick is produced, never matched.
tick_goal(Goal, Variable) :-
    body_surface_for_term(Goal, now/1, time, no_refs, wrapper(expr, _), _),
    arg(1, Goal, Variable),
    var(Variable).

% Does any rule in this program read the tick? lower.pl only emits the
% __tick counter table, and emit_ts.pl only emits the per-tick advance, for
% programs that answer yes -- every other emitted module stays byte-identical
% to what it was before now/1 was lowered.
program_uses_tick(prog(_, Rules), UsesTick) :-
    (   member(Rule, Rules),
        rule_body(Rule, Body),
        conjunction_goals(Body, Goals),
        member(Goal, Goals),
        tick_goal(Goal, _)
    ->  UsesTick = true
    ;   UsesTick = false
    ).

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
% Every ref an Initial row seeds, declared or not. engine.pl:seed_store/3
% stores EVERY Initial row (rel_kind/3's own fallback makes an undeclared ref
% a Set rel), so such a ref is part of the oracle's final state whether or not
% any rule or decl mentions it. Without this the emitted program has no table
% for it, no boot seed, and no finalSelect entry -- the run grades identical
% (nothing ever writes it) while the FINAL state silently drops the rows.
% Caught by spine_semantics.pl:xref_rev_is_pin_data_not_live_head, whose
% Initial carries a known_repo(2) row its own rules never read.
seeded_refs(Initial, Refs) :-
    findall(Ref, ( member(Row, Initial), rel_ref(Row, Ref) ), Refs0),
    sort(Refs0, Refs).

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

% STRUCT-AS-ROWS: the declared type goes through 0_type_plane.pl:
% column_storage/3 before it becomes a STORAGE kind, so `int`/`text` are
% unchanged, `json` resolves to text (the untyped-json inline path, which used
% to reach lower.pl:column_def/3 with no matching clause at all), and a
% declared struct type becomes ref(TypeName), the third storage kind
% types-as-rels verdict Q6 named this slot for.
%
% The witness cross-check compares against the STORAGE kind, and only for the
% two scalar kinds. A ref column has no literal witness by construction (a
% struct value is compound, never atomic/1), and a struct value in a column
% declared int is already the type_arrival_shape_mismatch refusal.
column_type_at_decl(Decls, Rules, Initial, Schedule, Ref, Columns, Position, Type) :-
    nth1(Position, Columns, Column),
    ( memberchk(col_type(Ref, Column, DeclaredType), Decls)
    -> type_definitions(Decls, Types),
       column_storage(Types, DeclaredType, Storage),
       findall(WitnessType,
                ( column_source_args(Rules, Initial, Schedule, Ref, Args),
                  nth1(Position, Args, Witness),
                  atomic(Witness),
                  literal_witness_type(Witness, WitnessType)
                ), WitnessTypes),
       ( Storage = ref(_)
       -> Type = Storage
       ; member(WitnessType, WitnessTypes), WitnessType \== Storage
       -> throw(unsupported_construct(
                    decl_type_conflicts_witness(Ref, Position, Storage, WitnessType)))
       ; Type = Storage
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
% One fixpoint over EVERY rule head, level and edge alike. Edge heads used to
% be excluded, on the literal-witness-only rule, and check_edge_head_column_
% types/2 refused the resulting mismatches by name
% (edge_head_column_type_mismatch) rather than resolving them. That refusal is
% the "real fix, out of this ruling's scope" the C2 comment below names, and
% this is that fix: an edge head's column inherits its type from the body
% variable that feeds it, exactly the way a level head's does. The
% cross-check itself stays -- it now fires only where two DIFFERENT body atoms
% feed one head column with genuinely disagreeing types, which is a real
% program defect rather than an artefact of where the fixpoint stopped.
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
    column_type_fixpoint(Rules, RefColumns, SeedMap, SeedMap, Settled),
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
               ), LiteralWitnesses),
       % now/1 is a witness the way a literal is: the kernel tick is an
       % INTEGER (engine.pl run_ticks/7 counts from 1, step 8 "never an
       % arrival"), so a head column fed only by now/1 -- which has no
       % literal anywhere in the program, by construction -- would otherwise
       % take C2's "zero witnesses -> text" default and store the tick as
       % TEXT. Downstream that is either a named refusal
       % (comparison_operand_not_int on `EventTick > TurnTick`) or an
       % affinity comparison the oracle does not perform.
       ( now_bound_head_position(Rules, Ref, Position)
       -> AtomicWitnesses = [0 | LiteralWitnesses]
       ;  AtomicWitnesses = LiteralWitnesses
       ),
       ( AtomicWitnesses == []
       -> Seed = open(none)
       ;  forall(member(Witness, AtomicWitnesses), integer(Witness))
       -> Seed = open(int)
       ;  Seed = open(text)
       )
    ).

% A rule head position whose variable is exactly the one now/1 binds in that
% same rule's body. Anything else -- the tick flowing through a `:=`, or
% through another rel first -- is already covered by the normal contribution
% path or by edge_head_column_type_mismatch.
now_bound_head_position(Rules, Ref, Position) :-
    member(Rule, Rules),
    rule_head_ref(Rule, Ref),
    rule_body(Rule, Body),
    conjunction_goals(Body, Goals),
    member(TickGoal, Goals),
    tick_goal(TickGoal, TickVariable),
    rule_head(Rule, Head),
    Head =.. [_ | HeadArgs],
    nth1(Position, HeadArgs, HeadArg),
    HeadArg == TickVariable,
    !.

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
column_type_fixpoint(Rules, RefColumns, SeedMap, Current, Settled) :-
    findall(Ref-TypeList,
            ( member(Ref-Contributions, Current),
              maplist(raw_contribution, Contributions, TypeList) ),
            TypeMap),
    findall(Ref-Merged,
            ( member(Ref-Seeds, SeedMap),
              rule_head_contributions(Rules, RefColumns, TypeMap, Ref,
                                      RuleContributions),
              merge_contribution_lists(Seeds, RuleContributions, Merged) ),
            Next),
    ( Next == Current
    -> Settled = Current
    ;  column_type_fixpoint(Rules, RefColumns, SeedMap, Next, Settled)
    ).

rule_head_contributions(Rules, RefColumns, TypeMap, Ref, Contributions) :-
    findall(HeadTypes,
            ( member(Rule, Rules),
              rule_head_ref(Rule, Ref),
              rule_head_contribution(RefColumns, TypeMap, Rule, HeadTypes) ),
            Contributions).

rule_head_contribution(RefColumns, TypeMap, Rule, HeadTypes) :-
    rule_head(Rule, Head),
    rule_body(Rule, Body),
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

% Operator inventory from registry.pl's expression/5 (rank R5 of
% plans/2026-07-29-prolog-org-review.md), not a local list.
arithmetic_expression(Expr, Left, Right) :-
    compound(Expr), Expr =.. [Functor, Left, Right],
    expression(Functor/2, arithmetic, _, _, _).

% Positionwise combine, only where the seed is open/1: text dominates, then
% int, then none. A frozen(Type) position is returned unchanged.
merge_contribution_lists(Seeds, RuleContributions, Merged) :-
    foldl(merge_one_rule_contribution, RuleContributions, Seeds, Merged).

merge_one_rule_contribution(Types, Acc0, Acc) :- maplist(merge_contribution, Types, Acc0, Acc).

merge_contribution(_, frozen(Type), frozen(Type)) :- !.
merge_contribution(Contribution, open(Existing), open(Merged)) :-
    merge_type(Contribution, Existing, Merged).

% STRUCT-AS-ROWS: ref(Type) travels through the fixpoint and these clauses
% come FIRST, ahead of the scalar widening. A head column fed by a ref body
% column is itself a ref column -- letting the text clause win would store the
% dictionary id and RENDER IT AS AN INTEGER at the boundary, which is exactly
% the "tick log prints ids" failure Edge 1 exists to prevent, and it would be
% silent. Two different declared types meeting in one column, or a ref meeting
% a scalar, is a named refusal instead: there is no widening that keeps the
% rendering honest.
merge_type(ref(Type), none, ref(Type)) :- !.
merge_type(none, ref(Type), ref(Type)) :- !.
merge_type(ref(Type), ref(Type), ref(Type)) :- !.
merge_type(Left, Right, _) :-
    ( Left = ref(_) ; Right = ref(_) ), !,
    throw(unsupported_construct(column_ref_type_conflict(Left, Right))).
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
% walks). Three ACCEPTED shapes, everything else a NAMED refusal:
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
%   sampled_conjunction(TriggerAtoms, SampleAtoms, NegAtoms, GuardGoals) --
%     the same positive conjunction plus any mix of latest(Atom) samples,
%     not(Atom) negations, and comparison/bind guards. TriggerAtoms remain
%     the only occurrence sources. Everything past them is read against the
%     CURRENT store exactly as engine.pl:solve/2 reads it once the firing
%     atom's own arguments are bound: SampleAtoms and NegAtoms are unwrapped
%     plain rel atoms (joined, or NOT EXISTS'd, by lower.pl), GuardGoals are
%     the WHERE conditions and the `:=` bindings, folded left to right the
%     way solve/2 resolves a conjunction.
% Everything else names the SPECIFIC blocking construct instead of a
% blanket edge_body_shape reason, so the scoreboard's per-construct tally
% stays precise as this widens further.
%   departure_trigger(FinalizeAtom, OtherAtoms, SampleAtoms, NegAtoms,
%     GuardGoals) -- the body binds exactly one rel atom with finalize/1.
%     THAT is the only occurrence source, and it fires the tick AFTER the row
%     left (engine.pl tick/7's DepartureCarry; update-arm verdict "departure =
%     next-tick occurrence"). Every OTHER positive atom is a plain join
%     against the current store, NOT a trigger, and that is faithful rather
%     than a narrowing: engine.pl DOES also raise an arrival item for each of
%     them, but such an occurrence leaves finalize/1 standing in the body and
%     `solve(finalize(_), _) :- !, fail` (body.pl:113) makes that arm derive
%     nothing, always. The update arm
%     `changed(K,Old,New) <+ finalize(r(K,Old)), r(K,New)` is exactly this
%     shape: one departure arm, `r(K,New)` joined.
edge_trigger_shape(Body, unsupported(Reason)) :-
    conjunction_goals(Body, Goals),
    edge_registered_refusal(Body, Goals, Reason), !.
edge_trigger_shape(Body,
                   departure_trigger(FinalizeAtom, OtherAtoms, SampleAtoms,
                                     NegAtoms, GuardGoals)) :-
    conjunction_goals(Body, Goals),
    edge_departure_goals(Goals, FinalizeAtoms, OtherAtoms, SampleAtoms,
                         NegAtoms, GuardGoals),
    FinalizeAtoms \== [],
    !,
    ( FinalizeAtoms = [FinalizeAtom] -> true
    ; throw(unsupported_construct(edge_body_multiple_finalize(Body))) ).
edge_trigger_shape(Body, unmarked_conjunction(Atoms)) :-
    conjunction_goals(Body, Goals),
    maplist(plain_positive_atom, Goals),
    !, Atoms = Goals.
edge_trigger_shape(Body,
                   sampled_conjunction(TriggerAtoms, SampleAtoms, NegAtoms,
                                       GuardGoals)) :-
    conjunction_goals(Body, Goals),
    edge_sampled_goals(Goals, TriggerAtoms, SampleAtoms, NegAtoms, GuardGoals),
    TriggerAtoms \== [],
    ( SampleAtoms \== [] ; NegAtoms \== [] ; GuardGoals \== [] ),
    !.
edge_trigger_shape(Body, unsupported(edge_body_shape(Body))).

% The four goal classes are disjoint by construction: plain_positive_atom/1
% is exactly "no registry row", and latest/1, not/1 and every comparison or
% bind operator each carry one. The cuts say so; without them a failure
% deeper in the list would backtrack into a classification that cannot hold.
edge_sampled_goals([], [], [], [], []).
edge_sampled_goals([latest(Atom) | Rest], TriggerAtoms, [Atom | SampleAtoms],
                   NegAtoms, GuardGoals) :-
    plain_positive_rel_atom(Atom), !,
    edge_sampled_goals(Rest, TriggerAtoms, SampleAtoms, NegAtoms, GuardGoals).
edge_sampled_goals([not(Atom) | Rest], TriggerAtoms, SampleAtoms,
                   [Atom | NegAtoms], GuardGoals) :-
    plain_positive_rel_atom(Atom), !,
    edge_sampled_goals(Rest, TriggerAtoms, SampleAtoms, NegAtoms, GuardGoals).
edge_sampled_goals([Goal | Rest], TriggerAtoms, SampleAtoms, NegAtoms,
                   [Goal | GuardGoals]) :-
    ( guard_or_bind_goal(Goal) ; tick_goal(Goal, _) ), !,
    edge_sampled_goals(Rest, TriggerAtoms, SampleAtoms, NegAtoms, GuardGoals).
edge_sampled_goals([Atom | Rest], [Atom | TriggerAtoms], SampleAtoms, NegAtoms,
                   GuardGoals) :-
    plain_positive_atom(Atom),
    edge_sampled_goals(Rest, TriggerAtoms, SampleAtoms, NegAtoms, GuardGoals).

% The same split with a fourth bucket in front: finalize/1 around a plain rel
% atom. Disjoint by construction for the same reason edge_sampled_goals/5 is,
% and it delegates the other three buckets to that predicate rather than
% restating them.
edge_departure_goals([], [], [], [], [], []).
edge_departure_goals([finalize(Atom) | Rest], [Atom | FinalizeAtoms], OtherAtoms,
                     SampleAtoms, NegAtoms, GuardGoals) :-
    plain_positive_rel_atom(Atom), !,
    edge_departure_goals(Rest, FinalizeAtoms, OtherAtoms, SampleAtoms, NegAtoms,
                         GuardGoals).
edge_departure_goals([Goal | Rest], FinalizeAtoms, OtherAtoms, SampleAtoms,
                     NegAtoms, GuardGoals) :-
    edge_sampled_goals([Goal], GoalTriggers, GoalSamples, GoalNegs, GoalGuards),
    !,
    edge_departure_goals(Rest, FinalizeAtoms, RestOthers, RestSamples, RestNegs,
                         RestGuards),
    append(GoalTriggers, RestOthers, OtherAtoms),
    append(GoalSamples, RestSamples, SampleAtoms),
    append(GoalNegs, RestNegs, NegAtoms),
    append(GoalGuards, RestGuards, GuardGoals).

edge_registered_refusal(Body, Goals, Reason) :-
    findall(Priority-Candidate,
            ( member(Goal, Goals),
              edge_goal_refusal(Goal, Body, Priority, Candidate)
            ),
            Candidates),
    keysort(Candidates, [_-Reason | _]).

edge_goal_refusal(latest(Atom), Body, 1, edge_body_with_latest(Body)) :-
    \+ plain_positive_rel_atom(Atom).
% finalize/1 IS a departure trigger now (TICK PHASE ALIGNMENT target 2): the
% runtime carries the tick's net -delta rows of every LISTENED rel into a
% per-rel departure frontier, and the arm reads that table exactly the way an
% arrival arm reads __frontier_. What stays refused is finalize around
% anything but ONE plain rel atom -- a conjunction, an expression, a nested
% wrapper -- because occurrence_trigger/4 unifies the departed ROW against the
% finalize argument, and there is no row shape for those.
edge_goal_refusal(finalize(Atom), Body, 2, edge_body_with_finalize(Body)) :-
    \+ plain_positive_rel_atom(Atom).
% pre/1 is NOT a wider arm, which is why the edge-body arc widened everything
% around it and left this one standing. engine.pl fires occurrences ONE AT A
% TIME and pre(Atom) reads the store as the writes so far THIS TICK left it
% (engine.pl step 4), so two arrivals for one key fold 0 -> 1 -> 2. A sampled
% base-table read -- the shape latest/1 got -- projects (clicks,1) twice and
% lands 1 where the oracle pins 2 (receipt quoted in SCOREBOARD.md's
% "edge_body_needs_pre" section, run against
% merge_family.pl:batched_increments_both_count). The fold is also CROSS-ARM,
% so no per-arm recursive CTE expresses it either. It needs an ordered
% occurrence loop in the runtime; refused by name until that exists.
edge_goal_refusal(Goal, Body, 3, edge_body_needs_pre(Body)) :-
    body_surface_for_term(Goal, _, sample, refs_of_arg(_, pos, sampled),
                          wrapper(rel_atom, refuse(goal)), refused).
% now/1 is lowered around a plain variable; `now(7)` or `now(f(X))` would be a
% MATCH against the tick, which engine.pl's solve(now(Tick), ctx(_,_,Tick))
% permits by unification and this lowering has no shape for.
edge_goal_refusal(now(Argument), Body, 4, edge_body_with_now(Body)) :-
    \+ var(Argument).
% Negation is LIVE around ONE plain relation atom (lowered to NOT EXISTS
% against that rel's current table, the same text compile_negative_uses/4
% emits for a level body). not/1 around a conjunction, a comparison or
% another wrapper stays refused: compile_negative_uses/4 walks rel atoms
% only, and a nested guard would be silently dropped from the emitted
% condition rather than refused.
edge_goal_refusal(not(Atom), Body, 5, edge_body_with_negation(Body)) :-
    \+ plain_positive_rel_atom(Atom).
% decode/2 and json_each/2 are blocked BELOW the arm, on the value encoding,
% which is why widening the arm did not reach them. A compound value that
% ARRIVES is stored as canonical term text (`fresh(tag_w1,body1)` --
% sweep.pl:term_text/2, mirroring ticklog.pl's own value_json/2), while
% compile_pattern_arg/7's compound branch destructures the json1 tagged form
% (`json_extract(Expr,'$.fn')` / `'$.args[N]'`) that the emitter writes for a
% compound a HEAD expression produces. The two encodings do not meet, and that
% mismatch is the one SCOREBOARD.md has carried since phase C ("compound
% arrival text vs json1 match"). Lowering decode/2 on top of it would answer
% zero rows where the oracle unifies. The object-pattern half (`{name: X}`)
% needs a third shape again, and json_each/2 needs a table-valued read.
% A decode arc owns the encoding decision first; the arm is not the blocker.
%
% STRUCT-AS-ROWS flipped decode/2's registry row to `lower` so a LEVEL body
% over a struct-typed column compiles to a dictionary join. This clause
% therefore names the two functors directly instead of reading the refused
% status off the row: the arm is still the blocker for an EDGE body, and the
% status word can no longer say so.
edge_goal_refusal(Goal, Body, 8, edge_body_needs_json_destructure(Body)) :-
    nonvar(Goal),
    ( Goal = decode(_, _) ; Goal = json_each(_, _) ).

% The ordered goal list, with next/1 and variadic combine spliced away and
% not/1 left whole. The splicing used to be a local `Term =.. [combine | ...]`
% test plus a next/1 clause; it is now the registry's splice_bare role, walked
% by 0_body_walk.pl (rank R1). not/1 stays one goal because callers depend on
% it: negated_guard_goal/2 below selects a not/1 goal out of this list and
% inspects its inside.
conjunction_goals(Body, Goals) :-
    body_conjunction_goals(Body,
                           walk_policy(descend_not(false), splice_bare(true)),
                           Goals).

plain_positive_atom(Goal) :-
    compound(Goal),
    \+ body_surface_for_term(Goal, _, _, _, _, _).

plain_positive_rel_atom(Goal) :-
    conjunction_goals(Goal, [Goal]),
    plain_positive_atom(Goal).

% Trigger refs a shape can fire from -- used by the same-key conflict-risk
% check below. marked_single fires from exactly one ref; unmarked_conjunction
% fires from ANY of its atoms' refs (engine.pl unmarked_items/2 wraps every
% one independently). sampled_conjunction excludes its sampled, negated and
% guard goals: none of them is an occurrence source.
shape_trigger_refs(marked_single(Atom), [Ref]) :- rel_ref(Atom, Ref).
shape_trigger_refs(unmarked_conjunction(Atoms), Refs) :-
    findall(Ref, ( member(Atom, Atoms), rel_ref(Atom, Ref) ), Refs0), sort(Refs0, Refs).
shape_trigger_refs(sampled_conjunction(TriggerAtoms, _, _, _), Refs) :-
    findall(Ref, ( member(Atom, TriggerAtoms), rel_ref(Atom, Ref) ), Refs0),
    sort(Refs0, Refs).
% One source, the finalize'd rel: the joined atoms are read, never fired from
% (see the shape's own note -- an arrival occurrence on one of them leaves
% finalize standing and body.pl's solve/2 fails it).
shape_trigger_refs(departure_trigger(FinalizeAtom, _, _, _, _), [Ref]) :-
    rel_ref(FinalizeAtom, Ref).

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
    % NegAtoms deliberately excluded: compile_negative_atom_args/6 runs in
    % `check` mode and never binds a head variable, so a negated atom's
    % column types cannot be the source of a head column's value.
    ; Shape = sampled_conjunction(TriggerAtoms, SampleAtoms, _, _)
      -> append(TriggerAtoms, SampleAtoms, BodyAtoms)
    % The departed row's own columns feed the head exactly as a trigger row's
    % do (the departure frontier carries the rel's declared columns), so the
    % finalize'd atom belongs in this list beside the joins and samples.
    ; Shape = departure_trigger(FinalizeAtom, OtherAtoms, SampleAtoms, _, _)
      -> append([[FinalizeAtom], OtherAtoms, SampleAtoms], BodyAtoms)
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

% ═══ mid-tick level state on the non-trigger side of an edge arm ═══════════
% The refusal that used to live here (`edge_body_joins_arrival_fed_level`) is
% GONE, closed by the TICK PHASE ALIGNMENT arc. It was a runtime-seam
% placeholder: the emitted mid-tick level plane was insert-only, so a level row
% an arrival retracted this tick was still in its table when a NON-TRIGGER atom
% joined it (clock_rel_join_storms tick 3 derived three diag_seen rows where
% the oracle derives one). Both pipelines now freeze the plane where engine.pl
% freezes it -- 1_incremental.ts:recomputeLevelsBeforeEdges runs the retracting
% half between applyLevelsBeforeEdges and applyEdges, and the naive referee
% calls recomputeLevels once before the edge batch and once after -- so the
% whole class compiles and grades identical in both emitter modes.

% ═══ supported-subset gate ═══════════════════════════════════════════════════
% Refuses (with a specific term, not a generic failure) any construct wider
% than what lower.pl knows how to emit: an edge rule body must classify as
% marked_single, unmarked_conjunction, or sampled_conjunction
% (edge_trigger_shape/2 above); a level rule must not carry an aggregate head
% (aggregate_head, level_eval.pl) and must not reference pre/1, now/1,
% decode/2 or json_each/2 (none of the two target fixtures need them;
% lower.pl has no SQL shape for them yet).
%
% FORMAL MODEL: TICK-MODEL.md (same directory) is the semiring/grading
% semantics behind this gate. The cross-plane refusals below
% (log_on_level_headed_rel, latest_in_level_rule, pre_in_level_rule, and
% engine.pl's finalize_in_level_rule / keyed_level_head) are hand-proven
% instances of its ring/grade discipline; the planned clock checker
% (TICK-MODEL.md section 6) generalizes them and lives here when built.

% Two entries on purpose. check_supported_subset/1 takes SURFACE syntax and
% expands it through the declared phase order; callers that have already
% expanded call check_supported_subset_expanded/1 directly. compile.pl's
% program_plan/2 used the sugared entry on an already-expanded program, which
% expanded a second time for nothing (rank R3 of
% plans/2026-07-29-prolog-org-review.md).
check_supported_subset(SugaredProg) :-
    expand_program(SugaredProg, ExpandedProg, _),
    check_supported_subset_expanded(ExpandedProg).

% The cross-plane trigger conditions come from 0_program_check.pl, shared with
% the oracle's engine:check_program/1 (rank R2 of
% plans/2026-07-29-prolog-org-review.md). This door keeps its own order, which
% INTERLEAVES the shared classes with compiler-only capability refusals, and
% its own exception vocabulary: every refusal here wraps in
% unsupported_construct/1, and keyed_log_rel additionally carries the key
% positions the emitter would have needed.
%
% Order is fixture data. A program violating two classes reports a different
% one at each door, so the two segments below sit exactly where the four
% separate forall/2 goals they replaced used to sit.
check_supported_subset_expanded(Program) :-
    Program = prog(Decls, Rules),
    % STRUCT-AS-ROWS: the declared value plane is checked FIRST on both doors
    % (engine.pl:engine_check_order/1 opens with the same two classes), before
    % anything else reads a column type -- every later check that asks a
    % column's storage kind would otherwise ask it of a type that does not
    % resolve.
    shared_refusal(Program, [ type_cycle, column_type_unknown ]),
    forall(( member(Rule, Rules), rule_reserved_construct(Rule, Construct) ),
           throw(unsupported_construct(Construct))),
    shared_refusal(Program, [ log_on_level_headed_rel,
                              % TICK PHASE ALIGNMENT target 2 closed a hole
                              % this list did not have to cover before: while
                              % finalize/1 was a REFUSED registry row, a
                              % finalize in a level body was caught by
                              % check_level_rule_shape's generic refused-goal
                              % path, which reported the enclosing head
                              % (level_body_goal) where the oracle reported
                              % finalize_in_level_rule -- the drift the
                              % cross_plane_check_parity unit recorded. The row
                              % is live now, that path is gone, and without
                              % this entry the compiler would ACCEPT a program
                              % the oracle rejects (measured). Placed where
                              % engine.pl's own engine_check_order/1 places it,
                              % ahead of latest/pre, so the two doors also
                              % agree on WHICH class a program violating
                              % several of them reports.
                              finalize_in_level_rule,
                              latest_in_level_rule,
                              pre_in_level_rule,
                              keep_on_non_log_rel,
                              % RANK R2 HOLE CLOSURES. Both were checked by the
                              % oracle alone, so the compiler accepted programs
                              % the reference door rejects. They run here, after
                              % the classes that were already shared and before
                              % the per-rule capability checks, so an
                              % already-refused program keeps its current
                              % diagnostic.
                              missing_retention,
                              aggregate_in_edge_head ]),
    forall(( member(Rule, Rules), rule_is_edge(Rule) ), check_edge_rule_shape(Rule)),
    forall(( member(Rule, Rules), rule_is_level(Rule) ), check_level_rule_shape(Rule)),
    check_no_edge_head_conflict_risk(Decls, Rules),
    shared_refusal(Program, [ keyed_level_head, keyed_log_rel ]).

shared_refusal(Program, Order) :-
    (   first_violation(Program, Order, violation(Name, Payload))
    ->  compiler_refusal(Name, Payload, Construct),
        throw(unsupported_construct(Construct))
    ;   true
    ).

compiler_refusal(type_cycle,            Names, type_cycle(Names)).
compiler_refusal(column_type_unknown,    Name, column_type_unknown(Name)).
compiler_refusal(keyed_level_head,        Ref, keyed_level_head(Ref)).
% The only payload that differs from the oracle's: lowering needs the
% positions to explain which key decl is at fault.
compiler_refusal(keyed_log_rel,   Ref-Positions, keyed_log_rel(Ref, Positions)).
compiler_refusal(log_on_level_headed_rel, Ref, log_on_level_headed_rel(Ref)).
compiler_refusal(keep_on_non_log_rel,     Ref, keep_on_non_log_rel(Ref)).
compiler_refusal(finalize_in_level_rule,  Ref, finalize_in_level_rule(Ref)).
compiler_refusal(latest_in_level_rule,    Ref, latest_in_level_rule(Ref)).
compiler_refusal(pre_in_level_rule,       Ref, pre_in_level_rule(Ref)).
compiler_refusal(missing_retention,       Ref, missing_retention(Ref)).
% Named WITH the offending head reference, where the oracle's bare
% aggregate_in_edge_head names nothing. A compiler refusal has to say which
% rule to edit; the oracle's term is the one fixtures already pin.
compiler_refusal(aggregate_in_edge_head,  Ref, aggregate_in_edge_head(Ref)).

% Both are the shared walk (rank R1 of
% plans/2026-07-29-prolog-org-review.md). The oracle's engine:body_latest_ref/2
% and engine:body_pre_ref/2 call the same implementation under the same policy,
% and the body_walk_characterization unit asserts the two sides agree case by
% case, which is what makes the cross-plane refusal parity real rather than
% coincidental.
level_body_latest_ref(Body, Ref) :-
    body_wrapper_refs(Body, latest,
                      walk_policy(descend_not(true), splice_bare(false)),
                      Ref).

level_body_pre_ref(Body, Ref) :-
    body_wrapper_refs(Body, pre,
                      walk_policy(descend_not(true), splice_bare(false)),
                      Ref).

% engine.pl:listened_departure_refs/2, clause for clause: the rels some EDGE
% rule binds with finalize/1, sorted. The policy is the oracle's own --
% descend_not(false), because a NEGATED finalize is not a departure the engine
% listens for (and the compiler refuses it separately as
% edge_body_with_negation, since not/1 accepts one plain rel atom only).
% Only these rels get a departure frontier table; every other rel's emitted
% text is unchanged by the existence of this feature.
listened_departure_refs(Rules, Refs) :-
    findall(Ref,
            ( member(Rule, Rules), rule_is_edge(Rule), rule_body(Rule, Body),
              body_finalize_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

body_finalize_ref(Body, Ref) :-
    body_wrapper_refs(Body, finalize,
                      walk_policy(descend_not(false), splice_bare(false)),
                      Ref).

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

% Shared walk (rank R1), same policy as before: the conjunction spine only,
% not/1 opaque and next/1 and combine unspliced. A reserved construct nested
% inside either wrapper is therefore still not reached; the review recorded
% that as drift against body_ref_uses/2, which does splice. Widening it would
% start refusing programs that load today, so it is left as it stands and
% named here rather than changed under cover of a refactor.
reserved_construct_in_body(Body, Construct) :-
    walk_body(Body, walk_policy(descend_not(false), splice_bare(false)),
              Events),
    member(event(_, _, surface(Functor/_, _, _, LowerRole, reserved), _),
           Events),
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
    % COMPILER-ONLY refusal, deliberately NOT mirrored into 0_program_check.pl
    % or engine.pl: the oracle DOES solve now/1 in a level body (solve/2 reads
    % the tick straight out of ctx/3, and level_closure/5 is handed the same
    % Tick), so this is a capability gap named precisely, not a semantics
    % change. Lowering it would need the tick inside the level DELETE/INSERT
    % pair as well as inside an edge arm, and no fixture writes one; leaving
    % it to the guard fold, which never sees a level body's now/1, would drop
    % the binding silently.
    ( rule_body_tick_ref(Body, TickGoal)
    -> throw(unsupported_construct(now_in_level_rule(Head, TickGoal)))
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

% Any now/1 goal a level body reaches, negation included -- the refusal is
% about the level plane having no tick source in its emitted SQL, and that is
% true on either side of a not/1.
rule_body_tick_ref(Body, Goal) :-
    walk_body(Body, walk_policy(descend_not(true), splice_bare(true)), Events),
    member(event(_, _, surface(now/1, _, _, _, _), Goal), Events),
    !.

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

% Same inventory as arithmetic_expression/3 above. The arity test stays
% separate: this one looks for an arithmetic FUNCTOR anywhere in a head
% argument, including inside a deeper compound.
contains_arithmetic_functor(Arg, Arg) :-
    compound(Arg), Arg =.. [Functor | SubArgs], SubArgs \== [],
    expression(Functor/2, arithmetic, _, _, _), !.
contains_arithmetic_functor(Arg, Found) :-
    compound(Arg), Arg =.. [_ | SubArgs], member(SubArg, SubArgs), contains_arithmetic_functor(SubArg, Found).

% Shared walk (rank R1) over the conjunction spine, with not/1 opaque and no
% splicing, exactly the reach this had before. A negated forbidden goal is
% covered separately by negated_guard_goal/2.
%
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
body_forbidden_goal(Body, Forbidden) :-
    walk_body(Body, walk_policy(descend_not(false), splice_bare(false)),
              Events),
    member(event(_, _, surface(_, _, _, LowerRole, refused), Term), Events),
    refused_goal_term(LowerRole, Term, Forbidden).

refused_goal_term(infix(refuse(comparison)), Term, comparison(Term)) :- !.
refused_goal_term(infix(refuse(goal)), Term, bind(Term)) :- !.
refused_goal_term(wrapper(_, refuse(goal)), Term, Term).
