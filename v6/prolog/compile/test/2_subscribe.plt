% 2_subscribe.plt : receipts for subscribed_rels/4 (the lazy-compute seed).
%
% One predicate, compute only, wired to nothing yet. These pin the two
% branches of the spec: the strict null-query rule (no query, nothing, per
% ruling zero_query_semantics 2026-08-03), and the subscribe dependency closure
% (transitive, positive and negated reads, sampler wrappers included).

:- use_module(library(plunit)).
:- use_module(library(lists)).
:- use_module('../../5_subscribe/2_subscribe', [ subscribed_rels/4 ]).
:- use_module('../../compile', [ program_plan/2 ]).
:- use_module('../../7_lower/lower', [ lower_program/2, boot_statements/7 ]).
:- use_module('../../emit_ts', [ emit_program/5 ]).
:- use_module('../parse_dl_dcg', [ parse_dl/4, parse_dl_file/4 ]).
:- use_module('../../3_analyze/analyze', [ declared_refs/2 ]).
:- use_module('../../3_analyze/0_rel_record', [ relplan_parts/6 ]).
:- use_module('../../0_dot_expand/0_body_walk',
              [ body_relation_atoms/4, body_wrapper_refs/4 ]).

:- op(1150, xfx, <+).

% Captured while THIS file loads; plunit_tests.pl's own test_dir_fact/1 is
% asserted after its ensure_loaded of this file, so it is not in scope here.
:- dynamic(cone_test_dir/1).
:- prolog_load_context(directory, ConeHere), assertz(cone_test_dir(ConeHere)).

:- begin_tests(subscribe_cone).

% Strict per ruling zero_query_semantics 2026-08-03: an empty query list means
% no analysis entry point, so NOTHING is subscribed -- even for a program whose
% rules form a fully connected chain. Harness-side subscription roots arrive
% with the pruning rung.
test(zero_query_subscribes_nothing) :-
    Decls = [kind(root/1, log), keep(root/1, all),
             kind(mid/2, set),
             kind(top/1, set)],
    Rules = [(mid(Left, Right) <- root(Left), root(Right)),
             (top(Result) <- mid(_, Result))],
    subscribed_rels(Decls, Rules, [], Cone),
    Cone == [].

% The cone is only the query's subscribe chain; the rule nowhere reachable
% from the seed (e <- d) stays out even though d and e are declared.
test(hand_computed_cone) :-
    Decls = [kind(a/1, set), kind(b/1, set), kind(c/1, set),
             kind(d/1, set), kind(e/1, set)],
    Rules = [(b(Value) <- a(Value)),
             (c(Value) <- b(Value)),
             (e(Value) <- d(Value))],
    subscribed_rels(Decls, Rules, [c(Value)], Cone),
    Cone == [a/1, b/1, c/1].

% pre/1 still names its sampled rel, so the sampler reference joins the cone
% alongside the bare read that feeds it.
test(sampler_included) :-
    Decls = [kind(trigger/1, log), keep(trigger/1, all),
             kind(sample/1, set)],
    Rules = [(sample(Total) <- trigger(Item), pre(sample(Total)))],
    subscribed_rels(Decls, Rules, [sample(Item)], Cone),
    sort([sample/1, trigger/1], Expect),
    Cone == Expect.

% A negated body read is still a dependency: the negated rel joins the cone.
test(negation_included) :-
    Decls = [kind(ok/1, set), kind(blocked/1, set),
             kind(gate/1, set)],
    Rules = [(gate(Name) <- ok(Name), not(blocked(Name)))],
    subscribed_rels(Decls, Rules, [gate(Name)], Cone),
    sort([gate/1, ok/1, blocked/1], Expect),
    Cone == Expect.

% ── real-program shapes the synthetic kind/2 decls above never produced ──────
%
% Each test below pins ONE extension the wire forced. The .plt corpus was
% written against hand-built kind/2 decls and `<-` rules; the Decls and Rules
% compile.pl:program_plan/2 actually hands over are post-expansion, and they
% look nothing like that.

% EXTENSION 1: a real .dl6 `rel` declaration lands as col_type/3 rows, never
% kind/2 -- kind/2 only appears for a log rel. Seeding from an explicit query
% still walks that shape; the seed and the one rule's read both show up.
test(decl_walk_reads_the_real_decl_forms) :-
    string_codes(
      "rel seed(id: int).\nrel job(id: int).\nrel trail(id: int) log keep(all).\njob(Id) <- seed(Id).\n",
      Codes),
    parse_dl(Codes, prog(Decls, Rules), _, []),
    \+ memberchk(kind(seed/1, _), Decls),
    subscribed_rels(Decls, Rules, [job(Id)], Cone),
    Cone == [job/1, seed/1],
    !.

