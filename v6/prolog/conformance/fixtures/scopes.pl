% fixtures/scopes.pl : the scopes/switch-flow promotion. Twelve scenarios out
% of `git show ac2aafdc:v6/prolog/labs/switch_flow.pl` (89 checks) and its
% distillation plans/2026-07-27-switch-flow.md section 7, all written in the
% MINIMAL KERNEL the ruling landed on: zero stored forest rels (no sub,
% sub_path, demand, scope_queue, switch_scope, scope_done as engine concepts),
% zero new tick phases, zero new tick-item kinds. Every scope root is an
% ordinary program keyed Set rel; switchMap/mergeMap/exhaustMap/concatMap are
% the scope row's PRIMARY KEY shape ([1] = switch, [1,2] = merge, [1] + guard
% = exhaust, [1] + guard + a program queue rel = concat); completion is an
% edge write into a rel strictly upstream of the scope root plus a negation;
% the demand projection is an ordinary derived rel the program heads itself.
% All twelve promote cleanly; nothing was dropped and the reference engine
% needed zero changes. Owner: coordinator.
%
% Rulings honored: subscription_kernel = minimal_with_coverage_check_and_
% ghost_view (rulings.pl); salt_minting = content_addressed (so no instance
% column, no per-fill demand identity anywhere below); stale_fill_policy =
% not_applicable_under_content_salts (fills are unconditional cache updates,
% never gated); effect_abort gets no fixtures here (runtime world-cost
% machinery with no store observability).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% 1. switch-as-keyed-replace (switch_flow.pl section 7.1, "derived_switch":
% keyed_replace_alone_is_switch_map). A new outer row REPLACES the old scope
% row; IVM retracts everything the old row supported (route_view, demanded)
% in the same tick the new row's cone comes up. No teardown statement exists
% anywhere in this program.
fixture(switch_as_keyed_replace,
  prog([ kind(route_change/2, log), keep(route_change/2, all),
         kind(route_row/2, set),
         keyed(open_scope/2, [1]) ],
       [ (open_scope(SessionId, route_data(RouteId)) <+ route_change(SessionId, RouteId)),
         (demanded(Target, SessionId) <- open_scope(SessionId, Target)),
         (route_view(RouteId, Body) <- demanded(route_data(RouteId), _), route_row(RouteId, Body)) ]),
  [ route_row(settings, body_settings), route_row(profile, body_profile) ],
  [ [ +route_change(session_one, settings) ],
    [ +route_change(session_one, profile) ] ],
  [ deltas(open_scope/2,
      [ [ +open_scope(session_one, route_data(settings)) ],
        [ -open_scope(session_one, route_data(settings)), +open_scope(session_one, route_data(profile)) ],
        [] ]),
    deltas(route_view/2,
      [ [ +route_view(settings, body_settings) ],
        [ -route_view(settings, body_settings), +route_view(profile, body_profile) ],
        [] ]),
    final(open_scope/2, [ open_scope(session_one, route_data(profile)) ]),
    final(route_view/2, [ route_view(profile, body_profile) ]),
    ticks(3) ]).

