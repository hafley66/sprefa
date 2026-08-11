% golden_coverage.pl : the language-kept-in-check gate.
%
% Reads registry.pl's construct inventory and v6/dl/fixtures/golden-flex.dl6's
% own parsed term, and FAILS -- naming the construct -- when a live registry row
% is not exercised by the golden. Adding a construct to the registry therefore
% breaks this gate until the golden flexes it. That is the whole mechanism: the
% surface cannot grow in silence.
%
% TWO DETECTION PATHS, because two registry roles erase themselves at parse time:
%
%   TERM   the default. The parsed `program(Decls, Rules, Queries)` term is
%          walked for every functor/arity and every atom, so `latest/1`,
%          `decode/2`, `count/1`, `:=/2`, `mod/2`, `sh_decl/4`, `probe/4`,
%          `enum_decl/2`, `type_decl/2` and friends are found where they live.
%
%   TEXT   for the `splice_bare` rows (`next/1`, `combine/variadic`). Those are
%          spliced into an ordinary conjunction by parse_dl.pl's
%          build_surface_item/5 and leave NO trace in the term, so the only
%          honest check is the source spelling. Comments are stripped first (a
%          construct named only in the header must not satisfy its own gate) and
%          the pattern is a call shape, `name(`, not a bare word.
%
% EXPECTED-ABSENT rows carry a reason, and the gate asserts BOTH halves: the
% reason text must appear in the golden's header (so the file explains itself)
% and, for a row whose reason is its registry status, that status must still be
% `refused` or `reserved`. A unsupported construct that quietly becomes acceptance fails here
% too -- the gate is symmetric.
%
% Run: swipl -q -l golden_coverage.pl -g run -g halt
%      exit 0 = covered, exit 1 = a named construct is missing.

:- use_module('../registry', [surface/5, expression/5]).
:- use_module('../parse_dl', [parse_dl_file/4]).

% Resolved at LOAD time (prolog_load_context/2 is a read-time-only predicate),
% so the gate runs from any cwd -- the same fix `just arch` needed.
:- dynamic(golden_file/1).
:- prolog_load_context(directory, Here),
   atomic_list_concat([Here, '/../../../dl/fixtures/golden-flex.dl6'], Path),
   assertz(golden_file(Path)).

% ═══ expected_absent(Signature, Reason) ═════════════════════════════════════
% Every live-or-registered construct the golden deliberately does NOT contain,
% with the reason that must also be written in the golden's own header.

expected_absent(json_each/2,    'registry status `refused`').
expected_absent(json_array/1,   'registry status `refused`').
expected_absent(json_object/2,  'registry status `refused`').
expected_absent(sg_pattern/3,   'registry status `refused`').
expected_absent(tagged_brace/1, 'reserved').
expected_absent(set/0,          'removed word').
expected_absent(scan/variadic,  'reserved removed word').
expected_absent(zip/2,          'reserved').
expected_absent(subscribe/1,    'reserved lifecycle wrappers').
expected_absent(unsubscribe/1,  'reserved lifecycle wrappers').
expected_absent(complete/1,     'reserved lifecycle wrappers').
expected_absent(error/1,        'reserved lifecycle wrappers').
expected_absent(pre/2,           'seeded pre is covered by conformance only').

% Registry rows whose presence is checked against the SOURCE TEXT.
%
% `combine/variadic` needs the text path because the term walk keys on
% Name/Arity and a variadic row has no arity to match.
%
% `next/1` no longer NEEDS it: parse_dl.pl used to desugar both splice rows
% into a plain conjunction at parse time, which erased the spelling from the
% term (and handed the two doors different terms for one source), and it
% stopped. The text check is kept anyway because it is the stronger
% assertion of the two -- it says the golden's own SOURCE spells the word,
% which is what this file is for.
text_detected(next/1,           'next(').
text_detected(combine/variadic, 'combine(').

% ═══ the run ════════════════════════════════════════════════════════════════

run :-
    golden_file(Path),
    parse_dl_file(Path, Program, _Bindings, Findings),
    ( Findings == []
    -> true
    ;  format("GOLDEN_COVERAGE FAIL: golden-flex.dl6 has parse findings ~q~n", [Findings]), halt(1)
    ),
    read_source_text(Path, Source),
    strip_comments(Source, Code),
    term_signatures(Program, Signatures),
    required_rows(Rows),
    findall(Problem, row_problem(Rows, Signatures, Code, Source, Problem), Problems),
    length(Rows, Total),
    findall(Signature,
            ( member(row(Signature, _), Rows), \+ expected_absent(Signature, _) ),
            Exercised),
    length(Exercised, ExercisedCount),
    findall(Signature, expected_absent(Signature, _), Absent),
    length(Absent, AbsentCount),
    ( Problems == []
    -> format("GOLDEN_COVERAGE ~w registry constructs = ~w exercised + ~w named absences~n",
              [Total, ExercisedCount, AbsentCount]),
       format("GOLDEN_COVERAGE absences: ~w~n", [Absent]),
       format("GOLDEN COVERAGE HOLDS~n")
    ;  forall(member(Problem, Problems), format("GOLDEN_COVERAGE FAIL: ~w~n", [Problem])),
       length(Problems, Failed),
       format("GOLDEN_COVERAGE ~w registry constructs, ~w unaccounted for~n", [Total, Failed]),
       halt(1)
    ),
    ignore(Source = _).

