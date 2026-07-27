% mode_lab.pl : (cardinality, lifetime) mode analysis with dominance.
%
% Run:  swipl -q -l v6/prolog/labs/mode_lab.pl -g go -g halt
%       swipl -q -l v6/prolog/labs/mode_lab.pl -g report -g halt
%
% THE TASK (ARCH.pl: algorithm(mode_analysis, static_subs, fold, unbuilt);
% task(mode_lab)). Every ask carries a two-column mode:
%
%   mode(Ask, card(det | semidet | multi), lifetime(finite | until(S) | never))
%
% This is a STATIC analysis. It computes modes over a program graph plus the
% link-time bind facts. It runs no ticks, spawns no processes, and reads no
% runtime rows except to grade the static-vs-runtime distinction in section 9.
%
% WHAT THIS LAB FIXES IN plans/2026-07-27-mode-dominance.md (AUDIT finding 13):
%
% 1. The plan writes `min` for two different operations pointing opposite ways.
%    They are named here and they are NOT the same operator:
%
%      scope_min  dominance / switch_map nesting. The inner ends when its own
%                 binding ends OR the enclosing scope ends. Disjunction.
%      join_max   a rule body joining several inputs. The join keeps deriving
%                 until EVERY input has stopped producing. Conjunction.
%
% 2. `until(S1)` vs `until(S2)` is not an order question. Lifetime is the free
%    distributive lattice over a set of end-signals:
%
%      finite    = the formula TRUE   (something always ends it)
%      never     = the formula FALSE  (nothing ever ends it)
%      until(F)  = the monotone boolean formula F over signal names
%
%    scope_min = OR, join_max = AND, and both are total, commutative,
%    associative, idempotent and mutually distributive on the canonical form
%    (DNF as an antichain of clauses). finite is the identity of join_max and
%    the annihilator of scope_min; never is the identity of scope_min and the
%    annihilator of join_max. until(a) and until(b) are INCOMPARABLE, and the
%    lattice never has to compare them: it combines them.
%
% 3. Mode analysis is a POST-LINK pass. `every` is a bind, not a program
%    construct, so a rel's lifetime is not decided until a bind is attached.
%    An unlinked program has no modes (graded: reject(no_bind_for(_))), and
%    relinking the same program text flips `change_log` from never to finite
%    (graded: relink_flips_change_log_to_finite).
%
% 4. Static lifetime and runtime lifetime are different objects. The static
%    lifetime answers "will this ask complete on its own". A runtime
%    subscription can END without completing, by teardown (the range-DELETE
%    of its demand-row path prefix). Section 9 grades both directions.
%
% ENCODING. Program text is facts; the analysis is a fold over the rel graph
% run to a least fixpoint (the graph is cyclic in ghcacher: poll -> fetch ->
% cache -> cache_tag -> poll), with dominance applied at every step. No
% sqlite, no rx, no tick engine.

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).
:- use_module('../src/grader.pl').

:- discontiguous rel/4.
:- discontiguous fact_rel/2.
:- discontiguous effect_sig/4.
:- discontiguous bind/3.
:- discontiguous unbind/2.
:- discontiguous register_rel/3.
:- discontiguous rule/5.
:- discontiguous demand_rule/3.
:- discontiguous scope/4.
:- discontiguous scope_edge/2.
:- discontiguous in_scope/3.
:- discontiguous ask/5.
:- discontiguous keep/3.
:- discontiguous parent_program/2.
:- discontiguous runtime_sub/4.

% ═══════════════════════════════════════════════════════════════════════════
% 1. THE LIFETIME LATTICE: two operators, one canonical form
% ═══════════════════════════════════════════════════════════════════════════
%
% Canonical form: finite | never | until(Clauses), where Clauses is a sorted
% list of sorted signal lists (DNF), reduced to an antichain by absorption.
% An empty disjunction is FALSE = never; a clause that is the empty
% conjunction is TRUE = finite.

until_signal(Signal, until([[Signal]])).

dnf_lifetime(Clauses, Lifetime) :-
    (   Clauses == []          -> Lifetime = never
    ;   memberchk([], Clauses) -> Lifetime = finite
    ;   Lifetime = until(Clauses)
    ).

normalize_clauses(RawClauses, Normalized) :-
    maplist(sort, RawClauses, PerClauseSorted),
    sort(PerClauseSorted, UniqueClauses),
    exclude(strictly_absorbed(UniqueClauses), UniqueClauses, Normalized).

strictly_absorbed(AllClauses, Clause) :-
    member(OtherClause, AllClauses),
    OtherClause \== Clause,
    ord_subset(OtherClause, Clause).

% scope_min : DOMINANCE. lifetime(inner) = scope_min(own binding, scope).
% Either end-condition ends the inner stream, so this is OR on formulas.
scope_min(Left, Right, Result) :-
    (   Left == finite  -> Result = finite
    ;   Right == finite -> Result = finite
    ;   Left == never   -> Result = Right
    ;   Right == never  -> Result = Left
    ;   Left = until(LeftClauses), Right = until(RightClauses),
        append(LeftClauses, RightClauses, Combined),
        normalize_clauses(Combined, Normalized),
        dnf_lifetime(Normalized, Result)
    ).