% 2. merge policy (switch_flow.pl section 7.3: keyed(open_tab/2, [1, 2]) is
% "the three derived policies share every rule"'s merge row). Key = [outer,
% value]: two scopes under the same outer identity coexist as siblings, and
% closing one is an ordinary negated-completion teardown that never touches
% the other's derived cone.
fixture(merge_policy,
  prog([ kind(open_request/2, log), keep(open_request/2, all),
         kind(close_request/2, log), keep(close_request/2, all),
         kind(closed/2, log), keep(closed/2, all),
         kind(tab_row/2, set),
         keyed(open_tab/2, [1, 2]) ],
       [ (open_tab(SessionId, TabId) <+ open_request(SessionId, TabId)),
         (closed(SessionId, TabId) <+ close_request(SessionId, TabId)),
         (live_tab(SessionId, TabId) <- open_tab(SessionId, TabId), not(closed(SessionId, TabId))),
         (demanded(tab(TabId), SessionId) <- live_tab(SessionId, TabId)),
         (tab_view(TabId, Body) <- demanded(tab(TabId), _), tab_row(TabId, Body)) ]),
  [ tab_row(tab_a, body_a), tab_row(tab_b, body_b) ],
  [ [ +open_request(session_one, tab_a), +open_request(session_one, tab_b) ],
    [ +close_request(session_one, tab_a) ] ],
  [ deltas(open_tab/2,
      [ [ +open_tab(session_one, tab_a), +open_tab(session_one, tab_b) ], [], [] ]),
    deltas(live_tab/2,
      [ [ +live_tab(session_one, tab_a), +live_tab(session_one, tab_b) ],
        [ -live_tab(session_one, tab_a) ], [] ]),
    deltas(tab_view/2,
      [ [ +tab_view(tab_a, body_a), +tab_view(tab_b, body_b) ],
        [ -tab_view(tab_a, body_a) ], [] ]),
    final(open_tab/2, [ open_tab(session_one, tab_a), open_tab(session_one, tab_b) ]),
    final(live_tab/2, [ live_tab(session_one, tab_b) ]),
    final(tab_view/2, [ tab_view(tab_b, body_b) ]),
    ticks(3) ]).

