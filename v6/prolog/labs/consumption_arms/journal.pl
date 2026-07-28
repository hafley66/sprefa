% journal.pl : the ASSERTION SET and the ROUND JOURNAL.
%
% assertion(Number, Round, Text, Checks) -- Round is the MINTING round;
% Checks are the lab check names that validate it. Every check named here
% must exist, and lab.pl proves that.
%
% amends(Number, Round) -- a later round amended that assertion. The text
% carried by the assertion is always the AMENDED text; the amendment record
% is what says the earlier reading was wrong.
%
% round(Number, Aim, Findings) -- the fixpoint record. The lab stops at a
% round that finds nothing new. Rounds 1 to 3 live inside the thread files;
% rounds 4 to 7 are adversarial passes in rounds.pl.

:- module(ca_journal, [ assertion/4, round/3, amends/2 ]).

:- use_module(library(lists)).

% ═══ THREAD 2 : consumption axis ═══════════════════════════════════════════

assertion(1, 1,
 'the consumption axis is spelled by the key declaration and nothing else: switch is key(key), queue is key(queue, ordinal) plus a min-ordinal level view plus a done rel. Neither side needs a construct',
 [r1_switch_is_the_key_declaration, r1_queue_is_the_ordinal_key_declaration]).

assertion(2, 1,
 'the queue ordinal is mintable in the language today: a keyed counter read through pre/1 chains across occurrences inside one tick, so three pushes in one tick get ordinals 1, 2, 3. No engine stamp column has to be exposed',
 [r1_ordinal_mints_in_language_across_one_tick]).

assertion(3, 1,
 'pacing (a) costs one drain tick whatever the queue length; pacing (b) costs one drain tick per item, so a queue of N settles in N plus two ticks',
 [r1_pacing_a_lands_every_item_in_one_tick, r1_pacing_b_lands_one_item_per_drain_tick,
  r3_pacing_b_drain_count_is_queue_length_plus_two, r3_pacing_a_drain_count_is_flat_in_queue_length]).

assertion(4, 2,
 'AMENDS 3. Into a Log consumer the two pacings deliver identical rows and differ only in the tick index; into a KEYED consumer pacing (a) loses N minus 1 of N items, because N writes to one key inside one tick fold to the last one',
 [r1_both_pacings_deliver_the_same_rows_into_a_log_consumer,
  r2_pacing_a_loses_two_of_three_items_at_a_keyed_consumer,
  r2_pacing_b_keeps_all_three_items_at_a_keyed_consumer]).

assertion(5, 2,
 'under pacing (a) the surviving item is not even the last one queued: the within-tick fold order is the standard order of the ready view TERMS, so moving the payload column ahead of the ordinal column changes the winner. Pacing (a) preserves no order the program stated',
 [r2_pacing_a_survivor_is_decided_by_column_order_not_by_ordinal]).

assertion(6, 3,
 'a durable pending rel does not on its own sidestep the non-durable Ti carry: after a restart the queue ROW is intact and the min-ordinal head is recomputed and present, and the run produces zero ticks. The row survives, the firing does not',
 [r3_crash_restart_stalls_with_the_queue_intact]).

assertion(7, 3,
 're-delivering the durable rows as arrivals does not restart the queue either, because a Set arrival already present in the store is not an occurrence. Restart needs a boot occurrence policy, not a replay',
 [r3_replaying_the_durable_rows_as_arrivals_also_stalls,
  r3_a_genuinely_fresh_arrival_drains_the_whole_queue,
  r3_the_pacing_and_boot_occurrence_slots_are_both_named]).

assertion(8, 3,
 'under pacing (b) the engine drain cap becomes a data-dependent queue-length cap: 99 items settle in 101 ticks, 100 items throw drain_overflow(100). Under pacing (a) the same 100 items settle in 3 ticks',
 [r3_pacing_b_of_ninety_nine_items_survives_the_cap,
  r3_pacing_b_of_one_hundred_items_throws_drain_overflow,
  r3_pacing_a_of_one_hundred_items_does_not_throw]).

% ═══ THREAD 1 : lifecycle arms ═════════════════════════════════════════════

assertion(9, 1,
 'subscribe and unsubscribe are next and finalize on the DEMAND rel; complete is finalize on the LIVE SCOPE rel. All three ground out to shipped kernel forms with no construct',
 [r1_subscribe_and_unsubscribe_are_next_and_finalize_on_the_demand_rel,
  r1_complete_is_finalize_on_the_live_scope_rel,
  r1_next_fires_in_the_arrival_tick_and_finalize_one_drain_later]).

