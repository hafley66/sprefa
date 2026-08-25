% arm_census.pl : coverage census for lower.pl.
%
% Answers "which lower.pl unsupported_construct throw sites and multi-clause
% predicate arms does NO corpus program reach".
%
% Two parts, one deterministic run each:
%
%   STATIC throw census  -- enumerate every throw(unsupported_construct(_))
%                           site in lower.pl (exact file:line + construct name),
%                           cross against compile/out/manifest.json `reason`
%                           functors. A name present in the manifest is REACHED
%                           (a corpus program threw it during the sweep); absent
%                           means UNREACHED (a hypothesis, per the repo law that
%                           a refusal is a hypothesis).
%
%   DYNAMIC arm census   -- run the corpus compile (program_plan + lower_program
%                           + boot_statements + catalog_decl_rows) under
%                           library(prolog_coverage), dump lower.pl clause
%                           coverage, and report never-entered clauses of
%                           multi-clause predicates (unreached arms).
%
% Run (from v6/prolog):
%   swipl -q -s compile/scripts/arm_census.pl -g census -t halt
%
% The dynamic leg is the only place SWI's own coverage library is used; no
% compiler file is modified and no bespoke instrumentation is written (the
% build-vs-buy law: library(prolog_coverage) already reports per-clause
% "never executed" with source line numbers).

