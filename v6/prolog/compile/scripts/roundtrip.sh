#!/usr/bin/env bash
# roundtrip.sh : phase D parser grade runner (plans/2026-07-28-tsv2-phase-d-
# parser-header.md). Three grades:
#   G1 (binding) - for every fixture in v6/prolog/conformance/fixtures/*.pl,
#     parse_dl(print_dl(Term)) is a variant (=@=) of Term. Also regenerates
#     every v6/prolog/compile/dl_view/<fixture>.dl6 rendering as a side effect
#     (the "language you can see" deliverable).
#   G2 (real-file) - v6/dl/fixtures/ghcacher.dl6 and conformance.dl6 parse
#     without error; unsupported_surface(...) findings are collected and
#     printed, never silently dropped. Reports Decls/Rules counts per file.
#   G3 (no-regression sanity) - the existing conformance suite
#     (v6/prolog/conformance/go.pl) stays green untouched, since this parser
#     never edits compile.pl/analyze.pl/strat.pl/lower.pl/emit_ts.pl.
#
# The grading logic is a Prolog program written to a temp file at RUN TIME
# (not a committed .pl file) so this directory gains exactly the one script
# file the phase D contract lists, per its strict file-ownership rule.
#
# Usage: bash v6/prolog/compile/scripts/roundtrip.sh

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILE_DIR="$(cd "$HERE/.." && pwd)"
PROLOG_DIR="$(cd "$COMPILE_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROLOG_DIR/../.." && pwd)"

FIXTURES_DIR="$PROLOG_DIR/conformance/fixtures"
DL_VIEW_DIR="$COMPILE_DIR/dl_view"
GHCACHER_FILE="$REPO_ROOT/v6/dl/fixtures/ghcacher.dl6"
CONFORMANCE_DL_FILE="$REPO_ROOT/v6/dl/fixtures/conformance.dl6"
CONFORMANCE_GO="$PROLOG_DIR/conformance/go.pl"

GRADER="$(mktemp /tmp/roundtrip-grader-XXXXXX.pl)"
trap 'rm -f "$GRADER"' EXIT

cat > "$GRADER" <<PLEOF
:- use_module('$COMPILE_DIR/parse_dl').
:- use_module('$COMPILE_DIR/print_dl').
:- use_module('$COMPILE_DIR/compile', [program_plan/2]).
:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(filesex)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- dynamic(g1_failed_flag/0).
:- dynamic(g2_failed_flag/0).

fixtures_dir('$FIXTURES_DIR').
dl_view_dir('$DL_VIEW_DIR').
ghcacher_file('$GHCACHER_FILE').
conformance_dl_file('$CONFORMANCE_DL_FILE').

fixture_files(Files) :-
    fixtures_dir(Dir),
    directory_files(Dir, Entries),
    msort(Entries, Ordered),
    findall(Path,
            ( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl'),
              atomic_list_concat([Dir, '/', Entry], Path) ),
            Files).

