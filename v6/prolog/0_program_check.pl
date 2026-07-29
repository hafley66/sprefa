% 0_program_check.pl : one implementation of every invalid-program trigger
% both doors test, with each door keeping its own exception vocabulary.
%
% Rank 2 of plans/2026-07-29-prolog-org-review.md. The review found six trigger
% classes written twice, once in the oracle's engine:check_program/1 and once
% in the compiler's analyze:check_supported_subset/1, plus two classes the
% ORACLE alone checked, which let the compiler accept programs the reference
% door rejects.
%
%   program_violation(+CheckName, +Program, -Payload)
%
% succeeds once per violating witness. Program is prog(Decls, Rules). Payload
% is whatever that class needs to name the offender, in a shape both doors can
% project from; it is NOT an exception term.
%
% WHAT THIS FILE DELIBERATELY DOES NOT OWN, and why the shape is
% trigger-only rather than a whole shared check_program/1:
%
%   Exception terms. The oracle throws a bare term, the compiler wraps in
%   unsupported_construct/1 and, for keyed Log, carries the key positions its
%   emitter would have needed. Those terms are fixture data on both sides.
%   Each door's adapter builds its own.
%
%   Check ORDER. The two doors run these classes in different orders, and the
%   compiler interleaves them with its own capability refusals (edge body
%   shape, head arithmetic, conflict risk). A program violating two classes
%   therefore reports different ones at the two doors, and that too is fixture
%   data. Each door declares its own order; first_violation/3 walks whatever
%   order it is given.
%
%   Compiler capability refusals. Anything the reference engine executes
%   happily and only this compiler cannot emit stays in analyze.pl. The oracle
%   is deliberately the wider language.

:- module(program_check,
          [ program_violation/3,
            first_violation/3,
            % Declaration queries, shared by both doors (rank R9 of the same
            % review). The oracle and the compiler each carried a clause-for-
            % clause identical resolver, the oracle's taking an extra Rules
            % argument it never read.
            relation_kind/3,
            declared_key/3
          ]).

:- use_module(library(lists)).
:- use_module('0_body_walk', [body_wrapper_refs/4]).
:- use_module('0_type_plane',
              [ type_definitions/2, type_cycle_witness/2, declared_type_name/2 ]).