assertion(10, 3,
 'every arm in the ruled vocabulary is ROW granularity once the right rel is named. There is no rel-level arm, which is what makes the family one construct rather than two',
 [r3_every_arm_is_row_granularity_on_some_rel]).

assertion(11, 3,
 'three of the six arms fire on a rel the arm does not name (subscribe, unsubscribe, complete). That mismatch is the whole content of SLOT-ARM-ARGUMENT',
 [r3_three_arms_fire_on_a_rel_they_do_not_name]).

assertion(12, 1,
 'the error arm survives ONLY as an enum-variant destructure over the envelope rel, and only over a LOG envelope. The second-channel reading is refused on three independent grounds: the failure-is-a-value envelope ruling, an exception is not a row, and a non-row can never appear in the tick log that item-9 grading diffs',
 [r1_the_error_arm_is_an_ordinary_variant_destructure,
  r2_the_second_channel_reading_is_refused_on_three_grounds,
  r6_a_log_envelope_never_swallows_an_error]).

assertion(13, 2,
 'the error arm is NOT terminal and it is not even guaranteed to fire. rx error is the last notification a subscription receives; here the rel keeps producing next rows in the very next tick, and on a KEYED envelope an error row arriving in the same tick as a later ok row is replaced before any arm sees it',
 [r2_the_rel_keeps_producing_after_the_error_arm_fires,
  r6_a_keyed_envelope_swallows_an_error_delivered_in_the_same_tick,
  r6_the_same_two_rows_one_tick_apart_do_fire_the_error_arm]).

assertion(14, 2,
 'an error arm over a rel whose decl declares no error variant loads, runs and never fires, with no diagnostic. Same class as finalize over a Log rel',
 [r2_an_error_arm_with_no_matching_variant_is_silently_dead]).

assertion(15, 3,
 'the arm family is not timing symmetric: next, subscribe and error fire in the tick of their plus delta; finalize, unsubscribe and complete fire one drain tick after their minus delta, because a minus delta only becomes an occurrence through the departure carry',
 [r3_plus_side_arms_fire_at_t_and_minus_side_arms_at_t_plus_one,
  r1_next_fires_in_the_arrival_tick_and_finalize_one_drain_later]).

% ═══ THREAD 3 : channel ════════════════════════════════════════════════════

assertion(16, 1,
 'a channel with N readers and M writers composes today out of a Log rel, a keyed cursor rel, an arithmetic guard and a min aggregate. M writers in one tick get distinct ordinals; readers advance independently one ordinal per tick; a late reader catches up from its own cursor, not from the newest row',
 [r1_two_writers_in_one_tick_get_distinct_ordinals,
  r1_two_readers_advance_independently_and_the_watermark_follows,
  r1_a_late_reader_catches_up_from_its_own_cursor]).

assertion(17, 2,
 'keep(count(N)) prunes with no delta of any kind: the tick logs of the same program under keep(all) and keep(count(2)) are identical while the final states differ by a row. Retention is invisible to the tick-log grading currency',
 [r2_the_prune_is_invisible_in_the_tick_log,
  r3_retention_already_removes_log_rows_without_a_delta,
  r5_keep_all_shows_the_row_that_keep_count_one_erased]).

assertion(18, 2,
 'a static keep(count(N)) permanently stalls a lagging reader: its cursor never moves again, the run goes quiescent with three empty ticks, and the watermark that would have prevented the prune is sitting in the final state saying the pruned ordinal was unread',
 [r2_a_static_keep_count_permanently_stalls_the_lagging_reader,
  r2_the_same_program_under_keep_all_loses_nothing]).

assertion(19, 3,
 'keep(count(N)) is a function of the log alone and the retention a channel needs is a function of a JOIN, so no static bound expresses it. The smallest honest spelling is retention as an ORDINARY RULE (s1): zero new decl words, the prune becomes a visible minus delta, and its whole cost is lifting the retract_from_log law that retention itself already violates invisibly',
 [r3_every_retention_option_is_priced_both_ways,
  r3_retention_already_removes_log_rows_without_a_delta,
  r3_explicit_log_retraction_throws_today]).

% ═══ THREAD 4 : transition collapse logging ════════════════════════════════