% Generalizes compile.pl:read_fixture_term/4 (which stops at the first Name
% match) to collect EVERY fixture in a file, replaying each file's own
% \`:- op(...)\` directives exactly as consult would -- the same technique
% sweep.pl already uses for the same reason, reproduced here locally since
% sweep.pl exports nothing this script could import instead.
read_all_fixtures(File, Entries) :-
    open(File, read, Stream),
    call_cleanup(scan_fixtures(Stream, Entries), close(Stream)).

% entry/5 carries Initial/Schedule alongside Prog/Bindings now (previously
% entry/3, Prog/Bindings only) -- write_one_view/6 below needs the real
% fixture rows to compute compile.pl:program_plan/2's plan and, from it,
% which EDB refs actually have evidence (print_dl.pl:augmented_decls/6's
% WitnessedRefs argument). g1_one/4's own round-trip check still only
% ever touches Prog/Bindings, unchanged.
scan_fixtures(Stream, Entries) :-
    read_term(Stream, Candidate, [variable_names(Bindings)]),
    ( Candidate == end_of_file
    -> Entries = []
    ; Candidate = (:- Directive)
    -> call(Directive), scan_fixtures(Stream, Entries)
    ; Candidate = fixture(Name, Prog, Initial, Schedule, _)
    -> Entries = [entry(Name, Prog, Bindings, Initial, Schedule) | Rest],
       scan_fixtures(Stream, Rest)
    ; scan_fixtures(Stream, Entries)
    ).

% ═══ G1 : round-trip every fixture, and regenerate dl_view/*.dl6 ═══════════

g1 :-
    fixture_files(Files),
    findall(File-Entries, ( member(File, Files), read_all_fixtures(File, Entries) ), Pairs),
    findall(result(File, Name, Status),
            ( member(File-Entries, Pairs), member(entry(Name, Prog, Bindings, _Initial, _Schedule), Entries),
              g1_one(Name, Prog, Bindings, Status)
            ),
            Results),
    include(is_pass, Results, Passes),
    length(Results, Total), length(Passes, PassCount),
    format("G1 round-trip: ~w / ~w fixtures pass~n", [PassCount, Total]),
    ( PassCount =:= Total
    -> true
    ; assertz(g1_failed_flag),
      forall(( member(result(File, Name, Status), Results), Status \\== pass ),
             format("  FAIL ~w (~w): ~q~n", [Name, File, Status]))
    ),
    write_dl_views(Pairs).

is_pass(result(_, _, pass)).

g1_one(_Name, Prog, Bindings, Status) :-
    ( catch(do_g1_one(Prog, Bindings, Status0), Caught, Status0 = fail(exception(Caught)))
    -> Status = Status0
    ; Status = fail(silent_failure)
    ).

do_g1_one(Prog, Bindings, Status) :-
    print_dl_program(Prog, Bindings, Text),
    atom_codes(Text, Codes),
    parse_dl(Codes, Prog2, _Bindings2, _Findings),
    ( Prog =@= Prog2 -> Status = pass ; Status = fail(not_variant) ).

write_dl_views(Pairs) :-
    dl_view_dir(ViewDir),
    ( exists_directory(ViewDir) -> true ; make_directory(ViewDir) ),
    forall(member(_File-Entries, Pairs),
           forall(member(entry(Name, Prog, Bindings, Initial, Schedule), Entries),
                  write_one_view(ViewDir, Name, Prog, Bindings, Initial, Schedule))).

% Every fixture's view is regenerated, not just the ~60 that compile
% through compile.pl:program_plan/2 -- the "language you can see"
% deliverable covers the FULL conformance corpus (120 fixtures), most of
% which use constructs compile.pl's term-door subset does not support yet
% (recursive/departure/derived-edge fixtures named in the compiled-vs-
% unsupported sweep). augmented_view_text/6 below renders the EDB-decl-
% synthesized text (print_dl.pl:print_dl_program_with_edb_types/7, same
% predicate the text-door receipt uses) for a fixture program_plan/2
% accepts; anything program_plan/2 refuses (unsupported_construct or any
% other error) falls back to the PLAIN print_dl_program/3 rendering this
% file always produced, unaugmented -- write_one_view/6 never leaves a
% fixture without a view just because it is outside the term door's
% supported subset.
write_one_view(ViewDir, Name, Prog, Bindings, Initial, Schedule) :-
    atomic_list_concat([ViewDir, '/', Name, '.dl6'], OutPath),
    ( catch(augmented_view_text(Name, Prog, Bindings, Initial, Schedule, Text), _, fail)
    -> true
    ; catch(print_dl_program(Prog, Bindings, Text), _, fail)
    ),
    !,
    catch(
        setup_call_cleanup(open(OutPath, write, S), format(S, "~w", [Text]), close(S)),
        _,
        true
    ).
write_one_view(_, _, _, _, _, _).

augmented_view_text(Name, Prog, Bindings, Initial, Schedule, Text) :-
    program_plan(fixture(Name, Prog, Initial, Schedule, [])-Bindings, Plan),
    Plan = plan(_, ExpandedProg, RelPlans, ArrivalTargets, _, _),
    ExpandedProg = prog(ExpandedDecls, _),
    witnessed_refs(Initial, Schedule, WitnessedRefs),
    print_dl_program_with_edb_types(Prog, Bindings, ExpandedDecls, RelPlans, ArrivalTargets,
                                    WitnessedRefs, Text).

% Same evidence check as text_door_receipt.pl:witnessed_refs/3 (see that
% predicate's header, and print_dl.pl:augmented_decls/6's, for why a
% witness-less EDB ref must NOT get a synthesized decl).
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

% ═══ G2 : real-file parse (ghcacher.dl6, conformance.dl6) ══════════════════

g2 :-
    ghcacher_file(GhFile), conformance_dl_file(ConfFile),
    g2_one('ghcacher.dl6', GhFile),
    g2_one('conformance.dl6', ConfFile).

g2_one(Label, Path) :-
    format("~n--- G2 ~w ---~n", [Label]),
    catch(
        ( parse_dl_file(Path, prog(Decls, Rules), _Bindings, Findings),
          length(Decls, DeclCount), length(Rules, RuleCount),
          format("  Decls: ~w  Rules: ~w~n", [DeclCount, RuleCount]),
          ( Findings == []
          -> format("  Findings: none~n")
          ; length(Findings, FindingCount),
            format("  Findings (~w):~n", [FindingCount]),
            forall(member(F, Findings), format("    ~q~n", [F]))
          )
        ),
        Err,
        ( assertz(g2_failed_flag), format("  PARSE ERROR: ~q~n", [Err]) )
    ).

main :-
    format("=== G1 ROUND-TRIP ===~n"),
    g1,
    format("~n=== G2 REAL-FILE PARSE ===~n"),
    g2,
    format("~n"),
    ( g1_failed_flag -> format("G1: FAILURES PRESENT~n") ; format("G1: ALL PASS~n") ),
    ( g2_failed_flag -> format("G2: PARSE ERROR PRESENT~n") ; format("G2: NO PARSE ERRORS~n") ),
    ( (g1_failed_flag ; g2_failed_flag) -> halt(1) ; halt(0) ).
PLEOF

echo "=== phase D roundtrip grader ==="
swipl -q -l "$GRADER" -g main -t 'halt(1)'
GRADER_STATUS=$?

echo ""
echo "=== G3 conformance suite (no-regression sanity) ==="
CONFORMANCE_OUTPUT="$(swipl -q -l "$CONFORMANCE_GO" -g go -g halt 2>&1)"
PASS_COUNT="$(printf '%s\n' "$CONFORMANCE_OUTPUT" | grep -c '^PASS' || true)"
FAIL_COUNT="$(printf '%s\n' "$CONFORMANCE_OUTPUT" | grep -c '^FAIL' || true)"
echo "conformance: $PASS_COUNT pass / $FAIL_COUNT fail"
if [ "$FAIL_COUNT" != "0" ]; then
  printf '%s\n' "$CONFORMANCE_OUTPUT" | grep '^FAIL' || true
  G3_STATUS=1
else
  G3_STATUS=0
fi

echo ""
if [ "$GRADER_STATUS" -eq 0 ] && [ "$G3_STATUS" -eq 0 ]; then
  echo "roundtrip.sh: ALL GRADES PASS"
  exit 0
else
  echo "roundtrip.sh: FAILURES PRESENT (grader exit=$GRADER_STATUS, G3 exit=$G3_STATUS)"
  exit 1
fi
