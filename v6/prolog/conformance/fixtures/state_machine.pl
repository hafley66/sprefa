% fixtures/state_machine.pl : the rxjs-jutsu receipt. An async state machine
% with pattern-matching scans, written entirely in already-ruled constructs:
% keyed rels are the state registers, edge rules with envelope-constructor
% patterns are the match arms, pre + := is the scan, a level view over fold
% state is the derived output, and state-derived demand rides Set-rel dedup.
% The redux/xstate shape (combineReducers = mergeByKeyScan) with effects.
% Owner: coordinator.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% The machine, ghcacher-flavored: one fetch lifecycle per endpoint.
%   phases: idle -> fetching (on poll_due) -> idle (on fresh/unchanged)
%                                          -> idle + retries+1 (on error)
%   fetch_wanted: a LEVEL view over the fetching phase = the demand surface
%     (asserts on entry, retracts on exit, dedups by membership).
%   cache: keyed replace on the fresh arm ONLY (pattern match selects).
%   gave_up: level view over the retries fold (asserts at >= 2).
% Envelope arms are matched STRUCTURALLY in trigger position:
% fetch_result(E, fresh(Tag, Body)) vs (E, unchanged) vs (E, error(_)) are
% three rules, one per constructor, exactly enum-match-as-rules.
fixture(async_state_machine_with_pattern_scan,
  prog([ kind(poll_due/1, log),     keep(poll_due/1, all),
         kind(fetch_result/2, log), keep(fetch_result/2, all),
         keyed(phase/2, [1]),
         keyed(cache/3, [1]),
         keyed(retries/2, [1]) ],
       [ (phase(Endpoint, fetching) <+
            only(poll_due(Endpoint)), pre(phase(Endpoint, idle))),

         (phase(Endpoint, idle) <+
            only(fetch_result(Endpoint, fresh(_, _))), pre(phase(Endpoint, fetching))),
         (phase(Endpoint, idle) <+
            only(fetch_result(Endpoint, unchanged)), pre(phase(Endpoint, fetching))),
         (phase(Endpoint, idle) <+
            only(fetch_result(Endpoint, error(_))), pre(phase(Endpoint, fetching))),

         (cache(Endpoint, Tag, Body) <+
            only(fetch_result(Endpoint, fresh(Tag, Body)))),

         (retries(Endpoint, Next) <+
            only(fetch_result(Endpoint, error(_))),
            pre(retries(Endpoint, SoFar)), Next := SoFar + 1),
         (retries(Endpoint, 0) <+
            only(fetch_result(Endpoint, fresh(_, _)))),

         (fetch_wanted(Endpoint) <- phase(Endpoint, fetching)),
         (gave_up(Endpoint) <- retries(Endpoint, Count), Count >= 2) ]),
  [ phase(gh_repos, idle), retries(gh_repos, 0) ],
  [ [ +poll_due(gh_repos) ],
    [ +fetch_result(gh_repos, fresh(tag_v1, body_one)) ],
    [ +poll_due(gh_repos) ],
    [ +fetch_result(gh_repos, error(500)) ],
    [ +poll_due(gh_repos) ],
    [ +fetch_result(gh_repos, error(502)) ] ],
  [ % the demand view tracks the phase register: assert on entry, retract on
    % exit, three full cycles.
    deltas(fetch_wanted/1,
      [ [ +fetch_wanted(gh_repos) ], [ -fetch_wanted(gh_repos) ],
        [ +fetch_wanted(gh_repos) ], [ -fetch_wanted(gh_repos) ],
        [ +fetch_wanted(gh_repos) ], [ -fetch_wanted(gh_repos) ], [] ]),
    % the fresh arm wrote the cache once; the error arms never touched it.
    final(cache/3, [ cache(gh_repos, tag_v1, body_one) ]),
    % the scan: 0 -> reset 0 (equal-row no-op) -> 1 -> 2.
    deltas(retries/2,
      [ [], [], [], [ -retries(gh_repos, 0), +retries(gh_repos, 1) ],
        [], [ -retries(gh_repos, 1), +retries(gh_repos, 2) ], [] ]),
    % the derived alarm crosses its threshold on the second error.
    deltas(gave_up/1, [ [], [], [], [], [], [ +gave_up(gh_repos) ], [] ]),
    final(phase/2, [ phase(gh_repos, idle) ]) ]).

% The two-events-one-tick stress: an error and the recovering fresh arrive in
% ONE tick, in that order. Under q1 stamps the occurrences chain: the error
% arm bumps retries 0 -> 1, then the fresh arm resets to 0; the boundary
% shows the NET no-op on retries (R2 rider) while cache still lands. This is
% rxjs scan semantics surviving batching, the exact place the pre-ruling
% surface undercounted.
fixture(same_tick_error_then_fresh_chains_arms,
  prog([ kind(fetch_result/2, log), keep(fetch_result/2, all),
         keyed(cache/3, [1]),
         keyed(retries/2, [1]) ],
       [ (retries(Endpoint, Next) <+
            only(fetch_result(Endpoint, error(_))),
            pre(retries(Endpoint, SoFar)), Next := SoFar + 1),
         (retries(Endpoint, 0) <+
            only(fetch_result(Endpoint, fresh(_, _)))),
         (cache(Endpoint, Tag, Body) <+
            only(fetch_result(Endpoint, fresh(Tag, Body)))) ]),
  [ retries(gh_repos, 0) ],
  [ [ +fetch_result(gh_repos, error(500)),
      +fetch_result(gh_repos, fresh(tag_v2, body_two)) ] ],
  [ deltas(retries/2, [ [], [] ]),
    final(retries/2, [ retries(gh_repos, 0) ]),
    final(cache/3, [ cache(gh_repos, tag_v2, body_two) ]) ]).
