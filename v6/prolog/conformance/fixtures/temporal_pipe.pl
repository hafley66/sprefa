% fixtures/temporal_pipe.pl : the temporal_pipe lab's RUNTIME checks promoted
% into the shared fixture format, RE-GRADED under the rulings (AGGREGATE.md 5c).
%
% Scope: the pipe DESUGAR is lab-internal rewriting machinery and is not
% promoted. What runs here is the SEMANTICS the desugared rules exercise,
% written out as the plain three-rule chain the lab's `hand_feed` program
% spells by hand (temporal_pipe.pl:446-459): demand join, response fold,
% client delivery, with only/1 at the stage boundaries and the FIRST stage
% left unmarked (the lab's ambiguity 9, ruled behavior under q6).
%
% Two spelling adjustments from the lab text, both forced by the reference
% engine's body-goal set:
%   * the lab's binding item `Result = fresh(_Tag, Body)` is written
%     `decode(Result, fresh(_Tag, Body))`; `=` is not an engine body goal and
%     decode/2 is the supported destructuring form for an enum-shaped row.
%   * the lab hand-fed empty arrival ticks to drain the carry
%     (temporal_pipe.pl:485-486, :510, :529). Under q5 the engine appends
%     drain ticks itself, so those trailing empties are gone from every
%     Schedule below and the ticks(N) expectations count what the engine adds.
%     Empty ticks that remain MID-schedule are idle ticks with a later arrival
%     behind them, not stand-ins for the trailing drains.
%
% not promoted:
%   * pipe_glyph_needs_quotes, pipe_below_comma_inverts_stages,
%     pipe_above_comma_groups_stages, pipe_under_arrow_keeps_one_head,
%     pipe_at_arrow_priority_clashes, pipe_above_arrow_becomes_clause_chain,
%     dot_access_truncates_on_space, dot_access_in_fact_arg_becomes_a_rule,
%     bang_negation_is_unlexable, struct_and_match_blocks_do_not_read.
%     These grade what prolog's READER does with a stand-in glyph. They are
%     surface-syntax receipts for surface_dcg, with no engine behavior in them.
%   * chain_desugars_to_three_rules, piped_atom_is_the_only_trigger,
%     carried_columns_are_minimal, head_variable_bound_nowhere_is_rejected,
%     nested_pipe_parses_but_desugar_rejects. Desugar-time machinery: the
%     engine runs plain rules and has no chain rewriter to check.
%   * cut_kinds_are_yield_then_edge_append, chain_without_cut_rejected,
%     cut_law_depends_on_declarations. The boundary law is a STATIC check that
%     reads the head rel's kind out of the declaration table (ruling q3, and
%     the lab's ambiguity 7). The reference engine ships no boundary or
%     pairwise-disjointness checker, so there is no runtime behavior to pin;
%     these three stay lab receipts until that checker exists.
%   * variable_skipping_a_stage_still_flows and name_reuse_across_stages_is_a_join
%     had runtime content and are folded into desugared_trace_equals_hand_written
%     as its final(change_log/3, ...) expectation: Endpoint is bound in stage 1,
%     absent from stage 2, and joined again in stage 3, so bob (subscribed to a
%     different endpoint) never appears in the output.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ the ghcacher chain, hand written ═══════════════════════════════════════
% demand_row(Endpoint, Result)  <+ watch, cache_tag, every_300, fetch
% folded_row(Endpoint, Stars)   <+ demand_row, decode, stars_of
% change_log(Endpoint, Stars, Client) <+ folded_row, latest(subscribed_to)
%
% Stage 1 carries no marker: it fires on ANY of its four body atoms, which is
% the documented default and the lab's ambiguity 9 (the pipe buys a single
% trigger atom at boundaries only, never at the head of a chain).

fixture(desugared_trace_equals_hand_written,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(folded_row/2, log), keep(folded_row/2, all),
         kind(change_log/3, log), keep(change_log/3, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
             fetch(Endpoint, PrevTag, Bucket, Result) ),
         ( folded_row(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars) ),
         ( change_log(Endpoint, Stars, Client) <+
             folded_row(Endpoint, Stars), latest(subscribed_to(Client, Endpoint)) ) ]),
  [],
  [ [ +watch(cli), +cache_tag(cli, no_tag), +stars_of(body1, 42),
      +subscribed_to(alice, cli), +subscribed_to(bob, other) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ] ],
  [ deltas(demand_row/2, [ [], [], [ +demand_row(cli, fresh(tag_w1, body1)) ],
                           [], [], [] ]),
    deltas(folded_row/2, [ [], [], [], [ +folded_row(cli, 42) ], [], [] ]),
    deltas(change_log/3, [ [], [], [], [], [ +change_log(cli, 42, alice) ], [] ]),
    ticks(6),
    final(change_log/3, [ change_log(cli, 42, alice) ]) ]).