:- module(arm_census, [ census/0 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(filesex)).
:- use_module(library(pcre)).
:- use_module(library(prolog_coverage)).
:- use_module(library(http/json)).

:- use_module('../../compile', [ program_plan/3, default_intern_mode/1 ]).
:- use_module('../../7_lower/lower',
              [ lower_program/2, boot_statements/7, catalog_decl_rows/6 ]).
:- use_module('../../emit_ts', [ emit_program/5 ]).
:- use_module('../4_emit_jsonschema', [ jsonschema_text/3, option_rows/3 ]).
:- use_module('../7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('../8_emit_rust_types', [ rust_types_text/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══════════════════════════════════════════════════════════════════════════
% top level
% ═══════════════════════════════════════════════════════════════════════════

census :-
    throw_sites(Sites),
    manifest_reason_names(ReasonNames),
    classify_throws(Sites, ReasonNames, Reached, Unreached),
    length(Sites, TotalSites),
    length(Reached, ReachedCount),
    length(Unreached, UnreachedCount),
    format("═══ THROW CENSUS ═══~n"),
    format("total throw sites: ~w   reached: ~w   unreached: ~w~n~n",
           [TotalSites, ReachedCount, UnreachedCount]),
    format("reached (construct name appears in manifest.json reason):~n"),
    forall(member(Line-Name, Reached), format("  lower.pl:~w  ~w~n", [Line, Name])),
    format("~nunreached (hypothesis; no corpus program throws it):~n"),
    forall(member(Line-Name, Unreached), format("  lower.pl:~w  ~w~n", [Line, Name])),
    format("~n═══ ARM CENSUS ═══~n"),
    arm_census(UnreachedArms, MultiTotal, DeadPreds),
    length(UnreachedArms, UnreachedArmCount),
    format("multi-clause predicates: ~w   unreached arms: ~w   fully-dead predicates: ~w~n",
           [MultiTotal, UnreachedArmCount, DeadPreds]),
    forall(member(Line-Name/Arity-TotalArms, UnreachedArms),
           format("  lower.pl:~w  ~w/~w  (~w arms total)~n",
                  [Line, Name, Arity, TotalArms])),
    halt.

% ═══════════════════════════════════════════════════════════════════════════
% static throw census
% ═══════════════════════════════════════════════════════════════════════════

% every throw(unsupported_construct(<reason>)) site in lower.pl, as Line-Name
% where Name is the reason functor (or `Residue` for the one variable rethrow,
% which we normalise to its bound functor relation_pattern_not_lowerable).
throw_sites(Sites) :-
    lower_source(Str),
    findall(Line-Name, throw_site(Str, Line, Name), Raw),
    maplist(normalise_site, Raw, Sites).

normalise_site(Line-Name, Line-Name) :- Name \== "Residue", !.
normalise_site(Line-"Residue", Line-"relation_pattern_not_lowerable").

throw_site(Str, Line, Name) :-
    sub_string(Str, Before, Len, _, "throw(unsupported_construct("),
    line_no(Str, Before, Line),
    Start is Before + Len,
    leading_ident_at(Str, Start, Name).

% count newlines in the prefix to get a 1-based line number
line_no(Str, Before, Line) :-
    sub_string(Str, 0, Before, _, Prefix),
    split_string(Prefix, "\n", "", Lines),
    length(Lines, Line).

leading_ident_at(Str, Start, Name) :-
    string_length(Str, Total),
    skip_ws(Str, Start, Total, S1),
    ident_span(Str, S1, Total, End),
    Length is End - S1,
    sub_string(Str, S1, Length, _, Name).

skip_ws(Str, P, Total, P) :- P >= Total, !.
skip_ws(Str, P, Total, R) :-
    sub_string(Str, P, 1, _, C),
    ( C == " " ; C == "\t" ; C == "\n" ),
    !, P1 is P + 1, skip_ws(Str, P1, Total, R).
skip_ws(_, P, _, P).

ident_span(Str, P, Total, End) :-
    ( P < Total,
      sub_string(Str, P, 1, _, C),
      ident_char(C)
    -> P1 is P + 1, ident_span(Str, P1, Total, End)
    ;  End is P
    ).

ident_char(C) :-
    ( C == "_"
    ; C @>= "a", C @=< "z"
    ; C @>= "A", C @=< "Z"
    ; C @>= "0", C @=< "9"
    ).

% functor names recorded in the committed manifest's `reason` field
manifest_reason_names(Names) :-
    manifest_file(Path),
    setup_call_cleanup(open(Path, read, Stream),
                       json_read_dict(Stream, Entries, []),
                       close(Stream)),
    findall(Name,
            ( member(Entry, Entries),
              get_dict(reason, Entry, Reason),
              Reason \== '',
              string_leading_ident(Reason, Name) ),
            Names0),
    sort(Names0, Names).

string_leading_ident(S, Name) :-
    string_codes(S, Codes),
    skip_ws_codes(Codes, C1),
    take_ident_codes(C1, NC, _),
    string_codes(Name, NC).

skip_ws_codes([C|T], R) :- ( C =< 32 ; C == 9 ), !, skip_ws_codes(T, R).
skip_ws_codes(C, C).

take_ident_codes([C|T], [C|R], Rest) :-
    ( C >= 0'a, C =< 0'z ; C >= 0'A, C =< 0'Z ; C >= 0'0, C =< 0'9 ; C == 0'_ ),
    !, take_ident_codes(T, R, Rest).
take_ident_codes(C, [], C).

classify_throws(Sites, ReasonNames, Reached, Unreached) :-
    partition(site_reached(ReasonNames), Sites, Reached, Unreached).

site_reached(ReasonNames, _Line-Name) :-
    memberchk(Name, ReasonNames).

% ═══════════════════════════════════════════════════════════════════════════
% paths (resolved at load time, so the script runs from any cwd)
% ═══════════════════════════════════════════════════════════════════════════

:- dynamic(census_home/1).
:- prolog_load_context(directory, Here),
   assertz(census_home(Here)).

prolog_dir(Dir) :- census_home(ScriptsDir), atomic_list_concat([ScriptsDir, '/../..'], Dir).

lower_source(Str) :-
    prolog_dir(Dir),
    atomic_list_concat([Dir, '/lower.pl'], Path),
    setup_call_cleanup(open(Path, read, S), read_string(S, _, Str), close(S)).

lower_file(Path) :-
    prolog_dir(Dir),
    atomic_list_concat([Dir, '/lower.pl'], Path).

manifest_file(Path) :-
    prolog_dir(Dir),
    atomic_list_concat([Dir, '/compile/out/manifest.json'], Path).

fixtures_dir(Dir) :- prolog_dir(Here), atomic_list_concat([Here, '/conformance/fixtures'], Dir).

fixture_files(Files) :-
    fixtures_dir(Dir),
    directory_files(Dir, Entries),
    msort(Entries, Ordered),
    findall(Path,
            ( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl'),
              atomic_list_concat([Dir, '/', Entry], Path) ),
            Files).

% ═══════════════════════════════════════════════════════════════════════════
% dynamic arm census (library(prolog_coverage))
% ═══════════════════════════════════════════════════════════════════════════

arm_census(UnreachedArms, MultiTotal, DeadPreds) :-
    lower_clause_counts(Counts),          % Name/Arity-ClauseCount, from source
    findall(N/A-Cnt, (member(N/A-Cnt, Counts), Cnt >= 2), Multi),
    length(Multi, MultiTotal),
    cov_dir(Dir),
    ( exists_directory(Dir) -> true ; make_directory_path(Dir) ),
    coverage(corpus_compile, [dir(Dir), modules([lower]), color(false)]),
    atomic_list_concat([Dir, '/lower.pl.cov'], CovPath),
    unreached_lines(CovPath, UnreachedLines),  % list of Line-Name(atom)
    lower_source_lines(SrcLines),
    classify_unreached(UnreachedLines, SrcLines, Counts, UnreachedArms, DeadPreds).

% Each unreached clause's head is reconstructed from the lower.pl source line
% (joining multi-line heads), so name/arity is exact even for overloaded names.
% An unreached clause of a multi-clause predicate is an "unreached arm"; an
% unreached clause of a single-clause predicate is a "fully-dead predicate".
classify_unreached(UnreachedLines, SrcLines, Counts, UnreachedArms, DeadPreds) :-
    findall(Line-Name/Arity-TotalArms,
            ( member(Line-_, UnreachedLines),
              head_at(SrcLines, Line, Name/Arity),
              member(Name/Arity-TotalArms, Counts),
              TotalArms >= 2 ),
            Arms0),
    sort(Arms0, UnreachedArms),
    findall(Name/Arity,
            ( member(Line-_, UnreachedLines),
              head_at(SrcLines, Line, Name/Arity),
              member(Name/Arity-1, Counts) ),
            Dead0),
    sort(Dead0, Dead),
    length(Dead, DeadPreds).

% ═══════════════════════════════════════════════════════════════════════════
% lower.pl clause inventory from the source (authoritative name/arity + clause
% count; the current_predicate/2 enumeration drops overloaded arities, so the
% source parser is the oracle)
% ═══════════════════════════════════════════════════════════════════════════

lower_clause_counts(Counts) :-
    lower_file(Path),
    setup_call_cleanup(open(Path, read, S), clause_sigs(S, Sigs), close(S)),
    setof(Sig, member(Sig, Sigs), DistinctSigs),
    findall(Sig-Cnt, (member(Sig, DistinctSigs), group_count(Sig, Sigs, Cnt)), Counts0),
    sort(Counts0, Counts).

clause_sigs(S, Sigs) :-
    read_term(S, Term, [module(arm_census)]),
    ( Term == end_of_file
    -> Sigs = []
    ; Term = (:- _)
    -> clause_sigs(S, Sigs)
    ; clause_head(Term, Sig),
      Sigs = [Sig | Rest],
      clause_sigs(S, Rest)
    ).

clause_head((Head :- _), Sig) :- !, functor(Head, N, A), Sig = N/A.
clause_head(Head, Sig) :- functor(Head, N, A), Sig = N/A.

group_count(Sig, Sigs, Cnt) :-
    findall(_, member(Sig, Sigs), Found),
    length(Found, Cnt).

lower_source_lines(Lines) :-
    lower_file(Path),
    setup_call_cleanup(open(Path, read, S), read_string(S, _, Str), close(S)),
    split_string(Str, "\n", "", Lines).

% reconstruct the clause head starting at source line Line (1-based), joining
% continuation lines until the head term parses
head_at(Lines, Line, Name/Arity) :-
    head_term(Lines, Line, 0, [], Head),
    functor(Head, Name, Arity).

head_term(Lines, Line, K, Acc, Head) :-
    ( Idx is Line + K, nth1(Idx, Lines, NextLine) -> true ; fail ),
    ( Acc == [] -> Acc1 = NextLine ; atomic_list_concat([Acc, NextLine], ' ', Acc1) ),
    normalize_head_text(Acc1, Norm),
    ( catch(read_term_from_atom(Norm, Head, [module(arm_census)]), _, fail)
    -> true
    ; K1 is K + 1, head_term(Lines, Line, K1, Acc1, Head)
    ).

% strip a trailing '.' (fact terminator), then cut at the ' :-' rule separator
normalize_head_text(Text, Norm) :-
    strip_dot(Text, T0),
    ( sub_string(T0, Before, _, _, " :-")
    -> sub_string(T0, 0, Before, _, Norm)
    ; Norm = T0
    ).

strip_dot(S, S1) :-
    string_length(S, L),
    ( L > 0, sub_string(S, _, 1, 0, ".")
    -> L1 is L - 1, sub_string(S, 0, L1, _, S1)
    ; S1 = S
    ).

% the lower-relevant slice of sweep.pl's sweep_one/5, faithful to its full
% compile + emit call chain (emit_ts:emit_program reaches lower:catalog_all_rows),
% but with no file output so nothing under compile/out is touched.
corpus_compile :-
    fixture_files(Files),
    forall(member(File, Files),
           ( read_all_fixtures(File, Entries),
             forall(member(entry(Name, Term, Bindings), Entries),
                    corpus_one(Name, Term, Bindings)) )).

corpus_one(_, Term, Bindings) :-
    default_intern_mode(Mode),
    catch(( program_plan(Term-Bindings, [intern(Mode)], Plan),
            lower_program(Plan, Lowered),
            Term = fixture(Name, _Prog, Initial, _Schedule, _Expectations),
            Plan = plan(_, prog(Decls, Rules), Types, RelPlans, _, _, _, _, Mode),
            Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
            boot_statements(Mode, Decls, Types, RelPlans, Initial,
                            LevelStatements, BootStatements),
            emit_program(Name, Plan, Lowered, BootStatements, _Text),
            ignore(( catch(( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                               SchemaRows, _),
                             option_rows(Decls, SchemaRows, SchemaRowsOpt),
                             jsonschema_text(Name, SchemaRowsOpt, _) ),
                           _SchemaError, fail )
                   ; true )),
            ignore(( catch(( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                               TypeRows, _),
                             option_rows(Decls, TypeRows, TypeRowsOpt),
                             ts_types_text(Name, TypeRowsOpt, _) ),
                           _TsError, fail )
                   ; true )),
            ignore(( catch(( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                               TypeRows2, _),
                             option_rows(Decls, TypeRows2, TypeRowsOpt2),
                             rust_types_text(Name, TypeRowsOpt2, _) ),
                           _RustError, fail )
                   ; true ))
          ),
          unsupported_construct(_), true).

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
    -> Entries = [entry(Name, Candidate, Bindings) | Rest],
        scan_fixtures(Stream, Rest)
     ; scan_fixtures(Stream, Entries)
     ).


% parse the annotated .cov: every "###" clause (never executed) as Line-Name
unreached_lines(CovPath, Unreached) :-
    setup_call_cleanup(open(CovPath, read, S), read_string(S, _, Str), close(S)),
    split_string(Str, "\n", "", Lines),
    findall(Line-Name, unreached_line(Lines, Line, Name), Unreached).

unreached_line(Lines, Line, Name) :-
    member(Text, Lines),
    re_matchsub("^\\s*(\\d+)\\s+###\\s+(.*)$", Text, Sub, []),
    get_dict(1, Sub, LineStr),
    get_dict(2, Sub, Src),
    number_string(Line, LineStr),
    string_leading_ident(Src, NameStr),
    atom_string(Name, NameStr).

% unreached arms: an unreached clause whose predicate has >= 2 clauses
cov_dir('/tmp/arm_census_cov').