% The earlier parity against analyze.pl:declared_refs/2 anchored the DELETE of
% the compat value. Under the strict null-query rule a decl the analyze pass
% still sees must not leak into a query-less cone by accident.
test(declared_rels_do_not_leak_into_a_queryless_cone) :-
    string_codes(
      "rel seed(id: int).\nrel job(id: int).\nrel trail(id: int) log keep(all).\n",
      Codes),
    parse_dl(Codes, prog(Decls, Rules), _, []),
    declared_refs(Decls, AnalyzeRefs),
    memberchk(seed/1, AnalyzeRefs),
    subscribed_rels(Decls, Rules, [], Cone),
    Cone == [],
    !.

% EXTENSION 2: edge arms. `<+` rules carry subscribe exactly as `<-` rules do,
% and cone_fixpoint/3 matched only `<-` before the wire -- an edge-headed rel
% contributed neither its head nor a single body read. This chain is
% head-to-body-to-head twice, so a missed arm shows up as a missing rel rather
% than a missing edge. The pre/1 read rides an edge arm here because that is
% where golden-flex.dl6 puts every one of its own.
test(edge_arm_chain_including_pre) :-
    Decls = [kind(tick_source/1, log), keep(tick_source/1, all),
             keyed(counter/2, [1]),
             col_type(rollup/2, id, int),
             col_type(unrelated/1, id, int)],
    Rules = [(counter(Id, 1) <+ tick_source(Id), not(counter(Id, _))),
             (counter(Id, Next) <+ tick_source(Id), pre(counter(Id, Prev)),
                                   Next := Prev + 1),
             (rollup(Id, Total) <- counter(Id, Total))],
    subscribed_rels(Decls, Rules, [rollup(_, _)], Cone),
    Cone == [counter/2, rollup/2, tick_source/1].

% EXTENSION 3: next/1 and combine splice their arguments in, so the atoms they
% wrap are ordinary reads. golden-flex.dl6 reaches sensor/2 through exactly one
% combine and nothing else, so before the shared walk that rel was invisible.
test(next_and_combine_spliced) :-
    Decls = [col_type(pickable/1, id, int), col_type(sensor/1, id, int),
             col_type(monitored/1, id, int), col_type(display/1, id, int)],
    Rules = [(monitored(Id) <- combine(pickable(Id), sensor(Id))),
             (display(Id) <- next(monitored(Id)))],
    subscribed_rels(Decls, Rules, [display(_)], Cone),
    Cone == [display/1, monitored/1, pickable/1, sensor/1].

% EXTENSION 4: the registry decides what a rel read is, so decode/2's json
% pattern, a `:=` bind, a comparison and now/1 contribute nothing WITHOUT this
% module naming any of them -- while a bind rel read (interval/2) and a host
% response rel, both plain atoms, do count. The old membership test against
% kind/2 decls got the first half right by accident and the second half wrong.
test(registry_decides_what_a_read_is) :-
    Decls = [col_type(payload/2, id, int),
             col_type(card/2, id, int),
             col_type(roll_call/2, id, int)],
    Rules = [(card(Id, Species) <- payload(Id, Blob),
                                   decode(Blob, {species:Species})),
             (roll_call(Id, Stamped) <- payload(Id, _), interval(1, Bucket),
                                        '__host_response_weigh'(Id, Label),
                                        Id > 0, now(Tick),
                                        Stamped := concat([Bucket, Label, Tick]))],
    subscribed_rels(Decls, Rules, [card(_, _)], CardCone),
    CardCone == [card/2, payload/2],
    subscribed_rels(Decls, Rules, [roll_call(_, _)], RollCone),
    RollCone == ['__host_response_weigh'/2, interval/2, payload/2,
                 roll_call/2].

