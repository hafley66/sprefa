% syntax.pl : Q5 SYNTAX OVERLOAD TABLE.
%
% Every current use of the symbols the design wants to reuse, the collision
% each reuse causes, a real parse receipt where one is feasible, and at least
% two alternative spellings PER woe inside the rx / prolog / SQL vocabulary
% law. This file prices options; it does not pick winners. The one place it
% states a default is where a hypothesis removes the construct entirely,
% because zero constructs beats every spelling by definition.
%
% USER DIRECTIVES FOLDED IN (2026-07-28):
%   - "for the @async i really want to avoid at symbols for as long as
%     possible": @async is ONE priced row, never the default, and the no-at
%     options are listed first.
%   - "i would take <++ over any @ marker".
%   - the dissolution hypothesis (Ta is not a frontier, it is a rel): priced
%     as woe_ta_marker's option A, and it wins by construct count.

:- module(mf_syntax, [ collision/5, alternative/4, woe/1, parse_receipt/2 ]).

:- use_module(library(lists)).
:- use_module('../../conformance/body').

woe(woe_level_arm_arrow).
woe(woe_ssu_arrow).
woe(woe_bar_separator).
woe(woe_match_word).
woe(woe_lifecycle_words).
woe(woe_ta_marker).
woe(woe_event_arm_arrow).

% ═══ collisions ═════════════════════════════════════════════════════════════
% collision(Woe, Symbol, Where, Nature, Severity)
% Severity: high | medium | low

collision(woe_level_arm_arrow, '->', prolog_term_form,
          'ISO if-then-else, op(1050, xfy). A body containing (Guard -> Then) is indistinguishable from a level arm, and body.pl:126 classifies the WHOLE if-then term as a single trigger atom named (->)/2 with no error anywhere. Receipt: parse_receipt(level_arrow_absorbed_as_trigger_atom).',
          high).
collision(woe_level_arm_arrow, '->', dl_surface,
          'ruling q8_key_vs_arrow already assigns -> the meaning "the program/world column split on effect rels" (rulings.pl:50). The v5 idiom matches() -> (body) is live in the ledger. Reusing -> as the level-arm arrow contradicts a standing ruling.',
          high).
collision(woe_ssu_arrow, '=>', prolog_term_form,
          'SWI single-sided-unification rules. op(1200, xfx), the same priority as (:-), so a top-level Head => Body. in a .pl file IS an SSU clause, not data. Receipt: parse_receipt(ssu_arrow_is_clause_priority).',
          high).
collision(woe_bar_separator, '|', prolog_term_form,
          'op(1105, xfy) plus the list-tail role. {a | b} parses, which makes an arm separator silently a disjunction term. Also collides with the deferred |> (ruling cut_pipe). Receipt: parse_receipt(bar_parses_as_operator).',
          medium).
collision(woe_match_word, 'match', sql_vocabulary,
          'SQLite has a MATCH operator (FTS and rtree). The word passes the vocabulary law but carries a different meaning in the family it comes from. Bigger problem: match promises exhaustiveness (Rust, ML) and under the typed-columns ruling there are no enum types, so no exhaustiveness check is decidable. Scenario e2 is the receipt.',
          medium).
collision(woe_lifecycle_words, 'finalize', rx_vocabulary,
          'rxjs finalize() runs on unsubscribe / complete / error of a SUBSCRIPTION. The design uses it per ROW. Same vocabulary family, different granularity, which is the worst kind of collision: it reads as correct and is not.',
          high).
collision(woe_lifecycle_words, 'next', rx_vocabulary,
          'rxjs next IS the right word for the + envelope. The collision is with v5 @next, which names the Ti FRONTIER, a different axis. One word, two axes.',
          medium).
collision(woe_ta_marker, '@async', at_marker,
          'the user has asked to avoid @ as long as possible. Also collides with v5 @async, which named a genuinely different thing (an out-of-band extraction op), so continuity here is a false friend.',
          medium).
collision(woe_event_arm_arrow, '+>', prolog_term_form,
          'NO collision: current_op/3 reports nothing for +>, <++ or +>>. Receipt: parse_receipt(arrow_family_is_free). This row exists so the table is not read as "every arrow is taken".',
          low).

% ═══ alternatives (>= 2 per woe) ════════════════════════════════════════════
% alternative(Woe, Spelling, Pro, Con)

