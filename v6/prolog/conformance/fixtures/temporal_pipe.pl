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
% siblings folded 2026-08-20 (same throw edge_body_needs_json_destructure):
% trigger_marker_is_what_stops_backlog_replay,
% unmarked_chain_replays_to_late_subscriber,
% unmarked_first_stage_refires_on_late_watch, pipe_stage_costs_one_tick. See git.

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
% siblings folded 2026-08-20 (same throw edge_body_needs_json_destructure):
% guard_stage_silent_when_muted, guard_stage_silent_below_threshold. See git.

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