assertion(20, 1,
 'exactly one instrumentation site is reachable for the collapse event: the keyed store write. Every frontier the ruling names (arrival batching, occurrence loop, drain carry) funnels through it, and duplicate Set adds, Log appends and level recomputes cannot collapse at all',
 [r1_exactly_one_instrumentation_site_is_reachable,
  r1_a_duplicate_set_add_is_not_a_collapse,
  r1_a_log_append_is_not_a_collapse]).

assertion(21, 1,
 'the collapse count is WRITES per key per tick, one event per key, not one event per lost intermediate. Three writes mint collapse(Tick, Ref, Key, 3, _)',
 [r1_two_writes_one_key_one_tick_mints_one_event,
  r1_three_writes_mint_one_event_counting_writes_not_intermediates,
  r1_one_write_per_key_mints_no_event]).

assertion(22, 2,
 'the event must fire on write count and NOT on delta presence. A net-zero pair of writes (v0 to v1 to v0) leaves the boundary showing nothing at all for that rel, and that is the case where silence is most misleading. An equal-row rewrite counts too, because the occurrence happened even though the store did not move',
 [r2_a_net_zero_pair_of_writes_shows_no_delta_at_all,
  r2_the_event_still_fires_on_the_net_zero_pair,
  r2_an_equal_row_rewrite_still_counts_as_a_collapsed_write]).

assertion(23, 3,
 'a trace-only collapse event is not conformance-checkable: two runs with different collapse counts produce byte-identical tick logs for the collapsed rel, so the item-9 grading currency cannot see the difference. SLOT-COLLAPSE-CHANNEL',
 [r3_two_runs_with_different_collapse_counts_share_a_tick_log,
  r3_the_grading_gap_is_a_named_slot]).

% ═══ THREAD 5 : level rule as signed edge ══════════════════════════════════

assertion(24, 1,
 'the level-as-signed-edge desugar agrees on every DELTA of the plus half: the level row and the edge write both land in the arrival tick, in the same order, with the same rows. It does not agree byte for byte on the whole log',
 [r1_the_plus_half_lands_the_same_deltas_in_the_arrival_tick]).

assertion(25, 2,
 'the desugar is inexpressible on the minus half. Edge heads only append or replace and nothing in the kernel retracts, so the edge form never retracts at all; a finalize arm observes the departure but writes a NEW row one drain tick later. rx CAN express the desugar (a scan owning a set it may shrink), which locates the gap in the kernel, not in the idea',
 [r2_the_level_form_retracts_in_the_departure_tick,
  r2_the_edge_form_never_retracts_at_all,
  r2_a_finalize_arm_writes_a_row_and_cannot_remove_one,
  r3_the_claim_holds_on_the_plus_half_and_fails_on_the_minus_half]).

% ═══ the three findings the closing rounds added ═══════════════════════════

assertion(26, 5,
 'a Log row appended and pruned inside ONE tick carries no delta of any sign anywhere in the run: it is written, retained out, and never appears in the tick log at all. Retention is not merely invisible, it can erase a row from the grading record entirely, and the same program under keep(all) shows the row',
 [r5_a_row_appended_and_pruned_in_one_tick_has_no_delta_of_any_sign,
  r5_keep_all_shows_the_row_that_keep_count_one_erased]).

assertion(27, 6,
 'whether an error variant is observed at all depends on the envelope rel key declaration and on how the scheduler batched arrivals: on a KEYED envelope an error row arriving in the same tick as a later ok row is replaced before any arm sees it, and the same two rows one tick apart do fire the error arm. The ruled collapse event is the only mechanism anywhere that reports the drop, which is where the trace obligation earns its keep',
 [r6_a_keyed_envelope_swallows_an_error_delivered_in_the_same_tick,
  r6_the_same_two_rows_one_tick_apart_do_fire_the_error_arm,
  r6_the_collapse_event_is_what_reports_the_swallowed_error,
  r6_a_log_envelope_never_swallows_an_error]).

assertion(28, 4,
 'the edge form of a level rule mints one trailing quiescence tick the level form never mints. It comes from the edge WRITE carrying itself into the next tick, not from the head kind, which is why a level rel feeding an edge rule mints the same tick',
 [r4_the_edge_form_mints_one_extra_quiescence_tick,
  r4_the_quiescence_tick_comes_from_the_edge_write_not_the_head_kind]).

% ═══ amendments ════════════════════════════════════════════════════════════

amends(4, 3).
amends(24, 4).
amends(17, 5).
amends(12, 6).
amends(13, 6).

