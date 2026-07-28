% desugar.pl : Q1 LEGALITY MATRIX + the desugaring itself.
%
% The design under test says a `match` block is the SOURCE-MAJOR view of a
% rule set and each arm desugars to exactly one kernel rule (the sugar law).
% This file writes that transform down and grades every cell of the 9x4
% matrix the contract asks for. No cell is blank: each is legal, refuse with
% a NAMED error, or ambiguous against a NAMED slot.
%
% Columns:
%   arm_plus    an arm under the `+>` (event axis) arrow
%   arm_level   an arm under the `->` (state axis) arrow
%   body_edge   the classic term form: (Head <+ Body)
%   body_level  the classic term form: (Head <- Body)
%
% Every refusal is grounded in the oracle. The two that are ALREADY engine
% law rather than lab opinion:
%   departed/1 in a level rule -> engine.pl check_program/1 :111-112 throws
%     departed_in_level_rule(Ref). This lab reuses that exact error term.
%   departed/1 as a READ -> body.pl:102 `solve(departed(_), _) :- !, fail.`

:- module(mf_desugar,
          [ legality/3, desugar_cell/3, matrix_row/1, matrix_column/1,
            cell_item/2, slot/1, desugar_match/2, desugar_arm/2 ]).

:- use_module(library(lists)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ the grid ═══════════════════════════════════════════════════════════════

matrix_row(bare_atom).
matrix_row(next_form).
matrix_row(finalize_form).
matrix_row(complete_form).
matrix_row(async_atom).
matrix_row(comparison_guard).
matrix_row(row_pattern).
matrix_row(enum_destructure).
matrix_row(negation).

matrix_column(arm_plus).
matrix_column(arm_level).
matrix_column(body_edge).
matrix_column(body_level).

% Representative item per row. async_atom is spelled with the NO-AT
% function-wrapper candidate (user directive 2026-07-28: avoid @ symbols as
% long as possible); the @async spelling occupies the same cell.
cell_item(bare_atom,        cache(key_column, value_column)).
cell_item(next_form,        next(cache(key_column, value_column))).
cell_item(finalize_form,    finalize(cache(key_column, value_column))).
cell_item(complete_form,    complete).
cell_item(async_atom,       async(cache(key_column, value_column))).
cell_item(comparison_guard, size_column < 10).
cell_item(row_pattern,      phase(endpoint_column, 'fetching')).
cell_item(enum_destructure, decode(body_column, '{}'(tag: tag_column))).
cell_item(negation,         not(live(key_column))).

% ═══ named ambiguity slots ══════════════════════════════════════════════════

slot('SLOT-COMPLETE').
slot('SLOT-CAUSE').
slot('SLOT-SPILL').
slot('SLOT-NEST').
slot('SLOT-TA-MARK').
slot('SLOT-ARROW').
slot('SLOT-LEVEL-ARMS').
% Added by this lab (the contract permits adding, never silently resolving):
slot('SLOT-SUGAR-SCOPE').   % may match-only sugar words appear in a classic body?
slot('SLOT-UPDATE-ARM').    % if an update/transition arm exists, per-occurrence or per-boundary?

% ═══ Q1: legality(Row, Column, Verdict) ═════════════════════════════════════
% Verdict = legal(Note) | refuse(Error) | ambiguous(Slot).

% ── bare atom ───────────────────────────────────────────────────────────────
% Under the match block the subject atom IS the trigger, so the arm carries a
% marker the classic body does not: only/1 (engine.pl marked_items/2 :143-145).
legality(bare_atom, arm_plus,   legal('trigger; only(Atom), engine.pl:144')).
% A level rule has no occurrences at all: level_closure/5 runs over the whole
% base (engine.pl:286), and only/1 in a level body is an ORDINARY READ
% (body.pl:99, level_eval.pl:109). The marker is inert, so a `->` arm's
% subject is a JOIN, not a trigger. Legal, but the arrow changes what the
% subject MEANS, which is the SLOT-LEVEL-ARMS question.
legality(bare_atom, arm_level,  legal('join, NOT a trigger; marker inert, body.pl:99')).
legality(bare_atom, body_edge,  legal('any-body-atom trigger, C2 ruling, engine.pl:147-155')).
legality(bare_atom, body_level, legal('ordinary level read, body.pl:110')).

% ── next() ──────────────────────────────────────────────────────────────────
legality(next_form, arm_plus,   legal('sugar-identical to bare atom; only(Atom)')).
legality(next_form, arm_level,  refuse(lifecycle_arm_in_level_arm(next))).
legality(next_form, body_edge,  ambiguous('SLOT-SUGAR-SCOPE')).
legality(next_form, body_level, refuse(lifecycle_arm_in_level_arm(next))).

% ── finalize() ──────────────────────────────────────────────────────────────
legality(finalize_form, arm_plus,   legal('only(departed(Atom)); scopes.pl:148 is the shipped shape')).
% NOT a lab opinion: engine.pl:111-112 already throws this at load time.
legality(finalize_form, arm_level,  refuse(departed_in_level_rule('cache'/2))).
legality(finalize_form, body_edge,  legal('departed/1, engine_core.pl:121 unmarked, scopes.pl:148 marked')).
legality(finalize_form, body_level, refuse(departed_in_level_rule('cache'/2))).

% ── complete ────────────────────────────────────────────────────────────────
% Deferred by the contract. This lab has a candidate answer (verdict doc
% ambiguity 6): a scope closing is an ordinary retraction of the scope row,
% so `complete` == `finalize(scope_row)` and costs zero constructs. Recorded,
% not resolved.
legality(complete_form, arm_plus,   ambiguous('SLOT-COMPLETE')).
legality(complete_form, arm_level,  refuse(lifecycle_arm_in_level_arm(complete))).
legality(complete_form, body_edge,  ambiguous('SLOT-COMPLETE')).
legality(complete_form, body_level, refuse(lifecycle_arm_in_level_arm(complete))).

% ── async-marked atom (the Ta row) ──────────────────────────────────────────
% Graded ambiguous, NOT legal-by-default: this lab's Q2/Q4f finding is that
% Ta has no semantics an rx scheduler can carry and dissolves into a pending
% rel. SLOT-TA-MARK now covers both the spelling and the existence question.
legality(async_atom, arm_plus,   ambiguous('SLOT-TA-MARK')).
% A level rule is a maintained view: there is no "later" in it to defer to.
legality(async_atom, arm_level,  refuse(async_in_level_rule)).
legality(async_atom, body_edge,  ambiguous('SLOT-TA-MARK')).
legality(async_atom, body_level, refuse(async_in_level_rule)).

% ── comparison guard ────────────────────────────────────────────────────────
% Legal in the ORACLE everywhere (body.pl:45-46, 109). The tsv2 compiler
% refuses two of these cells today (analyze.pl edge_marked_with_extra_goal /
% edge_body_needs_comparison) -- a lowering gap, not a language gap.
legality(comparison_guard, arm_plus,   legal('arm guard; oracle legal, tsv2 refuses edge_marked_with_extra_goal')).
legality(comparison_guard, arm_level,  legal('level guard, body.pl:109')).
legality(comparison_guard, body_edge,  legal('oracle legal, tsv2 refuses edge_body_needs_comparison')).
legality(comparison_guard, body_level, legal('level guard, body.pl:109')).

% ── row pattern (constant in a column) ──────────────────────────────────────
legality(row_pattern, arm_plus,   legal('constant-tag unification on the trigger; state_machine.pl precedent')).
legality(row_pattern, arm_level,  legal('constant-tag unification on the join')).
legality(row_pattern, body_edge,  legal('plain unification')).
legality(row_pattern, body_level, legal('plain unification')).

% ── enum destructure ────────────────────────────────────────────────────────
legality(enum_destructure, arm_plus,   legal('arm guard; oracle body.pl:105, tsv2 refuses edge_body_needs_json_destructure')).
legality(enum_destructure, arm_level,  legal('body.pl:105')).
legality(enum_destructure, body_edge,  legal('oracle legal, tsv2 refuses edge_body_needs_json_destructure')).
legality(enum_destructure, body_level, legal('body.pl:105')).

% ── not() ───────────────────────────────────────────────────────────────────
% Legal everywhere in the oracle, but the two EDGE cells are unstratified:
% level_closure/5 stratifies level rules only (engine.pl:284-286), so an edge
% body negating an EDGE-headed rel reads the mid-loop store. Scenario d
% proves this is arrival-order dependent. Legal-with-hazard, not refused,
% because scopes.pl:139 relies on the safe (level-headed) case.
legality(negation, arm_plus,   legal('UNSTRATIFIED over edge-headed rels, scenario d')).
legality(negation, arm_level,  legal('stratified, level_eval.pl:121-142')).
legality(negation, body_edge,  legal('UNSTRATIFIED over edge-headed rels, scenario d')).
legality(negation, body_level, legal('stratified, level_eval.pl:121-142')).

% ═══ the transform ══════════════════════════════════════════════════════════
%% desugar_cell(+Row, +Column, -Kernel) is det.
%  Succeeds with the kernel form on a legal cell; throws the cell's named
%  error on a refuse cell; throws ambiguous(Slot) on an ambiguous cell.

desugar_cell(Row, Column, Kernel) :-
    legality(Row, Column, Verdict),
    cell_item(Row, Item),
    (   Verdict = refuse(Error)      -> throw(Error)
    ;   Verdict = ambiguous(Slot)    -> throw(ambiguous(Slot))
    ;   desugar_item(Column, Item, Kernel)
    ).

% Arms split two ways: a LIFECYCLE/subject item becomes the rule's trigger
% goal, everything else becomes an ordinary guard goal in the same body.
desugar_item(arm_plus, next(Atom),     trigger(only(Atom))) :- !.
desugar_item(arm_plus, finalize(Atom), trigger(only(departed(Atom)))) :- !.
desugar_item(arm_plus, Item,           trigger(only(Item))) :- subject_atom(Item), !.
desugar_item(arm_plus, Item,           guard(Item)).

desugar_item(arm_level, Item, join(Item)) :- subject_atom(Item), !.
desugar_item(arm_level, Item, guard(Item)).

desugar_item(body_edge,  Item, goal(Item)).
desugar_item(body_level, Item, goal(Item)).

% ═══ whole match blocks (the sugar law: one kernel rule per arm) ════════════
% MatchBlock = match(Subject, [arm(Arrow, Item, Guards, Head) ...])
% Arrow is the quoted atom '+>' or '->'.
%
% NOTE, recorded as verdict-doc ambiguity 2: an arm ALWAYS emits the MARKED
% spelling only(...), because the match subject is by construction the sole
% trigger. The 110-fixture corpus writes the same rules unmarked
% (engine_core.pl:121). The two are occurrence-identical whenever the body
% holds exactly one rel atom (engine.pl trigger_items/2 :136-138 falls back
% to unmarked_items only when NO marker is present, and unmarked_items skips
% guard goals via body.pl:112-126), and they DIVERGE the moment the arm
% carries a second rel atom as a guard. Graded by tick log, not term equality.

desugar_match(match(_Subject, Arms), Rules) :- maplist(desugar_arm, Arms, Rules).

desugar_arm(arm('+>', Item, Guards, Head), (Head <+ Body)) :- !,
    desugar_item(arm_plus, Item, trigger(Trigger)),
    conjoin([Trigger | Guards], Body).
desugar_arm(arm('->', Item, Guards, Head), (Head <- Body)) :-
    desugar_item(arm_level, Item, join(Join)),
    conjoin([Join | Guards], Body).

conjoin([Goal], Goal) :- !.
conjoin([Goal | Rest], (Goal, More)) :- conjoin(Rest, More).

% A subject atom is a plain positive rel atom: not a comparison, not a
% wrapper goal. Same classification body.pl:112-126 makes.
subject_atom(Item) :-
    compound(Item),
    functor(Item, Name, Arity), Arity >= 1,
    \+ memberchk(Name, [not, only, pre, departed, now, decode, json_each,
                        next, finalize, async, (:=), (is),
                        (<), (=<), (>), (>=), (==), (\==)]).
