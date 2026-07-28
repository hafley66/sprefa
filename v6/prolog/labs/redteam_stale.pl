% redteam_stale.pl -- RED TEAM probes against the stale-fill trichotomy
% (plans/2026-07-27-switch-flow.md section 8, ambiguity 4).
%
% Run:  swipl -q -l v6/prolog/labs/redteam_stale.pl -g go -g halt
%       swipl -q -l v6/prolog/labs/redteam_stale.pl -g report -g halt
%
% The claim under attack, in three parts:
%   (a) orphan-as-a-row needs ONE FEWER primitive than drop
%   (b) abort and drop are observationally identical in the store
%   (c) drop is the only reading that ADDS a primitive
%
% What the switch_flow lab measured: the STORE. What this file adds as a second
% observer: the WORLD LEDGER (calls actually spent upstream) and CRASH
% DETERMINISM (whether the same schedule produces the same store when a kill -9
% lands in the middle). Both are observable to a user holding an API bill or a
% db file; neither appears in switch_flow's `Final` term.
%
% The model is deliberately a scale copy of the SHIPPING runtime, not of the
% Prolog forest: v6/dl/src/1_hosts.ts's HostRunner.
%   - effect_cache(full_digest, state) is `Cache` here, keyed (Target, Salt).
%   - fire-once = the cache-row membership test (1_hosts.ts:450-454).
%   - boot replay = DELETE pending, then re-present every live demand row
%     (1_hosts.ts:414-428 replayableRequests).
%   - deltas are NOT durable, demand rows ARE. `crash` empties in-flight only.
%   - HostRunner subscribes INSERTS only (1_hosts.ts:386-388), so nothing in
%     the shipping runtime reacts to a demand-row DELETE; the `abort` policy
%     below is the machinery that would have to be added.
%
% Style: maplist/foldl/include/exclude only, never findall (per the copy_term
% severing hazard the switch_flow lab documents at engine.pl:151).