% ═══ q6: the marker is what stops backlog replay ════════════════════════════
% The lab's richer scenario, kept over the minimal pair already in
% fixtures/engine_core.pl (marker_stops_backlog_replay /
% unmarked_edge_replays_backlog): here the subscriber joins mid-stream over a
% THREE-stage chain whose first stage is unmarked, so the run also shows that
% the marker narrows the trigger at boundaries while the chain head keeps
% any-atom. Ticks 4 and 5 are idle ticks that let the chain reach change_log
% before carol arrives at tick 6.
%
% REJECTED READING (temporal_pipe.pl:820-828, pre-ruling): the lab observed
% the unmarked run's carol delivery as the LAST tick of a 6-tick hand-fed
% schedule. Under q5 the engine owns the drains: carol's write at tick 6
% leaves carry, so the unmarked run costs a 7th tick and the marked run stops
% at 6.

fixture(trigger_marker_is_what_stops_backlog_replay,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(folded_row/2, log), keep(folded_row/2, all),
         kind(change_log/3, log), keep(change_log/3, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
             fetch(Endpoint, PrevTag, Bucket, Result) ),
         ( folded_row(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars) ),
         ( change_log(Endpoint, Stars, Client) <+
             folded_row(Endpoint, Stars), latest(subscribed_to(Client, Endpoint)) ) ]),
  [],
  [ [ +watch(cli), +cache_tag(cli, no_tag), +stars_of(body1, 42),
      +subscribed_to(alice, cli), +subscribed_to(bob, other) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ],
    [ ],
    [ ],
    [ +subscribed_to(carol, cli) ] ],
  [ deltas(change_log/3, [ [], [], [], [], [ +change_log(cli, 42, alice) ], [] ]),
    ticks(6),
    final(change_log/3, [ change_log(cli, 42, alice) ]) ]).

% The unmarked twin: identical rules with only/1 removed from both boundaries.
% Carol's arrival re-fires the last rule against the standing folded_row and
% replays the backlog she was never present for.
fixture(unmarked_chain_replays_to_late_subscriber,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(folded_row/2, log), keep(folded_row/2, all),
         kind(change_log/3, log), keep(change_log/3, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
             fetch(Endpoint, PrevTag, Bucket, Result) ),
         ( folded_row(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars) ),
         ( change_log(Endpoint, Stars, Client) <+
             folded_row(Endpoint, Stars), subscribed_to(Client, Endpoint) ) ]),
  [],
  [ [ +watch(cli), +cache_tag(cli, no_tag), +stars_of(body1, 42),
      +subscribed_to(alice, cli), +subscribed_to(bob, other) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ],
    [ ],
    [ ],
    [ +subscribed_to(carol, cli) ] ],
  [ deltas(change_log/3, [ [], [], [], [],
                           [ +change_log(cli, 42, alice) ],
                           [ +change_log(cli, 42, carol) ],
                           [] ]),
    ticks(7),
    final(change_log/3, [ change_log(cli, 42, alice),
                          change_log(cli, 42, carol) ]) ]).

% The other half of ambiguity 9, ruled and kept: the marked boundaries refuse
% a late subscriber, and in the SAME program a late `watch` row re-fires the
% unmarked first stage against the standing fetch backlog. The `other`
% endpoint's response lands at tick 3 with nothing watching it; watch(other)
% at tick 6 replays it and the whole chain runs again behind it.
fixture(unmarked_first_stage_refires_on_late_watch,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(folded_row/2, log), keep(folded_row/2, all),
         kind(change_log/3, log), keep(change_log/3, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
             fetch(Endpoint, PrevTag, Bucket, Result) ),
         ( folded_row(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars) ),
         ( change_log(Endpoint, Stars, Client) <+
             folded_row(Endpoint, Stars), latest(subscribed_to(Client, Endpoint)) ) ]),
  [],
  [ [ +watch(cli), +cache_tag(cli, no_tag), +cache_tag(other, no_tag),
      +stars_of(body1, 42), +subscribed_to(alice, cli), +subscribed_to(dave, other) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)),
      +fetch(other, no_tag, bucket1, fresh(tag_w1, body1)) ],
    [ ],
    [ ],
    [ +watch(other) ] ],
  [ deltas(change_log/3, [ [], [], [], [],
                           [ +change_log(cli, 42, alice) ],
                           [], [],
                           [ +change_log(other, 42, dave) ],
                           [] ]),
    ticks(9),
    final(change_log/3, [ change_log(cli, 42, alice),
                          change_log(other, 42, dave) ]) ]).

% ═══ one tick per stage ═════════════════════════════════════════════════════
% Every standing row is seeded, so the run has exactly ONE outside arrival:
% the fetch response. A three-stage chain then delivers three ticks later, and
% the engine's own drain closes the run. Distinct from engine_core's
% edge_chain_hops_tick_per_stage (two stages, no join, no fold).
%
% REJECTED READING (temporal_pipe.pl:719-727, pre-ruling): the lab read the
% hop ticks off a six-tick hand-fed schedule and never counted the drains,
% because it supplied the empty ticks itself. Under q5 the tick count is the
% engine's: 1 scheduled + 3 drains.

