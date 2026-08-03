% text_door_receipt.pl : the text-door parity gate.
%
% CONTRACT (dynamic, no frozen count): every fixture that compiles through
% the TERM door (compile_fixture/3, reading the fixture's Prolog term
% directly) must ALSO compile through the TEXT door (print the same
% program as .dl6 surface text, then compile_dl6/2 that text) and produce
% byte-identical emitted TypeScript. Zero failures, whatever the corpus
% size turns out to be -- a hardcoded `CompiledCount =:= 34` here was the
% earlier defect: it froze the receipt to a stale sweep-era count, so
% adding term-door-compilable fixtures (or fixing a text-door gap) moved
% CompiledCount and turned the receipt red for a reason that had nothing
% to do with text-door parity itself.
%
% EDB DECL SYNTHESIS: many term-door-compilable fixtures declare zero
% decls at all (e.g. expressions.pl's comparison_filters_rows: Decls=[],
% every column typed only by its Initial rows' literal ints, PHASE C2
% RULING 1's own literal-witness inference). print_dl_program/3 alone
% only ever prints a decl line for a ref that already has a literal Decls
% entry (see print_dl.pl's own header note on decl_ref_order/2 -- that
% restriction is required for G1's round-trip grade, print_dl_program/3
% is intentionally left unchanged). Printed without decls, an EDB column
% with no witness inside the TEXT itself defaults to TEXT (compile_dl6/2
% has no Initial/Schedule rows to read; the text door's only typing
% evidence is decls), and the fixture's own int arithmetic/comparisons
% then refuse: unsupported_construct(arith_operand_not_int(...,text)).
% print_dl.pl:print_dl_program_with_edb_types/6 fixes this HERE, at the
% caller that already holds the compile plan (RelPlans + ArrivalTargets,
% both compile.pl:program_plan/2 outputs), by synthesizing
% `col_type(Ref, Column, Type)` decl facts for exactly the EDB refs
% (ArrivalTargets: never a rule head, so per the one-rel-one-rule-kind law
% their typing can only ever come from a decl or a literal witness) the
% surface program itself left undeclared -- decl authority is respected
% (declared_refs/2 on the EXPANDED decls is consulted first; an already-
% declared ref, including every enum variant ref the enum sugar implies,
% is never touched).
%
% WITNESSED REFS ONLY: a second restriction print_dl.pl:augmented_decls/6
% enforces (see its own header for the full mechanism) -- an EDB ref with
% zero rows in EITHER Initial or Schedule anywhere in the fixture does not
% get a synthesized decl at all, even if it is otherwise an undeclared
% arrival target. Synthesizing one for a genuinely witness-less ref turns
% analyze.pl's neutral "no opinion yet" fixpoint placeholder into a
% FROZEN, sticky `text` contribution, which can silently override a
% differently-typed sibling that DOES have real evidence when both union
% into the same derived rel (found on timeless_rail.pl's
% clean_state_no_diags and eight siblings: eprintln_waiver_line unions
% waiver_block_comment, which never receives a row in these fixtures,
% with waiver_trailing_comment, which does and is genuinely `int` --
% synthesizing waiver_block_comment's fallback `text` as a decl pinned
% the union to `text` and broke the fixture's own `WaiverLine >= LineNo -
% 1` comparison, which the real term door accepts). witnessed_refs/3
% below computes the WitnessedRefs argument augmented_decls/6 needs,
% scanning Initial and Schedule directly (the same two clauses of
% analyze.pl:column_source_args/5 that read fixture rows; the third
% clause, atomic literals appearing directly in a rule body/head, is not
% reproduced here -- no fixture in the current corpus has an EDB ref
% typed ONLY that way, and leaving it out is the conservative direction,
% since a ref this undercounts as unwitnessed just stays undeclared, same
% as before this fix landed).
%
% SABOTAGE RECEIPT (run by hand, reverted -- proves the gate actually
% checks the synthesized types, not just "did a decl line print somewhere"):
% forced print_dl.pl:augmented_decls/6 to emit
% `col_type(shared_count/3, shared, text)` instead of `int` for one column
% (expressions.pl's shared_count/3, shared position) while leaving every
% other synthesized decl correct. Receipt went red:
%   TEXT_DOOR compiled=60 byte_identical=58 failures=2
%     TEXT_DOOR_FAIL head_expression_evaluates_derived_column unsupported_construct(arith_operand_not_int(_1088+_1090-_1076,_1076,text))
%     TEXT_DOOR_FAIL comparison_filters_rows unsupported_construct(arith_operand_not_int(_1052+_1054-_1040,_1040,text))
% (shared_count/3's `shared` column feeds a head arithmetic expression in
% BOTH fixtures -- union_size's `LeftSize + RightSize - Shared` and
% comparison_filters_rows' own copy -- so pinning it to the wrong type
% broke two fixtures at once, not the one the sabotage targeted; the text
% door refuses outright rather than emitting the TEXT-collapse silently,
% exactly PHASE C2 RULING 1's own arith_operand_not_int guard). Reverted
% before landing; the fixture set passes byte_identical=60 again below.

:- module(text_door_receipt, [run/0]).

:- use_module(library(filesex)).
:- use_module(library(lists)).
:- use_module('../../compile', [compile_dl6/2, compile_fixture/3, compile_program/6,
                             program_plan/2]).
:- use_module('../../print_dl', [print_dl_program/3, augmented_decls/6]).

:- dynamic(compile_dir_fact/1).
:- prolog_load_context(directory, Here), assertz(compile_dir_fact(Here)).

run :-
    compile_dir_fact(ScriptsDir),
    file_directory_name(ScriptsDir, CompileDir),
    atomic_list_concat([CompileDir, '/../conformance/fixtures'], FixturesDir),
    atomic_list_concat([CompileDir, '/out/text-door'], OutDir),
    make_directory_path(OutDir),
    fixture_files(FixturesDir, Files),
    findall(Status,
            ( member(File, Files), read_all_fixtures(File, Entries),
              member(entry(Name, Term, Bindings), Entries),
              grade_one(File, Name, Term, Bindings, OutDir, Status)
            ),
            Statuses),
    include(reached_text_door, Statuses, ReachedTextDoor),
    include(is_identical, Statuses, Identical),
    include(is_failure, Statuses, Failures),
    length(ReachedTextDoor, CompiledCount),
    length(Identical, IdenticalCount),
    length(Failures, FailureCount),
    format("TEXT_DOOR compiled=~w byte_identical=~w failures=~w~n",
           [CompiledCount, IdenticalCount, FailureCount]),
    forall(member(failure(Name, Detail), Failures),
           format("  TEXT_DOOR_FAIL ~w ~q~n", [Name, Detail])),
    ( FailureCount =:= 0
    -> true
    ; halt(1)
    ).

fixture_files(Dir, Files) :-
    directory_files(Dir, Entries),
    msort(Entries, Ordered),
    findall(Path,
            ( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl'),
              atomic_list_concat([Dir, '/', Entry], Path) ),
            Files).

read_all_fixtures(File, Entries) :-
    open(File, read, Stream),
    call_cleanup(scan_fixtures(Stream, Entries), close(Stream)).

scan_fixtures(Stream, Entries) :-
    read_term(Stream, Candidate, [variable_names(Bindings)]),
    ( Candidate == end_of_file
    -> Entries = []
    ; Candidate = (:- Directive)
    -> call(Directive), scan_fixtures(Stream, Entries)
    ; Candidate = fixture(Name, _, _, _, _)
    -> Entries = [entry(Name, Candidate, Bindings) | Rest], scan_fixtures(Stream, Rest)
    ; scan_fixtures(Stream, Entries)
    ).

% Stage 1 (TERM door): compile_fixture/3 either succeeds (this fixture is
% inside the term-door-supported subset, proceed to stage 2) or throws
% unsupported_construct(_), which is the term door's own ordinary refusal
% for a construct check_supported_subset/1 (analyze.pl) does not cover --
% out of scope for a TEXT-door parity receipt, so excluded entirely
% (`skipped`, matching the sweep's own supported/unsupported split).
%
% Stage 2 (TEXT door, grade_text_door/6 below): every failure downstream of
% a successful term-door compile is now a real failure(Name, Detail), NEVER
% silently reclassified as `skipped` -- that reclassification (the old
% single-catch classify/3 covering BOTH stages at once) is exactly what let
% the 20 lifted expression/aggregate fixtures' text-door refusals hide as
% "skipped" while the receipt printed failures=0.
grade_one(File, Name, Term, Bindings, OutDir, Status) :-
    format(atom(SweepTermOut), '~w/~w.sweep.ts', [OutDir, Name]),
    catch(quiet(compile_fixture(Name, File, SweepTermOut)), TermDoorError, true),
    ( var(TermDoorError)
    -> grade_text_door(Name, Term, Bindings, OutDir, Status)
    ; classify_term_door_error(Name, TermDoorError, Status)
    ).

classify_term_door_error(_Name, unsupported_construct(_), skipped) :- !.
classify_term_door_error(Name, Error, failure(Name, Error)).

% The TermOut reference build below is deliberately re-compiled with EMPTY
% Initial/Schedule ([], []) -- same as the original text_door_receipt.pl --
% so that both sides of the byte-comparison are built from decls + rules
% ALONE, zero fixture-harness rows, matching what compile_dl6/2 (the text
% door) ever sees (.dl6 surface syntax has no Initial/Schedule facts at
% all). This is exactly why an EDB ref typed ONLY by a literal witness
% needs its inferred type synthesized INTO the decl list before EITHER
% side compiles here: with Initial/Schedule emptied, the witness evidence
% is gone from both builds, and an EDB column with no decl and no witness
% falls to compile.pl's own "zero witnesses -> text" default (PHASE C2
% RULING 1) on the TermOut side too, not just the printed-text side --
% the same AugmentedProg (RawDecls + synthesized EDB col_type facts) has
% to feed BOTH compile_program/6 (TermOut) and print_dl_program/3
% (TextFile) or the two builds type the same column two different ways
% and diverge for a reason that has nothing to do with the text door
% itself.
grade_text_door(Name, Term, Bindings, OutDir, Status) :-
    format(atom(TermOut), '~w/~w.term.ts', [OutDir, Name]),
    format(atom(TextOut), '~w/~w.text.ts', [OutDir, Name]),
    format(atom(TextFile), '~w/~w.dl6', [OutDir, Name]),
    catch(
        ( Term = fixture(Name, Prog, Initial, Schedule, _Expectations),
          raw_program_parts(Prog, RawDecls, RawRules, Queries),
          program_plan(fixture(Name, Prog, Initial, Schedule, [])-Bindings, Plan),
          Plan = plan(_, ExpandedProg, RelPlans, ArrivalTargets, _, _, _),
          ExpandedProg = prog(ExpandedDecls, _),
          witnessed_refs(Initial, Schedule, WitnessedRefs),
          augmented_decls(RawDecls, ExpandedDecls, RelPlans, ArrivalTargets, WitnessedRefs,
                          AugmentedDecls),
          rebuild_program(Queries, AugmentedDecls, RawRules, AugmentedProg),
          print_dl_program(AugmentedProg, Bindings, Text),
          write_text(TextFile, Text),
          quiet(compile_program(Name, fixture(Name, AugmentedProg, [], [], []), Bindings,
                                [], TermOut, emit_ts:emit_program)),
          quiet(compile_dl6(TextFile, TextOut)),
          read_file_to_string(TermOut, TermText, []),
          read_file_to_string(TextOut, TextDoorText, []),
          ( TermText == TextDoorText
          -> Status = identical(Name)
          ; Status = failure(Name, byte_difference)
          )
        ),
        TextDoorError,
        Status = failure(Name, TextDoorError)
    ).

raw_program_parts(prog(Decls, Rules), Decls, Rules, none).
raw_program_parts(program(Decls, Rules, Queries), Decls, Rules, some(Queries)).

rebuild_program(none, Decls, Rules, prog(Decls, Rules)).
rebuild_program(some(Queries), Decls, Rules, program(Decls, Rules, Queries)).

% witnessed_refs(Initial, Schedule, Refs): every Ref = Name/Arity that has
% at least one row, addition or retraction alike, in Initial or any
% Schedule tick -- the ref-granular evidence check augmented_decls/6
% needs (this predicate's own header explains why). Mirrors
% analyze.pl:column_source_args/5's Initial and Schedule clauses (its
% rule-occurrence clause is deliberately not reproduced, see the module
% header).
witnessed_refs(Initial, Schedule, Refs) :-
    findall(Ref,
            ( ( member(Row, Initial), functor(Row, RowName, RowArity)
              ; member(Batch, Schedule), member(Signed, Batch),
                ( Signed = +Atom ; Signed = -Atom ), functor(Atom, RowName, RowArity)
              ),
              Ref = RowName/RowArity
            ),
            Refs0),
    sort(Refs0, Refs).

write_text(File, Text) :-
    setup_call_cleanup(open(File, write, Stream),
                       format(Stream, '~w', [Text]),
                       close(Stream)).

quiet(Goal) :-
    with_output_to(string(_), Goal).

% "Reached the text door" = passed the term door and made it into
% grade_text_door/6, whatever that stage's own outcome was -- the
% CompiledCount the receipt prints. Deliberately does NOT include
% `skipped` (term-door refusals, out of this receipt's scope).
reached_text_door(identical(_)).
reached_text_door(failure(_, _)).

is_identical(identical(_)).
is_failure(failure(_, _)).