:- module(redteam_stale, [go/0, report/0]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('../conformance/rulings').

:- discontiguous check/2.
:- discontiguous scenario/3.

% ═══════════════════════════════════════════════════════════════════════════
% MODEL
% ═══════════════════════════════════════════════════════════════════════════
%
% st(Demand, InFlight, Cache, Store, Ledger)
%   Demand   : d(Scope, Target, Salt)          durable rows
%   InFlight : f(Target, Salt)                 process memory only
%   Cache    : c(Target, Salt, pending|done)   durable, = effect_cache
%   Store    : row(Target, Body)               durable, set semantics
%   Ledger   : call(Target,Salt) | abort(Target,Salt), NEWEST FIRST.
%              The ledger is the WORLD: one `call` = one request that left the
%              machine and was billed / rate-limited / rate-counted.
%
% cfg(Policy, SaltMode)
%   Policy   : abort | drop | orphan
%   SaltMode : content | instance

policy(abort).
policy(drop).
policy(orphan).

salt_mode(content).
salt_mode(instance).

% Content-addressed: the demand key IS the content, so two subscribers to the
% same target share one request. Per-instance: every subscription mints its own
% identity, so two subscribers to the same target never share.
mint_salt(content,  _Scope, Target, Target).
mint_salt(instance,  Scope, _Target, Scope).

empty_state(st([], [], [], [], [])).

run_items(Cfg, Items, State) :-
    empty_state(Zero),
    foldl(apply_item(Cfg), Items, Zero, State).

% peak in-flight concurrency across the whole run: the rate-limit observable.
run_peak(Cfg, Items, State, Peak) :-
    empty_state(Zero),
    foldl(peak_step(Cfg), Items, Zero-0, State-Peak).

peak_step(Cfg, Item, State0-Peak0, State-Peak) :-
    apply_item(Cfg, Item, State0, State),
    inflight_count(State, Live),
    Peak is max(Peak0, Live).

% ── open: plant a demand row; fire the effect unless the cache already holds
%    this exact (Target, Salt). Mirrors runEffectOnce's SELECT full_digest.
apply_item(cfg(_Policy, SaltMode), open(Scope, Target),
           st(Demand0, Flight0, Cache0, Store, Ledger0),
           st(Demand,  Flight,  Cache,  Store, Ledger)) :-
    mint_salt(SaltMode, Scope, Target, Salt),
    Demand = [d(Scope, Target, Salt)|Demand0],
    (   memberchk(c(Target, Salt, _), Cache0)
    ->  Flight = Flight0, Cache = Cache0, Ledger = Ledger0
    ;   Flight = [f(Target, Salt)|Flight0],
        Cache  = [c(Target, Salt, pending)|Cache0],
        Ledger = [call(Target, Salt)|Ledger0] ).

% ── close: delete this scope's demand rows. Under `abort` ONLY, a close-time
%    handler walks in-flight looking for runs whose demand support hit zero and
%    kills them (the Go-context shape rulings.pl records). Under drop/orphan the
%    close step never reads InFlight at all -- which is the primitive count.
apply_item(cfg(Policy, _), close(Scope),
           st(Demand0, Flight0, Cache0, Store, Ledger0),
           st(Demand,  Flight,  Cache,  Store, Ledger)) :-
    exclude(demand_of_scope(Scope), Demand0, Demand),
    (   Policy == abort
    ->  include(unsupported_flight(Demand), Flight0, Doomed),
        subtract(Flight0, Doomed, Flight),
        foldl(abort_one, Doomed, Cache0-Ledger0, Cache-Ledger)
    ;   Flight = Flight0, Cache = Cache0, Ledger = Ledger0 ).

% ── arrive: the response comes back. The world spend already happened at the
%    call; arrival is only about whether the body lands.
apply_item(cfg(Policy, _), arrive(Target, Salt, Body),
           st(Demand, Flight0, Cache0, Store0, Ledger),
           st(Demand, Flight,  Cache,  Store,  Ledger)) :-
    (   selectchk(f(Target, Salt), Flight0, Flight)
    ->  mark_done(Target, Salt, Cache0, Cache),
        land_or_not(Policy, Demand, Target, Salt, Body, Store0, Store)
    ;   Flight = Flight0, Cache = Cache0, Store = Store0 ).

% ── arrive_unacked: the response rows COMMIT but the effect_cache UPDATE to
%    'done' has not run yet. 1_hosts.ts:497 (runtime.commit) and 1_hosts.ts:501
%    (UPDATE effect_cache SET state='done') are two separate awaits against two
%    separate handles; there is no transaction spanning them. This item is that
%    window.
apply_item(cfg(Policy, _), arrive_unacked(Target, Salt, Body),
           st(Demand, Flight0, Cache, Store0, Ledger),
           st(Demand, Flight,  Cache, Store,  Ledger)) :-
    (   selectchk(f(Target, Salt), Flight0, Flight)
    ->  land_or_not(Policy, Demand, Target, Salt, Body, Store0, Store)
    ;   Flight = Flight0, Store = Store0 ).

% ── crash: kill -9. In-flight runs live in process memory and die. Demand rows,
%    cache rows and store rows are on disk and survive. The LEDGER survives too,
%    because money already left.
apply_item(_, crash,
           st(Demand, _Flight, Cache, Store, Ledger),
           st(Demand, [],      Cache, Store, Ledger)).

% ── boot: replayableRequests. Delete every 'pending' cache row (a surviving
%    pending can only belong to a dead process), then re-present every live
%    demand row through the same fire-once gate.
apply_item(_, boot,
           st(Demand, _Flight0, Cache0, Store, Ledger0),
           st(Demand, Flight,   Cache,  Store, Ledger)) :-
    exclude(pending_cache, Cache0, Cache1),
    foldl(replay_demand, Demand, replay(Cache1, [], Ledger0), replay(Cache, Flight, Ledger)).

demand_of_scope(Scope, d(Scope, _, _)).

unsupported_flight(Demand, f(_, Salt)) :- \+ memberchk(d(_, _, Salt), Demand).

abort_one(f(Target, Salt), Cache0-Ledger0, Cache-Ledger) :-
    exclude(==(c(Target, Salt, pending)), Cache0, Cache),
    Ledger = [abort(Target, Salt)|Ledger0].

pending_cache(c(_, _, pending)).

replay_demand(d(_, Target, Salt), replay(Cache0, Flight0, Ledger0),
                                  replay(Cache,  Flight,  Ledger)) :-
    (   memberchk(c(Target, Salt, _), Cache0)
    ->  Cache = Cache0, Flight = Flight0, Ledger = Ledger0
    ;   Cache  = [c(Target, Salt, pending)|Cache0],
        Flight = [f(Target, Salt)|Flight0],
        Ledger = [call(Target, Salt)|Ledger0] ).

% The whole trichotomy, in five lines. `abort` never reaches here (its flight
% was already removed at close time), so only drop and orphan differ.
land_or_not(Policy, Demand, Target, Salt, Body, Store0, Store) :-
    (   memberchk(d(_, _, Salt), Demand)
    ->  add_row(Target, Body, Store0, Store)          % ordinary answered fill
    ;   Policy == orphan
    ->  add_row(Target, Body, Store0, Store)          % orphan-as-a-row
    ;   Store = Store0 ).                             % drop (and abort)

add_row(Target, Body, Store0, Store) :- sort([row(Target, Body)|Store0], Store).

mark_done(Target, Salt, Cache0, Cache) :-
    (   selectchk(c(Target, Salt, pending), Cache0, Rest)
    ->  Cache = [c(Target, Salt, done)|Rest]
    ;   Cache = Cache0 ).

% ── accessors ───────────────────────────────────────────────────────────────
store_of(st(_, _, _, Store, _), Sorted)    :- sort(Store, Sorted).
ledger_of(st(_, _, _, _, Ledger), Chrono)  :- reverse(Ledger, Chrono).
inflight_count(st(_, Flight, _, _, _), N)  :- length(Flight, N).

world_calls(State, N)  :- ledger_of(State, L), include(is_call, L, Cs),  length(Cs, N).
world_aborts(State, N) :- ledger_of(State, L), include(is_abort, L, As), length(As, N).
store_size(State, N)   :- store_of(State, Rows), length(Rows, N).

is_call(call(_, _)).
is_abort(abort(_, _)).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS
% ═══════════════════════════════════════════════════════════════════════════

% The bare stale fill: one scope opens, dies, its response arrives afterwards.
scenario(SaltMode, bare_orphan, Items) :-
    mint_salt(SaltMode, session_one, feed, Salt),
    Items = [ open(session_one, feed), close(session_one),
              arrive(feed, Salt, body_one) ].

% Same, with a kill -9 between the scope's death and the response.
scenario(SaltMode, bare_orphan_crashed, Items) :-
    mint_salt(SaltMode, session_one, feed, Salt),
    Items = [ open(session_one, feed), close(session_one),
              crash, boot, arrive(feed, Salt, body_one) ].

% goal-endurance.sh phase 1: the scope is STILL ALIVE when the process dies.
scenario(SaltMode, endurance_phase_one, Items) :-
    mint_salt(SaltMode, session_one, feed, Salt),
    Items = [ open(session_one, feed), crash, boot,
              arrive(feed, Salt, body_one) ].

% goal-endurance.sh phase 2: a third boot must not re-fire.
scenario(SaltMode, endurance_phase_two, Items) :-
    scenario(SaltMode, endurance_phase_one, Head),
    append(Head, [crash, boot], Items).

% The commit/ack window: rows committed, effect_cache still 'pending', kill -9.
scenario(SaltMode, unacked_commit_crash, Items) :-
    mint_salt(SaltMode, session_one, feed, Salt),
    Items = [ open(session_one, feed),
              arrive_unacked(feed, Salt, body_one),
              crash, boot ].

% Two subscribers to ONE target; the first leaves while the second still wants it.
scenario(SaltMode, shared_target_one_leaves, Items) :-
    mint_salt(SaltMode, session_two, feed, SaltTwo),
    Items = [ open(session_one, feed), open(session_two, feed),
              close(session_one), arrive(feed, SaltTwo, body_one) ].

% The identical reopen: close and immediately reopen the SAME demand, then the
% FIRST subscription's response arrives. This is switch_flow's
% content_addressed_demand_cannot_detect_a_stale_fill, run under both salt modes
% and all three policies.
scenario(SaltMode, identical_reopen, Items) :-
    mint_salt(SaltMode, session_one, feed, SaltOne),
    Items = [ open(session_one, feed), close(session_one),
              open(session_two, feed),
              arrive(feed, SaltOne, first_body) ].

% Typeahead: eight keystrokes, each replacing the previous scope, one response.
scenario(SaltMode, typeahead_eight, Items) :-
    numlist(1, 8, Keys),
    maplist(keystroke_pair(SaltMode), Keys, Pairs),
    append(Pairs, Opens),
    mint_salt(SaltMode, 8, query(8), LastSalt),
    append(Opens, [arrive(query(8), LastSalt, hits(8))], Items).

% Each keystroke closes the previous scope, then opens a new one on new content.
keystroke_pair(_SaltMode, 1, [open(1, query(1))]) :- !.
keystroke_pair(_SaltMode, Key, [close(Prev), open(Key, query(Key))]) :- Prev is Key - 1.

% A firehose of N distinct short-lived scopes, every response landing orphaned.
firehose(SaltMode, N, Items) :-
    numlist(1, N, Keys),
    maplist(firehose_triple(SaltMode), Keys, Triples),
    append(Triples, Items).

firehose_triple(SaltMode, Key,
                [ open(Key, feed(Key)), close(Key), arrive(feed(Key), Salt, body(Key)) ]) :-
    mint_salt(SaltMode, Key, feed(Key), Salt).

% ═══════════════════════════════════════════════════════════════════════════
% B1 -- THE STORE IS NOT THE ONLY OBSERVER
% ═══════════════════════════════════════════════════════════════════════════

% The lab's claim, reproduced honestly: in the STORE the two are the same row set.
check(b1_abort_and_drop_agree_in_the_store,
  ( forall(salt_mode(SaltMode),
      ( scenario(SaltMode, bare_orphan, Items),
        run_items(cfg(abort, SaltMode), Items, AbortState),
        run_items(cfg(drop,  SaltMode), Items, DropState),
        store_of(AbortState, Rows), store_of(DropState, Rows),
        Rows == [] )) )).

% ...and the world ledger separates them immediately. abort emits an abort event
% and frees the in-flight slot; drop does not. `abort` is therefore a distinct
% observable to anything that meters concurrency: rate limiters, connection
% pools, spawn budgets.
check(b1_the_world_ledger_separates_abort_from_drop,
  ( scenario(content, bare_orphan, Items),
    run_peak(cfg(abort, content), Items, AbortState, AbortPeak),
    run_peak(cfg(drop,  content), Items, DropState,  DropPeak),
    world_aborts(AbortState, 1), world_aborts(DropState, 0),
    AbortPeak == 1, DropPeak == 1,
    ledger_of(AbortState, AbortLedger), ledger_of(DropState, DropLedger),
    AbortLedger \== DropLedger )).

% The concurrency observable, at scale: eight keystrokes with one flight each.
% abort holds ONE connection open; drop holds EIGHT. Identical stores.
check(b1_typeahead_peak_concurrency_is_eight_to_one,
  ( scenario(content, typeahead_eight, Items),
    run_peak(cfg(abort,  content), Items, AbortState,  AbortPeak),
    run_peak(cfg(drop,   content), Items, DropState,   DropPeak),
    run_peak(cfg(orphan, content), Items, OrphanState, OrphanPeak),
    AbortPeak == 1, DropPeak == 8, OrphanPeak == 8,
    world_calls(AbortState, 8), world_calls(DropState, 8), world_calls(OrphanState, 8),
    store_of(AbortState, [row(query(8), hits(8))]),
    store_of(DropState,  [row(query(8), hits(8))]) )).

% The store cannot see the difference even at scale, which is exactly why the
% v5 gh-cache rate incident was invisible: a 304 lands no rows.
check(b1_the_store_is_blind_to_the_eight_to_one_gap,
  ( scenario(content, typeahead_eight, Items),
    run_items(cfg(abort, content), Items, AbortState),
    run_items(cfg(drop,  content), Items, DropState),
    store_of(AbortState, Rows), store_of(DropState, Rows) )).

% Abort is a THIRD state component read at close time. drop and orphan never
% touch InFlight on a close; abort must. That is the primitive the trichotomy
% scored as ZERO.
check(b1_only_abort_reads_inflight_at_close_time,
  ( scenario(content, bare_orphan, Full),
    append(Prefix, [arrive(_, _, _)], Full),
    run_items(cfg(abort,  content), Prefix, AbortState),
    run_items(cfg(drop,   content), Prefix, DropState),
    run_items(cfg(orphan, content), Prefix, OrphanState),
    inflight_count(AbortState, 0),
    inflight_count(DropState, 1),
    inflight_count(OrphanState, 1) )).

% ═══════════════════════════════════════════════════════════════════════════
% B2 -- RETENTION OF ORPHANS
% ═══════════════════════════════════════════════════════════════════════════

% Orphan rows accumulate one per dead scope, with no reader and no bound. abort
% and drop are flat at zero over the same schedule.
check(b2_orphan_rows_grow_one_per_dead_scope,
  ( firehose(content, 4, SmallItems),
    firehose(content, 40, LargeItems),
    run_items(cfg(orphan, content), SmallItems, SmallState),
    run_items(cfg(orphan, content), LargeItems, LargeState),
    store_size(SmallState, 4), store_size(LargeState, 40),
    run_items(cfg(drop,  content), LargeItems, DropState),
    run_items(cfg(abort, content), LargeItems, AbortState),
    store_size(DropState, 0), store_size(AbortState, 0) )).

% Growth is linear in scope deaths and independent of readers: nobody ever
% opened a scope that reads any of these rows.
check(b2_orphan_growth_is_linear_and_reader_independent,
  ( maplist(orphan_store_size, [1, 2, 8, 32], Sizes),
    Sizes == [1, 2, 8, 32] )).

orphan_store_size(N, Size) :-
    firehose(content, N, Items),
    run_items(cfg(orphan, content), Items, State),
    store_size(State, Size).

% Ruling q10 (rulings.pl, USER-FINAL) puts `keep` on Log rels only. switch_flow's
% own orphan destination is declared `kind(cache_row/2, set)` (switch_flow.pl
% content_feed_program, recovered at ac2aafdc). A Set rel has no keep clause, so
% under the ruling as written there is NO retention expression that reaches an
% orphan row. Bounding orphans therefore needs either a keep clause on Set rels
% (a ruling change) or a Log-rel destination (a semantics change: stamps,
% multiset deltas, occurrence identity). Both are primitives.
check(b2_q10_keep_cannot_reach_the_orphan_destination,
  ( ruling(q10_retention, keep_clause_required_on_log, user, _),
    lab_orphan_destination_kind(set),
    \+ keep_reaches_kind(set) )).

lab_orphan_destination_kind(set).   % switch_flow.pl content_feed_program
keep_reaches_kind(log).             % ruling q10: "ranges over Log rels only"

% ═══════════════════════════════════════════════════════════════════════════
% B3 -- EXACTLY-ONCE INTERACTION (kill -9, boot replay, effect_cache)
% ═══════════════════════════════════════════════════════════════════════════

% goal-endurance.sh phase 1 passes under ALL THREE readings: the scope is alive,
% so the demand row is on disk and boot replay re-fires it. Nothing here
% discriminates, which is why the endurance script does not settle this ruling.
check(b3_all_three_readings_pass_endurance_phase_one,
  ( forall(policy(Policy),
      ( scenario(content, endurance_phase_one, Items),
        run_items(cfg(Policy, content), Items, State),
        store_of(State, [row(feed, body_one)]),
        world_calls(State, 2) )) )).

% Phase 2 too: the 'done' cache row survives the second boot, so no re-fire.
check(b3_all_three_readings_pass_endurance_phase_two,
  ( forall(policy(Policy),
      ( scenario(content, endurance_phase_two, Items),
        run_items(cfg(Policy, content), Items, State),
        store_of(State, [row(feed, body_one)]),
        world_calls(State, 2),
        store_size(State, 1) )) )).

% THE KILL. Extend the endurance scenario with a scope death mid-nap. Under
% orphan-as-a-row the SAME schedule produces two different stores depending on
% whether a crash landed inside the flight, because the durable trigger for a
% response is the demand row and the orphan reading has just deleted it. abort
% and drop are crash-deterministic.
check(b3_orphan_as_a_row_is_not_crash_deterministic,
  ( scenario(content, bare_orphan, Quiet),
    scenario(content, bare_orphan_crashed, Crashed),
    run_items(cfg(orphan, content), Quiet,   QuietState),
    run_items(cfg(orphan, content), Crashed, CrashedState),
    store_of(QuietState,   [row(feed, body_one)]),
    store_of(CrashedState, []),
    QuietState \== CrashedState )).

check(b3_abort_and_drop_are_crash_deterministic,
  ( forall(member(Policy, [abort, drop]),
      ( scenario(content, bare_orphan, Quiet),
        scenario(content, bare_orphan_crashed, Crashed),
        run_items(cfg(Policy, content), Quiet,   QuietState),
        run_items(cfg(Policy, content), Crashed, CrashedState),
        store_of(QuietState, Rows), store_of(CrashedState, Rows),
        Rows == [] )) )).

% The only way to make an orphan crash-durable is to keep a demand row alive
% past the scope's death -- which is not a fill policy at all, it is
% rulings.pl's own reformulation: DEMAND FROM A LONGER-LIVED SCOPE. Model it as
% a second, never-closed scope and watch the crashed store match the quiet one
% under every policy, orphan included.
check(b3_a_longer_lived_demand_restores_crash_determinism_for_every_policy,
  ( forall(policy(Policy),
      ( Quiet   = [ open(cache_rule, feed), open(session_one, feed),
                    close(session_one), arrive(feed, feed, body_one) ],
        Crashed = [ open(cache_rule, feed), open(session_one, feed),
                    close(session_one), crash, boot, arrive(feed, feed, body_one) ],
        run_items(cfg(Policy, content), Quiet,   QuietState),
        run_items(cfg(Policy, content), Crashed, CrashedState),
        store_of(QuietState,   [row(feed, body_one)]),
        store_of(CrashedState, [row(feed, body_one)]) )) )).

% Exactly-once is exactly-once IN THE STORE, not in the world. The window
% between runtime.commit (1_hosts.ts:497) and the effect_cache UPDATE
% (1_hosts.ts:501) is two separate awaits with no transaction spanning them; a
% kill -9 inside it leaves a 'pending' row that boot replay DELETES, so the
% demand re-fires against a cache miss. Store: one row. World: two calls.
% This is independent of which stale-fill reading is chosen.
check(b3_store_exactly_once_is_not_world_exactly_once,
  ( forall(policy(Policy),
      ( scenario(content, unacked_commit_crash, Items),
        run_items(cfg(Policy, content), Items, State),
        store_size(State, 1),
        world_calls(State, 2) )) )).

% ═══════════════════════════════════════════════════════════════════════════
% B4 -- THE DETECTABILITY PROOF CUTS THE OTHER WAY
% ═══════════════════════════════════════════════════════════════════════════

% Under content-addressed salts the identical reopen makes ALL THREE readings
% produce the SAME store. `drop` has nothing to drop, because the arriving body
% is a valid answer to the live demand; `abort` cancelled and refired and the
% refire's answer is byte-identical. The trichotomy has no store-observable
% content whatsoever under this salt ruling.
check(b4_under_content_salts_all_three_readings_agree,
  ( scenario(content, identical_reopen, Items),
    maplist(reopen_store(content, Items), [abort, drop, orphan], Stores),
    Stores = [First, First, First],
    First == [row(feed, first_body)] )).

% Under per-instance salts they split into exactly two groups: {abort, drop}
% refuse, {orphan} admits. So the trichotomy only EXISTS once salts are
% per-instance, which is the salt ruling, not the fill ruling.
check(b4_under_instance_salts_the_readings_split_two_ways,
  ( scenario(instance, identical_reopen, Items),
    maplist(reopen_store(instance, Items), [abort, drop, orphan], Stores),
    Stores = [AbortRows, DropRows, OrphanRows],
    AbortRows == DropRows,
    AbortRows == [],
    OrphanRows == [row(feed, first_body)] )).

reopen_store(SaltMode, Items, Policy, Rows) :-
    run_items(cfg(Policy, SaltMode), Items, State),
    store_of(State, Rows).

% The cost of the primitive `drop` requires. Two subscribers, one target:
% content-addressed spends ONE call and never aborts (support is refcounted by
% the shared key); per-instance spends TWO and aborts one. The `drop` reading's
% instance column is therefore not a column, it is a world-call multiplier.
check(b4_instance_salts_double_the_world_calls_on_a_shared_target,
  ( scenario(content,  shared_target_one_leaves, ContentItems),
    scenario(instance, shared_target_one_leaves, InstanceItems),
    run_items(cfg(abort, content),  ContentItems,  ContentState),
    run_items(cfg(abort, instance), InstanceItems, InstanceState),
    world_calls(ContentState, 1),  world_aborts(ContentState, 0),
    world_calls(InstanceState, 2), world_aborts(InstanceState, 1),
    store_of(ContentState,  [row(feed, body_one)]),
    store_of(InstanceState, [row(feed, body_one)]) )).

% Same shape as examples/gh-cache.dl's rate law: the request id is
% content-addressed on (head, kind, args) and an already-fired id never
% re-fires, so a coarse clock bucket is what advances it. Model the daemon
% re-ticking 12 times inside one bucket: content-addressed spends ONE call,
% per-instance spends TWELVE. This is the 720-vs-12 defect restated at the
% demand layer, and it is invisible in the store (a 304 lands no rows).
check(b4_content_addressing_is_the_gh_cache_rate_law,
  ( retick_items(12, Items),
    run_items(cfg(drop, content),  Items, ContentState),
    run_items(cfg(drop, instance), Items, InstanceState),
    world_calls(ContentState, 1),
    world_calls(InstanceState, 12) )).

% ...and abort THROWS THAT SAVING AWAY. Cancelling on demand-support-zero also
% deletes the in-flight lock (rulings.pl's abort take: "its pending cache row
% deleted"), so a demand that flaps faster than the effect completes re-issues
% every time. Under content salts, drop spends 1 call and abort spends 12 on
% the identical schedule. abort is not the free reading either: it trades a
% storage leak for a call-thrash, and the trade is only correct when the
% demand's departure is final rather than a flap.
check(b1_abort_thrashes_a_flapping_demand_that_drop_would_have_cached,
  ( retick_items(12, Items),
    run_items(cfg(drop,  content), Items, DropState),
    run_items(cfg(abort, content), Items, AbortState),
    world_calls(DropState, 1),
    world_calls(AbortState, 12),
    world_aborts(AbortState, 12),
    store_of(DropState, []), store_of(AbortState, []) )).

retick_items(N, Items) :-
    numlist(1, N, Ticks),
    maplist(retick, Ticks, Pairs),
    append(Pairs, Items).

retick(Tick, [open(Tick, endpoint), close(Tick)]).

% ═══════════════════════════════════════════════════════════════════════════
% B5 -- IS ORPHAN-AS-A-ROW A DEAD-LETTER QUEUE?
% ═══════════════════════════════════════════════════════════════════════════
%
% A dead-letter queue is (i) a SECOND destination, distinct from the primary,
% (ii) with its own consumer, and (iii) durable -- a DLQ that loses messages is
% a broken DLQ. Test the three properties against orphan-as-a-row as the lab
% spells it.

% (i) fails: the orphan lands in the SAME rel as the answered fill. One
% destination, so structurally not a DLQ. The user's "we're not AMQP" instinct
% is correct about the SHAPE.
check(b5_orphan_lands_in_the_same_rel_as_an_answered_fill,
  ( Answered = [ open(session_one, feed), arrive(feed, feed, body_one) ],
    scenario(content, bare_orphan, Orphaned),
    run_items(cfg(orphan, content), Answered, AnsweredState),
    run_items(cfg(orphan, content), Orphaned, OrphanedState),
    store_of(AnsweredState, [row(feed, body_one)]),
    store_of(OrphanedState, [row(feed, body_one)]) )).

% (iii) fails: b3_orphan_as_a_row_is_not_crash_deterministic already showed the
% orphan is lost across a kill -9. So orphan-as-a-row is a best-effort delivery
% to a destination nobody named -- strictly WEAKER than a DLQ on the one axis
% that makes a DLQ worth having. It is not AMQP; it is the half of AMQP's
% problem statement without the guarantee.
check(b5_the_orphan_carries_no_delivery_guarantee,
  ( scenario(content, bare_orphan_crashed, Items),
    run_items(cfg(orphan, content), Items, State),
    store_of(State, []),
    world_calls(State, 1) )).

% ═══════════════════════════════════════════════════════════════════════════
% RUNNER
% ═══════════════════════════════════════════════════════════════════════════

go :-
    aggregate_all(count, check(_, _), Total),
    run_checks(Passed),
    format("~n~w/~w PASS~n", [Passed, Total]),
    (   Passed =:= Total
    ->  true
    ;   format("FAILURES PRESENT~n", []), halt(1) ).

run_checks(Passed) :-
    nb_setval(redteam_passed, 0),
    forall(check(Name, Goal), run_one(Name, Goal)),
    nb_getval(redteam_passed, Passed).

run_one(Name, Goal) :-
    (   catch(Goal, Error, (format("FAIL  ~w  (~q)~n", [Name, Error]), fail))
    ->  format("PASS  ~w~n", [Name]),
        nb_getval(redteam_passed, SoFar), Next is SoFar + 1,
        nb_setval(redteam_passed, Next)
    ;   format("FAIL  ~w~n", [Name]) ).

report :-
    forall(member(Name, [ bare_orphan, bare_orphan_crashed, endurance_phase_one,
                          endurance_phase_two, unacked_commit_crash,
                          shared_target_one_leaves, identical_reopen,
                          typeahead_eight ]),
           forall(salt_mode(SaltMode), report_scenario(SaltMode, Name))).

report_scenario(SaltMode, Name) :-
    scenario(SaltMode, Name, Items),
    format("~n~w / ~w~n", [Name, SaltMode]),
    forall(policy(Policy),
           ( run_peak(cfg(Policy, SaltMode), Items, State, Peak),
             store_of(State, Rows), ledger_of(State, Ledger),
             world_calls(State, Calls), world_aborts(State, Aborts),
             format("  ~w~t~10| calls=~w aborts=~w peak=~w~n    store  ~q~n    world  ~q~n",
                    [Policy, Calls, Aborts, Peak, Rows, Ledger]) )).