fixture(pipe_stage_costs_one_tick,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(folded_row/2, log), keep(folded_row/2, all),
         kind(change_log/3, log), keep(change_log/3, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
             fetch(Endpoint, PrevTag, Bucket, Result) ),
         ( folded_row(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars) ),
         ( change_log(Endpoint, Stars, Client) <+
             folded_row(Endpoint, Stars), latest(subscribed_to(Client, Endpoint)) ) ]),
  [ watch(cli), cache_tag(cli, no_tag), every_300(bucket1),
    stars_of(body1, 42), subscribed_to(alice, cli) ],
  [ [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ] ],
  [ deltas(demand_row/2, [ [ +demand_row(cli, fresh(tag_w1, body1)) ], [], [], [] ]),
    deltas(folded_row/2, [ [], [ +folded_row(cli, 42) ], [], [] ]),
    deltas(change_log/3, [ [], [], [ +change_log(cli, 42, alice) ], [] ]),
    ticks(4) ]).

% ═══ a chain whose last write lands on a keyed head ═════════════════════════
% The cache-tag loop: stage 1 reads the keyed rel it will eventually replace,
% stage 2 writes the new tag. The replace shows up as -old/+new at the
% boundary, and the replaced row becomes a T+1 occurrence that re-triggers the
% unmarked first stage without matching (the standing response carries the old
% tag), so the run drains in one extra tick.
%
% REJECTED READING (temporal_pipe.pl:729-733, pre-ruling): four delta ticks,
% ending on the replace. Under q5 the engine appends the drain that consumes
% the replaced row's carry, so the trail is five ticks with a silent last one.

fixture(chain_into_keyed_head_replaces,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         keyed(cache/2, [1]) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), cache(Endpoint, PrevTag), every_300(Bucket),
             fetch(Endpoint, PrevTag, Bucket, Result) ),
         ( cache(Endpoint, Tag) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(Tag, _Body)) ) ]),
  [ cache(cli, no_tag) ],
  [ [ +watch(cli) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ] ],
  [ deltas(demand_row/2, [ [], [], [ +demand_row(cli, fresh(tag_w1, body1)) ],
                           [], [] ]),
    deltas(cache/2, [ [], [], [],
                      [ -cache(cli, no_tag), +cache(cli, tag_w1) ],
                      [] ]),
    ticks(5),
    final(cache/2, [ cache(cli, tag_w1) ]) ]).

% ═══ one stage holding negation AND a comparison ════════════════════════════
% The lab's program 5: a single stage joins, destructures, compares and
% negates. All four items sit inside ONE time cut, which is where they were
% before the pipe existed; neither the comparison nor the negation is affected
% by the boundary in front of them.

fixture(guard_stage_fires_on_negation_and_comparison,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(alert/2, log),      keep(alert/2, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), every_300(Bucket),
             fetch(Endpoint, no_tag, Bucket, Result) ),
         ( alert(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars),
             Stars > 100, not(muted(Endpoint)) ) ]),
  [],
  [ [ +watch(cli), +stars_of(body1, 420) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ] ],
  [ deltas(alert/2, [ [], [], [], [ +alert(cli, 420) ], [] ]),
    ticks(5),
    final(alert/2, [ alert(cli, 420) ]) ]).

% Same program, muted endpoint: the negation kills the write, so the tick that
% consumed the carry produces nothing and the run stops one tick earlier (no
% write means no carry means no further drain).
fixture(guard_stage_silent_when_muted,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(alert/2, log),      keep(alert/2, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), every_300(Bucket),
             fetch(Endpoint, no_tag, Bucket, Result) ),
         ( alert(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars),
             Stars > 100, not(muted(Endpoint)) ) ]),
  [],
  [ [ +watch(cli), +stars_of(body1, 420), +muted(cli) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ] ],
  [ deltas(alert/2, [ [], [], [], [] ]),
    ticks(4),
    final(alert/2, []) ]).

% Same program, 42 stars: the comparison kills the write.
fixture(guard_stage_silent_below_threshold,
  prog([
         kind(every_300/1, log),  keep(every_300/1, all),
         kind(fetch/4, log),      keep(fetch/4, all),
         kind(demand_row/2, log), keep(demand_row/2, all),
         kind(alert/2, log),      keep(alert/2, all) ],
       [ ( demand_row(Endpoint, Result) <+
             watch(Endpoint), every_300(Bucket),
             fetch(Endpoint, no_tag, Bucket, Result) ),
         ( alert(Endpoint, Stars) <+
             demand_row(Endpoint, Response),
             decode(Response, fresh(_Tag, Body)), stars_of(Body, Stars),
             Stars > 100, not(muted(Endpoint)) ) ]),
  [],
  [ [ +watch(cli), +stars_of(body1, 42) ],
    [ +every_300(bucket1) ],
    [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ] ],
  [ deltas(alert/2, [ [], [], [], [] ]),
    ticks(4),
    final(alert/2, []) ]).
