% grade_expansion.pl : run EVERY conformance fixture whose program carries a
% `:=` goal twice through the reference engine -- once as written, once after
% 0_assign_expand.pl erases every `:=` -- and diff the two tick logs plus the
% two final states byte for byte.
%
% Loaded the same way oracle_dump.pl loads: ensure_loaded of ticklog.pl pulls
% go.pl -> engine.pl + every fixtures/*.pl into the (user) context, so
% fixture/5 and print_ticklog/3 are callable unqualified.
%
% Run: swipl -q -l grade_expansion.pl -g grade_all -g halt

:- ensure_loaded('../../conformance/ticklog').
:- use_module('0_assign_expand', [expand_assign_program/2]).

grade_all :-
    findall(Name, assign_fixture(Name), Names),
    length(Names, Total),
    format("ASSIGN FIXTURES: ~w~n", [Total]),
    foldl(grade_one, Names, 0-0, Identical-Differing),
    format("~nRESULT identical=~w differing=~w of ~w~n",
           [Identical, Differing, Total]),
    ( Differing =:= 0 -> true ; halt(1) ).

% once/1 rather than a bare cut: a cut here would prune the fixture/5
% choicepoint too and the whole sweep would report exactly one fixture.
assign_fixture(Name) :-
    fixture(Name, prog(_, Rules), _, _, _),
    once(( member(Rule, Rules), term_has_assign(Rule) )).

term_has_assign(Term) :-
    nonvar(Term),
    ( Term = (_ := _)
    -> true
    ;  compound(Term),
       Term =.. [_ | Args],
       member(Arg, Args),
       term_has_assign(Arg)
    ).

grade_one(Name, Identical0-Differing0, Identical-Differing) :-
    fixture(Name, Program, Initial, Schedule, _),
    % copy_term BEFORE expanding: the expansion works by BINDING the `:=`
    % left-hand variable, and those variable cells are shared with Program
    % itself, so expanding in place would silently rewrite the very term the
    % written-spelling leg is supposed to grade.
    %
    % Each leg's THROW is graded as a value, not as a harness failure. Several
    % fixtures (arithmetic_rejects_non_int_operand_at_runtime) exist precisely
    % to exercise an engine rejection, and "both spellings reject with the same
    % error" is exactly as strong a composition receipt as two equal logs --
    % oracle_dump.pl reports the same class as ORACLE_THROW.
    (  catch(( copy_term(Program, ProgramToExpand),
               expand_assign_program(ProgramToExpand, Expanded),
               outcome(Program,  Initial, Schedule, WrittenLog),
               outcome(Expanded, Initial, Schedule, ExpandedLog) ),
             Error,
             ( format("THREW    ~w ~q~n", [Name, Error]), fail ))
    -> (  WrittenLog == ExpandedLog
       -> format("IDENTICAL ~w~n", [Name]),
          Identical is Identical0 + 1, Differing = Differing0
       ;  format("DIFFERING ~w~n", [Name]),
          Identical = Identical0, Differing is Differing0 + 1
       )
    ;  Identical = Identical0, Differing is Differing0 + 1
    ).

outcome(Program, Initial, Schedule, Outcome) :-
    catch(( with_output_to(string(Text), print_ticklog(Program, Initial, Schedule)),
            Outcome = log(Text) ),
          Thrown,
          Outcome = threw(Thrown)).