:- use_module('compile/registry', [surface_for_term/6]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ── the ordered driver ───────────────────────────────────────────────────────

% First violation in the caller's declared order, as violation(Name, Payload).
% Fails when the program violates none of the listed classes.
first_violation(Program, OrderedChecks, violation(Name, Payload)) :-
    member(Name, OrderedChecks),
    program_violation(Name, Program, Payload),
    !.

% ── declaration queries these triggers share ─────────────────────────────────
% Kept here rather than in either door so the fallback-to-set rule has ONE
% statement. A relation is a Log only when declared so; a keyed relation is a
% Set by construction; everything undeclared is a Set.

declared_kind(Decls, Ref, Kind) :- memberchk(kind(Ref, Kind), Decls).

relation_kind(Decls, Ref, log) :- declared_kind(Decls, Ref, log), !.
relation_kind(Decls, Ref, set) :- declared_kind(Decls, Ref, set), !.
relation_kind(Decls, Ref, set) :- memberchk(keyed(Ref, _), Decls), !.
relation_kind(_, _, set).

% Key positions, or FAILURE when the relation carries none. Both doors rely on
% the failure rather than a default, so this must never fall through to [].
declared_key(Decls, Ref, Positions) :- memberchk(keyed(Ref, Positions), Decls).

head_ref(Head, Name/Arity) :- functor(Head, Name, Arity).

level_headed(Rules, Ref) :- member((Head <- _), Rules), head_ref(Head, Ref).

% Registry-driven, so adding an aggregate row reaches this check. The set it
% recognizes is exactly the one both doors' local recognizers already
% recognized: count, sum, min, max, json_array through the head(_) rows, plus
% json_object/2.
aggregate_head_ref(Head, Ref) :-
    compound(Head),
    Head =.. [_ | Args],
    Args \== [],
    member(Arg, Args),
    aggregate_argument(Arg),
    !,
    head_ref(Head, Ref).

aggregate_argument(Arg) :- nonvar(Arg), Arg = json_object(_, _), !.
aggregate_argument(Arg) :-
    nonvar(Arg),
    surface_for_term(Arg, _/1, aggregate, no_refs, head(_), _).

% ── the six mirrored trigger classes ─────────────────────────────────────────

% Key positions are one-based relation columns. Invalid positions previously
% survived planning and failed later while DDL selected key columns. Duplicate
% positions produced a malformed repeated-column UNIQUE constraint.
program_violation(key_position_out_of_range, prog(Decls, _),
                  key_position_out_of_range(Ref, Position, Arity)) :-
    member(keyed(Ref, Positions), Decls),
    Ref = _/Arity,
    member(Position, Positions),
    ( Position < 1 ; Position > Arity ),
    !.

program_violation(key_position_duplicate, prog(Decls, _),
                  key_position_duplicate(Ref, Position)) :-
    member(keyed(Ref, Positions), Decls),
    select(Position, Positions, Rest),
    memberchk(Position, Rest),
    !.

% A keyed relation headed by a level rule. Keys only mean replace on an edge
% write, so a level head silently accumulates instead of replacing.
program_violation(keyed_level_head, prog(Decls, Rules), Ref) :-
    member(keyed(Ref, _), Decls),
    level_headed(Rules, Ref).

% A keyed Log. A keyed relation is a Set by construction, so this is a
% contradiction in the declarations. Payload carries the positions too; the
% oracle's adapter drops them, the compiler's keeps them.
program_violation(keyed_log_rel, prog(Decls, _), Ref-Positions) :-
    member(keyed(Ref, Positions), Decls),
    declared_kind(Decls, Ref, log).

% A Log relation headed by a level rule. Level views are recomputed, so there
% is no append for the Log plane to record.
program_violation(log_on_level_headed_rel, prog(Decls, Rules), Ref) :-
    member(kind(Ref, log), Decls),
    level_headed(Rules, Ref).

% Retention is meaningful only on the Log plane.
program_violation(keep_on_non_log_rel, prog(Decls, _), Ref) :-
    member(keep(Ref, _), Decls),
    relation_kind(Decls, Ref, Kind),
    Kind \== log.

% latest/1 samples an occurrence stream; a level rule has no occurrences.
% Descends not/1, so any depth of negation still refuses (rank R1's walker).
program_violation(latest_in_level_rule, prog(_, Rules), Ref) :-
    member((_ <- Body), Rules),
    body_wrapper_refs(Body, latest,
                      walk_policy(descend_not(true), splice_bare(false)), Ref).

program_violation(pre_in_level_rule, prog(_, Rules), Ref) :-
    member((_ <- Body), Rules),
    body_wrapper_refs(Body, pre,
                      walk_policy(descend_not(true), splice_bare(false)), Ref).

% finalize/1 is a departure occurrence; a level rule has no occurrences.
% Descends not/1 like the latest/pre checks: negation does not supply a
% departure occurrence to the level plane.
program_violation(finalize_in_level_rule, prog(_, Rules), Ref) :-
    member((_ <- Body), Rules),
    body_wrapper_refs(Body, finalize,
                      walk_policy(descend_not(true), splice_bare(false)),
                      Ref).

% ── the value plane (STRUCT-AS-ROWS) ─────────────────────────────────────────

% A cycle among declared struct types. A value's content key is computed FROM
% its children's content keys, so a cyclic reference graph has no key at all
% (types-as-rels verdict: interned_graph_is_a_dag, and the crack the round-1
% lab named). The entity plane -- extrinsic ids, which DO permit cycles -- is
% out of this arc's scope, so this is a refusal, not a capability gap.
program_violation(type_cycle, prog(Decls, _), Names) :-
    type_definitions(Decls, Types),
    type_cycle_witness(Types, Names).

% A column type that is neither a primitive nor a declared struct type. The
% parser accepts a bare identifier in type position (that is what makes
% `at: span` spellable at all), so a typo would otherwise become a column
% with no storage kind at all.
program_violation(column_type_unknown, prog(Decls, _), Name) :-
    type_definitions(Decls, Types),
    declared_column_type_use(Decls, Name),
    \+ memberchk(Name, [int, text, json]),
    \+ declared_type_name(Types, Name).

% ── the two classes only the oracle used to check ────────────────────────────

% A Log relation with no keep/2 is unbounded history by accident rather than
% by declaration. The compiler accepted this until rank R2; the fixture
% engine_core.pl:log_without_retention_rejected sat in the sweep's "compiled"
% bucket against an oracle that throws.
program_violation(missing_retention, prog(Decls, _), Ref) :-
    member(kind(Ref, log), Decls),
    \+ memberchk(keep(Ref, _), Decls).

% An aggregate in an edge head. Aggregates are a grouped recomputation over a
% bag of derivations; an edge rule fires per occurrence and has no bag. The
% compiler accepted this until rank R2, letting a compound aggregate argument
% reach generic head-expression lowering.
program_violation(aggregate_in_edge_head, prog(_, Rules), Ref) :-
    member((Head <+ _), Rules),
    aggregate_head_ref(Head, Ref).

% ── helper for the value-plane classes ───────────────────────────────────────

declared_column_type_use(Decls, Name) :- member(col_type(_, _, Name), Decls).
declared_column_type_use(Decls, Name) :-
    member(type_decl(_, Specs), Decls), member(col(_, Name), Specs).