% 3. exhaust policy (switch_flow.pl section 7.3: "exhaust is the switch key
% plus one guard"). Key = [outer] alone plus a not(live_tab(...)) guard on
% the planting rule: a value arriving while a scope lives is DISCARDED, not
% queued and not replacing. Tick 2 grades the discard directly (open_tab's
% own delta is empty even though open_request(tab_b) arrived); after the
% live scope closes, a later arrival is accepted normally.
fixture(exhaust_policy,
  prog([ kind(open_request/2, log), keep(open_request/2, all),
         kind(close_request/2, log), keep(close_request/2, all),
         kind(closed/2, log), keep(closed/2, all),
         kind(tab_row/2, set),
         keyed(open_tab/2, [1]) ],
       [ (open_tab(SessionId, TabId) <+ open_request(SessionId, TabId), not(live_tab(SessionId, _))),
         (closed(SessionId, TabId) <+ close_request(SessionId, TabId)),
         (live_tab(SessionId, TabId) <- open_tab(SessionId, TabId), not(closed(SessionId, TabId))),
         (demanded(tab(TabId), SessionId) <- live_tab(SessionId, TabId)),
         (tab_view(TabId, Body) <- demanded(tab(TabId), _), tab_row(TabId, Body)) ]),
  [ tab_row(tab_a, body_a), tab_row(tab_c, body_c) ],
  [ [ +open_request(session_one, tab_a) ],
    [ +open_request(session_one, tab_b) ],
    [ +close_request(session_one, tab_a) ],
    [ +open_request(session_one, tab_c) ] ],
  [ deltas(open_tab/2,
      [ [ +open_tab(session_one, tab_a) ], [], [],
        [ -open_tab(session_one, tab_a), +open_tab(session_one, tab_c) ], [] ]),
    deltas(live_tab/2,
      [ [ +live_tab(session_one, tab_a) ], [], [ -live_tab(session_one, tab_a) ],
        [ +live_tab(session_one, tab_c) ], [] ]),
    deltas(tab_view/2,
      [ [ +tab_view(tab_a, body_a) ], [], [ -tab_view(tab_a, body_a) ],
        [ +tab_view(tab_c, body_c) ], [] ]),
    final(open_tab/2, [ open_tab(session_one, tab_c) ]),
    final(live_tab/2, [ live_tab(session_one, tab_c) ]),
    final(tab_view/2, [ tab_view(tab_c, body_c) ]),
    ticks(5) ]).

% 4. concat via program queue rel (switch_flow.pl section 7.3: "the derived
% concat dequeue costs one tick not two", generalized here past queue depth
% one to a genuine FIFO). queue_slot is a dense-integer-ordinal keyed rel
% (storage_integer_keys), queue_head is the min-ordinal aggregate view over
% the undrained slots, and the dequeue is a finalize(live_tab(...)) trigger
% (r4) reading last tick's queue head: arrival order is preserved (tab_b
% drains before tab_c because it queued first), the swap lands exactly one
% tick after the close it waited on, and queue_head_tab (the visible "current
% queue position" view) retracts the instant its slot drains.
fixture(concat_program_queue,
  prog([ kind(open_request/2, log), keep(open_request/2, all),
         kind(close_request/2, log), keep(close_request/2, all),
         kind(closed/2, log), keep(closed/2, all),
         kind(drained/2, log), keep(drained/2, all),
         kind(tab_row/2, set),
         keyed(open_tab/2, [1]),
         keyed(queue_next/2, [1]),
         keyed(queue_slot/3, [1, 2]) ],
       [ (open_tab(SessionId, TabId) <+ open_request(SessionId, TabId), not(live_tab(SessionId, _))),
         (queue_next(SessionId, Next) <+ open_request(SessionId, TabId), latest(live_tab(SessionId, _)),
            pre(queue_next(SessionId, SoFar)), Next := SoFar + 1),
         (queue_slot(SessionId, Next, TabId) <+ open_request(SessionId, TabId), latest(live_tab(SessionId, _)),
            pre(queue_next(SessionId, SoFar)), Next := SoFar + 1),
         (closed(SessionId, TabId) <+ close_request(SessionId, TabId)),
         (live_tab(SessionId, TabId) <- open_tab(SessionId, TabId), not(closed(SessionId, TabId))),
         (queue_head(SessionId, min(Ordinal)) <- queue_slot(SessionId, Ordinal, _), not(drained(SessionId, Ordinal))),
         (queue_head_tab(SessionId, TabId) <- queue_head(SessionId, Ordinal), queue_slot(SessionId, Ordinal, TabId)),
         (drained(SessionId, Ordinal) <+ finalize(live_tab(SessionId, _)), pre(queue_head(SessionId, Ordinal))),
         (open_tab(SessionId, TabId) <+ finalize(live_tab(SessionId, _)), pre(queue_head_tab(SessionId, TabId))),
         (demanded(tab(TabId), SessionId) <- live_tab(SessionId, TabId)),
         (tab_view(TabId, Body) <- demanded(tab(TabId), _), tab_row(TabId, Body)) ]),
  [ queue_next(session_one, 0),
    tab_row(tab_a, body_a), tab_row(tab_b, body_b), tab_row(tab_c, body_c) ],
  [ [ +open_request(session_one, tab_a) ],
    [ +open_request(session_one, tab_b) ],
    [ +open_request(session_one, tab_c) ],
    [ +close_request(session_one, tab_a) ],
    [],
    [ +close_request(session_one, tab_b) ] ],
  [ deltas(open_tab/2,
      [ [ +open_tab(session_one, tab_a) ], [], [], [],
        [ -open_tab(session_one, tab_a), +open_tab(session_one, tab_b) ], [],
        [ -open_tab(session_one, tab_b), +open_tab(session_one, tab_c) ], [] ]),
    deltas(queue_slot/3,
      [ [], [ +queue_slot(session_one, 1, tab_b) ], [ +queue_slot(session_one, 2, tab_c) ],
        [], [], [], [], [] ]),
    deltas(queue_head_tab/2,
      [ [], [ +queue_head_tab(session_one, tab_b) ], [], [],
        [ -queue_head_tab(session_one, tab_b), +queue_head_tab(session_one, tab_c) ], [],
        [ -queue_head_tab(session_one, tab_c) ], [] ]),
    final(open_tab/2, [ open_tab(session_one, tab_c) ]),
    final(tab_view/2, [ tab_view(tab_c, body_c) ]),
    ticks(8) ]).

% 5. scope_done as ordinary rule, three spellings in one fixture (switch_flow.pl
% section 7.4: terminal_enum_arm_derives_scope_done / conjunctive_body_derives_
% scope_done / explicit_rule_head_derives_scope_done, "all three are plain
% level rules with no new syntax"). (a) a terminal enum arm joins the scope
% root to a matching stream value; (b) a conjunctive body (forkJoin) closes
% on the LAST of two results, fired via the unmarked any-atom rule so either
% arrival can trigger the check; (c) an explicit close signal. All three tear
% down in the same tick their signal completes, self_completion_needs_no_
% settling_phase: no settling phase exists in this engine at all.
fixture(scope_done_three_spellings,
  prog([ kind(stream_event/2, log), keep(stream_event/2, all),
         kind(result_x/1, log), keep(result_x/1, all),
         kind(result_y/1, log), keep(result_y/1, all),
         kind(close_signal/1, log), keep(close_signal/1, all),
         kind(closed_a/1, log), keep(closed_a/1, all),
         kind(closed_b/1, log), keep(closed_b/1, all),
         kind(closed_c/1, log), keep(closed_c/1, all),
         keyed(open_scope_a/2, [1]),
         keyed(open_scope_b/1, [1]),
         keyed(open_scope_c/1, [1]) ],
       [ (closed_a(ScopeId) <+ stream_event(Target, done), latest(open_scope_a(ScopeId, Target))),
         (live_scope_a(ScopeId, Target) <- open_scope_a(ScopeId, Target), not(closed_a(ScopeId))),
         (closed_b(ScopeId) <+ result_x(ScopeId), result_y(ScopeId), open_scope_b(ScopeId)),
         (live_scope_b(ScopeId) <- open_scope_b(ScopeId), not(closed_b(ScopeId))),
         (closed_c(ScopeId) <+ close_signal(ScopeId), latest(open_scope_c(ScopeId))),
         (live_scope_c(ScopeId) <- open_scope_c(ScopeId), not(closed_c(ScopeId))) ]),
  [ open_scope_a(pane_one, feed_target), open_scope_b(fork_one), open_scope_c(panel_one) ],
  [ [ +result_x(fork_one) ],
    [ +result_y(fork_one), +stream_event(feed_target, done), +close_signal(panel_one) ] ],
  [ deltas(live_scope_a/2, [ [], [ -live_scope_a(pane_one, feed_target) ], [] ]),
    deltas(live_scope_b/1, [ [], [ -live_scope_b(fork_one) ], [] ]),
    deltas(live_scope_c/1, [ [], [ -live_scope_c(panel_one) ], [] ]),
    final(live_scope_a/2, []),
    final(live_scope_b/1, []),
    final(live_scope_c/1, []),
    ticks(3) ]).

% 6. completion propagation: a two-stage pipeline whose downstream completes
% at the lattice-predicted tick (switch_flow.pl section 3, the mode-lattice
% runtime table: join_max composes end_a/end_b for the outer conjunctively,
% scope_min composes the outer's death against the inner's own signal
% disjunctively). No lattice-formula code is invoked; the composition falls
% out structurally, outer death = max(end_a tick, end_b tick) via the
% conjunctive-body spelling from fixture 5, inner death = min(outer death,
% end_c tick) via an ordinary POSITIVE JOIN to live_outer (whichever arrives
% first kills it). Two signal schedules picked from the lab's 21: outer_x's
% inner dies from its OWN signal (end_c before the parent), outer_y's inner
% dies from the PARENT's death (end_c arrives too late to matter).
fixture(completion_propagation_lattice_tick,
  prog([ kind(end_a_signal/1, log), keep(end_a_signal/1, all),
         kind(end_b_signal/1, log), keep(end_b_signal/1, all),
         kind(end_c_signal/1, log), keep(end_c_signal/1, all),
         kind(closed_outer/1, log), keep(closed_outer/1, all),
         kind(closed_inner/2, log), keep(closed_inner/2, all),
         keyed(open_outer/1, [1]),
         keyed(open_inner/2, [1, 2]) ],
       [ (closed_outer(OuterId) <+ end_a_signal(OuterId), end_b_signal(OuterId), open_outer(OuterId)),
         (live_outer(OuterId) <- open_outer(OuterId), not(closed_outer(OuterId))),
         (closed_inner(OuterId, InnerId) <+ end_c_signal(InnerId), latest(open_inner(OuterId, InnerId))),
         (live_inner(OuterId, InnerId) <- open_inner(OuterId, InnerId), live_outer(OuterId),
            not(closed_inner(OuterId, InnerId))) ]),
  [ open_outer(outer_x), open_inner(outer_x, inner_x),
    open_outer(outer_y), open_inner(outer_y, inner_y) ],
  [ [],
    [ +end_a_signal(outer_x), +end_a_signal(outer_y) ],
    [ +end_c_signal(inner_x) ],
    [ +end_b_signal(outer_x), +end_b_signal(outer_y) ],
    [ +end_c_signal(inner_y) ] ],
  [ deltas(live_outer/1, [ [], [], [], [ -live_outer(outer_x), -live_outer(outer_y) ], [], [] ]),
    deltas(live_inner/2,
      [ [], [], [ -live_inner(outer_x, inner_x) ], [ -live_inner(outer_y, inner_y) ], [], [] ]),
    final(live_outer/1, []),
    final(live_inner/2, []),
    ticks(6) ]).

% 7. takeUntil: keyed replace + negated scope_done (switch_flow.pl section 6,
% "leaving_the_state_tears_the_scope_down_in_the_same_tick"). open_fetch is a
% keyed scope root over the state-machine's own phase register
% (fixtures/state_machine.pl's fetch lifecycle, reused). scope_done negates
% the SAME register a positive read elsewhere in the program already reads,
% so leaving the fetching phase kills the scope in the same tick the register
% moves, and a fresh poll_due later replants under the same key.
fixture(take_until_keyed_replace_negated_done,
  prog([ kind(poll_due/1, log), keep(poll_due/1, all),
         kind(fetch_result/2, log), keep(fetch_result/2, all),
         keyed(phase/2, [1]),
         keyed(open_fetch/1, [1]) ],
       [ (phase(Endpoint, fetching) <+ poll_due(Endpoint), pre(phase(Endpoint, idle))),
         (phase(Endpoint, idle) <+ fetch_result(Endpoint, _), pre(phase(Endpoint, fetching))),
         (open_fetch(Endpoint) <+ poll_due(Endpoint), pre(phase(Endpoint, idle))),
         (scope_done(Endpoint) <- open_fetch(Endpoint), not(phase(Endpoint, fetching))),
         (live_fetch(Endpoint) <- open_fetch(Endpoint), not(scope_done(Endpoint))),
         (demanded(fetch_of(Endpoint), Endpoint) <- live_fetch(Endpoint)) ]),
  [ phase(gh_repos, idle) ],
  [ [ +poll_due(gh_repos) ],
    [ +fetch_result(gh_repos, fresh(tag_v1, body_one)) ],
    [ +poll_due(gh_repos) ] ],
  [ deltas(live_fetch/1,
      [ [ +live_fetch(gh_repos) ], [ -live_fetch(gh_repos) ], [ +live_fetch(gh_repos) ], [] ]),
    deltas(phase/2,
      [ [ -phase(gh_repos, idle), +phase(gh_repos, fetching) ],
        [ -phase(gh_repos, fetching), +phase(gh_repos, idle) ],
        [ -phase(gh_repos, idle), +phase(gh_repos, fetching) ], [] ]),
    final(live_fetch/1, [ live_fetch(gh_repos) ]),
    final(phase/2, [ phase(gh_repos, fetching) ]),
    ticks(4) ]).

% 8. state-flap netting (switch_flow.pl section 1, Change A's headline:
% same_tick_state_flap_nets_to_zero_scope_churn). An error arm and a
% recovering poll_due arrive in ONE tick, in that order; the phase register
% chains fetching -> idle -> fetching within the tick (r6, pre reads the
% evolving pre-state), but the switch reads only the boundary, and the R2
% rider already says intermediate fold states are not observable. Net result:
% zero delta on phase, open_fetch, live_fetch AND demanded, not just two
% teardowns collapsing to zero, nothing moves at all except the raw arrivals.
fixture(state_flap_nets_to_zero_scope_churn,
  prog([ kind(poll_due/1, log), keep(poll_due/1, all),
         kind(fetch_result/2, log), keep(fetch_result/2, all),
         keyed(phase/2, [1]),
         keyed(open_fetch/1, [1]) ],
       [ (phase(Endpoint, fetching) <+ poll_due(Endpoint), pre(phase(Endpoint, idle))),
         (phase(Endpoint, idle) <+ fetch_result(Endpoint, _), pre(phase(Endpoint, fetching))),
         (open_fetch(Endpoint) <+ poll_due(Endpoint), pre(phase(Endpoint, idle))),
         (scope_done(Endpoint) <- open_fetch(Endpoint), not(phase(Endpoint, fetching))),
         (live_fetch(Endpoint) <- open_fetch(Endpoint), not(scope_done(Endpoint))),
         (demanded(fetch_of(Endpoint), Endpoint) <- live_fetch(Endpoint)) ]),
  [ phase(gh_repos, fetching), open_fetch(gh_repos) ],
  [ [ +fetch_result(gh_repos, error(500)), +poll_due(gh_repos) ] ],
  [ deltas(phase/2, [ [] ]),
    deltas(live_fetch/1, [ [] ]),
    deltas(demanded/2, [ [] ]),
    final(phase/2, [ phase(gh_repos, fetching) ]),
    final(live_fetch/1, [ live_fetch(gh_repos) ]),
    ticks(1) ]).

% 9. fill-as-cache-update (switch_flow.pl section 8, the "orphan-as-a-row"
% reading the salt_minting/stale_fill_policy rulings landed on: a fill is a
% content-addressed cache update, never gated on demand, never stale). A
% response arrives after its original demander's scope already died; it
% lands in cache_row unconditionally (nobody subscribed, no view moves), and
% a NEW demander of the same identity reads it immediately with no refetch.
% The classic SWR shape, with no fill tick-item and no orphan rel: cache_row
% is an ordinary program rel written by an ordinary edge rule.
fixture(fill_as_cache_update_swr,
  prog([ kind(fill_arrived/2, log), keep(fill_arrived/2, all),
         keyed(open_feed/2, [1]),
         keyed(cache_row/2, [1]) ],
       [ (demanded(Target, SessionId) <- open_feed(SessionId, Target)),
         (cache_row(Target, Body) <+ fill_arrived(Target, Body)),
         (feed_view(Target, Body) <- demanded(Target, _), cache_row(Target, Body)) ]),
  [],
  [ [ +open_feed(session_one, alpha) ],
    [ -open_feed(session_one, alpha) ],
    [ +fill_arrived(alpha, orphan_body) ],
    [ +open_feed(session_two, alpha) ] ],
  [ deltas(cache_row/2, [ [], [], [ +cache_row(alpha, orphan_body) ], [] ]),
    deltas(feed_view/2, [ [], [], [], [ +feed_view(alpha, orphan_body) ] ]),
    deltas(demanded/2,
      [ [ +demanded(alpha, session_one) ], [ -demanded(alpha, session_one) ], [],
        [ +demanded(alpha, session_two) ] ]),
    final(cache_row/2, [ cache_row(alpha, orphan_body) ]),
    final(feed_view/2, [ feed_view(alpha, orphan_body) ]),
    ticks(4) ]).

% 10. demand laziness (switch_flow.pl section 7.6, "the demand projection +
% bind lifecycle" survivor: demanded/2 is a program rule, and effect_call
% exists only while some scope's demand row is live). Two independent
% identities open and close on their own schedules; each effect_call row is
% born and dies in lockstep with its own demand, never before, never after.
fixture(demand_laziness_effect_rows,
  prog([ keyed(open_feed/2, [1]) ],
       [ (demanded(Target, SessionId) <- open_feed(SessionId, Target)),
         (effect_call(Target) <- demanded(Target, _)) ]),
  [],
  [ [ +open_feed(session_one, alpha) ],
    [ -open_feed(session_one, alpha) ],
    [ +open_feed(session_two, alpha) ],
    [ +open_feed(session_three, beta) ],
    [ -open_feed(session_two, alpha), -open_feed(session_three, beta) ] ],
  [ deltas(effect_call/1,
      [ [ +effect_call(alpha) ], [ -effect_call(alpha) ], [ +effect_call(alpha) ],
        [ +effect_call(beta) ], [ -effect_call(alpha), -effect_call(beta) ] ]),
    final(effect_call/1, []),
    ticks(5) ]).

% 11. shared demand refcount (switch_flow.pl section 7.6, the sub_lifetimes
% finding this whole lab traces back to: "demand is refcounted by IVM
% support, not by the sub row"). Two scopes demand the same identity; one
% dies and effect_call shows ZERO delta (the other's support alone keeps the
% projection alive); only when the second dies does the projection follow.
fixture(shared_demand_refcount,
  prog([ keyed(open_feed/2, [1]) ],
       [ (demanded(Target, SessionId) <- open_feed(SessionId, Target)),
         (effect_call(Target) <- demanded(Target, _)) ]),
  [],
  [ [ +open_feed(session_one, alpha), +open_feed(session_two, alpha) ],
    [ -open_feed(session_one, alpha) ],
    [ -open_feed(session_two, alpha) ] ],
  [ deltas(effect_call/1, [ [ +effect_call(alpha) ], [], [ -effect_call(alpha) ] ]),
    final(effect_call/1, []),
    ticks(3) ]).

% 12. the zombie-scope NEGATIVE case (plans/2026-07-27-redteam-minimal-kernel.md
% section A2b, "the structural guarantee that was traded away: BROKEN").
% Section 7.2's correct nesting rule is `live_detail(PaneId, detail(ItemId)) <-
% open_pane(PaneId, _), open_detail(PaneId, ItemId).`; the join to
% open_pane(PaneId, _) is what makes the inner scope die when its parent
% closes. THIS fixture drops that one body atom, reproducing A2b exactly: the
% outer pane closes at tick 3 and NOTHING else moves (live_detail, demanded
% and even the closed pane's own demand keep standing), then a late response
% at tick 4 is admitted into detail_view under a scope that has been dead for
% a full tick. This is TODAY's (unchecked) engine behavior, asserted
% honestly, not a workaround: the redteam finding is that the minimal kernel
% moved the scope tree into rule text where no check reads it. Obligation 1
% of the subscription_kernel ruling (rulings.pl) is scope_cover_check, a
% static scope-key column-flow refusal for exactly this shape (ARCH.pl:
% task(scope_cover_check, unbuilt, [mode_lab])). It does not exist yet. WHEN
% IT LANDS, this fixture's expectations flip from the leak below to
% throws(scope_cover_violation(live_detail/2)) or whatever term the checker
% settles on; until then this fixture is the honest record of the gap, and it
% must never be silently deleted.
fixture(zombie_scope_negative_case_a2b,
  prog([ keyed(open_pane/2, [1]),
         keyed(open_detail/2, [1, 2]) ],
       [ % REJECTED READING dropped on purpose (A2b): the correct rule reads
         % `live_detail(PaneId, detail(ItemId)) <- open_pane(PaneId, _),
         %  open_detail(PaneId, ItemId).` -- this is the one-atom-missing
         % broken variant the redteam ran, kept broken deliberately.
         (live_detail(PaneId, detail(ItemId)) <- open_detail(PaneId, ItemId)),
         (demanded(Target, PaneId) <- live_detail(PaneId, Target)),
         (detail_view(ItemId, Body) <- demanded(detail(ItemId), _), detail_row(ItemId, Body)) ]),
  [],
  [ [ +open_pane(pane_one, item_list), +open_detail(pane_one, item_a) ],
    [ +detail_row(item_a, body_a) ],
    [ -open_pane(pane_one, item_list) ],
    [ +detail_row(item_a, late_body) ] ],
  [ deltas(detail_view/2,
      [ [], [ +detail_view(item_a, body_a) ], [], [ +detail_view(item_a, late_body) ] ]),
    deltas(live_detail/2,
      [ [ +live_detail(pane_one, detail(item_a)) ], [], [], [] ]),
    final(detail_view/2, [ detail_view(item_a, body_a), detail_view(item_a, late_body) ]),
    final(live_detail/2, [ live_detail(pane_one, detail(item_a)) ]),
    ticks(4) ]).
