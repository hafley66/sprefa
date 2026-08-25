% Shared implementation of invalid-program triggers checked by both doors.
%
%   program_violation(+CheckName, +Program, -Payload)
%
% succeeds once per violating witness. Program is prog(Decls, Rules). Payload
% is whatever that class needs to name the offender, in a shape both doors can
% project from; it is NOT an exception term.
%
:- module(program_check,
          [ program_violation/3,
            first_violation/3,
            ast_capture_names/2,
            % Declaration queries shared by both doors.
            relation_kind/3,
            declared_key/3,
            level_headed/2
          ]).

:- use_module(library(lists)).
:- use_module(library(pcre)).
:- use_module('../1_expand/0_body_walk',
              [ body_wrapper_refs/4, walk_body/3, body_relation_atoms/4,
                event_relation_atom/2, body_reserved_word/4 ]).
:- use_module('../../0_type_plane',
              [ type_definitions/2, type_cycle_witness/2, declared_type_name/2,
                type_definition/4, relation_columns_and_types/5, column_storage/3,
                relation_value_shape/3, relation_value_term/4 ]).
:- use_module('../../compile/registry', [surface_for_term/6, surface/5]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- discontiguous program_violation/3.

% ── the ordered driver ───────────────────────────────────────────────────────

% First violation in the caller's declared order, as violation(Name, Payload).
% Fails when the program violates none of the listed classes.
first_violation(Program, OrderedChecks, violation(Name, Payload)) :-
    member(Name, OrderedChecks),
    program_violation(Name, Program, Payload),
    !.

ordered_aggregate_name(json_group_array).
ordered_aggregate_name(group_concat).

% ── declaration queries these triggers share ─────────────────────────────────
% Kept here rather than in either door so the fallback-to-set rule has ONE
% statement. A relation is a Log only when declared so; a keyed relation is a
% Set by construction; everything undeclared is a Set.

declared_kind(Decls, Ref, Kind) :- memberchk(kind(Ref, Kind), Decls).

% Every clause below the first answers `set`, so a ground reference needs one
% memberchk/2 over the declarations and not three, two of which fail and walk
% the whole list. A non-ground reference can be BOUND by the second and third
% scans, so it keeps them.
relation_kind(Decls, Ref, Kind) :-
    ground(Ref),
    !,
    (   declared_kind(Decls, Ref, log)
    ->  Kind = log
    ;   Kind = set
    ).
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
    surface_for_term(Arg, _/_Arity, aggregate, no_refs, head(_), _).

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

program_violation(regexp_pattern_not_literal, prog(_, Rules),
                  regexp_pattern_not_literal) :-
    member(Rule, Rules),
    rule_body_goal(Rule, regexp(_, Pattern)),
    \+ string(Pattern),
    !.

program_violation(regexp_pattern_outside_subset, prog(_, Rules),
                  regexp_pattern_outside_subset(Pattern)) :-
    member(Rule, Rules),
    rule_body_goal(Rule, regexp(_, Pattern)),
    string(Pattern),
    regexp_pattern_outside_subset(Pattern),
    !.

program_violation(regexp_pattern_invalid, prog(_, Rules),
                  regexp_pattern_invalid(Pattern, Message)) :-
    member(Rule, Rules),
    rule_body_goal(Rule, regexp(_, Pattern)),
    string(Pattern),
    \+ regexp_pattern_outside_subset(Pattern),
    regexp_pattern_pcre_error(Pattern, Message),
    !.

program_violation(cst_capture_unused, prog(_, Rules), Name) :-
    member(Rule, Rules),
    rule_body_goal(Rule,
                   cst(_, _, _, _, cst_bindings(CaptureNames, _, RuleNames))),
    member(Name, CaptureNames),
    \+ memberchk(Name, RuleNames),
    !.

program_violation(cst_variable_uncaptured, prog(_, Rules), Name) :-
    member(Rule, Rules),
    rule_body_goal(Rule,
                   cst(_, _, _, _, cst_bindings(CaptureNames, CandidateNames,
                                                 _))),
    member(Name, CandidateNames),
    \+ memberchk(Name, CaptureNames),
    !.

program_violation(cst_regexp_pattern_not_literal, prog(_, Rules),
                  regexp_pattern_not_literal) :-
    member(Rule, Rules),
    rule_body_goal(Rule, cst(_, _, _, Query, _)),
    cst_regexp_pattern(Query, Pattern),
    \+ string(Pattern),
    !.

program_violation(cst_regexp_pattern_outside_subset, prog(_, Rules),
                  regexp_pattern_outside_subset(Pattern)) :-
    member(Rule, Rules),
    rule_body_goal(Rule, cst(_, _, _, Query, _)),
    cst_regexp_pattern(Query, Pattern),
    string(Pattern),
    regexp_pattern_outside_subset(Pattern),
    !.

program_violation(cst_regexp_pattern_invalid, prog(_, Rules),
                  regexp_pattern_invalid(Pattern, Message)) :-
    member(Rule, Rules),
    rule_body_goal(Rule, cst(_, _, _, Query, _)),
    cst_regexp_pattern(Query, Pattern),
    string(Pattern),
    \+ regexp_pattern_outside_subset(Pattern),
    regexp_pattern_pcre_error(Pattern, Message),
    !.

cst_regexp_pattern(Query, Pattern) :-
    sub_term(predicate(Kind, _, Right), Query),
    memberchk(Kind, [match, not_match]),
    ( Right = string(Pattern) -> true ; Pattern = Right ).

program_violation(regexp_operand_not_text, prog(Decls, Rules),
                  regexp_operand_not_text(Ref, Column, Type)) :-
    type_definitions(Decls, Types),
    declared_column_table(Decls, Types, Rules, Table),
    member(Rule, Rules),
    rule_body_goal(Rule, regexp(Operand, _)),
    var(Operand),
    rule_body_column_variable(Table, Rule, BodyVariable, Ref, Column, Type),
    BodyVariable == Operand,
    Type \== text,
    !.

program_violation(ast_query_not_literal, prog(_, Rules), ast_query_not_literal) :-
    member(Rule, Rules),
    rule_body_goal(Rule, ast(_, _, _, Query)),
    \+ string(Query),
    !.

program_violation(ast_lang_unknown, prog(_, Rules), Lang) :-
    member(Rule, Rules),
    rule_body_goal(Rule, ast(_, _, Lang, _)),
    (   \+ atom(Lang)
    ;   \+ memberchk(Lang, [rust, ts, tsx, js, go, kotlin, md, md_inline, html])
    ),
    !.

program_violation(ast_query_single_quote, prog(_, Rules), ast_query_single_quote) :-
    member(Rule, Rules),
    rule_body_goal(Rule, ast(_, _, _, Query)),
    string(Query),
    sub_string(Query, _, 1, _, "'"),
    !.

program_violation(ast_no_named_capture, prog(_, Rules), ast_no_named_capture) :-
    member(Rule, Rules),
    rule_body_goal(Rule, ast(_, _, _, Query)),
    string(Query),
    ast_capture_names(Query, []),
    !.

ast_capture_names(Query, Names) :-
    string_codes(Query, Codes),
    ast_capture_codes(Codes, [], ReversedNames),
    reverse(ReversedNames, Names).

ast_capture_codes([], Names, Names).
ast_capture_codes([0'@ | Rest], Seen, Names) :-
    ast_capture_identifier(Rest, IdentifierCodes, Remaining),
    IdentifierCodes \== [],
    atom_codes(Name, IdentifierCodes),
    ( memberchk(Name, Seen)
    -> ast_capture_codes(Remaining, Seen, Names)
    ; ast_capture_codes(Remaining, [Name | Seen], Names)
    ),
    !.
ast_capture_codes([_ | Rest], Seen, Names) :-
    ast_capture_codes(Rest, Seen, Names).

ast_capture_identifier([Code | Rest], [Code | More], Remaining) :-
    ast_identifier_start(Code),
    ast_identifier_tail(Rest, More, Remaining),
    !.
ast_capture_identifier(Rest, [], Rest).

ast_identifier_tail([Code | Rest], [Code | More], Remaining) :-
    ast_identifier_code(Code),
    !,
    ast_identifier_tail(Rest, More, Remaining).
ast_identifier_tail(Rest, [], Rest).

ast_identifier_start(Code) :- code_type(Code, alpha), !.
ast_identifier_start(0'_).

ast_identifier_code(Code) :- code_type(Code, alnum), !.
ast_identifier_code(0'_).

regexp_pattern_outside_subset(Pattern) :-
    string_codes(Pattern, Codes),
    \+ regexp_subset_codes(Codes).

regexp_subset_codes([]).
regexp_subset_codes([92, Escape | Rest]) :-
    memberchk(Escape, [0'd, 0'w, 0's, 0'b, 0'., 0'\\, 0'+, 0'*, 0'?,
                       0'(, 0'), 0'[, 0'], 0'{, 0'}, 0'|, 0'^, 0'$]),
    !,
    regexp_subset_codes(Rest).
regexp_subset_codes([92 | _]) :- !, fail.
regexp_subset_codes([40, 63 | _]) :- !, fail.
regexp_subset_codes([Quantifier, 43 | _]) :-
    memberchk(Quantifier, [0'*, 0'+, 0'?, 0'}]),
    !,
    fail.
regexp_subset_codes([_ | Rest]) :-
    regexp_subset_codes(Rest).

regexp_pattern_pcre_error(Pattern, Message) :-
    catch(( re_compile(Pattern, _, []), fail ),
          Error,
          message_to_string(Error, Message)).

% ── the value plane (STRUCT-AS-ROWS) ─────────────────────────────────────────

% A cycle among declared struct types. A value's content key is computed FROM
% its children's content keys, so a cyclic reference graph has no key at all
% (types-as-rels verdict: interned_graph_is_a_dag, and the crack the round-1
% lab named). The entity plane -- extrinsic ids, which DO permit cycles -- is
% out of this arc's scope, so this is a unsupported construct, not a capability gap.
program_violation(type_cycle, prog(Decls, _), Names) :-
    type_definitions(Decls, Types),
    type_cycle_witness(Types, Names).

% A column type that is neither a primitive nor a declared struct type. The
% parser accepts a bare identifier in type position (that is what makes
% `at: span` spellable at all), so a typo would otherwise become a column
% with no storage kind at all.
% json_list(T) and list(T) are checked by 0_type_plane.pl:column_storage/3
% instead of by the primitive list here: their two failure modes carry reasons
% (list_of_relation_refs vs list_element_not_scalar) and collapsing both into
% column_type_unknown would lose which one it was.
program_violation(column_type_unknown, prog(Decls, _), Name) :-
    type_definitions(Decls, Types),
    declared_column_type_use(Decls, Name),
    \+ memberchk(Name, [int, text, json, bool, float, bytes]),
    \+ Name = json_list(_),
    \+ Name = list(_),
    \+ Name = id(_),
    \+ anonymous_column_type(Name),
    \+ declared_template_application(Decls, Name),
    \+ declared_type_name(Types, Name).

anonymous_column_type(product_type(_)).
anonymous_column_type(sum_type(_)).
anonymous_column_type(arrow_type(_, _)).

declared_template_application(Decls, Application) :-
    compound(Application),
    functor(Application, Name, Arity),
    member(rel_template(Segments, Parameters, _), Decls),
    atomic_list_concat(Segments, '__', Name),
    length(Parameters, Arity).

% An argument sitting in a ref-typed column that is not a relation value of
% that column's type. `span(file(Repo, At), Start, End)` is a relation value;
% `span('src/a.rs', ...)`, `span(fpath(Name), ...)` and `span(file(Repo), ...)`
% are three ways of not being one.
%
% This exists because the shape used to be caught only BY ACCIDENT. The emitter
% compiled any compound argument into `json_extract(<column>, '$.fn') = ...`;
% for a ref column that column holds an INTEGER endpoint, so the extract was
% always NULL and the rule answered nothing. The pre-existing
% join_column_type_mismatch unsupported construct fired only when the phantom TEXT expression
% happened to land against an INT column -- a coincidence of the OTHER
% operand's typing. Against a text column, and `path` is text, nothing fired.
% (plans/2026-07-30-file-span-spine-reconciled.md section 3.2.)
%
% The search descends through well-formed relation values, so a bad leaf under
% a good parent is reported at the leaf, naming the leaf's own column.
program_violation(relation_pattern_not_a_relation_value, prog(Decls, Rules),
                  pattern(Ref, Column, TypeName, Value)) :-
    type_definitions(Decls, Types),
    Types \== [],
    member(Rule, Rules),
    rule_relation_atom(Rule, Atom),
    compound(Atom),
    functor(Atom, Name, Arity),
    AtomRef = Name/Arity,
    relation_columns_and_types(Decls, Types, AtomRef, Columns, ColumnTypes),
    nth1(Position, ColumnTypes, DeclaredType),
    declared_type_name(Types, DeclaredType),
    nth1(Position, Columns, AtomColumn),
    arg(Position, Atom, Argument),
    relation_argument_violation(Types, AtomRef, AtomColumn, DeclaredType,
                                Argument, pattern(Ref, Column, TypeName, Value)),
    !.

program_violation(reserved_relation_value_carrier, prog(Decls, _), obj/1) :-
    declared_ref(Decls, obj/1).

declared_ref(Decls, Ref) :-
    member(Decl, Decls),
    ( Decl = kind(Ref, _)
    ; Decl = keyed(Ref, _)
    ; Decl = keep(Ref, _)
    ; Decl = col_type(Ref, _, _)
    ),
    !.

% The SAME law one hop out, where the offending value is not written down.
% relation_argument_violation/6 opens with nonvar(Value), so the class above
% sees only concrete arguments; a VARIABLE carrying a text leaf into a ref
% column passed both doors and the emitter wrote that text straight into a
% column its own DDL declares INTEGER NOT NULL and puts in the primary key
% (burr B1 of plans/2026-07-30-relpattern-adversarial-review.md). sqlite stores
% text in an integer-affinity column without complaint, and the boundary render
% then read it as a dictionary id.
%
% A variable is not refused for being a variable. What is refused is a variable
% appearing at a ref-typed column AND at some other declared column of a
% different type in the same rule, which is a contradiction whatever the two
% types are: the surrogate id is storage, never a value anything else may hold
% (the types-round-2 surrogate-mate ruling), so no program can want a variable
% to be both a `file` and a text at once.
%
% The report is taken from the REF side: that column is the one whose declared
% type the program is contradicting, and naming it matches the concrete class
% above, which also names the ref column rather than whatever the phantom
% expression collided with.
%
% Scope, stated: only DECLARED column types take part. An undeclared column has
% no type here to contradict (0_program_check.pl sees prog/2 and never the
% literal witnesses analyze.pl infers from), so nothing is asserted about it,
% and this class stays a statement about what the program itself wrote down.
program_violation(relation_column_type_conflict, prog(Decls, Rules),
                  conflict(Ref, Column, TypeName, OtherRef, OtherColumn, OtherType)) :-
    type_definitions(Decls, Types),
    Types \== [],
    member(Rule, Rules),
    rule_column_variable(Decls, Types, Rule, Variable, Ref, Column, TypeName),
    declared_type_name(Types, TypeName),
    rule_column_variable(Decls, Types, Rule, Other, OtherRef, OtherColumn, OtherType),
    Other == Variable,
    OtherType \== TypeName,
    \+ column_type_assignable(Types, TypeName, OtherType),
    !.

% ── the head column type wall (ruling type_gate_widening) ────────────────────
%
% A variable carries a value out of a body column and into a HEAD column. When
% both columns are declared and their storage disagrees, the two doors used to
% answer differently and neither answer was defensible: the reference engine
% keeps whatever term the body held, while the emitted program writes it into a
% column of the other affinity, so the same rule prints `4` on one door and
% `"4"` on the other, or the emitted CHECK on a bool column drops the row
% entirely and the tick log just has one fewer row in it. 41 of the type
% matrix's divergent cells were this one shape at a level head or an aggregate
% head.
%
% The edge head has had this wall since PHASE C2 (analyze.pl:
% check_edge_head_column_types/2, which reads INFERRED types and so is a
% compiler capability check). This one is the DECLARED-type statement, it is
% shared, and it is what the ruling widens to the remaining head positions.
%
% What is allowed to mix is SQLite's own numeric widening, and only in the
% direction affinity performs losslessly: an int column's value entering a
% float column becomes a double. The reverse is a unsupported construct -- REAL to INTEGER
% converts only when the value happens to have no fractional part, so the same
% program would keep or lose a row depending on the data.
%
% SCOPE, stated: only DECLARED types take part, both sides. An undeclared
% column has no type here to contradict (the divergence undeclared columns
% still show is a different open question -- what a bare column's default
% should be -- and is not this ruling's).
% COST: the first draft called relation_columns_and_types/5 inside BOTH loops,
% and each call is a findall over the whole Decls list, so the work was
% quadratic in declarations for no reason. The declared column table is built
% ONCE per program here and the loops index it.
%
% Honest about the measurement: `just compile-speed` reports a parse-phase
% regression on all four pinned programs, and that regression is NOT this. It
% reproduces at base a87ca936 with this arc's changes stashed, to the
% inference, so the baseline is stale against main and this check does not
% appear in any gated phase's numbers at all.
program_violation(head_column_type_conflict, prog(Decls, Rules),
                  conflict(HeadRef, HeadColumn, HeadType,
                           BodyRef, BodyColumn, BodyType)) :-
    type_definitions(Decls, Types),
    declared_column_table(Decls, Types, Rules, Table),
    Table \== [],
    member(Rule, Rules),
    rule_head_column_variable(Table, Rule, HeadVariable,
                              HeadRef, HeadColumn, HeadType),
    rule_body_column_variable(Table, Rule, BodyVariable,
                              BodyRef, BodyColumn, BodyType),
    BodyVariable == HeadVariable,
    \+ column_type_assignable(Types, BodyType, HeadType),
    !.


% ── two shapes that used to be a door DISAGREEMENT ───────────────────────────
%
% Burrs B3, B4 and B9 of plans/2026-07-30-relpattern-adversarial-review.md. The
% compiler refused a relation value under not/1 (relation_pattern_not_lowerable)
% and in an edge rule (relation_value_in_edge_rule); the reference engine ran
% both. Neither appeared in any graded fixture, so nothing in the corpus ever
% put the two answers side by side, and the review had to run them by hand to
% find out they differed.
%
% CHOSEN: refuse on BOTH doors, not implement the lowering. Stated plainly
% because it is a capability decision and not a cleanup:
%
%   Implementing them is not small. A relation value under not/1 needs its
%   dictionary joins scoped INSIDE the NOT EXISTS subquery -- hoisting them to
%   the outer body, which is where the rewrite appends them today, turns "no
%   such fpath exists, therefore not(...) holds" into "no rows at all", the
%   opposite answer. An edge rule has no dictionary-join seam at all:
%   edge_statements_for_rule/4 compiles trigger occurrences against RelPlans,
%   and the per-level joins have nowhere to go. Both are new execution shape.
%
%   A door disagreement is the worse of the two states. A program that the
%   reference engine runs and the compiler rejects cannot be graded, so its
%   behaviour is whatever the engine happens to do, unpinned. Refusing on both
%   doors makes the boundary a fact of the LANGUAGE, one name, one fixture,
%   gradeable -- and deleting a shared unsupported construct is how the capability arrives
%   later, with the fixtures flipping from throws/1 to rows.
%
% The compiler's own residue guards stay where they are, as the last-resort
% backstop for anything entering lower.pl directly (compile/test/plunit_tests.pl
% calls lower_program/2 with a hand-built plan and reaches exactly that path).
program_violation(relation_value_under_negation, prog(Decls, Rules),
                  pattern(Ref, Column, TypeName, Value)) :-
    type_definitions(Decls, Types),
    Types \== [],
    member(Rule, Rules),
    rule_body(Rule, Body),
    body_relation_atoms(Body,
                        walk_policy(descend_not(true), splice_bare(true)),
                        neg, Atom),
    relation_value_in_ref_column(Decls, Types, Atom, Ref, Column, TypeName, Value),
    !.

program_violation(relation_value_in_edge_rule, prog(Decls, Rules),
                  pattern(Ref, Column, TypeName, Value)) :-
    type_definitions(Decls, Types),
    Types \== [],
    member(Rule, Rules),
    rule_is_edge(Rule),
    rule_relation_atom(Rule, Atom),
    relation_value_in_ref_column(Decls, Types, Atom, Ref, Column, TypeName, Value),
    !.

% A HIGHER-ORDER goal: a body goal that names the relation it reads in an
% ARGUMENT rather than as its own functor. `call/N` is the whole family
% (`call(src, Name, Value)`, `call(Rel, Name, Value)`, `call(src(Name, Value))`)
% and it has no registry row, so nothing recognized it as a construct at all.
% What claimed it instead was the edb_definition ruling: a rel no rule heads is
% pure input, so `call/3` became a REAL relation with synthesized columns and a
% real table that no world push ever fills. Three spellings, zero rows, zero
% unsupported construct, on both doors.
%
% Refused by name, and by the name the question already has:
% labs/generic_scan_instantiation reached the same boundary from the other
% side and called it dynamic_relation_name -- a relation name is a ground
% functor the program writes down, never a value carried in an argument.
%
% WHAT KEEPS THIS NARROW, and it has to be kept narrow: `call` is a legal
% relation name here and the alpha's own flagship uses one
% (fixtures/3_flagship_callgraph.pl declares `call/2`, straight out of v5's
% examples/callgraph-ast.dl). So the trigger is an UNDECLARED, UNHEADED
% `call/N` goal. A program that means the relation says so, the way it says so
% for every other relation; a program that declares nothing and writes
% prolog's own apply functor meant the apply, and gets told the language has
% no such thing instead of getting an empty table.
%
% This is a REFUSAL and not a construct: nothing new is spellable, one silent
% wrong answer becomes an error. Scope is exactly `call`; the wider class (any
% reserved registry word silently deriving nothing on the ORACLE door, which
% `zip/2` also does today) is answered one class below, by reserved_body_word.
program_violation(dynamic_relation_name, prog(Decls, Rules), call/Arity) :-
    member(Rule, Rules),
    rule_body_goal(Rule, Goal),
    nonvar(Goal),
    functor(Goal, call, Arity),
    Arity >= 1,
    \+ declared_relation(Decls, call/Arity),
    \+ headed_relation(Rules, call/Arity),
    !.

% A RESERVED registry word used as a body goal. The compiler already refused
% these by name (analyze.pl's own reserved_construct_in_body/2, now this
% class's compiler-side adapter); the reference engine had no clause for any
% of them, so `zip`, `subscribe`, `complete`, `unsubscribe` and `error` fell
% through solve/2's final clause and were looked up as ORDINARY RELATION ATOMS
% named `zip/2`, `subscribe/1` and so on. No such row is ever pushed, so the
% rule derived nothing, silently, on the door that defines the language.
%
% Fail-first receipt, oracle door, before this class existed:
%
%   F2 zip: rows=[]            F2 subscribe: rows=[]
%   F2 complete: rows=[]       F2 unsubscribe: rows=[]
%   F2 error: rows=[]
%
% against the compiler, which already answered
% `unsupported_construct(zip)` / `unsupported_construct(lifecycle_arm(...))`
% for the same five programs.
%
% WHY THIS IS NARROW, and it has to be, because of the edb_definition ruling:
% an UNDECLARED, UNHEADED relation atom is legal input, not a typo. So the
% trigger is not "a functor solve/2 has no clause for" -- that set is every
% EDB relation in every program. It is exactly the functors registry.pl marks
% `reserved`: words the language has CLAIMED and assigned no meaning. A word
% with no registry row at all stays a relation, which is what
% edb_definition says it is.
%
% `refused` rows are deliberately NOT included. pre/1 and json_each/2 carry
% that status because THIS COMPILER cannot emit them, and the reference engine
% executes both on purpose -- the oracle is the wider language
% (level_eval.pl's aggregate classification states the same asymmetry). A
% status-blind gate here would refuse programs the oracle is meant to run.
%
% POLICY, stated because it is a boundary and not an oversight: the walk does
% not descend not/1 and does not splice, which is the policy analyze.pl's
% reserved scan already used and whose narrowness that file already names. A
% reserved word nested inside not/1 or inside a splice is therefore still not
% reached, on either door -- unchanged by this class, and unchanged in
% behaviour too, since solve/2 reaches such a goal and looks it up as a
% relation exactly as before.
program_violation(reserved_body_word, prog(_, Rules), reserved(Ref, LowerRole)) :-
    member(Rule, Rules),
    rule_body(Rule, Body),
    body_reserved_word(Body,
                       walk_policy(descend_not(false), splice_bare(false)),
                       Ref, LowerRole),
    !.

% ── the two classes only the oracle used to check ────────────────────────────

% A Log relation with no keep/2 is unbounded history by accident rather than
% by declaration. The compiler accepted this until rank R2; the fixture
% engine_core.pl:log_without_retention_rejected sat in the sweep's "compiled"
% bucket against an oracle that throws.
program_violation(missing_retention, prog(Decls, _), Ref) :-
    member(kind(Ref, log), Decls),
    \+ memberchk(keep(Ref, _), Decls).

% A bounded log head prunes at tick end, so with two or more edge arms the
% surviving row is the one the last-running arm wrote, and arm order is source
% order. Broader than the keyed sibling edge_head_conflict_risk, which requires
% the arms to share a trigger ref because a keyed conflict is per occurrence:
% this bound applies per TICK, so arms on different triggers still collide.
% Every N, not only 1: with N kept and more arms than that, order still decides
% which N survive.
program_violation(retention_head_conflict_risk, prog(Decls, Rules), Ref-count(N)) :-
    member(keep(Ref, count(N)), Decls),
    relation_kind(Decls, Ref, log),
    findall(Ref, ( member(Rule, Rules), Rule = (Head <+ _),
                   head_ref(Head, Ref) ), Arms),
    Arms = [_, _ | _].

% An aggregate in an edge head. Aggregates are a grouped recomputation over a
% bag of derivations; an edge rule fires per occurrence and has no bag. The
% compiler accepted this until rank R2, letting a compound aggregate argument
% reach generic head-expression lowering.
program_violation(aggregate_in_edge_head, prog(_, Rules), Ref) :-
    member((Head <+ _), Rules),
    aggregate_head_ref(Head, Ref).

program_violation(aggregate_head_shape, prog(_, Rules), Name/Arity) :-
    member((Head <- _), Rules),
    compound(Head),
    Head =.. [_ | Args],
    member(Arg, Args),
    nonvar(Arg),
    functor(Arg, Name, Arity),
    ordered_aggregate_name(Name),
    \+ surface(Name/Arity, aggregate, _, _, _),
    !.

% An aggregate spelling on the registry's aggregate axis that NEITHER door
% implements. registry.pl marks it head(refuse(not_implemented)), which is a
% different statement from json_array's head(refuse(aggregate)): that pair is
% computed by the reference engine and refused only by the compiler, so the
% oracle stays the wider language, while this class is a word both doors know
% the name of and neither can evaluate.
%
% FAIL-FIRST RECEIPT, `roster(group_concat(Name)) <- member(Name)` with two
% members, before the registry row and this class existed:
%
%   oracle    rows=[out(group_concat(1)), out(group_concat(2))]
%   compiler  COMPILED CLEAN, emitting
%             json_object('fn','group_concat','args',json_array(b0."col1"))
%
% One row per input holding the TEXT of the call, where the author asked for
% one grouped row. It is the worst shape a unsupported construct can replace: not an error,
% not empty, a plausible-looking wrong answer.
%
% The payload carries the aggregates that DO work, read off the registry so it
% cannot go stale, because that list is the only thing this unsupported construct can tell a
% cold author that they can act on -- the one-line message renderer has no
% room for per-unsupported construct prose and the unsupported_messages unit holds it to a
% single clause.
%
% LEVEL rules only. An aggregate in an EDGE head is already
% aggregate_in_edge_head above, which is the more specific fact about that
% program, and both doors already agree on it.
program_violation(aggregate_not_implemented, prog(_, Rules),
                  unimplemented(Ref, Signature, Implemented)) :-
    member((Head <- _), Rules),
    compound(Head),
    Head =.. [_ | Args],
    member(Arg, Args),
    nonvar(Arg),
    surface_for_term(Arg, Signature, aggregate, _,
                     head(refuse(not_implemented)), _),
    head_ref(Head, Ref),
    implemented_aggregates(Implemented),
    !.

% A numeric aggregate reading a column whose declared type is not a number.
% sum/avg/min/max are exactly the set lower.pl runs through
% compile_aggregate_number_operand/5, and it refuses each one on the same
% condition; this is that statement on the shared trigger so the reference
% engine answers it too. Before this class the oracle reached
% level_eval.pl:agg_compute/3 and died inside lists:min_list/3 with
% error(type_error(evaluable, alpha/0), ...), which is not an answer about
% the program at all.
%
% SCOPE, stated, and it is the same scope the two column-type conflict classes
% above already state: only DECLARED types take part. 0_program_check.pl sees
% prog/2 and never the literal witnesses analyze.pl infers from, so an
% UNDECLARED text column stays outside this class on the oracle door while the
% compiler still refuses it off inference. That residue is caught one layer
% down by level_eval.pl's own value guard, which names it rather than throwing
% a SWI evaluable error.
%
% The operand must be a bare variable. min(A + B) and friends are the
% compiler's expression typing, which has no shared statement, and unwrapping
% one level further is the mistake rule_head_column_variable/6 records having
% made against nine struct fixtures.
program_violation(aggregate_operand_not_number, prog(Decls, Rules),
                  operand(Kind, BodyRef, BodyColumn, BodyType)) :-
    type_definitions(Decls, Types),
    declared_column_table(Decls, Types, Rules, Table),
    Table \== [],
    member(Rule, Rules),
    Rule = (Head <- _),
    compound(Head),
    Head =.. [_ | Args],
    member(Arg, Args),
    nonvar(Arg),
    numeric_aggregate_operand(Arg, Kind, Operand),
    var(Operand),
    rule_body_column_variable(Table, Rule, BodyVariable,
                              BodyRef, BodyColumn, BodyType),
    BodyVariable == Operand,
    \+ number_column_type(Types, BodyType),
    !.

numeric_aggregate_operand(Arg, Kind, Operand) :-
    functor(Arg, Kind, 1),
    memberchk(Kind, [sum, avg, min, max]),
    arg(1, Arg, Operand).

% column_storage/3 cuts on its first two arguments and throws on an
% unrecognized type, so the storage has to land in a fresh variable and be
% compared afterwards (the same shape column_type_assignable/3 uses).
number_column_type(Types, Type) :-
    column_storage(Types, Type, Storage),
    memberchk(Storage, [int, float]).

% The aggregate rows that lower, sorted: the registry's own answer to "what
% can I write instead".
implemented_aggregates(Names) :-
    findall(Name,
            surface_row_is_live_aggregate(Name),
            Names0),
    sort(Names0, Names).

surface_row_is_live_aggregate(Name) :-
    surface(Name/Arity, aggregate, _, head(lower), live),
    integer(Arity).

% ── helper for the value-plane classes ───────────────────────────────────────

declared_column_type_use(Decls, Name) :- member(col_type(_, _, Name), Decls).
declared_column_type_use(Decls, Name) :-
    member(type_decl(_, Specs), Decls), member(col(_, Name), Specs).
declared_column_type_use(Decls, Name) :-
    member(sh_decl(_, Inputs, Outputs, _), Decls),
    ( member(col(_, Name), Inputs) ; member(col(_, Name), Outputs) ).

% ── helpers for the relation-pattern class ───────────────────────────────────

% Head and every relation atom a body reaches, including the atom inside a
% latest/pre/finalize wrapper and inside any depth of not/1. A registry
% construct that is not one of those wrappers (decode/2, a comparison, `:=`)
% carries no rel columns at all -- its arguments are patterns and expressions
% -- so it is skipped rather than walked into.
rule_relation_atom((Head <- _), Head).
rule_relation_atom((Head <+ _), Head).
rule_relation_atom((_ <- Body), Atom) :- body_relation_atom(Body, Atom).
rule_relation_atom((_ <+ Body), Atom) :- body_relation_atom(Body, Atom).

% A relation the program WRITES DOWN: one column declaration per position, or
% a type declaration of the same width. Either is the program saying this name
% is a relation of mine.
declared_relation(Decls, Name/Arity) :-
    type_definitions(Decls, Types),
    relation_columns_and_types(Decls, Types, Name/Arity, Columns, _),
    length(Columns, Arity),
    Arity > 0.

headed_relation(Rules, Ref) :-
    (   member((Head <- _), Rules)
    ;   member((Head <+ _), Rules)
    ),
    head_ref(Head, Ref),
    !.

% A well-formed relation VALUE sitting in a ref-typed column of one atom.
% Factored out because three classes ask the same question of three different
% atom sets (every atom, the negated ones, the ones in an edge rule).
relation_value_in_ref_column(Decls, Types, Atom, Name/Arity, Column, TypeName, Value) :-
    compound(Atom),
    functor(Atom, Name, Arity),
    relation_columns_and_types(Decls, Types, Name/Arity, Columns, ColumnTypes),
    length(Columns, Arity),
    nth1(Position, ColumnTypes, TypeName),
    declared_type_name(Types, TypeName),
    nth1(Position, Columns, Column),
    arg(Position, Atom, Value),
    relation_value_shape(Types, TypeName, Value).

rule_is_edge((_ <+ _)).

rule_body((_ <- Body), Body).
rule_body((_ <+ Body), Body).

% Every body GOAL, whatever its surface class, including inside not/1 and
% inside a splice. Unlike body_relation_atom/2 below this does not care what
% the goal turns out to be, which is the point: the higher-order class is
% about a functor nothing else in the pipeline recognizes.
rule_body_goal(Rule, Goal) :- rule_body(Rule, Body), body_goal(Body, Goal).

body_goal(Body, Goal) :-
    walk_body(Body, walk_policy(descend_not(true), splice_bare(true)), Events),
    member(event(_, _, _, Goal), Events).

% Rank B11: the wrapper family lives in 0_body_walk.pl now, in one place, and
% this is the shared projection over it.
body_relation_atom(Body, Atom) :-
    body_relation_atoms(Body,
                        walk_policy(descend_not(true), splice_bare(true)),
                        _, Atom).

% Every VARIABLE argument of every relation atom a rule reaches, with the
% column it sits in and that column's declared type. Variable identity is the
% clause's own, so two occurrences of the same source variable are `==` and two
% anonymous `_` are not, which is exactly the flow question being asked.
% One entry per DISTINCT relation reference the rules mention, resolved once.
% A ref whose columns are not fully declared simply has no entry, which is the
% same all-or-nothing rule ref_column_names/4 applies.
declared_column_table(Decls, Types, Rules, Table) :-
    findall(Ref,
            ( member(Rule, Rules), rule_relation_atom(Rule, Atom),
              compound(Atom), functor(Atom, Name, Arity), Ref = Name/Arity ),
            Refs0),
    sort(Refs0, Refs),
    findall(rel_columns(Ref, Columns, ColumnTypes),
            ( member(Ref, Refs),
              Ref = _/Arity,
              relation_columns_and_types(Decls, Types, Ref, Columns, ColumnTypes),
              length(Columns, Arity) ),
            Table).



% The head's own declared columns. A head argument takes part only when the
% column type is a statement ABOUT THAT VARIABLE'S value:
%
%   bare variable    the value travels unchanged; checked
%   min(V) / max(V)  min and max return an element of the input, so the head
%                    column types V; checked through the wrapper
%
% Nothing else. count/sum/avg return a number the input's type says nothing
% about, group_concat and json_group_array return text and json whatever went
% in, an arithmetic head expression computes a new value, and a RELATION VALUE
% (`file(repo(Name), Path)`) puts its inner variable in the TARGET type's own
% column, not in the ref column that holds it -- the first draft of this
% predicate unwrapped every compound one level and refused nine struct
% fixtures and one aggregate fixture on exactly that mistake.
rule_head_column_variable(Table, Rule, Variable, Ref, Column, Type) :-
    rule_head(Rule, Head),
    compound(Head),
    functor(Head, Name, Arity),
    Ref = Name/Arity,
    memberchk(rel_columns(Ref, Columns, ColumnTypes), Table),
    nth1(Position, ColumnTypes, Type),
    nth1(Position, Columns, Column),
    arg(Position, Head, Argument),
    head_argument_variable(Argument, Variable).

head_argument_variable(Argument, Argument) :- var(Argument), !.
head_argument_variable(Argument, Variable) :-
    nonvar(Argument),
    ( Argument = min(Inner) ; Argument = max(Inner) ),
    var(Inner),
    Variable = Inner.

rule_body_column_variable(Table, Rule, Variable, Ref, Column, Type) :-
    rule_body(Rule, Body),
    body_relation_atom(Body, Atom),
    compound(Atom),
    functor(Atom, Name, Arity),
    Ref = Name/Arity,
    memberchk(rel_columns(Ref, Columns, ColumnTypes), Table),
    nth1(Position, ColumnTypes, Type),
    nth1(Position, Columns, Column),
    arg(Position, Atom, Variable),
    var(Variable).

rule_head((Head <- _), Head).
rule_head((Head <+ _), Head).

% Storage, not spelling: `json_list(text)` and `json` are one column kind, so a
% value moving between them is not a type mix. The one asymmetric pair is the
% affinity widening.
% column_storage/3 THROWS column_type_unknown on an unrecognized type and its
% clauses are cut on the first two arguments, so both storages have to be
% computed into fresh variables and compared afterwards. Passing a bound third
% argument makes every non-matching clause fall through to that throw.
column_type_assignable(Types, From, To) :-
    column_storage(Types, From, FromStorage),
    column_storage(Types, To, ToStorage),
    storage_assignable(FromStorage, ToStorage).

storage_assignable(Storage, Storage) :- !.
storage_assignable(ref(Name), idref(Name)) :- !.
storage_assignable(idref(Name), ref(Name)) :- !.
storage_assignable(json_list(_), json) :- !.
storage_assignable(json, json_list(_)) :- !.
storage_assignable(int, float).


rule_column_variable(Decls, Types, Rule, Argument, Ref, Column, Type) :-
    rule_relation_atom(Rule, Atom),
    compound(Atom),
    functor(Atom, Name, Arity),
    Ref = Name/Arity,
    relation_columns_and_types(Decls, Types, Ref, Columns, ColumnTypes),
    length(Columns, Arity),
    nth1(Position, ColumnTypes, Type),
    nth1(Position, Columns, Column),
    arg(Position, Atom, Argument),
    var(Argument).

% One argument in a ref-typed column. A well-formed relation value is not a
% violation by itself; the search continues INTO it, so the reported column is
% the innermost one that actually holds the bad term.
relation_argument_violation(Types, Ref, Column, TypeName, Value, Violation) :-
    nonvar(Value),
    (   relation_value_term(Types, TypeName, Value, Term)
    ->  type_definition(Types, TypeName, Columns, ColumnTypes),
        length(Columns, Arity),
        nth1(Position, ColumnTypes, ChildType),
        declared_type_name(Types, ChildType),
        nth1(Position, Columns, ChildColumn),
        arg(Position, Term, ChildValue),
        relation_argument_violation(Types, TypeName/Arity, ChildColumn,
                                    ChildType, ChildValue, Violation)
    ;   Violation = pattern(Ref, Column, TypeName, Value)
    ).