%% required_rows(-Rows) is det.
%  Every construct the golden must account for: registry surface/5 rows of any
%  status (a refused row must still be NAMED in the header) plus every
%  expression/5 operator, which is the second construct table.
required_rows(Rows) :-
    findall(row(Signature, surface(Status)),
            surface(Signature, _Axis, _Analyze, _Lower, Status),
            SurfaceRows),
    findall(row(Signature, expression),
            ( expression(Signature, _Family, _Precedence, _Sql, _Type),
              \+ surface(Signature, _, _, _, _) ),
            ExpressionRows),
    append(SurfaceRows, ExpressionRows, Rows0),
    sort(Rows0, Rows).

row_problem(Rows, Signatures, Code, Source, Problem) :-
    member(row(Signature, Kind), Rows),
    \+ row_satisfied(Signature, Kind, Signatures, Code, Source),
    row_failure(Signature, Kind, Signatures, Code, Source, Problem).

row_satisfied(Signature, Kind, Signatures, Code, Source) :-
    ( expected_absent(Signature, Reason)
    -> absence_verified(Signature, Kind, Reason, Signatures, Source)
    ;  construct_present(Signature, Signatures, Code)
    ).

%% absence_verified(+Signature, +Kind, +Reason, +Signatures, +Source) is semidet.
%  An expected absence is only honoured when (a) the golden's header carries the
%  reason text, (b) the construct really is absent from the program, and (c) a
%  reason that claims a registry status agrees with the registry.
absence_verified(Signature, Kind, Reason, Signatures, Source) :-
    sub_string(Source, _, _, _, Reason),
    \+ memberchk(Signature, Signatures),
    status_agrees(Signature, Kind, Reason).

status_agrees(_Signature, expression, _Reason) :- !.
status_agrees(Signature, surface(Status), Reason) :-
    ( sub_string(Reason, _, _, _, "refused")  -> Status == refused
    ; sub_string(Reason, _, _, _, "reserved") -> Status == reserved
    ; sub_string(Reason, _, _, _, "removed")  -> Status == refused
    ; true                                     % a defect reason, not a status claim
    ),
    ignore(Signature = _).

construct_present(Signature, Signatures, Code) :-
    ( text_detected(Signature, Marker)
    -> sub_string(Code, _, _, _, Marker)
    ;  memberchk(Signature, Signatures)
    ).

row_failure(Signature, _Kind, _Signatures, _Code, Source, Problem) :-
    expected_absent(Signature, Reason),
    \+ sub_string(Source, _, _, _, Reason),
    !,
    format(atom(Problem),
           "~w is listed absent for reason ~q, but the golden's header does not say so",
           [Signature, Reason]).
row_failure(Signature, Kind, Signatures, _Code, _Source, Problem) :-
    expected_absent(Signature, Reason),
    memberchk(Signature, Signatures),
    !,
    format(atom(Problem),
           "~w is listed absent (~q) but the golden DOES use it -- promote it out of expected_absent (~w)",
           [Signature, Reason, Kind]).
row_failure(Signature, surface(Status), _Signatures, _Code, _Source, Problem) :-
    expected_absent(Signature, Reason),
    !,
    format(atom(Problem),
           "~w is excused as ~q but its registry status is now ~w -- the excuse is stale",
           [Signature, Reason, Status]).
row_failure(Signature, Kind, _Signatures, _Code, _Source, Problem) :-
    format(atom(Problem),
           "~w (~w) is a registry construct the golden does not exercise -- add it to golden-flex.dl6, or record a named unsupported construct for it in expected_absent/2 AND in the golden's header",
           [Signature, Kind]).

% ═══ term walk ══════════════════════════════════════════════════════════════
%
% Every functor/arity in the program term, plus every atom as Name/0, so `true`
% (a zero-arity registry row) is found the same way `latest/1` is. Variadic
% registry rows (`combine/variadic`) are matched by the text path instead.

term_signatures(Term, Signatures) :-
    findall(Signature, term_signature(Term, Signature), Raw),
    sort(Raw, Signatures).

term_signature(Term, Name/0) :-
    atom(Term), Name = Term.
term_signature(Term, Signature) :-
    compound(Term),
    ( functor(Term, Name, Arity), Signature = Name/Arity
    ; arg(_Index, Term, Sub), term_signature(Sub, Signature)
    ).

% ═══ source text ════════════════════════════════════════════════════════════

read_source_text(Path, Source) :-
    setup_call_cleanup(open(Path, read, Stream),
                       read_string(Stream, _Length, Source),
                       close(Stream)).

%% strip_comments(+Source, -Code) is det.
%  `#` to end of line is the only comment form (parse_dl.pl skip_ws/2). A
%  construct named only in the header must not satisfy its own coverage row,
%  which is why the text path reads this rather than the raw file.
strip_comments(Source, Code) :-
    split_string(Source, "\n", "", Lines),
    findall(Kept, ( member(Line, Lines), code_part(Line, Kept) ), KeptLines),
    atomic_list_concat(KeptLines, '\n', Code0),
    atom_string(Code0, Code).

code_part(Line, Kept) :-
    ( sub_string(Line, Before, _, _, "#")
    -> sub_string(Line, 0, Before, _, Kept)
    ;  Kept = Line
    ).
