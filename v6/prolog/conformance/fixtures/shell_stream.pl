% fixtures/shell_stream.pl : shell_stream lab checks promoted into the shared
% fixture format (AGGREGATE.md 5a row: shell_stream). Effect fills (what a
% real `sprefa-extract` process would write to stdout and its exit code) are
% modeled as CANNED SCHEDULED ARRIVALS per FIXTURES.md "Out of scope for
% fixtures": canned rows are program-text identical, no real process spawns
% land in a fixture. `watch_request(Client, Args, Salt)` plays the role of an
% upstream watcher demanding an extraction; `Args` is the lab's `args` demand
% column (what to extract), `Salt` is the lab's `salt` demand column (why this
% extraction recurs). `fill(Args, Salt, Payload)` is the canned per-item
% response a bind would have written; `stream_item`/`stream_end` are the
% lab's SPLIT envelopes (ExtractEvent vs ExtractEnd) as two plain-term Log
% rels, one arrival per item, exactly one terminal arrival.
%
% fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations)
%
% not promoted (shell_stream.md / shell_stream.pl, 16 of 20 checks):
%   - live_pipe_matches_canned, live_pipe_spans_ticks, live_nonzero_exit_keeps_rows
%     [ORIGINAL, real-spawn version]: all three drive a real process_create
%     pipe (live_script/2, live_lines/3, read_lines/2). Out of scope per
%     FIXTURES.md ("Real process spawns"); live_nonzero_exit_keeps_rows is
%     promoted here under the SAME name but reconstructed from a canned
%     stream_end(_, err(_)) arrival instead of a spawned `sh -c ...; exit 3`.
%   - fixture_file_matches_canned [+ fixture_path/1, fixture_state/1]: reads
%     shell_stream_fixture.jsonl off disk. Out of scope per FIXTURES.md
%     ("the jsonl fixture file reading").
%   - mode_read_off_result_type, mode_is_functional,
%     multi_finite_needs_terminal_enum: grade the lab's own `result_mode/3`
%     and `effect_mode/3` predicates (mode/lifetime typing over the
%     Stream(Item,End)/Tail(Item)/FetchResult wrapper types). The reference
%     engine has no type-level mode analysis; nothing here is a
%     program/schedule/expectation trace.
%   - split_envelope_exhaustive, flat_envelope_costs_dead_arms,
%     missing_terminal_arm_rejected: run the lab's `covers/2` exhaustiveness
%     fold over `item_arms`/`end_arms` against the enum declarations. Static
%     analysis over the lab's own match-arm lists, not a tick trace.
%   - stream_effect_grounds_in_kernel: checks `lab_sugar/2` sugar-part lists
%     against `kernel/1` facts in ../src/kernel.pl (the tier-grounding
%     bookkeeping). No engine equivalent; not a trace.
%   - jsonl_line_parses: exercises the lab's own copied JSON DCG
%     (json_parse/2, a duplicate of books/v6/algos/json.pl). Out of scope per
%     FIXTURES.md ("stdlib string fns beyond concat"); the engine's own
%     decode/json_each machinery is unrelated to this DCG.
%   - lines_land_in_arrival_order: item sequencing within one tick is already
%     exercised by every Log-rel fixture in engine_core.pl/merge_family.pl
%     (stamped occurrence order); not one of the four checks the AGGREGATE
%     5a row names for this lab and adds no new engine law.
%   - done_finishes_lifetime, complete_derives_exactly_once: same happy-path
%     trace as terminal_is_terminal; the promoted terminal_is_terminal fixture
%     (the level view flipping running -> done) subsumes the lifetime claim.
%   - err_keeps_prior_rows, err_derives_no_completion: same territory as
%     live_nonzero_exit_keeps_rows (a mid-stream Err without a real spawn);
%     not separately named in the 5a row, and the promoted
%     live_nonzero_exit_keeps_rows fixture already grades "rows survive a
%     terminal failure value".
%   - the bind grammar itself (`bind extract = shell { ... stdout_line(text)
%     => Line{...}, exit(code) => match code {...} }`, the two-channel
%     stdout_line + exit declaration surface): this is surface_dcg territory,
%     not a fact the reference engine's Decls/Rules grammar can express; the
%     fixtures below start from the canned rows a correct bind would have
%     produced, never from the bind text.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ identity + dedup: demand rows are a SET, full-row keyed ════════════════
%
% `demand(Args, Salt)` is edge-derived from watch_request arrivals, keyed on
% BOTH columns ([1, 2]): the key IS the whole row, so two watchers deriving
% the identical (Args, Salt) pair collapse via the equal-row write no-op
% (r_equal_row_write) rather than a keyed replace, and a genuinely different
% Salt is simply a different key, not a conflict and not a replace. That is
% what makes a keyed rel behave as true SET membership here instead of
% latest-wins replace (contrast merge_family's key_last_write_wins, keyed on
% Args alone, where a new value at the same key DOES replace).
%
% The canned fill/response pair shows "Edge rules route fills": `response`
% fires on a bare `fill` occurrence, never on the latest demand carry,
% so two watchers still cost exactly one routed fill.