% ═══ the round journal ═════════════════════════════════════════════════════

round(1, 'build the arm table, both consumption spellings, the channel and the collapse model; assert what each thread looks like when it works',
      [ 'the whole consumption axis turned out to be a decl choice, so the switch-versus-queue question is not a construct question at all',
        'the queue ordinal minted in-language on the first try through pre/1, which killed a suspected need to expose the engine stamp as a column',
        'complete and unsubscribe both grounded to finalize on a DIFFERENT rel, which is why the arm table needed a subject column that the ruled vocabulary does not have',
        'the error arm ran as a plain variant destructure with no engine change',
        'the collapse event needed a model because engine.pl throws away the per-key write counts that produce its boundary diff' ]).

round(2, 'try to break round 1 by changing one thing at a time: the consumer key, a column order, the order of an error row, the retention bound',
      [ 'pacing (a) into a keyed consumer lost two of three items, which broke the round-1 reading that the pacings differ only in tick index',
        'moving the payload column ahead of the ordinal changed which item survived pacing (a), so pacing (a) preserves no stated order at all',
        'the error arm fired and the rel kept producing the next tick, so the rx word promises a termination the language does not deliver',
        'keep(count(2)) produced a tick log byte-identical to keep(all) while permanently stalling a reader',
        'the net-zero write pair broke the round-1 reading that a collapse event annotates a delta: there is no delta to annotate' ]).

round(3, 'try to break round 2 by attacking durability, scale and gradeability rather than data',
      [ 'restart from the durable queue produced zero ticks, so the C7-sidestep claim is conditional on a boot occurrence policy that does not exist',
        're-delivering the durable rows as arrivals also produced nothing, because an already-present Set row is not an occurrence',
        'pacing (b) turned the drain cap into a queue-length cap at exactly 100 items',
        'the arm family split three-and-three on firing tick, which the ruled vocabulary does not say anywhere',
        'two runs with different collapse counts produced identical tick logs, so the ruled trace event is not conformance-checkable where the ruling puts it' ]).

round(4, 'hunt fresh breaks at the edges of the round-1 shapes: an empty queue, a fully drained queue pushed again, a partially drained queue on restart, a reader that consumes and republishes onto its own channel, and a collapse on a drain tick rather than an arrival tick',
      [ 'the level and edge forms of one rule do NOT agree byte for byte: the edge write carries itself into a trailing quiescence tick the level form never mints, which broke assertion 24 and minted assertion 28',
        'the empty queue, the redrained queue and the self-feeding channel all held; the min aggregate over an empty undrained set produces no head row rather than an error or a spurious row',
        'a partially drained queue stalls on restart exactly like a full one, so assertion 6 is not an artefact of the all-or-nothing case',
        'the drain-tick collapse came out of the same instrumentation site as the arrival-tick collapse, confirming assertion 20 at the one place it could have needed a second site' ]).

round(5, 'attack the ordinal minting with duplicate rows, the collapse count with a keyed conflict, and retention with a bound smaller than one tick batch',
      [ 'a Log row appended and pruned inside ONE tick has no delta of any sign anywhere in the run, which sharpens assertion 17 past its round-2 wording and mints assertion 26',
        'the identical program under keep(all) shows the row, so the erasure is retention and not a Log-rel property',
        'two identical Log arrivals in one tick get distinct ordinals, so the queue does not deduplicate, which is the behaviour a queue must have',
        'two rules writing one key inside one occurrence throw keyed_conflict rather than collapsing, so the collapse count only ever counts writes from distinct occurrences' ]).

round(6, 'put the error arm on a KEYED envelope, which is the shape every stale-while-revalidate cache uses, and see whether the round-2 error findings survive',
      [ 'a keyed envelope SWALLOWS an error row that arrives in the same tick as a later ok row: the arm never fires and no rel anywhere records that a failure happened, which broke assertions 12 and 13 and minted assertion 27',
        'the same two rows one tick apart do fire the error arm, so whether a failure is observed is a function of scheduler batching',
        'the ruled collapse event reports the drop, which is the first place in this lab where the trace obligation is the only thing standing between a dropped failure and silence',
        'a Log envelope never swallows the error, so the escape is a decl choice and assertion 1 holds again at the error arm' ]).

round(7, 'close the fixpoint: attack the collapse count from the quiet side, stack retention against the finalize refusal on one row, check the pacing (b) drain against the collapse log, and replay every amendment',
      []).