alternative(woe_level_arm_arrow, 'keep <- and drop the mirrored arrow: an arm is head <- body like every other level rule',
            'zero new symbols, zero collisions, and the corpus already reads this way. Costs nothing.',
            'loses the source-major reading the design wanted; the subject is no longer visually first.').
alternative(woe_level_arm_arrow, '=> for BOTH arm arrows, with the axis carried by the arm ITEM instead of the arrow',
            'one arrow instead of two; the lifecycle word (next/finalize) already tells you the axis, so the arrow is redundant information.',
            'collides with SWI SSU at clause priority (woe_ssu_arrow); would need the DCG surface only, never the term form.').
alternative(woe_level_arm_arrow, '~> (tilde-arrow), unused in prolog and in the corpus',
            'free glyph, visually a weaker arrow than +>, which reads as "maintained" rather than "appended".',
            'not an rx, prolog or SQL word; the vocabulary law is about names, but a glyph nobody can pronounce is the same problem.').

alternative(woe_ssu_arrow, 'reserve => for the DCG surface only and never write it in a .pl term',
            'the surface is where humans read arms; the term form is a compiler intermediate and can spell it arm(Arrow, ...).',
            'the 110-fixture corpus IS term form, so every fixture would carry the compiler spelling instead of the human one.').
alternative(woe_ssu_arrow, 'do not use => at all; arms are arm/4 terms with a quoted arrow atom',
            'exactly what this lab does. Zero parse risk, and print_dl.pl renders whatever surface is chosen.',
            'term form stops being pleasant to read by hand.').

alternative(woe_bar_separator, 'newline plus indentation as the arm separator, no glyph at all',
            'matches how the DCG surface already reads rules (one per line, terminated by .).',
            'the term form has no whitespace, so it needs a list anyway.').
alternative(woe_bar_separator, 'comma, the same separator every other list in the language uses',
            'zero new symbols; arms ARE a list of rules.',
            'a comma inside an arm body is then two levels of comma, which reads badly without brackets.').

alternative(woe_match_word, 'groupBy, the rx word for "one source, arms keyed by a column"',
            'literally the rx operator this lowers to (lowering.pl complete_arm), and it makes no exhaustiveness promise.',
            'groupBy in rx keys by a function, not by a pattern; the analogy is close, not exact.').
alternative(woe_match_word, 'materialize, the rx word for "every event becomes a tagged envelope"',
            'this IS the design: next / error / complete envelopes. The most honest single word available.',
            'rx materialize has no error arm here and no per-row complete, so it over-promises in a different direction.').
alternative(woe_match_word, 'partition, the rx word for splitting one source by a predicate',
            'no exhaustiveness promise, and it is exactly what non-lifecycle arms do.',
            'rx partition is binary (two outputs); N arms stretch it.').

alternative(woe_lifecycle_words, 'the SQL trigger family: inserted / deleted arms with OLD and NEW row aliases',
            'SQL triggers are the exact prior art (AFTER INSERT / AFTER DELETE / AFTER UPDATE OF), the words are unambiguous inside the SQL family, and AFTER UPDATE gives OLD and NEW in ONE body, which dissolves the flagship transition rule s two-trigger cut problem entirely (scenario g1).',
            'introduces an update arm, whose per-occurrence-vs-per-boundary reading is a NEW ambiguity (SLOT-UPDATE-ARM, scenario a1).').
alternative(woe_lifecycle_words, 'next / departed, reusing the corpus word departed/1 instead of borrowing rx finalize',
            'departed/1 is already the shipped kernel goal (engine.pl:143, scopes.pl:148), so the arm word and the kernel word are one word.',
            'departed is not an rx, prolog or SQL word; it is an invented one, which the vocabulary law rejects.').
alternative(woe_lifecycle_words, 'next / delete, the SQL DML pair',
            'both are SQL words, and the pairing is obvious on first reading.',
            'delete reads as an imperative command, not as an observed event.').

% ── the Ta marker: no-at options FIRST, per the user directive ──────────────
alternative(woe_ta_marker, 'A. NO MARKER AT ALL: Ta dissolves into a pending rel plus a consuming rule (the dissolution hypothesis)',
            'zero constructs, which beats every spelling by definition. The queue becomes a durable rel (endurance law covers it for free), VISIBLE in the tick log (self-diagnosis law), and MATCHABLE with ordinary arms, which answers "is the carry itself matchable" with yes. Graded in scenarios f1-f4: it reproduces primitive Ta exactly. Directly parallel to clock_residency, which dissolved cadence the same way.',
            'costs one extra rule and one extra rel per deferred hop, and one extra quiescence tick (scenario f3). The user writes the queue instead of the engine hiding it.').