fixture(identical_demand_dedups,
  prog([ kind(watch_request/3, log), keep(watch_request/3, all),
         kind(fill/3, log),          keep(fill/3, all),
         kind(response/3, log),      keep(response/3, all),
         keyed(demand/2, [1, 2]) ],
       [ (demand(Args, Salt) <+ watch_request(_, Args, Salt)),
         (response(Args, Salt, Payload) <+ fill(Args, Salt, Payload), latest(demand(Args, Salt))) ]),
  [],
  [ [ +watch_request(alice, src, sha_aaa), +watch_request(bob, src, sha_aaa) ],
    [ +fill(src, sha_aaa, payload_one) ] ],
  [ final(demand/2,   [ demand(src, sha_aaa) ]),
    final(response/3, [ response(src, sha_aaa, payload_one) ]) ]).

% ═══ the two-salt law: same salt dedups, a new salt is a fresh row ══════════
%
% Salt is an ordinary column value carried in the arriving watch_request row,
% not engine machinery: a clock bucket salts a poll (recurs because time
% passed), an input digest salts an extraction (recurs because the tree
% changed). Tick 2 replays the identical (Args, Salt) pair and stays at one
% row (the equal-row no-op again); tick 3 changes only Salt and the keyed
% write lands as a genuinely new key, so the row count grows instead of
% replacing.

fixture(new_salt_refires_fresh_stream,
  prog([ kind(watch_request/3, log), keep(watch_request/3, all),
         keyed(demand/2, [1, 2]) ],
       [ (demand(Args, Salt) <+ watch_request(_, Args, Salt)) ]),
  [],
  [ [ +watch_request(alice, src, sha_aaa) ],
    [ +watch_request(bob,   src, sha_aaa) ],
    [ +watch_request(carol, src, sha_bbb) ] ],
  [ final(demand/2, [ demand(src, sha_aaa), demand(src, sha_bbb) ]) ]).

% REJECTED READING: salting demand rows with the ARRIVAL TICK instead of a
% clock bucket (polls) or an input digest (extraction) reintroduces the
% 720-vs-12 calls/hour failure AUDIT finding 9 measured: a demand row keyed
% by (Args, ArrivalTick) never dedups across ticks, so every tick that merely
% re-observes an unchanged input mints a fresh demand/fresh fill instead of
% the polling-period-shaped or change-shaped recurrence the two real salts
% give for free. Ruling: lab-consolidation PROVEN 4
% (plans/2026-07-27-aggregate-analysis.md:98, :165; labs/shell_stream.md
% sect 4, lab deleted, last copy at 2fff3f61). Two salts
% exist for two different recurrence reasons and neither is "time elapsed
% since the engine happened to tick"; arrival-tick salt is rejected outright,
% not re-graded into a fixture of its own.

% ═══ terminal_is_terminal: the split envelope flips a level view ═══════════
%
% ExtractEvent (item) and ExtractEnd (terminal) are SPLIT enums, modeled as
% two plain-term Log rels: `stream_item` arrives once per line, `stream_end`
% arrives exactly once and never again. `stream_status` is the level VIEW a
% downstream consumer reads: `running` while items exist and no terminal has
% landed, `done` once the terminal lands. The graded delta trace is the flip
% itself (retraction of running, assertion of done) at the tick the terminal
% arrives, with no boundary noise on the middle tick where a second item
% arrives and the view does not change.

fixture(terminal_is_terminal,
  prog([ kind(stream_item/3, log), keep(stream_item/3, all),
         kind(stream_end/2, log),  keep(stream_end/2, all) ],
       [ (stream_status(Args, running) <- stream_item(Args, _, _), not(stream_end(Args, _))),
         (stream_status(Args, done)    <- stream_end(Args, _)) ]),
  [],
  [ [ +stream_item(src, 1, alpha) ],
    [ +stream_item(src, 2, beta) ],
    [ +stream_end(src, done(0)) ] ],
  [ deltas(stream_status/2, [ [ +stream_status(src, running) ],
                              [],
                              [ -stream_status(src, running), +stream_status(src, done) ] ]) ]).

% ═══ live_nonzero_exit_keeps_rows: exit(nonzero) as a terminal VALUE ════════
%
% No process spawns; a nonzero exit is simply the `err(Message)` arm of the
% same stream_end rel used above, delivered as a canned arrival exactly as
% the bind's `match code { 0 => Done{code}, c => Err{msg} }` would have
% written it. The two item rows collected before the failing exit are never
% retracted (Log rels never retract; occurrences cannot un-happen), so the
% final rows include both the surviving items and the error terminal.

fixture(live_nonzero_exit_keeps_rows,
  prog([ kind(stream_item/3, log), keep(stream_item/3, all),
         kind(stream_end/2, log),  keep(stream_end/2, all) ],
       []),
  [],
  [ [ +stream_item(src, 1, alpha) ],
    [ +stream_item(src, 2, beta) ],
    [ +stream_end(src, err(sprefa_extract_exited_3)) ] ],
  [ final(stream_item/3, [ stream_item(src, 1, alpha), stream_item(src, 2, beta) ]),
    final(stream_end/2,  [ stream_end(src, err(sprefa_extract_exited_3)) ]) ]).