% ── golden-flex.dl6, the composition receipt's own program ───────────────────
%
% The four invariants the wire owes a REAL program. "Every pre-read rel is in
% the cone" is read as: no rule the cone admits has a pre/1 read the cone
% dropped. golden-flex.dl6's own pre/1 reads all sit on edge rules outside the
% display/pick_stats cone, so that clause holds vacuously HERE and
% non-vacuously in edge_arm_chain_including_pre above.
test(golden_flex_cone_invariants) :-
    cone_test_dir(Here),
    atomic_list_concat([Here, '/../../../dl/fixtures/golden-flex.dl6'], File),
    expand_uses(File, [], [], _, Program, _, Bindings, []),
    program_plan(fixture(golden_flex_cone_invariants, Program, [], [], [])
                 -Bindings, Plan),
    Plan = plan(_, prog(_, Rules), _, RelPlans, _, _, _, Cone, _),
    Cone \== [],
    findall(Ref,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Ref, _, _, _, _) ),
            AllRefs0),
    sort(AllRefs0, AllRefs),
    subtract(Cone, AllRefs, []),
    % Both query rels, and every rel their own rule bodies read.
    subtract([display/2, pick_stats/5], Cone, []),
    forall(( member(Rule, Rules), cone_head(Rule, HeadRef, Body),
             memberchk(HeadRef, Cone),
             body_relation_atoms(Body,
                                 walk_policy(descend_not(true),
                                             splice_bare(true)),
                                 _, Atom),
             functor(Atom, Name, Arity) ),
           memberchk(Name/Arity, Cone)),
    % Every pre/1 sampled ref of an admitted rule.
    forall(( member(Rule, Rules), cone_head(Rule, HeadRef, Body),
             memberchk(HeadRef, Cone),
             body_wrapper_refs(Body, pre,
                               walk_policy(descend_not(true),
                                           splice_bare(true)),
                               PreRef) ),
           memberchk(PreRef, Cone)),
    % A strict subset: a 55-rel program answering two queries must not subscribe
    % to all 55, or the wire proved nothing.
    length(Cone, ConeSize), length(AllRefs, AllSize),
    ConeSize < AllSize,
    !.

cone_head((Head <- Body), Name/Arity, Body) :- functor(Head, Name, Arity).
cone_head((Head <+ Body), Name/Arity, Body) :- functor(Head, Name, Arity).

% ── the wire: the cone reaches the emitted module ────────────────────────────

% The cone reaches the TICK PATH through the SUBSCRIBED_* consts, and the flag
% that selects them is off unless the environment says otherwise
% (runtime/3_subscribe.ts owns the env name; this pins that the module asks).
test(emitted_module_prunes_the_tick_path_behind_the_flag) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n? job(7, secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(emitted_module_prunes_the_tick_path_behind_the_flag,
                 Program, Bindings, Text),
    once(sub_atom(Text, _, _, _, 'const SUBSCRIBE_PRUNE = SubscribeCone.mode();')),
    once(sub_atom(Text, _, _, _,
                  'const SUBSCRIBED_LEVEL_STATEMENTS = SubscribeCone.levels(SUBSCRIBE_PRUNE, INCREMENTAL_LEVEL_STATEMENTS, subscribed_rels);')),
    once(sub_atom(Text, _, _, _,
                  'IncrementalRuntime.apply_levels_before_edges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS)')),
    once(sub_atom(Text, _, _, _, '  boot: SUBSCRIBED_BOOT,')),
    % incrementalPlan describes the compiled program rather than the tick
    % path's working lists, so it stays unpruned.
    once(sub_atom(Text, _, _, _, '  levels: INCREMENTAL_LEVEL_STATEMENTS,')),
    % Only the incremental path can honor a cone; the ordered path names
    % itself instead of answering a pruned question with an unpruned tick.
    once(sub_atom(Text, _, _, _, 'const SUBSCRIBE_PRUNE_TICK_PATH: string = "incremental";')),
    once(sub_atom(Text, _, _, _,
                  'if (SUBSCRIBE_PRUNE === "on" && SUBSCRIBE_PRUNE_TICK_PATH !== "incremental") {')),
    !.

% Every boot statement names its rel, which is the only thing that lets the
% filter keep ingestion (a seeded source row) while dropping derivation (a
% level head's t=0 recompute).
test(emitted_boot_statements_name_their_rel) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\njob(Id, Secs) <- seed(Id, Secs).\n? job(7, secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(emitted_boot_statements_name_their_rel, Program, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  '{ rel: "job", sql: `DELETE FROM "emitted_boot_statements_name_their_rel_job"`, params: [] },')),
    !.

% The hand-computed cone of this program: the query seeds job/2, job's one rule
% body reads seed/2, and unread/1 is declared and reachable from nothing.
test(emitted_module_carries_the_hand_computed_cone) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n? job(7, secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(emitted_module_carries_the_hand_computed_cone,
                 Program, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  'export const subscribed_rels: readonly string[] = ["job/2", "seed/2"];')),
    !.

% Strict at the emit seam, matching zero_query_subscribes_nothing: no query
% decl, no analysis entry point, so the emitted constant is empty.
test(zero_query_module_subscribes_nothing) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(zero_query_module_subscribes_nothing, Program, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  'export const subscribed_rels: readonly string[] = [];')),
    !.

emitted_text(Name, Program, Bindings, Text) :-
    program_plan(fixture(Name, Program, [], [], [])-Bindings, Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, _), Types, RelPlans, _, _, _, _, Mode),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Mode, Decls, Types, RelPlans, [], LevelStatements, Boot),
    emit_program(Name, Plan, Lowered, Boot, Text).

:- end_tests(subscribe_cone).