alternative(woe_ta_marker, 'B. <++ : a doubled edge arrow carrying the frontier on the RULE, not the atom',
            'the user s own preference over any @ marker. current_op reports <++ free (parse_receipt(arrow_family_is_free)). Carrying it on the rule is more honest than on the atom, since the frontier is a property of when the HEAD lands, not of how the body reads.',
            'arrow proliferation: three arrows (<-, <+, <++) plus their mirrors is six glyphs to teach. And it keeps a primitive queue, which scenario f1 shows is nondeterministic and therefore ungradeable under the item-9 law.').
alternative(woe_ta_marker, 'C. observeOn(async, Atom) : the literal rx word',
            'satisfies the vocabulary law exactly, and it is self-documenting for anyone who reads rx.',
            'THE WORD IS A TRAP. rx schedulers are semantically transparent: observeOn changes WHEN, never WHAT, so this spelling names an operator that provably does not deliver the semantics (lowering.pl ta_as_scheduler, graded direct_vacuous). Spelling a thing after an operator that cannot implement it is worse than an invented word.').
alternative(woe_ta_marker, 'D. async(Atom) : a function wrapper in the style of latest() and combine()',
            'no @, and it sits in the existing wrapper family (only/pre/departed/not are all function wrappers in the term form), so the parser needs nothing new.',
            'async is a JS keyword, not an rx, prolog or SQL word; and it keeps the primitive queue with all of B s semantic problems.').
alternative(woe_ta_marker, 'E. @async : v5 continuity',
            'one row of migration cost for anyone porting v5 programs.',
            'the user has asked to avoid @ as long as possible, and v5 @async named a different thing, so the continuity is a false friend. Listed for completeness, not recommended.').

alternative(woe_event_arm_arrow, '+> as the mirror of <+',
            'free glyph, and the mirroring is genuinely readable: <+ and +> are the same rule read from either end.',
            'two spellings for one rule means every diff, grep and error message has to handle both.').
alternative(woe_event_arm_arrow, 'no mirrored arrow: arms are written head <+ body like the corpus',
            'one spelling for one thing. The match block already supplies the source-major grouping, so the arrow does not have to.',
            'the arm then reads right-to-left inside a block that reads left-to-right.').

% ═══ parse receipts (real, run against the live reader) ═════════════════════
% parse_receipt(Name, Goal) : Goal must succeed.

parse_receipt(level_arrow_absorbed_as_trigger_atom, Goal) :-
    Goal = ( term_to_atom(Term, '(alpha, beta -> gamma)'),
             Term =.. ['->', (alpha, beta), gamma],
             % the receipt: the reference body classifier hands the WHOLE
             % if-then term back as one trigger atom, silently.
             body_atoms(Term, Atoms),
             Atoms = [Only],
             Only == Term ).

parse_receipt(ssu_arrow_is_clause_priority, Goal) :-
    Goal = ( current_op(Priority, xfx, '=>'),
             Priority =:= 1200,
             current_op(ClausePriority, xfx, ':-'),
             Priority =:= ClausePriority ).

parse_receipt(bar_parses_as_operator, Goal) :-
    Goal = ( current_op(_, xfy, '|'),
             term_to_atom(Term, '{a | b}'),
             Term = '{}'(Inner),
             Inner =.. ['|', a, b] ).

parse_receipt(arrow_family_is_free, Goal) :-
    Goal = ( forall(member(Candidate, ['+>', '<++', '+>>', '~>']),
                    \+ current_op(_, _, Candidate)) ).

% <- and <+ are not in the GLOBAL operator table at all, unlike -> and =>.
% engine.pl declares them at 1150 xfx inside its own module (engine.pl:72-73),
% and an op/3 directive inside a module is module-local in SWI, which is why
% level_eval.pl, ticklog.pl and every file in this lab has to re-declare them.
% That asymmetry is itself a Q5 fact: the arrows the design wants to ADD are
% free glyphs, and the arrows the language ALREADY uses were never free either,
% they are per-module declarations. A mirrored-arrow family multiplies that
% per-module declaration cost by two.
parse_receipt(existing_arrows_are_module_local_not_global, Goal) :-
    Goal = ( \+ current_op(_, _, '<-'),
             \+ current_op(_, _, '<+'),
             current_op(IfThen, xfy, '->'), IfThen =:= 1050 ).