% join_max : a rule body (or a rel's several rules). New derivations keep
% arriving until EVERY input has stopped, so this is AND on formulas.
join_max(Left, Right, Result) :-
    (   Left == never   -> Result = never
    ;   Right == never  -> Result = never
    ;   Left == finite  -> Result = Right
    ;   Right == finite -> Result = Left
    ;   Left = until(LeftClauses), Right = until(RightClauses),
        findall(Product,
                ( member(LeftClause, LeftClauses),
                  member(RightClause, RightClauses),
                  append(LeftClause, RightClause, Product) ),
                Products),
        normalize_clauses(Products, Normalized),
        dnf_lifetime(Normalized, Result)
    ).

scope_min_all(Lifetimes, Result) :- foldl(scope_min_flip, Lifetimes, never, Result).
scope_min_flip(Next, Accumulated, Result) :- scope_min(Accumulated, Next, Result).

join_max_all(Lifetimes, Result) :- foldl(join_max_flip, Lifetimes, finite, Result).
join_max_flip(Next, Accumulated, Result) :- join_max(Accumulated, Next, Result).

% The order, derived from the operators rather than asserted as a chain.
% Left ends no later than Right.
lifetime_leq(Left, Right) :-
    join_max(Left, Right, JoinResult),  JoinResult  == Right,
    scope_min(Left, Right, ScopeResult), ScopeResult == Left.

lifetime_incomparable(Left, Right) :-
    \+ lifetime_leq(Left, Right),
    \+ lifetime_leq(Right, Left).

% Readable spelling for the table. until([[disconnect]]) prints as
% until(disconnect); a multi-clause DNF prints as any_of / all_of.
pretty_lifetime(finite, finite).
pretty_lifetime(never, never).
pretty_lifetime(undetermined, undetermined).
pretty_lifetime(until(Clauses), until(Pretty)) :-
    (   Clauses = [SingleClause]
    ->  pretty_clause(SingleClause, Pretty)
    ;   maplist(pretty_clause, Clauses, PrettyClauses),
        Pretty = any_of(PrettyClauses)
    ).

pretty_clause([SingleSignal], SingleSignal) :- !.
pretty_clause(Signals, all_of(Signals)).

% Sample lifetimes the algebraic checks quantify over.
sample_lifetime(finite).
sample_lifetime(never).
sample_lifetime(until([[disconnect]])).
sample_lifetime(until([[outer_next]])).
sample_lifetime(until([[disconnect], [document_closed]])).
sample_lifetime(until([[disconnect, outer_next]])).

% ═══════════════════════════════════════════════════════════════════════════
% 2. CARDINALITY: Mercury's determinism, read off keys and result types
% ═══════════════════════════════════════════════════════════════════════════
%
% det     exactly 1. A det envelope effect: failure is a VALUE (the Error
%         arm), so the call cannot NOT produce a row.
% semidet 0 or 1. A keyed read with every key column bound: the key is a
%         primary key, so the functional dependency caps the answer at one.
% multi   many. Unkeyed read, tail ask, Stream/Tail result type.
%
% Conjunction is max on this chain. Mercury would call semidet AND multi
% `nondet` (0 or many); the plan's mode type has no nondet cell, so it
% flattens to multi. See mode_lab.md ambiguity 5.

card_rank(det, 1).
card_rank(semidet, 2).
card_rank(multi, 3).

card_join(Left, Right, Result) :-
    card_rank(Left, LeftRank),
    card_rank(Right, RightRank),
    (   LeftRank >= RightRank -> Result = Left ; Result = Right ).

card_join_all(Cards, Result) :- foldl(card_join_flip, Cards, det, Result).
card_join_flip(Next, Accumulated, Result) :- card_join(Accumulated, Next, Result).

% ═══════════════════════════════════════════════════════════════════════════
% 3. LINK-TIME FACTS: protocols and result types
% ═══════════════════════════════════════════════════════════════════════════
%
% protocol_lifetime is the POST-LINK truth. result_lifetime is the DECLARED
% claim the program text makes. The link obligation (T5, shell_stream.md
% section 2) is that the bind's lifetime is <= the declared lifetime: a bind
% may end sooner than the signature promises, never later. tail -f into a
% Stream-typed rel is therefore a link error, checked with lifetime_leq.

protocol_lifetime(shell(_),    finite).
protocol_lifetime(every(_),    never).
protocol_lifetime(tail_f(_),   never).
protocol_lifetime(watch_fs(_), never).
protocol_lifetime(sse(_),      until([[disconnect]])).

result_lifetime(envelope(_),  finite).
result_lifetime(stream(_, _), finite).
result_lifetime(tail(_),      never).

result_card(envelope(_),  det).
result_card(stream(_, _), multi).
result_card(tail(_),      multi).

% ═══════════════════════════════════════════════════════════════════════════
% 4. PROGRAM REPRESENTATION (facts). Programs may inherit and override.
% ═══════════════════════════════════════════════════════════════════════════
%
%   rel(Program, Name, Kind, Columns)         Kind = set | log
%     Columns = [col(Name, Type)]; Type = key(T) marks a key column.
%   fact_rel(Program, Name)                   bodiless clauses: finite rows
%   effect_sig(Program, Name, ProgramColumns, ResultType)
%     ResultType = envelope(Enum) | stream(ItemEnum, EndEnum) | tail(ItemEnum)
%   bind(Program, EffectName, Protocol)       LINK time; exactly one per effect
%   unbind(Program, EffectName)               suppress an inherited bind
%   register_rel(Program, Name, OverEffect)   register: lifetime of its `over`
%   rule(Program, RuleId, HeadRel, RuleKind, BodyAtoms)
%     RuleKind = level | edge; BodyAtoms = rel(N) | keyed(N) | departed(N)
%                                        | guard(Text)
%   demand_rule(Program, EffectName, SourceRel)
%     the rel whose rows ARE the effect's demand rows (magic-set rows)
%   scope(Program, ScopeName, EndSignal, ParentScope)
%   scope_edge(Program, switch_map(OuterRel, ScopeName))
%   in_scope(Program, Node, ScopeName)        Node = rel name or ask id
%   ask(Program, AskId, Form, TargetRel, BoundColumns)
%     Form = request | snapshot | tail | write
%   keep(Program, RelName, Bound)             q10 retention: STORAGE only

% ── ghcacher: the plan's five grading cases ────────────────────────────────

rel(ghcacher, watch,      set, [col(endpoint, key(url))]).
rel(ghcacher, cache,      set, [col(endpoint, key(url)), col(entry, entry)]).
rel(ghcacher, cache_tag,  set, [col(endpoint, key(url)), col(tag, tag)]).
rel(ghcacher, cache_body, set, [col(endpoint, key(url)), col(body, str)]).
rel(ghcacher, poll,       set, [col(endpoint, url), col(prev, tag), col(bucket, int)]).
rel(ghcacher, stars,      set, [col(endpoint, url), col(count, int)]).
rel(ghcacher, change_log, log, [col(endpoint, url), col(kind, str), col(value, str)]).
rel(ghcacher, fetch,      set, [col(endpoint, key(url)), col(prev, key(tag)), col(result, fetch_result)]).
rel(ghcacher, every_300,  log, [col(bucket, int)]).

fact_rel(ghcacher, watch).

effect_sig(ghcacher, fetch,     [endpoint, prev], envelope(fetch_result)).
effect_sig(ghcacher, every_300, [],               tail(tick_event)).

bind(ghcacher, fetch,     shell('curl -s -H "If-None-Match: {prev}" {endpoint}')).
bind(ghcacher, every_300, every(300)).

register_rel(ghcacher, cache, fetch).

rule(ghcacher, r_poll,       poll,       level, [rel(watch), rel(cache_tag), rel(every_300)]).
rule(ghcacher, r_cache_tag,  cache_tag,  level, [rel(cache)]).
rule(ghcacher, r_cache_body, cache_body, level, [rel(cache)]).
rule(ghcacher, r_stars,      stars,      level, [rel(cache_body)]).
rule(ghcacher, r_change_log, change_log, level, [rel(stars)]).

demand_rule(ghcacher, fetch, poll).

keep(ghcacher, change_log, all).

ask(ghcacher, ask_fetch_request,   request,  fetch,      [endpoint, prev]).
ask(ghcacher, ask_cache_bound,     snapshot, cache,      [endpoint]).
ask(ghcacher, ask_change_log_tail, tail,     change_log, []).

runtime_sub(ghcacher, sub_fetch_1,      ask_fetch_request,   ended(complete)).
runtime_sub(ghcacher, sub_change_log_1, ask_change_log_tail, ended(teardown)).

% ── ghcacher_scoped: the SAME program under a switch_map scope ─────────────
% The scope constructor dominates the timer, which is the whole of plan case 4
% and the second half of plan case 5.

parent_program(ghcacher_scoped, ghcacher).
rel(ghcacher_scoped, subscriber, set, [col(client, key(client_id))]).
fact_rel(ghcacher_scoped, subscriber).
scope(ghcacher_scoped, poll_scope, outer_next, root).
scope_edge(ghcacher_scoped, switch_map(subscriber, poll_scope)).
in_scope(ghcacher_scoped, every_300, poll_scope).

% ── ghcacher_unlinked: no bind for the clock. Post-link means NO MODE. ─────

parent_program(ghcacher_unlinked, ghcacher).
unbind(ghcacher_unlinked, every_300).

% ── ghcacher_relinked: same text, the clock bound to a finishing shell ─────
% AUDIT finding 13's third gap, made concrete: relinking flips the answer.
% Legal because the bind's lifetime (finite) is <= the declared tail(never).

parent_program(ghcacher_relinked, ghcacher).
bind(ghcacher_relinked, every_300, shell('seq 1 10')).

% ── eventing: the five ask rows from check_eventing.md ─────────────────────

rel(eventing, source_file,  set, [col(path, key(str))]).
rel(eventing, file_change,  log, [col(path, str)]).
rel(eventing, diagnostic,   set, [col(path, str), col(line, int), col(code, str), col(severity, str)]).
rel(eventing, diag_history, log, [col(path, str), col(line, int), col(code, str), col(opened_at, int)]).
rel(eventing, violation,    set, [col(code, key(str)), col(count, int), col(allowed, int)]).
rel(eventing, hook_window,  set, [col(turn, key(turn_id)), col(since, tick)]).
rel(eventing, turn_diag,    set, [col(turn, turn_id), col(path, str), col(line, int),
                                  col(code, str), col(opened_at, int)]).

fact_rel(eventing, source_file).
effect_sig(eventing, file_change, [], tail(fs_event)).
bind(eventing, file_change, watch_fs('.')).

rule(eventing, r_diagnostic,   diagnostic,   level, [rel(source_file), rel(file_change)]).
rule(eventing, r_diag_history, diag_history, edge,  [rel(diagnostic), guard('now(opened_at)')]).
rule(eventing, r_violation,    violation,    level, [rel(diagnostic)]).
rule(eventing, r_turn_diag,    turn_diag,    level, [keyed(hook_window), rel(diagnostic),
                                                     rel(diag_history), guard('opened_at > since')]).

keep(eventing, diag_history, count(5000)).

scope(eventing, lsp_connection, disconnect,       root).
scope(eventing, open_document,  document_closed,  lsp_connection).
scope_edge(eventing, switch_map(source_file, open_document)).
in_scope(eventing, ask_lsp_tail, open_document).

ask(eventing, ask_hook_write,      write,    hook_window,  [turn]).
ask(eventing, ask_hook_snapshot,   snapshot, turn_diag,    [turn]).
ask(eventing, ask_lsp_tail,        tail,     diagnostic,   []).
ask(eventing, ask_commit_gate,     snapshot, violation,    []).
ask(eventing, ask_dashboard_tail,  tail,     diag_history, []).

runtime_sub(eventing, sub_lsp_1,       ask_lsp_tail,       open).
runtime_sub(eventing, sub_dashboard_1, ask_dashboard_tail, ended(teardown)).

% ── shell_modes: the three result-type cells, including (multi, finite) ────

rel(shell_modes, fetch,    set, [col(endpoint, key(url)), col(result, fetch_result)]).
rel(shell_modes, extract,  log, [col(args, str), col(salt, digest)]).
rel(shell_modes, log_tail, log, [col(path, str)]).

effect_sig(shell_modes, fetch,    [endpoint],    envelope(fetch_result)).
effect_sig(shell_modes, extract,  [args, salt],  stream(extract_event, extract_end)).
effect_sig(shell_modes, log_tail, [path],        tail(log_event)).

bind(shell_modes, fetch,    shell('curl -s {endpoint}')).
bind(shell_modes, extract,  shell('sprefa-extract {args}')).
bind(shell_modes, log_tail, tail_f('{path}')).

ask(shell_modes, ask_fetch_env,     request, fetch,    [endpoint]).
ask(shell_modes, ask_extract_lines, request, extract,  [args, salt]).
ask(shell_modes, ask_log_tail,      tail,    log_tail, [path]).

% ── shell_mislinked: tail -f attached to a Stream-typed rel = link error ───

rel(shell_mislinked, extract, log, [col(args, str)]).
effect_sig(shell_mislinked, extract, [args], stream(extract_event, extract_end)).
bind(shell_mislinked, extract, tail_f('/var/log/app.log')).
ask(shell_mislinked, ask_extract_lines, request, extract, [args]).

% ── timer_alone / timer_scoped: the dominance flip in isolation ────────────

rel(timer_alone, every_300, log, [col(bucket, int)]).
effect_sig(timer_alone, every_300, [], tail(tick_event)).
bind(timer_alone, every_300, every(300)).
ask(timer_alone, ask_timer_tail, tail, every_300, []).

parent_program(timer_scoped, timer_alone).
scope(timer_scoped, outer_scope, outer_next, root).
scope_edge(timer_scoped, switch_map(outer_source, outer_scope)).
in_scope(timer_scoped, every_300, outer_scope).
in_scope(timer_scoped, ask_timer_tail, outer_scope).

% ── audit_join: the constructed case from AUDIT finding 13 ─────────────────
% job(name, bucket) <- config(name), timer(bucket).
% `min` says finite, which is false: job gains a row on every timer bucket.
% `join_max` says never, which is right.

rel(audit_join, config, set, [col(name, key(str))]).
rel(audit_join, timer,  log, [col(bucket, int)]).
rel(audit_join, job,    set, [col(name, str), col(bucket, int)]).
fact_rel(audit_join, config).
effect_sig(audit_join, timer, [], tail(tick_event)).
bind(audit_join, timer, every(60)).
rule(audit_join, r_job, job, level, [rel(config), rel(timer)]).
ask(audit_join, ask_job_tail, tail, job, []).

% ── fork_join family: N det envelopes joined conjunctively ─────────────────

rel(fork_join_det, fetch_a,  set, [col(id, key(int)), col(result, fetch_result)]).
rel(fork_join_det, fetch_b,  set, [col(id, key(int)), col(result, fetch_result)]).
rel(fork_join_det, fetch_c,  set, [col(id, key(int)), col(result, fetch_result)]).
rel(fork_join_det, combined, set, [col(id, key(int)), col(body_a, str), col(body_b, str), col(body_c, str)]).
effect_sig(fork_join_det, fetch_a, [id], envelope(fetch_result)).
effect_sig(fork_join_det, fetch_b, [id], envelope(fetch_result)).
effect_sig(fork_join_det, fetch_c, [id], envelope(fetch_result)).
bind(fork_join_det, fetch_a, shell('curl -s /a/{id}')).
bind(fork_join_det, fetch_b, shell('curl -s /b/{id}')).
bind(fork_join_det, fetch_c, shell('curl -s /c/{id}')).
rule(fork_join_det, r_combined, combined, level,
     [keyed(fetch_a), keyed(fetch_b), keyed(fetch_c)]).
ask(fork_join_det, ask_combined, snapshot, combined, [id]).

% one input is a never-timer: the conjunction becomes never
rel(fork_join_timer, fetch_a,  set, [col(id, key(int)), col(result, fetch_result)]).
rel(fork_join_timer, fetch_b,  set, [col(id, key(int)), col(result, fetch_result)]).
rel(fork_join_timer, every_60, log, [col(bucket, int)]).
rel(fork_join_timer, combined, set, [col(id, int), col(bucket, int)]).
effect_sig(fork_join_timer, fetch_a,  [id], envelope(fetch_result)).
effect_sig(fork_join_timer, fetch_b,  [id], envelope(fetch_result)).
effect_sig(fork_join_timer, every_60, [],   tail(tick_event)).
bind(fork_join_timer, fetch_a,  shell('curl -s /a/{id}')).
bind(fork_join_timer, fetch_b,  shell('curl -s /b/{id}')).
bind(fork_join_timer, every_60, every(60)).
rule(fork_join_timer, r_combined, combined, level,
     [keyed(fetch_a), keyed(fetch_b), rel(every_60)]).
ask(fork_join_timer, ask_combined, tail, combined, []).

% the same join under a scope: never gets dominated back to until()
parent_program(fork_join_timer_scoped, fork_join_timer).
scope(fork_join_timer_scoped, request_scope, outer_next, root).
scope_edge(fork_join_timer_scoped, switch_map(inbound_request, request_scope)).
in_scope(fork_join_timer_scoped, combined, request_scope).
in_scope(fork_join_timer_scoped, ask_combined, request_scope).

% a keyed state rel in the conjunction drags det down to semidet
rel(fork_join_semidet, fetch_a,  set, [col(id, key(int)), col(result, fetch_result)]).
rel(fork_join_semidet, cache,    set, [col(id, key(int)), col(entry, entry)]).
rel(fork_join_semidet, combined, set, [col(id, key(int)), col(body, str), col(entry, entry)]).
effect_sig(fork_join_semidet, fetch_a, [id], envelope(fetch_result)).
bind(fork_join_semidet, fetch_a, shell('curl -s /a/{id}')).
rule(fork_join_semidet, r_combined, combined, level, [keyed(fetch_a), keyed(cache)]).

% ── departure: ruling r4, departed/1 as a body form ────────────────────────

rel(departure, watch,      set, [col(endpoint, key(url))]).
rel(departure, unwatched,  log, [col(endpoint, url)]).
rel(departure, diagnostic, set, [col(path, str), col(code, str)]).
rel(departure, cleared,    log, [col(path, str), col(code, str)]).
rel(departure, file_change, log, [col(path, str)]).
fact_rel(departure, watch).
effect_sig(departure, file_change, [], tail(fs_event)).
bind(departure, file_change, watch_fs('.')).
rule(departure, r_unwatched,  unwatched,  edge,  [departed(watch)]).
rule(departure, r_diagnostic, diagnostic, level, [rel(file_change)]).
rule(departure, r_cleared,    cleared,    edge,  [departed(diagnostic)]).

% ── retention: q10 keep bounds are STORAGE, not subscription lifetime ──────

rel(retention_unbounded, feed, log, [col(value, int)]).
rel(retention_unbounded, tick, log, [col(bucket, int)]).
effect_sig(retention_unbounded, tick, [], tail(tick_event)).
bind(retention_unbounded, tick, every(1)).
rule(retention_unbounded, r_feed, feed, edge, [rel(tick)]).
keep(retention_unbounded, feed, all).
ask(retention_unbounded, ask_feed_tail, tail, feed, []).

rel(retention_bounded, feed, log, [col(value, int)]).
rel(retention_bounded, tick, log, [col(bucket, int)]).
effect_sig(retention_bounded, tick, [], tail(tick_event)).
bind(retention_bounded, tick, every(1)).
rule(retention_bounded, r_feed, feed, edge, [rel(tick)]).
keep(retention_bounded, feed, count(100)).
ask(retention_bounded, ask_feed_tail, tail, feed, []).

% ── sse_out: the until(disconnect) base case ───────────────────────────────

rel(sse_out, change_log, log, [col(endpoint, url), col(kind, str)]).
rel(sse_out, wire,       log, [col(client, client_id), col(row, str)]).
rel(sse_out, connection, log, [col(client, client_id)]).
fact_rel(sse_out, change_log).
effect_sig(sse_out, connection, [], tail(connection_event)).
bind(sse_out, connection, sse('/events')).
rule(sse_out, r_wire, wire, level, [rel(connection), rel(change_log)]).
ask(sse_out, ask_wire_tail, tail, wire, []).

all_programs([ghcacher, ghcacher_scoped, ghcacher_unlinked, ghcacher_relinked,
              eventing, shell_modes, shell_mislinked, timer_alone, timer_scoped,
              audit_join, fork_join_det, fork_join_timer, fork_join_timer_scoped,
              fork_join_semidet, departure, retention_unbounded, retention_bounded,
              sse_out]).

% ═══════════════════════════════════════════════════════════════════════════
% 5. PROGRAM RESOLUTION: inheritance with nearest-wins override
% ═══════════════════════════════════════════════════════════════════════════

program_chain(Program, [Program | Rest]) :-
    (   parent_program(Program, Parent)
    ->  program_chain(Parent, Rest)
    ;   Rest = []
    ).

p_rel(Program, Name, Kind, Columns) :-
    program_chain(Program, Chain), member(Owner, Chain), rel(Owner, Name, Kind, Columns).

p_fact_rel(Program, Name) :-
    program_chain(Program, Chain), member(Owner, Chain), fact_rel(Owner, Name).

p_effect_name(Program, Name) :-
    program_chain(Program, Chain),
    setof(EffectName,
          Owner^Columns^Result^( member(Owner, Chain),
                                 effect_sig(Owner, EffectName, Columns, Result) ),
          Names),
    member(Name, Names).

p_effect_sig(Program, Name, Columns, ResultType) :-
    p_effect_name(Program, Name),
    program_chain(Program, Chain),
    once(( member(Owner, Chain), effect_sig(Owner, Name, Columns, ResultType) )).

p_bind(Program, Effect, Protocol) :-
    program_chain(Program, Chain),
    \+ ( member(Suppressor, Chain), unbind(Suppressor, Effect) ),
    once(( member(Owner, Chain), bind(Owner, Effect, Protocol) )).

p_register(Program, Name, Over) :-
    program_chain(Program, Chain), member(Owner, Chain), register_rel(Owner, Name, Over).

p_rule(Program, RuleId, Head, Kind, Body) :-
    program_chain(Program, Chain), member(Owner, Chain), rule(Owner, RuleId, Head, Kind, Body).

p_demand_rule(Program, Effect, Source) :-
    program_chain(Program, Chain), member(Owner, Chain), demand_rule(Owner, Effect, Source).

p_scope(Program, ScopeName, Signal, Parent) :-
    program_chain(Program, Chain), member(Owner, Chain), scope(Owner, ScopeName, Signal, Parent).

p_in_scope(Program, Node, ScopeName) :-
    program_chain(Program, Chain), member(Owner, Chain), in_scope(Owner, Node, ScopeName).

p_scope_edge(Program, Edge) :-
    program_chain(Program, Chain), member(Owner, Chain), scope_edge(Owner, Edge).

p_ask(Program, AskId, Form, Target, Bound) :-
    program_chain(Program, Chain), member(Owner, Chain), ask(Owner, AskId, Form, Target, Bound).

p_keep(Program, Name, Bound) :-
    program_chain(Program, Chain), member(Owner, Chain), keep(Owner, Name, Bound).

p_runtime_sub(Program, SubId, AskId, Status) :-
    program_chain(Program, Chain), member(Owner, Chain), runtime_sub(Owner, SubId, AskId, Status).

% ═══════════════════════════════════════════════════════════════════════════
% 6. SCOPES: the dominance input
% ═══════════════════════════════════════════════════════════════════════════

scope_lifetime(_, root, never) :- !.
scope_lifetime(Program, ScopeName, Lifetime) :-
    p_scope(Program, ScopeName, EndSignal, ParentScope),
    scope_lifetime(Program, ParentScope, ParentLifetime),
    until_signal(EndSignal, OwnLifetime),
    scope_min(OwnLifetime, ParentLifetime, Lifetime).

% A node in several scopes ends when ANY of them ends, so scope_min again,
% with never (the ambient scope) as the identity.
node_scope_lifetime(Program, Node, Lifetime) :-
    findall(ScopeLifetime,
            ( p_in_scope(Program, Node, ScopeName),
              scope_lifetime(Program, ScopeName, ScopeLifetime) ),
            ScopeLifetimes),
    (   ScopeLifetimes == []
    ->  Lifetime = never
    ;   scope_min_all(ScopeLifetimes, Lifetime)
    ).

% ═══════════════════════════════════════════════════════════════════════════
% 7. THE ANALYSIS: a fold over the rel graph, run to a least fixpoint
% ═══════════════════════════════════════════════════════════════════════════
%
% ARCH.pl files mode_analysis under the `fold` species. The graph is cyclic
% (ghcacher's poll -> fetch -> cache -> cache_tag -> poll), so the fold is
% iterated from the bottom of the lattice (everything finite) until the
% assignment stops moving. Both operators are monotone and the lattice is
% finite for a fixed signal set, so the iteration terminates at the least
% fixpoint. See mode_lab.md deviation 1.

program_nodes(Program, Nodes) :-
    findall(Name,
            (   p_rel(Program, Name, _, _)
            ;   p_effect_sig(Program, Name, _, _)
            ;   p_register(Program, Name, _)
            ;   p_rule(Program, _, Name, _, _)
            ;   p_fact_rel(Program, Name)
            ),
            RawNodes),
    sort(RawNodes, Nodes).

assignment(Program, Assignment) :-
    program_nodes(Program, Nodes),
    maplist(initial_pair, Nodes, Initial),
    iterate_assignment(Program, Initial, 0, Assignment).

initial_pair(Name, Name-finite).

iterate_assignment(Program, Current, Depth, Result) :-
    Depth < 100,
    assignment_step(Program, Current, Next),
    (   Next == Current
    ->  Result = Current
    ;   Depth1 is Depth + 1,
        iterate_assignment(Program, Next, Depth1, Result)
    ).

assignment_step(Program, Current, Next) :-
    maplist(node_step(Program, Current), Current, Next).

node_step(Program, Current, Name-_, Name-Effective) :-
    own_lifetime(Program, Name, Current, Own),
    node_scope_lifetime(Program, Name, ScopeLifetime),
    scope_min(Own, ScopeLifetime, Effective).

% The per-node fold body. Effect, register, derived, fact, inert: five arms,
% checked in that order because a rel may be more than one of them.
own_lifetime(Program, Name, Current, Own) :-
    (   p_effect_sig(Program, Name, _, _)
    ->  effect_base_lifetime(Program, Name, Base),
        effect_demand_lifetime(Program, Name, Current, Demand),
        join_max(Base, Demand, Own)
    ;   p_register(Program, Name, Over)
    ->  lookup(Current, Over, Own)
    ;   p_rule(Program, _, Name, _, _)
    ->  findall(RuleLifetime,
                ( p_rule(Program, _, Name, _, Body),
                  body_lifetime(Current, Body, RuleLifetime) ),
                RuleLifetimes),
        join_max_all(RuleLifetimes, Own)
    ;   Own = finite
    ).

% POST-LINK: the bind decides. With no bind the analysis falls back to the
% declared claim so the table still prints, but the ask verdict rejects.
effect_base_lifetime(Program, Name, Base) :-
    (   p_bind(Program, Name, Protocol)
    ->  protocol_lifetime(Protocol, Base)
    ;   p_effect_sig(Program, Name, _, ResultType),
        result_lifetime(ResultType, Base)
    ).

% An effect keeps producing as long as new demand rows keep arriving. This is
% what makes plan case 5 work: fetch is finite per request, but the requests
% recur on the poll clock, so the register over it is never.
effect_demand_lifetime(Program, Name, Current, Demand) :-
    findall(SourceLifetime,
            ( p_demand_rule(Program, Name, SourceRel),
              lookup(Current, SourceRel, SourceLifetime) ),
            DemandLifetimes),
    (   DemandLifetimes == []
    ->  Demand = finite
    ;   join_max_all(DemandLifetimes, Demand)
    ).

body_lifetime(Current, Body, Lifetime) :-
    maplist(atom_lifetime(Current), Body, AtomLifetimes),
    join_max_all(AtomLifetimes, Lifetime).

atom_lifetime(Current, rel(Name),      Lifetime) :- lookup(Current, Name, Lifetime).
atom_lifetime(Current, keyed(Name),    Lifetime) :- lookup(Current, Name, Lifetime).
% ruling r4: a departure cannot happen more often than the arrival that
% preceded it, so a departure stream cannot outlive its source rel.
atom_lifetime(Current, departed(Name), Lifetime) :- lookup(Current, Name, Lifetime).
atom_lifetime(_,       guard(_),       finite).

lookup(Assignment, Name, Lifetime) :- memberchk(Name-Lifetime, Assignment).

% ── cardinality of a rule body (the forkJoin row) ─────────────────────────

atom_card(Program, keyed(Name), Card) :-
    (   p_effect_sig(Program, Name, _, ResultType)
    ->  result_card(ResultType, Card)
    ;   keyed_rel(Program, Name)
    ->  Card = semidet
    ;   Card = multi
    ).
atom_card(_, rel(_),      multi).
atom_card(_, departed(_), multi).
atom_card(_, guard(_),    det).

keyed_rel(Program, Name) :-
    p_rel(Program, Name, _, Columns),
    memberchk(col(_, key(_)), Columns).

key_columns(Program, Name, KeyColumns) :-
    p_rel(Program, Name, _, Columns),
    findall(ColumnName, member(col(ColumnName, key(_)), Columns), KeyColumns).

fully_keyed(Program, Name, BoundColumns) :-
    key_columns(Program, Name, KeyColumns),
    KeyColumns \== [],
    forall(member(KeyColumn, KeyColumns), memberchk(KeyColumn, BoundColumns)).

rule_mode(Program, RuleId, mode(Card, Lifetime)) :-
    p_rule(Program, RuleId, Head, _, Body),
    assignment(Program, Assignment),
    maplist(atom_card(Program), Body, Cards),
    card_join_all(Cards, Card),
    body_lifetime(Assignment, Body, BaseLifetime),
    node_scope_lifetime(Program, Head, ScopeLifetime),
    scope_min(BaseLifetime, ScopeLifetime, Lifetime).

% ═══════════════════════════════════════════════════════════════════════════
% 8. ASK MODES AND VERDICTS
% ═══════════════════════════════════════════════════════════════════════════
%
% Verdict = ok | warn(Reason) | reject(Reason). A reject means the analysis
% refuses to state a mode; a warn means the mode is stated and the CLI should
% say so before blocking the caller.

ask_mode(Program, AskId, mode(Card, Lifetime), Verdict) :-
    p_ask(Program, AskId, Form, Target, BoundColumns),
    (   ask_link_error(Program, Target, LinkError)
    ->  Card = undetermined, Lifetime = undetermined, Verdict = reject(LinkError)
    ;   assignment(Program, Assignment),
        ask_card(Program, Form, Target, BoundColumns, Card),
        ask_lifetime(Program, AskId, Form, Target, Assignment, Lifetime),
        ask_warning(Program, Form, Target, Lifetime, Verdict)
    ).

ask_card(Program, Form, Target, BoundColumns, Card) :-
    (   Form == request
    ->  (   p_effect_sig(Program, Target, _, ResultType)
        ->  result_card(ResultType, Card)
        ;   Card = multi
        )
    ;   Form == write
    ->  (   fully_keyed(Program, Target, BoundColumns) -> Card = det ; Card = multi )
    ;   Form == snapshot
    ->  (   fully_keyed(Program, Target, BoundColumns) -> Card = semidet ; Card = multi )
    ;   Form == tail
    ->  Card = multi
    ).

% Snapshot and write are finite BY CONSTRUCTION (a SELECT completes, a keyed
% write completes); only request and tail consult the mode table, and both are
% dominated by whatever scope holds the ask.
ask_lifetime(Program, AskId, Form, Target, Assignment, Lifetime) :-
    (   Form == snapshot
    ->  Lifetime = finite
    ;   Form == write
    ->  Lifetime = finite
    ;   Form == request
    ->  effect_base_lifetime(Program, Target, Base),
        node_scope_lifetime(Program, AskId, ScopeLifetime),
        scope_min(Base, ScopeLifetime, Lifetime)
    ;   Form == tail
    ->  lookup(Assignment, Target, Base),
        node_scope_lifetime(Program, AskId, ScopeLifetime),
        scope_min(Base, ScopeLifetime, Lifetime)
    ).

ask_warning(Program, Form, Target, Lifetime, Verdict) :-
    (   Form == tail, Lifetime == never
    ->  never_sources(Program, Target, Sources),
        Verdict = warn(tail_never_terminates(Sources))
    ;   Verdict = ok
    ).

% Two named rejections. Both are properties of the LINK, which is why the
% analysis cannot run before it.
ask_link_error(Program, Target, Error) :-
    reachable(Program, Target, Reached),
    member(Effect, Reached),
    p_effect_sig(Program, Effect, _, ResultType),
    (   \+ p_bind(Program, Effect, _)
    ->  Error = no_bind_for(Effect)
    ;   p_bind(Program, Effect, Protocol),
        protocol_lifetime(Protocol, BoundLifetime),
        result_lifetime(ResultType, DeclaredLifetime),
        \+ lifetime_leq(BoundLifetime, DeclaredLifetime),
        Error = bind_outlives_claim(Effect, BoundLifetime, DeclaredLifetime)
    ),
    !.

never_sources(Program, Target, Sources) :-
    reachable(Program, Target, Reached),
    findall(Effect,
            ( member(Effect, Reached),
              p_effect_sig(Program, Effect, _, _),
              effect_base_lifetime(Program, Effect, never) ),
            RawSources),
    sort(RawSources, Sources).

% ── dependency reachability (visited set; the graph is cyclic) ────────────

dep_edge(Program, Head, Dependency) :-
    p_rule(Program, _, Head, _, Body),
    member(Atom, Body),
    atom_rel(Atom, Dependency).
dep_edge(Program, Register, Over) :- p_register(Program, Register, Over).
dep_edge(Program, Effect, Source) :- p_demand_rule(Program, Effect, Source).

atom_rel(rel(Name),      Name).
atom_rel(keyed(Name),    Name).
atom_rel(departed(Name), Name).

reachable(Program, Start, Reached) :-
    reach_walk([Start], Program, [Start], Reached).

reach_walk([], _, Visited, Visited).
reach_walk([Node | Rest], Program, Visited, Reached) :-
    findall(Next,
            ( dep_edge(Program, Node, Next), \+ memberchk(Next, Visited) ),
            RawNext),
    sort(RawNext, NewNodes),
    append(NewNodes, Visited, Visited1),
    append(NewNodes, Rest, Queue),
    reach_walk(Queue, Program, Visited1, Reached).

% ── the printable table ───────────────────────────────────────────────────

mode_row(Program, AskId, Form, Card, PrettyLifetime, Verdict) :-
    p_ask(Program, AskId, Form, _, _),
    ask_mode(Program, AskId, mode(Card, Lifetime), Verdict),
    pretty_lifetime(Lifetime, PrettyLifetime).

report :-
    format("~w~t~24|~w~t~48|~w~t~58|~w~t~72|~w~t~116|~w~n",
           ['program', 'ask', 'form', 'card', 'lifetime', 'verdict']),
    all_programs(Programs),
    forall(( member(Program, Programs),
             mode_row(Program, AskId, Form, Card, Lifetime, Verdict) ),
           format("~w~t~24|~w~t~48|~w~t~58|~w~t~72|~w~t~116|~w~n",
                  [Program, AskId, Form, Card, Lifetime, Verdict])),
    nl,
    format("~w~t~24|~w~t~44|~w~t~54|~w~n", ['program', 'rule', 'card', 'lifetime']),
    forall(( member(RuleProgram, [fork_join_det, fork_join_timer,
                                  fork_join_timer_scoped, fork_join_semidet,
                                  audit_join, departure]),
             rule_mode(RuleProgram, RuleId, mode(RuleCard, RuleLifetime)),
             pretty_lifetime(RuleLifetime, PrettyRuleLifetime) ),
           format("~w~t~24|~w~t~44|~w~t~54|~w~n",
                  [RuleProgram, RuleId, RuleCard, PrettyRuleLifetime])).

% ═══════════════════════════════════════════════════════════════════════════
% 9. CHECKS
% ═══════════════════════════════════════════════════════════════════════════

% ── 9a. the two lattice operators ─────────────────────────────────────────

check(scope_min_total,
      forall(( sample_lifetime(Left), sample_lifetime(Right) ),
             ( scope_min(Left, Right, Result), ground(Result) ))).

check(join_max_total,
      forall(( sample_lifetime(Left), sample_lifetime(Right) ),
             ( join_max(Left, Right, Result), ground(Result) ))).

check(scope_min_commutative,
      forall(( sample_lifetime(Left), sample_lifetime(Right) ),
             ( scope_min(Left, Right, Forward), scope_min(Right, Left, Backward),
               Forward == Backward ))).

check(join_max_commutative,
      forall(( sample_lifetime(Left), sample_lifetime(Right) ),
             ( join_max(Left, Right, Forward), join_max(Right, Left, Backward),
               Forward == Backward ))).

check(scope_min_associative,
      forall(( sample_lifetime(A), sample_lifetime(B), sample_lifetime(C) ),
             ( scope_min(A, B, AB), scope_min(AB, C, LeftGrouped),
               scope_min(B, C, BC), scope_min(A, BC, RightGrouped),
               LeftGrouped == RightGrouped ))).

check(join_max_associative,
      forall(( sample_lifetime(A), sample_lifetime(B), sample_lifetime(C) ),
             ( join_max(A, B, AB), join_max(AB, C, LeftGrouped),
               join_max(B, C, BC), join_max(A, BC, RightGrouped),
               LeftGrouped == RightGrouped ))).

check(scope_min_idempotent,
      forall(sample_lifetime(Lifetime),
             ( scope_min(Lifetime, Lifetime, Result), Result == Lifetime ))).

check(join_max_idempotent,
      forall(sample_lifetime(Lifetime),
             ( join_max(Lifetime, Lifetime, Result), Result == Lifetime ))).

% finite = TRUE: identity of AND, annihilator of OR.
check(finite_is_join_max_identity,
      forall(sample_lifetime(Lifetime),
             ( join_max(finite, Lifetime, Result), Result == Lifetime ))).

check(finite_annihilates_scope_min,
      forall(sample_lifetime(Lifetime),
             ( scope_min(finite, Lifetime, Result), Result == finite ))).

% never = FALSE: identity of OR, annihilator of AND.
check(never_is_scope_min_identity,
      forall(sample_lifetime(Lifetime),
             ( scope_min(never, Lifetime, Result), Result == Lifetime ))).

check(never_annihilates_join_max,
      forall(sample_lifetime(Lifetime),
             ( join_max(never, Lifetime, Result), Result == never ))).

check(scope_min_of_two_signals_is_or,
      ( until_signal(disconnect, Left), until_signal(outer_next, Right),
        scope_min(Left, Right, Result),
        pretty_lifetime(Result, Pretty),
        Pretty == until(any_of([disconnect, outer_next])) )).

check(join_max_of_two_signals_is_and,
      ( until_signal(disconnect, Left), until_signal(outer_next, Right),
        join_max(Left, Right, Result),
        pretty_lifetime(Result, Pretty),
        Pretty == until(all_of([disconnect, outer_next])) )).

check(absorption_holds,
      forall(( sample_lifetime(Left), sample_lifetime(Right) ),
             ( join_max(Left, Right, Joined), scope_min(Left, Joined, Result),
               Result == Left ))).

check(join_max_distributes_over_scope_min,
      forall(( sample_lifetime(A), sample_lifetime(B), sample_lifetime(C) ),
             ( scope_min(B, C, ScopeBC), join_max(A, ScopeBC, LeftSide),
               join_max(A, B, JoinAB), join_max(A, C, JoinAC),
               scope_min(JoinAB, JoinAC, RightSide),
               LeftSide == RightSide ))).

check(lifetime_order_is_a_chain_at_the_ends,
      ( until_signal(disconnect, Until),
        lifetime_leq(finite, Until),
        lifetime_leq(Until, never),
        lifetime_leq(finite, never),
        \+ lifetime_leq(never, finite) )).

% AUDIT finding 13's first gap, answered: they are not ordered, and the
% lattice does not need them to be.
check(until_signals_are_incomparable,
      ( until_signal(disconnect, Left), until_signal(document_closed, Right),
        lifetime_incomparable(Left, Right) )).

check(canonical_form_absorbs,
      ( normalize_clauses([[disconnect], [disconnect, document_closed]], Normalized),
        Normalized == [[disconnect]] )).

% ── 9b. cardinality ───────────────────────────────────────────────────────

check(card_join_is_max_on_the_chain,
      ( card_join(det, det, det), card_join(det, semidet, semidet),
        card_join(semidet, multi, multi), card_join(det, multi, multi),
        card_join(multi, multi, multi) )).

check(det_envelope_result_is_det,
      ( result_card(envelope(fetch_result), Card), Card == det )).

check(bound_key_read_is_semidet,
      ( ask_card(ghcacher, snapshot, cache, [endpoint], Card), Card == semidet )).

check(unbound_key_read_is_multi,
      ( ask_card(ghcacher, snapshot, cache, [], Card), Card == multi )).

check(tail_ask_is_always_multi,
      forall(( p_ask(Program, AskId, tail, _, _),
               member(Program, [ghcacher, eventing, shell_modes, timer_alone]) ),
             ( ask_mode(Program, AskId, mode(Card, _), _), Card == multi ))).

% Mercury's nondet cell has nowhere to go in a 3-point cardinality; recorded
% rather than silently dropped. See mode_lab.md ambiguity 5.
check(nondet_flattens_to_multi,
      ( card_join(semidet, multi, Card), Card == multi )).

% ── 9c. the five plan cases (ghcacher) ────────────────────────────────────

check(case1_fetch_request_is_det_finite,
      ( ask_mode(ghcacher, ask_fetch_request, Mode, Verdict),
        Mode == mode(det, finite), Verdict == ok )).

check(case2_bound_key_cache_is_semidet_finite,
      ( ask_mode(ghcacher, ask_cache_bound, Mode, Verdict),
        Mode == mode(semidet, finite), Verdict == ok )).

check(case3_change_log_tail_is_multi_never,
      ( ask_mode(ghcacher, ask_change_log_tail, Mode, _),
        Mode == mode(multi, never) )).

check(case3_chain_bottoms_at_the_timer,
      ( never_sources(ghcacher, change_log, Sources), Sources == [every_300] )).

check(case3_whole_chain_is_never,
      ( assignment(ghcacher, Assignment),
        forall(member(RelName, [every_300, poll, fetch, cache, cache_tag,
                                cache_body, stars, change_log]),
               ( lookup(Assignment, RelName, Lifetime), Lifetime == never )),
        lookup(Assignment, watch, WatchLifetime), WatchLifetime == finite )).

check(case4_switch_map_flips_the_timer,
      ( assignment(timer_alone, AloneAssignment),
        lookup(AloneAssignment, every_300, AloneLifetime), AloneLifetime == never,
        assignment(timer_scoped, ScopedAssignment),
        lookup(ScopedAssignment, every_300, ScopedLifetime),
        pretty_lifetime(ScopedLifetime, Pretty), Pretty == until(outer_next) )).

check(case4_dominated_timer_flips_the_whole_ghcacher_chain,
      ( assignment(ghcacher_scoped, Assignment),
        forall(member(RelName, [every_300, poll, fetch, cache, change_log]),
               ( lookup(Assignment, RelName, Lifetime),
                 pretty_lifetime(Lifetime, Pretty), Pretty == until(outer_next) )) )).

% Plan case 5: the register's lifetime is its `over` stream's, and fetch is
% finite per request only until you notice that the requests recur on the
% poll clock.
check(case5_register_is_never_on_a_recurring_clock,
      ( assignment(ghcacher, Assignment),
        lookup(Assignment, cache, CacheLifetime), CacheLifetime == never,
        lookup(Assignment, fetch, FetchLifetime), FetchLifetime == never,
        p_bind(ghcacher, fetch, Protocol), protocol_lifetime(Protocol, finite) )).

check(case5_register_is_until_when_the_clock_is_dominated,
      ( assignment(ghcacher_scoped, Assignment),
        lookup(Assignment, cache, Lifetime),
        pretty_lifetime(Lifetime, Pretty), Pretty == until(outer_next),
        ask_mode(ghcacher_scoped, ask_change_log_tail, mode(multi, TailLifetime), Verdict),
        pretty_lifetime(TailLifetime, Pretty),
        Verdict == ok )).

% ── 9d. the five eventing ask rows ────────────────────────────────────────

check(eventing_has_exactly_five_asks,
      ( findall(AskId, p_ask(eventing, AskId, _, _, _), AskIds),
        length(AskIds, Count), Count == 5 )).

check(eventing_hook_write_is_det_finite,
      ( ask_mode(eventing, ask_hook_write, Mode, Verdict),
        Mode == mode(det, finite), Verdict == ok )).

check(eventing_hook_snapshot_is_multi_finite,
      ( ask_mode(eventing, ask_hook_snapshot, Mode, Verdict),
        Mode == mode(multi, finite), Verdict == ok )).

% The plan's table names one signal (disconnect); the lattice names both,
% because either one ends the subscription. See mode_lab.md deviation 3.
check(eventing_lsp_tail_is_dominated_by_two_scopes,
      ( ask_mode(eventing, ask_lsp_tail, mode(Card, Lifetime), Verdict),
        Card == multi, Verdict == ok,
        pretty_lifetime(Lifetime, Pretty),
        Pretty == until(any_of([disconnect, document_closed])) )).

check(eventing_lsp_tail_own_lifetime_is_never_before_dominance,
      ( assignment(eventing, Assignment),
        lookup(Assignment, diagnostic, Lifetime), Lifetime == never )).

check(eventing_commit_gate_is_multi_finite,
      ( ask_mode(eventing, ask_commit_gate, Mode, Verdict),
        Mode == mode(multi, finite), Verdict == ok )).

check(eventing_dashboard_tail_is_multi_never_with_a_warning,
      ( ask_mode(eventing, ask_dashboard_tail, Mode, Verdict),
        Mode == mode(multi, never),
        Verdict == warn(tail_never_terminates([file_change])) )).

% ── 9e. result-type modes, including the (multi, finite) cell ─────────────

check(det_envelope_effect_is_det_finite,
      ( ask_mode(shell_modes, ask_fetch_env, Mode, Verdict),
        Mode == mode(det, finite), Verdict == ok )).

check(stream_result_is_multi_finite,
      ( ask_mode(shell_modes, ask_extract_lines, Mode, Verdict),
        Mode == mode(multi, finite), Verdict == ok )).

check(tail_result_is_multi_never,
      ( ask_mode(shell_modes, ask_log_tail, mode(Card, Lifetime), Verdict),
        Card == multi, Lifetime == never,
        Verdict == warn(tail_never_terminates([log_tail])) )).

check(mode_is_a_function_of_the_result_type,
      forall(member(ResultType, [envelope(fetch_result),
                                 stream(extract_event, extract_end),
                                 tail(log_event)]),
             ( findall(Card-Lifetime,
                       ( result_card(ResultType, Card),
                         result_lifetime(ResultType, Lifetime) ),
                       Pairs),
               length(Pairs, 1) ))).

check(multi_finite_needs_a_terminal_enum,
      ( result_lifetime(stream(extract_event, extract_end), finite),
        result_lifetime(tail(log_event), never) )).

% ── 9f. the join operator, against the AUDIT counterexample ───────────────

check(join_max_gets_the_audit_case_right,
      ( rule_mode(audit_join, r_job, mode(Card, Lifetime)),
        Card == multi, Lifetime == never )).

check(scope_min_would_get_the_audit_case_wrong,
      ( scope_min_all([finite, never], WrongAnswer), WrongAnswer == finite,
        join_max_all([finite, never], RightAnswer), RightAnswer == never )).

check(join_of_two_until_bodies_needs_both_signals,
      ( until_signal(disconnect, Left), until_signal(outer_next, Right),
        body_lifetime([left_rel-Left, right_rel-Right],
                      [rel(left_rel), rel(right_rel)], Lifetime),
        pretty_lifetime(Lifetime, Pretty),
        Pretty == until(all_of([disconnect, outer_next])) )).

check(rule_lifetime_is_join_max_of_its_body,
      ( assignment(audit_join, Assignment),
        lookup(Assignment, config, ConfigLifetime), ConfigLifetime == finite,
        lookup(Assignment, timer,  TimerLifetime),  TimerLifetime  == never,
        lookup(Assignment, job,    JobLifetime),    JobLifetime    == never )).

% ── 9g. forkJoin ─────────────────────────────────────────────────────────

check(fork_join_of_det_envelopes_is_det,
      ( rule_mode(fork_join_det, r_combined, mode(Card, _)), Card == det )).

check(fork_join_of_finite_inputs_is_finite,
      ( rule_mode(fork_join_det, r_combined, mode(_, Lifetime)), Lifetime == finite )).

% The RULE proves det (three det envelopes, conjunctively). A snapshot ask on
% the rel it heads is still semidet: the row may not have landed yet. Rule
% mode and ask mode are different questions. See mode_lab.md deviation 4.
check(fork_join_snapshot_ask_is_semidet_not_det,
      ( rule_mode(fork_join_det, r_combined, mode(det, finite)),
        ask_mode(fork_join_det, ask_combined, Mode, Verdict),
        Mode == mode(semidet, finite), Verdict == ok )).

check(fork_join_with_a_never_input_is_never,
      ( rule_mode(fork_join_timer, r_combined, mode(Card, Lifetime)),
        Card == multi, Lifetime == never )).

check(fork_join_never_is_dominated_back_to_until,
      ( rule_mode(fork_join_timer_scoped, r_combined, mode(Card, Lifetime)),
        Card == multi,
        pretty_lifetime(Lifetime, Pretty), Pretty == until(outer_next) )).

check(fork_join_tail_ask_warns_unscoped_and_not_scoped,
      ( ask_mode(fork_join_timer, ask_combined, mode(multi, never), Unscoped),
        Unscoped == warn(tail_never_terminates([every_60])),
        ask_mode(fork_join_timer_scoped, ask_combined, mode(multi, ScopedLifetime), Scoped),
        pretty_lifetime(ScopedLifetime, until(outer_next)),
        Scoped == ok )).

check(fork_join_with_a_keyed_state_input_is_semidet,
      ( rule_mode(fork_join_semidet, r_combined, mode(Card, Lifetime)),
        Card == semidet, Lifetime == finite )).

% ── 9h. rejections and warnings ──────────────────────────────────────────

check(tail_ask_on_a_never_chain_warns_with_a_named_source,
      ( ask_mode(ghcacher, ask_change_log_tail, _, Verdict),
        Verdict == warn(tail_never_terminates([every_300])) )).

check(snapshot_over_a_never_rel_does_not_warn,
      ( ask_mode(eventing, ask_commit_gate, _, Verdict), Verdict == ok,
        assignment(eventing, Assignment),
        lookup(Assignment, violation, Lifetime), Lifetime == never )).

check(unlinked_program_has_no_mode,
      ( ask_mode(ghcacher_unlinked, ask_change_log_tail,
                 mode(Card, Lifetime), Verdict),
        Card == undetermined, Lifetime == undetermined,
        Verdict == reject(no_bind_for(every_300)) )).

check(every_ask_in_an_unlinked_program_is_rejected,
      forall(p_ask(ghcacher_unlinked, AskId, _, _, _),
             ( ask_mode(ghcacher_unlinked, AskId, _, Verdict),
               Verdict = reject(_) ))).

check(bind_outliving_its_claim_is_a_link_error,
      ( ask_mode(shell_mislinked, ask_extract_lines, _, Verdict),
        Verdict == reject(bind_outlives_claim(extract, never, finite)) )).

check(bind_ending_sooner_than_claimed_is_legal,
      ( \+ ask_link_error(ghcacher_relinked, change_log, _),
        p_bind(ghcacher_relinked, every_300, Protocol),
        protocol_lifetime(Protocol, finite),
        p_effect_sig(ghcacher_relinked, every_300, _, ResultType),
        result_lifetime(ResultType, never),
        lifetime_leq(finite, never) )).

% ── 9i. post-link: relinking flips the answer ────────────────────────────

check(relink_flips_change_log_to_finite,
      ( ask_mode(ghcacher, ask_change_log_tail, mode(multi, never), _),
        ask_mode(ghcacher_relinked, ask_change_log_tail, mode(multi, finite), Verdict),
        Verdict == ok )).

check(relink_leaves_the_program_text_alone,
      ( findall(RuleId, p_rule(ghcacher, RuleId, _, _, _), BaseRules),
        findall(RuleId, p_rule(ghcacher_relinked, RuleId, _, _, _), RelinkedRules),
        msort(BaseRules, Sorted), msort(RelinkedRules, Sorted) )).

check(sse_bind_gives_until_disconnect,
      ( ask_mode(sse_out, ask_wire_tail, mode(multi, Lifetime), Verdict),
        pretty_lifetime(Lifetime, Pretty), Pretty == until(disconnect),
        Verdict == ok )).

% ── 9j. retention (q10) does not touch lifetime ──────────────────────────

check(keep_bound_does_not_change_lifetime,
      ( ask_mode(retention_unbounded, ask_feed_tail, UnboundedMode, _),
        ask_mode(retention_bounded,   ask_feed_tail, BoundedMode,   _),
        UnboundedMode == BoundedMode,
        UnboundedMode == mode(multi, never) )).

check(keep_bound_is_storage_not_subscription,
      ( p_keep(retention_bounded, feed, count(100)),
        p_keep(retention_unbounded, feed, all),
        assignment(retention_bounded, BoundedAssignment),
        assignment(retention_unbounded, UnboundedAssignment),
        lookup(BoundedAssignment, feed, BoundedLifetime),
        lookup(UnboundedAssignment, feed, UnboundedLifetime),
        BoundedLifetime == UnboundedLifetime )).

% ── 9k. static lifetime vs runtime lifetime ──────────────────────────────

check(runtime_teardown_ends_a_never_subscription,
      ( p_runtime_sub(eventing, sub_dashboard_1, ask_dashboard_tail, ended(teardown)),
        ask_mode(eventing, ask_dashboard_tail, mode(_, never), _) )).

check(runtime_completion_never_happens_on_a_never_ask,
      forall(( p_runtime_sub(Program, _, AskId, ended(complete)),
               member(Program, [ghcacher, eventing]) ),
             ( ask_mode(Program, AskId, mode(_, Lifetime), _), Lifetime \== never ))).

% The two objects are allowed to disagree, and here they do: the static
% lifetime says "this ask does not complete on its own" while the runtime
% forest holds an ENDED subscription for it. Teardown is not completion.
check(static_never_coexists_with_an_ended_runtime_sub,
      ( ask_mode(eventing, ask_dashboard_tail, mode(_, never), _),
        p_runtime_sub(eventing, _, ask_dashboard_tail, ended(teardown)),
        \+ p_runtime_sub(eventing, _, ask_dashboard_tail, ended(complete)) )).

% ── 9l. departure (ruling r4) ────────────────────────────────────────────

check(departure_lifetime_follows_its_source,
      ( assignment(departure, Assignment),
        lookup(Assignment, watch, WatchLifetime), WatchLifetime == finite,
        lookup(Assignment, unwatched, UnwatchedLifetime), UnwatchedLifetime == finite )).

check(departure_of_a_never_rel_is_never,
      ( assignment(departure, Assignment),
        lookup(Assignment, diagnostic, DiagnosticLifetime), DiagnosticLifetime == never,
        lookup(Assignment, cleared, ClearedLifetime), ClearedLifetime == never )).

check(departure_atom_is_multi,
      ( atom_card(departure, departed(watch), Card), Card == multi )).

% ── 9m. shape of the analysis ────────────────────────────────────────────

check(fixpoint_converges_on_a_cyclic_program,
      ( reachable(ghcacher, poll, Reached),
        memberchk(fetch, Reached), memberchk(cache, Reached),
        memberchk(poll, Reached),
        assignment(ghcacher, Assignment), length(Assignment, Count), Count > 0 )).

check(every_body_atom_names_a_declared_rel,
      forall(( all_programs(Programs), member(Program, Programs),
               p_rule(Program, _, _, _, Body), member(Atom, Body),
               atom_rel(Atom, RelName) ),
             ( program_nodes(Program, Nodes), memberchk(RelName, Nodes) ))).

check(every_ask_gets_exactly_one_mode,
      forall(( all_programs(Programs), member(Program, Programs),
               p_ask(Program, AskId, _, _, _) ),
             ( findall(Mode-Verdict, ask_mode(Program, AskId, Mode, Verdict), Rows),
               length(Rows, 1) ))).

check(every_scope_edge_names_a_declared_scope,
      forall(( all_programs(Programs), member(Program, Programs),
               p_scope_edge(Program, switch_map(_, ScopeName)) ),
             p_scope(Program, ScopeName, _, _))).

check(scope_lifetimes_are_never_finite,
      forall(( all_programs(Programs), member(Program, Programs),
               p_scope(Program, ScopeName, _, _) ),
             ( scope_lifetime(Program, ScopeName, Lifetime),
               Lifetime \== finite ))).

check(all_lifetimes_are_canonical,
      forall(( all_programs(Programs), member(Program, Programs),
               assignment(Program, Assignment), member(_-Lifetime, Assignment) ),
             canonical_lifetime(Lifetime))).

canonical_lifetime(finite).
canonical_lifetime(never).
canonical_lifetime(until(Clauses)) :-
    normalize_clauses(Clauses, Normalized),
    Normalized == Clauses,
    Clauses \== [],
    \+ memberchk([], Clauses).

go :- run(check).
