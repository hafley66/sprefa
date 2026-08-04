% 2_subscribe.plt : receipts for subscribed_rels/4 (the lazy-compute seed).
%
% One predicate, compute only, wired to nothing yet. These pin the two
% branches of the spec: the null-query compat rule, and the subscribe
% dependency closure (transitive, positive and negated reads, sampler
% wrappers included).

:- use_module(library(plunit)).
:- use_module(library(lists)).
:- use_module('../../2_subscribe', [ subscribed_rels/4 ]).
:- use_module('../../compile', [ program_plan/2 ]).
:- use_module('../../lower', [ lower_program/2, boot_statements/5 ]).
:- use_module('../../emit_ts', [ emit_program/5 ]).
:- use_module('../parse_dl', [ parse_dl/4, parse_dl_file/4 ]).
:- use_module('../../analyze', [ declared_refs/2 ]).
:- use_module('../../0_body_walk',
              [ body_relation_atoms/4, body_wrapper_refs/4 ]).

:- op(1150, xfx, <+).

% Captured while THIS file loads; plunit_tests.pl's own test_dir_fact/1 is
% asserted after its ensure_loaded of this file, so it is not in scope here.
:- dynamic(cone_test_dir/1).
:- prolog_load_context(directory, ConeHere), assertz(cone_test_dir(ConeHere)).

:- begin_tests(subscribe_cone).

% An empty query list means no analysis entry point; the compat rule puts the
% whole program on the subscribe side, so every declared rel is in the cone.
test(zero_query_all_rels) :-
    Decls = [kind(root/1, log), keep(root/1, all),
             kind(mid/2, set),
             kind(top/1, set)],
    Rules = [(mid(Left, Right) <- root(Left), root(Right)),
             (top(Result) <- mid(_, Result))],
    subscribed_rels(Decls, Rules, [], Cone),
    Cone == [mid/2, root/1, top/1].

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
% kind/2 -- kind/2 only appears for a log rel. The decl walk therefore reads
% the same four forms analyze.pl:declared_refs/2 reads, which is what the
% zero-query compat value is built from.
test(decl_walk_reads_the_real_decl_forms) :-
    string_codes(
      "rel seed(id: int).\nrel job(id: int).\nrel trail(id: int) log keep(all).\n",
      Codes),
    parse_dl(Codes, prog(Decls, Rules), _, []),
    \+ memberchk(kind(seed/1, _), Decls),
    subscribed_rels(Decls, Rules, [], Cone),
    Cone == [job/1, seed/1, trail/1],
    !.

% The decl half of the compat value is analyze.pl's declared_refs/2 by a
% different route; drift between them would silently shrink or widen the
% constant every emitted module carries.
test(declared_rels_match_analyze) :-
    string_codes(
      "rel seed(id: int).\nrel job(id: int).\nrel trail(id: int) log keep(all).\n",
      Codes),
    parse_dl(Codes, prog(Decls, Rules), _, []),
    declared_refs(Decls, AnalyzeRefs),
    subscribed_rels(Decls, Rules, [], Cone),
    Cone == AnalyzeRefs,
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
    parse_dl_file(File, Program, Bindings, []),
    program_plan(fixture(golden_flex_cone_invariants, Program, [], [], [])
                 -Bindings, Plan),
    Plan = plan(_, prog(_, Rules), RelPlans, _, _, _, Cone),
    Cone \== [],
    findall(Ref, member(relplan(Ref, _, _, _, _), RelPlans), AllRefs0),
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

% The hand-computed cone of this program: the query seeds job/2, job's one rule
% body reads seed/2, and unread/1 is declared and reachable from nothing. The
% emitted constant is what a lazy consumer would read; nothing consumes it yet,
% so this line is the whole receipt that the compiler computed it at all.
test(emitted_module_carries_the_hand_computed_cone) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n? job(7, secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(emitted_module_carries_the_hand_computed_cone,
                 Program, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  'export const subscribedRels: readonly string[] = ["job/2", "seed/2"];')),
    !.

% The compat rule at the emit seam: no query decl means no analysis entry
% point, so every rel the program declares or mentions is on the subscribe side.
test(zero_query_module_carries_every_rel) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(zero_query_module_carries_every_rel, Program, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  'export const subscribedRels: readonly string[] = ["job/2", "seed/2", "unread/1"];')),
    !.

emitted_text(Name, Program, Bindings, Text) :-
    program_plan(fixture(Name, Program, [], [], [])-Bindings, Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, _), RelPlans, _, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Decls, RelPlans, [], LevelStatements, Boot),
    emit_program(Name, Plan, Lowered, Boot, Text).

:- end_tests(subscribe_cone).
