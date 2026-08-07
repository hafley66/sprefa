% parse_dl.pl : phase D parser. Plain SWI-Prolog DCG-and-recursive-descent
% over codes, .dl6 TEXT in, the fixture term form out: prog(Decls, Rules) with
% the SAME operators the conformance fixtures use (<-, <+, :=) and the same
% wrappers (latest/1, not/1, kind/2, keyed/2, keep/2, pre/1, finalize/1, now/1,
% decode/2, json_each/2). No consult of generated text, no new deps.
%
% Variable identity survives parsing: one Vars accumulator (Name-Var pairs)
% threads across the WHOLE file, exactly mirroring what compile.pl:
% read_fixture_term/4 gets from a single read_term/3 call over one fixture
% clause (a name reused anywhere in the file is the SAME variable object,
% which is what analyze.pl:rel_columns/4 needs for its `Arg == BoundVar`
% identity check to find the right column name). Bare `_` alone is always a
% fresh anonymous variable, never accumulated; `_Foo` is an ordinary named
% variable like any other (Prolog convention, kept for readability of
% "don't care but named" occurrences such as decode(Body, fresh(_Tag, X))).
%
% TWO SURFACE DIALECTS accepted by ONE grammar (no separate code paths):
%   (a) the tsv2 "term form made visible" spelling this file's own printer
%       (print_dl.pl) emits into dl_view/*.dl6 -- <-/<+ arrows, latest/pre/
%       departed/now/decode/json_each/:= as function-call-shaped body items,
%       arithmetic infix, `rel Name(cols) log [keep(...)] [key(...)].`
%       decls.
%   (b) the existing v6/dl surface (v6/dl/grammar/dl.langium), read here so
%       v6/dl/fixtures/ghcacher.dl6 and conformance.dl6 keep parsing: `rel
%       Name(col: type, ...).` decls, `!` postfix mutation atoms,
%       `!rel(args)` prefix negation, named args `col: val`, comparisons
%       spelled `=`/`!=`/`<=` (accepted as aliases of ==/\==/=<).
%
% THE CENTRAL SUPERSEDING DECISION (recorded in SYNTAX.md, cites dl.langium
% evidence): dl.langium's `Var: name=ID` rule makes EVERY bare identifier a
% variable unconditionally (dl.langium:153-154), which cannot spell a bareword
% constant match like `phase(Endpoint, fetching)` -- a real, corpus-attested
% construct (fixtures/state_machine.pl). This grammar resolves the tension by
% keeping "bare identifier => variable" as the ONE rule in BOTH dialects (so
% dialect (b) files, which never write an unquoted atom constant at all --
% confirmed by grep, they always use a quoted string or an int instead --
% keep parsing exactly as before) and spelling atom-literal CONSTANTS with
% single quotes ('fetching', 'none', 'idle'), matching how the underlying
% fixture Prolog source already reads a quoted atom. Double-quoted text is
% the distinct SWI string type (StrLit), matching dl.langium's StrLit exactly.
%
% Named args (`col: val`) resolve to positional order using the rel's own
% declared column order (threaded via a dynamic rel_column_order table built
% while scanning decls) -- this is surface sugar the term form can already
% hold positionally, not a term-form gap, so it is resolved silently rather
% than filed as a finding. Constructs the term form truly cannot hold (mutation
% `!(...)` and retention
% markers `rel(N)`) become unsupported_surface(...) findings, collected and
% returned, never silently dropped.

:- module(parse_dl,
          [ parse_dl/4,
            parse_dl_file/4,
            parse_dl_line_for_reason/2,
            % Exported for test/plunit_tests.pl:parse_error_positions, which
            % checks the line table against a prefix walk at every index of a
            % text; parse_dl/4 alone only reaches positions a refusal lands on.
            remaining_line_column/3,
            % Exported for the diag channel (diag.pl): a
            % refusal's underlying reason resolves through its relation
            % references to the offending statement's start line and column.
            % The line/column pair is read lazily, only when a diagnostic asks,
            % so a successful compile never pays for it.
            statement_location_for_reason/3,
            statement_location_for_reference/4
          ]).

:- set_prolog_flag(back_quotes, codes).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(registry,
              [ surface/5, body_surface_for_term/6,
                wrapper_lower_role/3, host_input_roles/3 ]).
:- use_module('../0_cst_query',
              [ parse_cst_query/2, ts_query_capture_names/2 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- dynamic(finding_fact/1).
:- dynamic(rel_column_order_fact/2).
:- dynamic(host_signature_fact/3).
:- dynamic(source_statement_fact/3).
:- discontiguous body_item/5.

% The four position values below live in non-backtrackable globals, not in
% dynamic facts like the rest of this file's parse state, because a dynamic
% fact COPIES its arguments on every read: the input length and the furthest
% mark are read once per parse alternative, and the line-start table is a
% term with one argument per source line.

record_finding(F) :- assertz(finding_fact(F)).
record_column_order(Name, Cols) :-
    retractall(rel_column_order_fact(Name, _)),
    assertz(rel_column_order_fact(Name, Cols)).
lookup_column_order(Name, Cols) :- rel_column_order_fact(Name, Cols).
record_host_signature(Name, Inputs, Outputs) :-
    retractall(host_signature_fact(Name, _, _)),
    assertz(host_signature_fact(Name, Inputs, Outputs)).

% ═══ entry points ════════════════════════════════════════════════════════════

parse_dl_file(FilePath, Prog, Bindings, Findings) :-
    read_file_to_codes(FilePath, Codes, []),
    parse_dl_source(FilePath, Codes, Prog, Bindings, Findings).

parse_dl(Codes, Prog, Bindings, Findings) :-
    parse_dl_source(none, Codes, Prog, Bindings, Findings).

parse_dl_source(_Source, Codes, _Prog, _Bindings, _Findings) :-
    var(Codes),
    !,
    throw(dl_parse_error(invalid_input, position(1, 1))).
parse_dl_source(_Source, Codes, Prog, Bindings, Findings) :-
    retractall(finding_fact(_)),
    retractall(rel_column_order_fact(_, _)),
    retractall(host_signature_fact(_, _, _)),
    retractall(source_statement_fact(_, _, _)),
    length(Codes, InputLength),
    nb_setval(parse_input_length, InputLength),
    nb_setval(parse_furthest_remaining, InputLength),
    build_line_starts(Codes),
    statements(Codes, Left, [], VarsFinal, ParsedDecls, ParsedRules, Queries),
    ( Left == []
    -> true
    ;  mark_furthest(Left), parse_failure(trailing_input)
    ),
    normalize_relation_value_decls(ParsedDecls, Decls),
    normalize_host_calls(Decls, ParsedRules, Rules),
    maplist(swap_pair, VarsFinal, BindingsRev),
    reverse(BindingsRev, Bindings),
    findall(F, finding_fact(F), Findings),
    ( Queries == [],
      \+ member(sh_decl(_, _, _, _), Decls),
      \+ member(bind_decl(_, _), Decls)
    -> Prog = prog(Decls, Rules)
    ; Prog = program(Decls, Rules, Queries)
    ).

parse_failure(Reason) :-
    furthest_line_col(Line, Column),
    throw(dl_parse_error(Reason, position(Line, Column))).

% A suffix is identified by how many codes remain in it, so the furthest mark
% is a MINIMUM remaining count and needs no arithmetic per call. The one thing
% this predicate may not do is walk the input: it runs at every DCG
% alternative, so anything proportional to file size here is quadratic.
mark_furthest(Suffix) :-
    length(Suffix, RemainingLength),
    nb_getval(parse_furthest_remaining, FurthestRemaining),
    RemainingLength < FurthestRemaining,
    !,
    nb_setval(parse_furthest_remaining, RemainingLength).
mark_furthest(_).

furthest_line_col(Line, Column) :-
    nb_getval(parse_furthest_remaining, RemainingLength),
    remaining_line_column(RemainingLength, Line, Column).

% arg/3 over the line-start table, so a position costs one binary search
% instead of a walk of every code before it.
remaining_line_column(RemainingLength, Line, Column) :-
    nb_getval(parse_input_length, InputLength),
    Index is InputLength - RemainingLength,
    nb_getval(parse_line_starts, LineStarts),
    nb_getval(parse_line_count, LineCount),
    line_containing(1, LineCount, Index, LineStarts, Line),
    arg(Line, LineStarts, LineStart),
    Column is Index - LineStart + 1.

line_containing(Low, High, Index, LineStarts, Line) :-
    (   Low >= High
    ->  Line = Low
    ;   Mid is (Low + High + 1) // 2,
        arg(Mid, LineStarts, MidStart),
        (   MidStart =< Index
        ->  line_containing(Mid, High, Index, LineStarts, Line)
        ;   Before is Mid - 1,
            line_containing(Low, Before, Index, LineStarts, Line)
        )
    ).

% split_string/4 does the newline scan below Prolog, so the table costs one
% pass over the LINES rather than one pass per position asked about.
build_line_starts(Codes) :-
    split_string(Codes, "\n", "", Lines),
    line_start_offsets(Lines, 0, Offsets),
    LineStarts =.. [line_starts | Offsets],
    length(Offsets, LineCount),
    nb_setval(parse_line_starts, LineStarts),
    nb_setval(parse_line_count, LineCount).

line_start_offsets([], _, []).
line_start_offsets([Line | Rest], Offset, [Offset | More]) :-
    string_length(Line, Length),
    Next is Offset + Length + 1,
    line_start_offsets(Rest, Next, More).

prolog:message(dl_parse_error(Reason, position(Line, Column))) -->
    [ 'parse error at line ~d, column ~d: ~w'-[Line, Column, Reason] ].

parse_dl_line_for_reason(Reason, Line) :-
    findall(Ref, reason_relation_reference(Reason, Ref), References0),
    sort(References0, References),
    ( member(Ref, References), statement_line_for_reference(rule, Ref, Line)
    -> true
    ;  member(Ref, References), statement_line_for_reference(decl, Ref, Line)
    -> true
    ).

statement_location_for_reason(Reason, Line, Column) :-
    findall(Ref, reason_relation_reference(Reason, Ref), References0),
    sort(References0, References),
    ( member(Ref, References), statement_location_for_reference(rule, Ref, Line, Column)
    ; member(Ref, References), statement_location_for_reference(decl, Ref, Line, Column) ).

reason_relation_reference(Reason, Name/Arity) :-
    sub_term(Name/Arity, Reason),
    atom(Name),
    integer(Arity).

% Statements are recorded whole, at their suffix length, and turned into
% references and line numbers only when a refusal asks: expanding each one into
% its reference set costs one assert per relation the statement mentions, and
% resolving its line costs a line-table lookup, on every successful parse.
%
% A refusal names the OFFENDING RULE, which is the rule that DEFINES the
% reference (its head), so resolution tries the defining statement first. The
% sub-term scan below it is only a fallback for a reference the refusal names
% that no rule heads (a body relation), and it can therefore still pick an
% earlier statement that merely mentions the relation.
statement_location_for_reference(rule, Reference, Line, Column) :-
    source_statement_fact(rule, Item, RemainingLength),
    statement_head_reference(Item, Reference),
    !,
    remaining_line_column(RemainingLength, Line, Column).
statement_location_for_reference(Kind, Reference, Line, Column) :-
    source_statement_fact(Kind, Item, RemainingLength),
    statement_reference(Kind, Item, Reference),
    !,
    remaining_line_column(RemainingLength, Line, Column).

statement_head_reference((Head <- _), Name/Arity) :-
    !,
    functor(Head, Name, Arity).
statement_head_reference((Head <+ _), Name/Arity) :-
    functor(Head, Name, Arity).

statement_line_for_reference(Kind, Reference, Line) :-
    statement_location_for_reference(Kind, Reference, Line, _Column).

statement_reference(rule, Rule, Name/Arity) :-
    sub_term(Term, Rule),
    compound(Term),
    functor(Term, Name, Arity),
    atom(Name),
    !.
statement_reference(decl, Declarations, Reference) :-
    member(Declaration, Declarations),
    declaration_source_ref(Declaration, Reference),
    !.

% Clause 3's variable first argument is a candidate for every Kind, so without
% the cuts each statement doubled parse_dl/4's identical solutions.
record_statement_source_lines(decl_list, Declarations, RemainingLength) :-
    !,
    assertz(source_statement_fact(decl, Declarations, RemainingLength)).
record_statement_source_lines(rule, Rule, RemainingLength) :-
    !,
    assertz(source_statement_fact(rule, Rule, RemainingLength)).
record_statement_source_lines(_, _, _).

declaration_source_ref(kind(Ref, _), Ref).
declaration_source_ref(keyed(Ref, _), Ref).
declaration_source_ref(keep(Ref, _), Ref).
declaration_source_ref(type_decl(Name, Specs), Name/Arity) :-
    length(Specs, Arity).
declaration_source_ref(col_type(Ref, _, _), Ref).
declaration_source_ref(sh_decl(Name, Inputs, Outputs, _), Name/Arity) :-
    append(Inputs, Outputs, Columns),
    length(Columns, Arity).

swap_pair(Name-Var, Name=Var).

% Plain RHS calls resolve against the completed declaration set, so declaration
% order does not change whether a call is a host probe or an ordinary relation.

normalize_host_calls(_, [], []).
normalize_host_calls(Decls, [Rule | Rest], [Normalized | More]) :-
    normalize_host_rule(Decls, Rule, Normalized),
    normalize_host_calls(Decls, Rest, More).

normalize_host_rule(Decls, (Head <- Body), (Head <- Normalized)) :-
    !,
    normalize_host_body(Decls, Body, Normalized).
normalize_host_rule(Decls, (Head <+ Body), (Head <+ Normalized)) :-
    !,
    normalize_host_body(Decls, Body, Normalized).
normalize_host_rule(Decls, match(Source, Arms), match(Source, Normalized)) :-
    !,
    normalize_host_arms(Decls, Arms, Normalized).
normalize_host_rule(_, Rule, Rule).

normalize_host_arms(Decls, (Left ; Right), (LeftNormalized ; RightNormalized)) :-
    !,
    normalize_host_arms(Decls, Left, LeftNormalized),
    normalize_host_arms(Decls, Right, RightNormalized).
normalize_host_arms(Decls, Arm, Normalized) :-
    normalize_host_rule(Decls, Arm, Normalized).

normalize_host_body(Decls, (Left, Right), (LeftNormalized, RightNormalized)) :-
    !,
    normalize_host_body(Decls, Left, LeftNormalized),
    normalize_host_body(Decls, Right, RightNormalized).
normalize_host_body(_, Probe, Probe) :-
    Probe = probe(_, _, _, _),
    !.
normalize_host_body(_, Item, Item) :-
    body_surface_for_term(Item, _, _, _, _, _),
    !.
normalize_host_body(Decls, Atom,
                    probe(Name, InputValues, OutputValues, Salts)) :-
    compound(Atom),
    functor(Atom, Name, _),
    member(sh_decl(Name, Inputs, _, _), Decls),
    !,
    Atom =.. [_ | Values],
    length(Inputs, InputCount),
    split_probe_values(InputCount, Values, SurfaceInputValues, OutputValues),
    host_input_roles(Name, Inputs, Roles),
    partition_host_input_values(Inputs, SurfaceInputValues, Roles,
                                InputValues, Salts).
normalize_host_body(_, Item, Item).

% ═══ top-level statement loop : one Vars accumulator threads across every
% decl and rule in the file (the whole-clause variable scope compile.pl's
% read_term call gets for free); Decls/Rules accumulate as the recursion
% unwinds, splicing in each decl_list statement's whole list at once ═══════

statements(S0, S, Vars0, Vars, Decls, Rules, Queries) :-
    skip_ws(S0, S1),
    mark_furthest(S1),
    ( S1 == []
    -> Decls = [], Rules = [], Queries = [], Vars = Vars0, S = S1
    ; ( statement(Kind, Item, Vars0, Vars1, S1, S2)
      -> length(S1, StatementRemaining),
         record_statement_source_lines(Kind, Item, StatementRemaining)
      ;  mark_furthest(S1), parse_failure(statement)
      ),
      statements(S2, S, Vars1, Vars, Decls1, Rules1, Queries1),
      ( Kind == decl_list -> append(Item, Decls1, Decls), Rules = Rules1, Queries = Queries1
      ; Kind == rule -> Decls = Decls1, Rules = [Item | Rules1], Queries = Queries1
      ; Kind == query -> Decls = Decls1, Rules = Rules1, Queries = [Item | Queries1]
      ; Kind == skip -> Decls = Decls1, Rules = Rules1, Queries = Queries1
      )
    ).

% ═══ whitespace + `#` line comments (plain predicate, not `-->`, since the
% rest of this parser is written with explicit S0/S args throughout) ════════

% mark_furthest keeps a maximum and every position these two scans walk past
% is behind the position they stop at, so one mark at the stop stands for all
% of them. Marking each code instead costs one call per whitespace and comment
% character in the file, which is most of the file.
skip_ws(S0, S) :-
    ( S0 = [C | S1], (code_type(C, space) ; C == 0'\n ; C == 0'\r)
    -> skip_ws(S1, S)
    ; S0 = [0'# | S1]
    -> skip_to_eol(S1, S2), skip_ws(S2, S)
    ; S = S0, mark_furthest(S0)
    ).

skip_to_eol(S0, S) :-
    ( S0 = [C | S1], C \== 0'\n -> skip_to_eol(S1, S)
    ; S0 = [0'\n | S1] -> S = S1
    ; S = S0
    ).

ws0(S0, S) :- skip_ws(S0, S).

% ═══ literal punctuation / keyword matching ════════════════════════════════

% A full match ends at S, which is past every code it consumed, so one mark
% there covers the whole literal. A PARTIAL match still has to name the
% deepest code it consumed, and only the failing recursion knows where that
% is, which is why the miss marks on the way out instead of on the way in.
lit_dcg([], S, S) :-
    mark_furthest(S).
lit_dcg([Code | Codes], Suffix, S) :-
    Suffix = [Code | Rest],
    (   lit_dcg(Codes, Rest, S)
    ->  true
    ;   mark_furthest(Suffix),
        fail
    ).

% whole-word keyword match: literal Codes followed by a non-identifier char
% (or end of input), so e.g. `rel` never fires on `related` and `sh` never
% fires on `shared_count`.
word(Codes, S0, S) :-
    lit_dcg(Codes, S0, S1),
    \+ (S1 = [C | _], (code_type(C, alnum) ; C == 0'_)),
    S = S1.

peek(C, S, S) :- S = [C | _], !.

% ═══ identifiers ════════════════════════════════════════════════════════════
% ident_start: letter or underscore. ident_rest: alnum or underscore. Case is
% NOT semantically distinguished at the lexer level; role (variable vs
% relation-name vs label) is decided by grammar POSITION, never spelling --
% see the module header's superseding-decision note.

ident(Name, S0, S) :-
    mark_furthest(S0),
    S0 = [C0 | Rest0],
    ( code_type(C0, alpha) ; C0 == 0'_ ), !,
    ident_rest_codes(Rest0, RestCodes, S),
    atom_codes(Name, [C0 | RestCodes]).

ident_rest_codes([C | Cs], [C | More], S) :-
    ( code_type(C, alnum) ; C == 0'_ ), !,
    ident_rest_codes(Cs, More, S).
ident_rest_codes(S, [], S).

% ═══ numbers ════════════════════════════════════════════════════════════════

integer_lit(Value, S0, S) :-
    mark_furthest(S0),
    ( S0 = [0'- | S1] -> Neg = true, S2 = S1 ; Neg = false, S2 = S0 ),
    S2 = [D0 | _], code_type(D0, digit), !,
    digits(S2, Digits, S),
    number_codes(Magnitude, Digits),
    ( Neg == true -> Value is -Magnitude ; Value = Magnitude ).

digits([C | Cs], [C | More], S) :- code_type(C, digit), !, digits(Cs, More, S).
digits(S, [], S) :- mark_furthest(S).

% A float token contains a decimal point or exponent, so integer spelling
% remains on integer_lit/3. Only finite IEEE-754 values enter the AST.
float_lit(Value, S0, S) :-
    mark_furthest(S0),
    phrase(float_codes(Codes), S0, S),
    number_codes(Value, Codes),
    float(Value),
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]).

float_codes(Codes) -->
    optional_minus(Sign),
    digits_codes(Int),
    float_tail(Tail),
    { append([Sign, Int, Tail], Codes) }.

optional_minus([0'-]) --> `-`, !.
optional_minus([]) --> [].

digits_codes([Digit | Rest]) -->
    [Digit], { code_type(Digit, digit) }, !,
    digits_codes_rest(Rest).

digits_codes_rest([Digit | Rest]) -->
    [Digit], { code_type(Digit, digit) }, !,
    digits_codes_rest(Rest).
digits_codes_rest([]) --> [].

float_tail(Codes) -->
    `.`, digits_codes(Fraction), exponent_codes(Exponent),
    { append([[0'.], Fraction, Exponent], Codes) }.
float_tail(Codes) -->
    exponent_codes_required(Codes).

exponent_codes(Codes) --> exponent_codes_required(Codes), !.
exponent_codes([]) --> [].

exponent_codes_required([Marker | Codes]) -->
    [Marker], { memberchk(Marker, [0'e, 0'E]) },
    exponent_sign(Sign),
    digits_codes(Digits),
    { append(Sign, Digits, Codes) }.

exponent_sign([Sign]) --> [Sign], { memberchk(Sign, [0'+, 0'-]) }, !.
exponent_sign([]) --> [].

% ═══ quoted atom 'text' and string "text" literals ══════════════════════════
% Both support \' \" \\ \n \t escapes and the doubled-quote escape ('' inside
% '...' is one literal quote, the plain Prolog convention).

quoted_atom_lit(Atom, S0, S) :-
    mark_furthest(S0),
    S0 = [0'\' | S1], !,
    quoted_chars(0'\', S1, Codes, S),
    atom_codes(Atom, Codes).

string_lit(Str, S0, S) :-
    mark_furthest(S0),
    S0 = [0'" | S1], !,
    quoted_chars(0'", S1, Codes, S),
    string_codes(Str, Codes).

quoted_chars(Quote, [Quote, Quote | Rest], [Quote | More], S) :- !,
    mark_furthest([Quote, Quote | Rest]),
    quoted_chars(Quote, Rest, More, S).
quoted_chars(Quote, [Quote | Rest], [], Rest) :- !.
quoted_chars(Quote, [0'\\, Esc | Rest], Codes, S) :- !,
    mark_furthest([0'\\, Esc | Rest]),
    escape_codes(Quote, Esc, Codes, More),
    quoted_chars(Quote, Rest, More, S).
quoted_chars(Quote, [C | Rest], [C | More], S) :-
    quoted_chars(Quote, Rest, More, S).

% THE ESCAPE RULE, and the other half of it lives in emit_ts.pl:js_template/2.
%
%   \n \t \r   the three real escapes
%   \\         one backslash
%   \' \"      the string's own quote character
%   \X         TWO characters: the backslash and X, unchanged
%
% The last line is the whole decision. This used to end in a catch-all
% `escape_code(C, C)` that DROPPED the backslash, so `\d` parsed as `d` and a
% regex written in a .dl6 string became a different regex with no error --
% which is why every regex in this repo's .dl6 files is backslash-free. The
% strings this language carries are regexes and shell fragments, where `\d`
% and `\.` are the common case, and there is no reading of `\d` under which
% `d` is what the author meant.
escape_codes(_, 0'n,  [0'\n  | More], More) :- !.
escape_codes(_, 0't,  [0'\t  | More], More) :- !.
escape_codes(_, 0'r,  [0'\r  | More], More) :- !.
escape_codes(_, 0'\\, [0'\\  | More], More) :- !.
escape_codes(Quote, Quote, [Quote | More], More) :- !.
escape_codes(_, Other, [0'\\, Other | More], More).

% ═══ variables (Name-Var accumulator threaded explicitly through every
% grammar predicate that can introduce or reference one) ════════════════════

get_or_make_var(Name, Vars0, Var, Vars) :-
    ( memberchk(Name-Existing, Vars0)
    -> Var = Existing, Vars = Vars0
    ; Vars = [Name-Var | Vars0]
    ).

% ═══ statement dispatch ══════════════════════════════════════════════════════

statement(Kind, Item, Vars0, Vars, S0, S) :-
    skip_ws(S0, S1),
    ( bind_decl_stmt(Item0, S1, S2) -> Kind = decl_list, Item = [Item0], Vars = Vars0, S = S2
    ; decl_a_stmt(Item0, S1, S2) -> Kind = decl_list, Item = Item0, Vars = Vars0, S = S2
    ; decl_b_stmt(Item0, S1, S2) -> Kind = decl_list, Item = Item0, Vars = Vars0, S = S2
    ; sh_decl_stmt(Item0, S1, S2) -> Kind = decl_list, Item = [Item0], Vars = Vars0, S = S2
    ; query_stmt(Item0, Vars0, Vars1, S1, S2) -> Kind = query, Item = Item0, Vars = Vars1, S = S2
    ; match_stmt(Item0, Vars0, Vars1, S1, S2) -> Kind = rule, annotate_cst_item(Item0, Vars1, Item), Vars = Vars1, S = S2
    ; rule_stmt(Item0, Vars0, Vars1, S1, S2) -> Kind = rule, annotate_cst_item(Item0, Vars1, Item), Vars = Vars1, S = S2
    ).

% ═══ dialect-A decl: `rel Name(col[: type], ...) [log] [keep(all|count(N))]
% [key(P, ...)].` (EXT surface -- dl.langium has no kind/keyed/keep
% expressed in one line at all, see SYNTAX.md). Every modifier is OPTIONAL
% and they may appear in ANY order (a plain findall-in-order loop, not a
% fixed kind-then-keep-then-keyed sequence): the fixture corpus declares a
% ref with any subset of {kind, keep, keyed} (a bare `keyed(Ref,[1])` alone
% with no kind/2 at all is a real corpus shape, engine_core.pl's
% log_without_retention_rejected declares kind/2 with NO keep/2 on purpose
% to test the missing-retention throw) and G1's round-trip is a `=@=` LIST
% variant check, so the printer must reproduce exactly the entries that were
% literally present -- never synthesize a default one, since decl_keep/3's
% own "all" fallback is an ANALYSIS-time convenience, not something the
% original author necessarily wrote. ════════════════════════════════════════

decl_a_stmt([enum_decl(Name, VariantTerms)], S0, S) :-
    word(`rel`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    enum_decl_variants(VariantTerms, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S),
    record_enum_column_orders(Name, VariantTerms).

decl_a_stmt(DeclList, S0, S) :-
    word(`rel`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_a_columns(Specs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    length(Specs, Arity),
    Ref = Name/Arity,
    column_spec_names(Specs, Cols),
    record_column_order(Name, Cols),
    ws0(S8, S9),
    typed_decl_entries(Ref, Specs, TypedDecls),
    decl_a_modifiers(Ref, Modifiers, S9, S10),
    append(TypedDecls, Modifiers, DeclList),
    ws0(S10, S11),
    lit_dcg(`.`, S11, S).

decl_a_modifiers(Ref, [Decl | Rest], S0, S) :-
    ( word(`log`, S0, S1) -> Decl = kind(Ref, log)
    ; keep_clause(Policy, S0, S1) -> Decl = keep(Ref, Policy)
    ; key_clause(Positions, S0, S1) -> Decl = keyed(Ref, Positions)
    ), !,
    ws0(S1, S2),
    decl_a_modifiers(Ref, Rest, S2, S).
decl_a_modifiers(Ref, Decls, S0, S) :-
    word(`set`, S0, S1), !,
    record_finding(unsupported_surface(removed_word(set))),
    ws0(S1, S2),
    decl_a_modifiers(Ref, Decls, S2, S).
decl_a_modifiers(_, [], S, S).

decl_a_columns([], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
decl_a_columns([Spec | Rest], S0, S) :-
    decl_a_column(Spec, S0, S1), ws0(S1, S2),
    ( lit_dcg(`,`, S2, S3) -> decl_a_columns(Rest, S3, S) ; Rest = [], S = S2 ).

decl_a_column(column(Name, Type), S0, S) :-
    ws0(S0, S1), ident(Name, S1, S2), ws0(S2, S3),
    ( lit_dcg(`:`, S3, S4)
    -> ws0(S4, S5), typed_column_type(Type, S5, S)
    ; Type = none, S = S3
    ).

typed_column_type(int, S0, S) :- word(`int`, S0, S), !.
typed_column_type(text, S0, S) :- word(`text`, S0, S), !.
typed_column_type(json, S0, S) :- word(`json`, S0, S), !.
typed_column_type(bool, S0, S) :- word(`bool`, S0, S), !.
% ruling list_spelling = list_of_type. The ONE parametric type, and its
% argument is a bare type word, never a nested list: 0_type_plane.pl's
% column_storage/3 refuses list(list(_)) and list(<struct>) by name, so the
% grammar stays permissive and the refusal stays where the reason lives.
typed_column_type(list(Element), S0, S) :-
    word(`list`, S0, S1), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    typed_column_type(Element, S4, S5), ws0(S5, S6),
    lit_dcg(`)`, S6, S).
typed_column_type(float, S0, S) :- word(`float`, S0, S), !.
% STRUCT-AS-ROWS (ruling compound_storage): a bare identifier in type
% position names a referenced relation value, and the column stores a ref to
% that relation's dictionary. A name with no matching `rel` declaration is
% not silently a text column: 0_type_plane.pl:column_storage/3 throws
% column_type_unknown, so a typo remains a named refusal.
typed_column_type(Name, S0, S) :- ident(Name, S0, S).

enum_decl_variants((First ; Rest), S0, S) :-
    enum_decl_variant(First, S0, S1),
    ws0(S1, S2),
    lit_dcg(`;`, S2, S3),
    ws0(S3, S4),
    enum_decl_variants(Rest, S4, S).
enum_decl_variants(Variant, S0, S) :-
    enum_decl_variant(Variant, S0, S).

enum_decl_variant(Variant, S0, S) :-
    ws0(S0, S1),
    ident(VariantName, S1, S2),
    ws0(S2, S3),
    lit_dcg(`(`, S3, S4),
    enum_decl_columns(Fields, S4, S5),
    ws0(S5, S6),
    lit_dcg(`)`, S6, S),
    Variant =.. [VariantName | Fields].

enum_decl_columns([], S0, S) :-
    ws0(S0, S1),
    peek(0'), S1, S),
    !.
enum_decl_columns([Field | Rest], S0, S) :-
    ws0(S0, S1),
    ident(ColumnName, S1, S2),
    ws0(S2, S3),
    lit_dcg(`:`, S3, S4),
    ws0(S4, S5),
    ident(TypeName, S5, S6),
    Field =.. [':', ColumnName, TypeName],
    ws0(S6, S7),
    ( lit_dcg(`,`, S7, S8)
    -> enum_decl_columns(Rest, S8, S)
    ; Rest = [], S = S7
    ).

record_enum_column_orders(RelName, VariantTerms) :-
    tag_rel_name(RelName, TagName),
    record_column_order(TagName, [id, tag]),
    forall(enum_decl_variant_term(VariantTerms, Variant),
           record_enum_variant_column_order(RelName, Variant)).

enum_decl_variant_term((Left ; Right), Variant) :-
    !,
    ( enum_decl_variant_term(Left, Variant)
    ; enum_decl_variant_term(Right, Variant)
    ).
enum_decl_variant_term(Variant, Variant).

record_enum_variant_column_order(RelName, Variant) :-
    Variant =.. [VariantName | Fields],
    maplist(enum_field_column_name, Fields, ColumnNames),
    atomic_list_concat([RelName, VariantName], '_', VariantRelName),
    record_column_order(VariantRelName, [id | ColumnNames]).

enum_field_column_name(Field, ColumnName) :-
    Field =.. [':', ColumnName, _].

tag_rel_name(RelName, TagName) :-
    atomic_list_concat([RelName, tag], '_', TagName).

column_spec_names([], []).
column_spec_names([column(Name, _) | Rest], [Name | More]) :-
    column_spec_names(Rest, More).

typed_decl_entries(_, [], []).
typed_decl_entries(Ref, [column(Column, Type) | Rest], Decls) :-
    ( Type == none
    -> Decls = More
    ; Decls = [col_type(Ref, Column, Type) | More]
    ),
    typed_decl_entries(Ref, Rest, More).

keep_clause(Policy, S0, S) :-
    word(`keep`, S0, S1), ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4),
    ( word(`all`, S4, S5) -> Policy = all
    ; word(`count`, S4, S5a), ws0(S5a, S5b), lit_dcg(`(`, S5b, S5c),
      ws0(S5c, S5d), integer_lit(N, S5d, S5e), ws0(S5e, S5f), lit_dcg(`)`, S5f, S5)
    -> Policy = count(N)
    ),
    ws0(S5, S6), lit_dcg(`)`, S6, S).

key_clause(Positions, S0, S) :-
    word(`key`, S0, S1), ws0(S1, S2), lit_dcg(`(`, S2, S3),
    int_list(Positions, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S).

int_list([N | Rest], S0, S) :-
    ws0(S0, S1), integer_lit(N, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> int_list(Rest, S4, S) ; Rest = [], S = S3 ).

% ═══ dialect-B decl: `rel ['(' INT ')'] Name(col: type, ...).` (real v6/dl
% surface, dl.langium:28-30). Retention is parsed and RECORDED as a finding
% (never lowered -- Key()/retention markers are frontier per the grammar's
% own comment, dl.langium:38). A bare dialect-B rel has the engine's set
% fallback, so it contributes no synthetic kind(set) declaration.

decl_b_stmt(DeclList, S0, S) :-
    word(`rel`, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`(`, S2, S3a)
    -> ws0(S3a, S3b), integer_lit(Retention, S3b, S3c), ws0(S3c, S3d),
       lit_dcg(`)`, S3d, S3), HasRetention = true
    ; S3 = S2, HasRetention = false
    ),
    ws0(S3, S4),
    ident(Name, S4, S5),
    ws0(S5, S6),
    lit_dcg(`(`, S6, S7),
    decl_b_columns(Name, Specs, S7, S8),
    ws0(S8, S9),
    lit_dcg(`)`, S9, S10),
    ws0(S10, S11),
    lit_dcg(`.`, S11, S),
    length(Specs, Arity),
    Ref = Name/Arity,
    column_spec_names(Specs, Cols),
    record_column_order(Name, Cols),
    ( HasRetention == true -> record_finding(unsupported_surface(retention_marker(Ref, Retention))) ; true ),
    typed_decl_entries(Ref, Specs, DeclList).

% Wrapper column types (Key(text)/Min(int)/Max(int)) parse -- and their
% wrapper name is recorded as an unsupported_surface(column_type_wrapper(...))
% finding, since kind/keyed/keep carry no per-column type info at all in
% this term form and the wrapped type is silently discarded otherwise.
% dl.langium's own comment (grammar lines 38-39) already calls Key() semantically
% inert and Min()/Max() a load error ("frontier") in the real bridge -- this
% just makes the same gap visible on the tsv2 side instead of swallowing it.
% Neither ghcacher.dl6 nor conformance.dl6 actually uses any of the three
% (grepped), so this path is UNTESTED by G2's real files; kept here anyway
% since a future .dl6 file using them must not be silently mis-lowered.

decl_b_columns(_, [], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
decl_b_columns(RelName, [column(ColName, Type) | Rest], S0, S) :-
    ws0(S0, S1), ident(ColName, S1, S2), ws0(S2, S3), lit_dcg(`:`, S3, S4),
    ws0(S4, S5), decl_b_column_type(RelName, ColName, Type, S5, S6), ws0(S6, S7),
    ( lit_dcg(`,`, S7, S8) -> decl_b_columns(RelName, Rest, S8, S) ; Rest = [], S = S7 ).

% ONE type vocabulary, read through typed_column_type/3 -- where `rel` decls
% already read it, and where host OUTPUT columns already read it
% (host_output_column_type/5 below is this exact two-clause shape). These
% clauses used to spell int|text|json by hand, so `float`, `bool` and a
% declared struct name all fell through to the wrapper clause, degraded to the
% untyped `none`, and reported unsupported_surface(column_type_wrapper(...)).
% Host INPUTS and bind columns were the last surfaces answering a narrower
% vocabulary than the `rel` decl sitting beside them.
%
% ORDER IS THE SEMANTICS. The wrapper forms go first: typed_column_type/3's
% last clause takes any bare identifier as a struct type name, and `Key` in
% `Key(text)` is a bare identifier, so reading types first would leave the
% `(text)` unconsumed and turn a named refusal into a parse error.
decl_b_column_type(RelName, ColName, none, S0, S) :-
    coltype(Wrapper, S0, S),
    Wrapper \== none,
    !,
    record_finding(unsupported_surface(column_type_wrapper(RelName, ColName, Wrapper))).
decl_b_column_type(_, _, Type, S0, S) :-
    typed_column_type(Type, S0, S).

coltype(Wrapper, S0, S) :-
    ( word(`Key`, S0, S1) -> Wrapper = 'Key'
    ; word(`Min`, S0, S1) -> Wrapper = 'Min'
    ; word(`Max`, S0, S1) -> Wrapper = 'Max'
    ), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4), ident(_, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S).
coltype(none, S0, S) :- ident(_, S0, S).

% ═══ selected world declarations ════════════════════════════════════════════

% A relation named in column type position supplies that relation's value
% domain. The parser keeps one surface declaration (`rel`) and normalizes the
% referenced relation to the existing type_decl IR so every downstream shape,
% interning, rendering, and refusal path remains shared.
normalize_relation_value_decls(Decls0, Decls) :-
    findall(Name,
            ( declared_column_type_name(Decls0, Name),
              relation_schema(Decls0, Name, _, _) ),
            Names0),
    sort(Names0, ValueRelationNames),
    normalize_relation_value_decls(Decls0, ValueRelationNames, [], Decls).

normalize_relation_value_decls([], _, _, []).
normalize_relation_value_decls([Head | Rest],
                               ValueNames, Seen,
                               [type_decl(Name, Specs), Head | More]) :-
    Head = col_type(Name/Arity, _, _),
    memberchk(Name, ValueNames),
    \+ memberchk(Name, Seen),
    !,
    relation_schema([Head | Rest],
                    Name, Name/Arity, Specs),
    normalize_relation_value_decls(Rest, ValueNames, [Name | Seen], More).
normalize_relation_value_decls([Head | Rest],
                               ValueNames, Seen, [Head | More]) :-
    Head = col_type(Name/_, _, _),
    memberchk(Name, ValueNames),
    !,
    normalize_relation_value_decls(Rest, ValueNames, Seen, More).
normalize_relation_value_decls([Decl | Rest], ValueNames, Seen,
                               [Decl | More]) :-
    normalize_relation_value_decls(Rest, ValueNames, Seen, More).

relation_schema(Decls, Name, Ref, Specs) :-
    once(member(col_type(Name/Arity, _, _), Decls)),
    Ref = Name/Arity,
    findall(col(Column, Type),
            member(col_type(Ref, Column, Type), Decls),
            Specs),
    length(Specs, Arity).

declared_column_type_name(Decls, Name) :-
    member(col_type(_, _, Name), Decls),
    \+ scalar_column_type(Name).
declared_column_type_name(Decls, Name) :-
    member(sh_decl(_, Inputs, Outputs, _), Decls),
    ( member(col(_, Name), Inputs) ; member(col(_, Name), Outputs) ),
    \+ scalar_column_type(Name).
declared_column_type_name(Decls, Name) :-
    member(bind_decl(_, Columns), Decls),
    member(col(_, Name), Columns),
    \+ scalar_column_type(Name).

scalar_column_type(int).
scalar_column_type(text).
scalar_column_type(json).
scalar_column_type(bool).
scalar_column_type(float).

bind_decl_stmt(bind_decl(Name, Columns), S0, S) :-
    word(`bind`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_b_columns(Name, Specs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S),
    specs_to_columns(Specs, Columns),
    column_spec_names(Specs, Names),
    record_column_order(Name, Names).

sh_decl_stmt(sh_decl(Name, Inputs, Outputs, template(Template)), S0, S) :-
    word(`sh`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_b_columns(Name, InputSpecs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`->`, S9, S10),
    ws0(S10, S11),
    lit_dcg(`(`, S11, S12),
    host_output_columns(Name, OutputSpecs, S12, S13),
    ws0(S13, S14),
    lit_dcg(`)`, S14, S15),
    ws0(S15, S16),
    lit_dcg(`=`, S16, S17),
    ws0(S17, S18),
    template_lit(Template, S18, S19),
    ws0(S19, S20),
    lit_dcg(`.`, S20, S),
    specs_to_columns(InputSpecs, Inputs),
    specs_to_columns(OutputSpecs, Outputs),
    append(InputSpecs, OutputSpecs, Specs),
    column_spec_names(Specs, Names),
    record_column_order(Name, Names),
    record_host_signature(Name, Inputs, Outputs).

sh_decl_stmt(unsupported_host_decl(Name, Columns), S0, S) :-
    word(`sh`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_b_columns(Name, Specs, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`=`, S9, S10),
    ws0(S10, S11),
    template_lit(_, S11, S12),
    ws0(S12, S13),
    lit_dcg(`.`, S13, S),
    specs_to_columns(Specs, Columns),
    length(Columns, Arity),
    column_spec_names(Specs, Names),
    record_column_order(Name, Names),
    record_finding(unsupported_surface(host_decl_inferred(Name/Arity))).

host_output_columns(_, [], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
host_output_columns(RelName, [column(ColName, Type) | Rest], S0, S) :-
    ws0(S0, S1), ident(ColName, S1, S2), ws0(S2, S3), lit_dcg(`:`, S3, S4),
    ws0(S4, S5), host_output_column_type(RelName, ColName, Type, S5, S6),
    ws0(S6, S7),
    ( lit_dcg(`,`, S7, S8)
    -> host_output_columns(RelName, Rest, S8, S)
    ; Rest = [], S = S7
    ).

host_output_column_type(RelName, ColName, none, S0, S) :-
    coltype(Wrapper, S0, S),
    Wrapper \== none,
    record_finding(
      unsupported_surface(column_type_wrapper(RelName, ColName, Wrapper))).
% STRUCT-AS-ROWS: a bare host OUTPUT type identifier survives into col_type/3
% so the shared type-plane check resolves a declared struct name or refuses an
% unknown spelling by name. Host inputs and bind columns retain decl-B's
% primitive-only surface.
host_output_column_type(_, _, Type, S0, S) :-
    typed_column_type(Type, S0, S).

specs_to_columns([], []).
specs_to_columns([column(Name, Type) | Rest], [col(Name, Type) | Columns]) :-
    specs_to_columns(Rest, Columns).

template_lit(Template, S0, S) :-
    S0 = [0'` | S1], !,
    template_codes(S1, Codes, S),
    string_codes(Template, Codes).

template_codes([0'` | S], [], S) :- !.
template_codes([0'\\, 0'` | Rest], [0'` | Codes], S) :- !,
    template_codes(Rest, Codes, S).
template_codes([0'\\, 0'\\ | Rest], [0'\\ | Codes], S) :- !,
    template_codes(Rest, Codes, S).
template_codes([Code | Rest], [Code | Codes], S) :-
    template_codes(Rest, Codes, S).

% ═══ `?` query line ═════════════════════════════════════════════════════════

query_stmt(query(Atom), Vars0, Vars, S0, S) :-
    lit_dcg(`?`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    head_args(Args, Vars0, Vars, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S),
    resolve_named_args(head, Name, Args, Positional),
    Atom =.. [Name | Positional].

% ═══ rule / fact: `HeadAtom (<- | <+) Body.` or `HeadAtom.` (bare fact) ══════

match_stmt(match(SourceAtom, Arms), Vars0, Vars, S0, S) :-
    word(`match`, S0, S1),
    ws0(S1, S2),
    head_atom(SourceAtom, Vars0, Vars1, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    match_arm_list(Arms, Vars1, Vars, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S).

% Match arms flow left to right on the .dl6 surface:
%   [;] Guards |-> Head    [;] Guards |+> Head
% The optional first semicolon is layout sugar. The retained term stays
% (Head <- Guards)/(Head <+ Guards), so expansion and runtime semantics do
% not acquire another arrow family.
match_arm_list(Arms, Vars0, Vars, S0, S) :-
    ws0(S0, S00),
    ( lit_dcg(`;`, S00, S01)
    -> ws0(S01, SArm)
    ; SArm = S00
    ),
    match_arm(First, Vars0, Vars1, SArm, S1),
    match_arm_tail(First, Arms, Vars1, Vars, S1, S).

match_arm_tail(First, Arms, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    ( lit_dcg(`;`, S1, S2)
    -> ws0(S2, S3),
       match_arm(Next, Vars0, Vars1, S3, S4),
       match_arm_tail(Next, Rest, Vars1, Vars, S4, S),
       Arms = (First ; Rest)
    ; Arms = First,
      Vars = Vars0,
      S = S1
    ).

match_arm(Arm, Vars0, Vars, S0, S) :-
    body(Guards, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`|->`, S2, S3)
    -> Arrow = (<-)
    ; lit_dcg(`|+>`, S2, S3)
    -> Arrow = (<+)
    ),
    ws0(S3, S4),
    head_atom(Head, Vars1, Vars, S4, S),
    ( Arrow == (<-)
    -> Arm = (Head <- Guards)
    ; Arm = (Head <+ Guards)
    ).

rule_stmt(Rule, Vars0, Vars, S0, S) :-
    head_atom(Head, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`<-`, S2, S3) -> Arrow = (<-), ws0(S3, S4), body(Body, Vars1, Vars, S4, S5)
    ; lit_dcg(`<+`, S2, S3) -> Arrow = (<+), ws0(S3, S4), body(Body, Vars1, Vars, S4, S5)
    ; Arrow = (<-), Body = true, Vars = Vars1, S5 = S2
    ),
    ws0(S5, S6),
    lit_dcg(`.`, S6, S),
    ( Arrow == (<-) -> Rule = (Head <- Body) ; Rule = (Head <+ Body) ).

% ═══ head atom : Name(HeadArg, ...) ══════════════════════════════════════════

% A dotted name in FUNCTOR position is a module path, a different grammar slot
% from the dotted name in value position that dot_chain/4 reads as member access.
head_atom(Term, Vars0, Vars, S0, S) :-
    dotted_path(Segments, S0, S1),
    ws0(S1, S2),
    lit_dcg(`(`, S2, S3),
    head_args(Args, Vars0, Vars, S3, S4),
    ws0(S4, S5),
    lit_dcg(`)`, S5, S),
    last(Segments, LocalName),
    resolve_named_args(head, LocalName, Args, PositionalArgs),
    path_atom_term(Segments, LocalName, PositionalArgs, Term).

% One segment is an ordinary atom; more is a module path the dot phase refuses.
path_atom_term([_Single], LocalName, Args, Term) :- !, Term =.. [LocalName | Args].
path_atom_term(Segments, _LocalName, Args, rel_path(Segments, Args)).

head_args([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
head_args([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), atom_arg(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> head_args(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).

% one call-site argument: `Name: Value` (named) or a plain expr (positional).
% Member tried first (needs a `:` lookahead after the identifier, not
% immediately followed by `=` (that would be the start of `:=`, a bind, only
% ever legal at body-item top level, never inside an arg list -- guarded
% here anyway so a stray `:=` inside parens fails cleanly instead of
% mis-parsing as a named arg called ":").

atom_arg(named(Name, Value), Vars0, Vars, S0, S) :-
    ident(Name, S0, S1), ws0(S1, S2),
    S2 = [0': , Next | _], Next \== 0'=, Next \== 0':, !,
    lit_dcg(`:`, S2, S3), ws0(S3, S4), expr(Value, Vars0, Vars, S4, S).
atom_arg(pos(Value), Vars0, Vars, S0, S) :-
    expr(Value, Vars0, Vars, S0, S).

% Named args are pure surface sugar over the term form's plain positional
% compound -- resolvable whenever the rel's column order is known, whether
% the call is ALL named, ALL positional, or a genuine MIX of both (real
% corpus case: v6/dl/fixtures/conformance.dl6's
% `proves_group_count(source, fanout: count(target))`). A mix is not a
% term-form gap (no new construct is needed to hold the result), so it
% resolves silently here rather than filing a finding: named args fill their
% column by name first, then the positional args fill whatever columns are
% left, in the order they were written.

resolve_named_args(_, _, [], []) :- !.
resolve_named_args(_, _, Args, Positional) :-
    ( forall(member(A, Args), A = pos(_))
    -> maplist(arg_value, Args, Positional)
    ),
    !.
resolve_named_args(Mode, RelName, Args, Positional) :-
    ( lookup_column_order(RelName, Cols)
    -> resolve_mixed_args(Mode, RelName, Args, Cols, Positional)
    ; record_finding(unsupported_surface(named_args_unresolved(RelName))),
      maplist(arg_value, Args, Positional)
    ).

arg_value(pos(V), V) :- !.
arg_value(named(_, V), V).

% NOTE: this places named-arg values into Positional via a plain recursive
% walk (place_named/4), never `forall/2` -- forall(Cond, Action) is defined
% as `\+ (Cond, \+ Action)`, and negation-as-failure UNDOES every binding
% made while proving its argument, including a binding to a Positional list
% cell created outside forall's own scope. Using forall here would silently
% throw away every named-arg placement and this predicate would just fail
% (found the hard way: it did).
% "which slots are still free" is decided from the ARGS structure (which
% column NAMES were explicitly named), never by testing var(Slot) on the
% resulting Positional cell: a Datalog column value is itself typically an
% unbound variable (a join variable, e.g. `target: target` binds column
% `target` to the ALSO-unbound variable `target` reused elsewhere in the
% rule), so var(Slot) cannot distinguish "not yet placed" from "placed with
% a variable" -- confirmed the hard way, it collapsed both cases to "free"
% and re-filled an already-named slot with a positional value.
resolve_mixed_args(Mode, RelName, Args, Cols, Positional) :-
    length(Cols, N),
    length(Positional, N),
    validate_named_columns(RelName, Args, Cols),
    place_named(Cols, 1, Args, Positional),
    findall(ColName, member(named(ColName, _), Args), NamedCols),
    findall(Idx, ( nth1(Idx, Cols, ColName), \+ memberchk(ColName, NamedCols) ), FreeIdxs),
    findall(V, member(pos(V), Args), PosValues),
    fill_partial_slots(Mode, RelName, N, FreeIdxs, PosValues, Positional).

validate_named_columns(RelName, Args, Cols) :-
    findall(Name, member(named(Name, _), Args), Names),
    ( member(Name, Names), \+ memberchk(Name, Cols)
    -> record_finding(unsupported_surface(unknown_named_arg(RelName, Name)))
    ; duplicate_name(Names, Duplicate)
    -> record_finding(unsupported_surface(duplicate_named_arg(RelName, Duplicate)))
    ; true
    ).

duplicate_name(Names, Name) :-
    select(Name, Names, Rest),
    memberchk(Name, Rest),
    !.

place_named([], _, _, _).
place_named([ColName | Cols], Idx, Args, Positional) :-
    ( member(named(ColName, V), Args) -> nth1(Idx, Positional, V) ; true ),
    Idx1 is Idx + 1,
    place_named(Cols, Idx1, Args, Positional).

fill_free_slots([], [], _).
fill_free_slots([Idx | Idxs], [V | Vs], Positional) :-
    nth1(Idx, Positional, V),
    fill_free_slots(Idxs, Vs, Positional).

fill_partial_slots(Mode, RelName, Arity, FreeIdxs, PosValues, Positional) :-
    length(PosValues, PosCount),
    length(FilledIdxs, PosCount),
    append(FilledIdxs, OmittedIdxs, FreeIdxs),
    fill_free_slots(FilledIdxs, PosValues, Positional),
    finish_omitted_slots(Mode, RelName/Arity, OmittedIdxs, Positional).

finish_omitted_slots(body, _, Idxs, Positional) :-
    fill_anonymous_slots(Idxs, Positional).
finish_omitted_slots(head, Ref, Idxs, Positional) :-
    ( Idxs == []
    -> true
    ; record_finding(unsupported_surface(partial_head(Ref))),
      fill_anonymous_slots(Idxs, Positional)
    ).

fill_anonymous_slots([], _).
fill_anonymous_slots([Idx | Rest], Positional) :-
    nth1(Idx, Positional, _),
    fill_anonymous_slots(Rest, Positional).

% ═══ body : comma-conjunction of body items, optional surrounding parens ════

body(Body, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    ( lit_dcg(`(`, S1, S2)
    -> body(Inner, Vars0, Vars1, S2, S3), ws0(S3, S4), lit_dcg(`)`, S4, S5),
       ws0(S5, S6),
       ( lit_dcg(`,`, S6, S7) -> ws0(S7, S8), body(Rest, Vars1, Vars, S8, S), Body = (Inner, Rest)
       ; Body = Inner, Vars = Vars1, S = S6
       )
    ; body_item(Item, Vars0, Vars1, S1, S2), ws0(S2, S3),
      ( lit_dcg(`,`, S3, S4) -> ws0(S4, S5), body(Rest, Vars1, Vars, S5, S), Body = (Item, Rest)
      ; Body = Item, Vars = Vars1, S = S3
      )
    ).

% ═══ one body item ═══════════════════════════════════════════════════════════
% Order: keyword-shaped calls first (latest/departed/pre/now/decode/json_each/
% not), then bare `true`, then bind (:=/is), then comparison, then dialect-B
% prefix negation (!rel(args)), then a plain host/relation or mutation atom.

body_item(Item, Vars0, Vars, S0, S) :-
    cst_item(Item, Vars0, Vars, S0, S),
    !.
body_item(Item, Vars0, Vars, S0, S) :-
    surface(Name/Arity, _, AnalyzeRole, LowerRole, Status),
    wrapper_lower_role(LowerRole, Shape, _),
    keyword_call(Name, InnerCodes, S0, S),
    !,
    parse_surface_wrapper(Shape, Arity, InnerCodes, Args, Vars0, Vars),
    build_surface_item(Name, AnalyzeRole, Status, Args, Item).

cst_item(cst(Path, Digest, Language, Query), Vars0, Vars, S0, S) :-
    word(`cst`, S0, S1),
    ws0(S1, S2), lit_dcg(`(`, S2, S3),
    expr(Path, Vars0, Vars1, S3, S4),
    ws0(S4, S5), lit_dcg(`,`, S5, S6),
    expr(Digest, Vars1, Vars2, S6, S7),
    ws0(S7, S8), lit_dcg(`,`, S8, S9),
    ws0(S9, S10), ident(Language, S10, S11),
    ws0(S11, S12), lit_dcg(`)`, S12, S13),
    ws0(S13, S14), lit_dcg(`{`, S14, S15),
    cst_block_codes(S15, InnerCodes, S16),
    parse_cst_query_or_error(InnerCodes, S16, Query),
    S = S16,
    Vars = Vars2.

parse_cst_query_or_error(Codes, Remaining, Query) :-
    catch(parse_cst_query(Codes, Query), _,
          ( mark_furthest(Remaining), parse_failure(cst_query) )),
    !.

cst_block_codes([0'} | Rest], [], Rest) :- !.
cst_block_codes([0'" | Rest], Codes, S) :-
    cst_block_string(Rest, StringCodes, S1),
    cst_block_codes(S1, MoreCodes, S),
    append([0'" | StringCodes], MoreCodes, Codes),
    !.
cst_block_codes([Code | Rest], [Code | More], S) :-
    cst_block_codes(Rest, More, S).

cst_block_string([0'" | Rest], [0'"], Rest) :- !.
cst_block_string([0'\\, Code | Rest], [0'\\, Code | More], S) :-
    !,
    cst_block_string(Rest, More, S).
cst_block_string([Code | Rest], [Code | More], S) :-
    cst_block_string(Rest, More, S).

annotate_cst_item((Head <- Body), Vars, (Head <- Annotated)) :-
    !,
    term_variables((Head, Body), RuleVariables),
    annotate_cst_body(Body, Head, Vars, RuleVariables, Annotated).
annotate_cst_item((Head <+ Body), Vars, (Head <+ Annotated)) :-
    !,
    term_variables((Head, Body), RuleVariables),
    annotate_cst_body(Body, Head, Vars, RuleVariables, Annotated).
annotate_cst_item(match(Source, Arms), Vars, match(Source, AnnotatedArms)) :-
    !,
    annotate_cst_arms(Arms, Vars, AnnotatedArms).
annotate_cst_item(Item, _, Item).

annotate_cst_arms((Left ; Right), Vars, (AnnotatedLeft ; AnnotatedRight)) :-
    !,
    annotate_cst_item(Left, Vars, AnnotatedLeft),
    annotate_cst_arms(Right, Vars, AnnotatedRight).
annotate_cst_arms(Arm, Vars, Annotated) :-
    annotate_cst_item(Arm, Vars, Annotated).

annotate_cst_body((Left, Right), Head, Vars, RuleVariables,
                  (AnnotatedLeft, AnnotatedRight)) :-
    !,
    annotate_cst_body(Left, Head, Vars, RuleVariables, AnnotatedLeft),
    annotate_cst_body(Right, Head, Vars, RuleVariables, AnnotatedRight).
annotate_cst_body(cst(Path, Digest, Language, Query), Head, Vars,
                  RuleVariables,
                  cst(Path, Digest, Language, Query,
                      cst_bindings(CaptureNames, CandidateNames, RuleNames))) :-
    !,
    ts_query_capture_names(Query, CaptureNames),
    term_variables((Path, Digest), InputVariables),
    cst_variable_names(RuleVariables, Vars, RuleNames),
    cst_variable_names(InputVariables, Vars, InputNames),
    cst_body_variable_names(Head, Path, Digest, Vars, InputNames,
                            CandidateNames).
annotate_cst_body(Item, _, _, _, Item).

cst_body_variable_names(Head, Path, Digest, Vars, InputNames, Names) :-
    term_variables(Head, HeadVariables),
    term_variables((Path, Digest), InputVariables),
    cst_variable_names(HeadVariables, Vars, HeadNames),
    cst_variable_names(InputVariables, Vars, InputNames0),
    append(InputNames, InputNames0, ExcludedNames0),
    sort(ExcludedNames0, ExcludedNames),
    subtract(HeadNames, ExcludedNames, WithoutInputs),
    subtract(WithoutInputs, [line, end_line], Names).

cst_variable_names([], _, []).
cst_variable_names([Variable | Rest], Vars, Names) :-
    ( cst_variable_name(Variable, Vars, Name) -> Names = [Name | More] ; Names = More ),
    cst_variable_names(Rest, Vars, More).

cst_variable_name(Variable, [Name-Candidate | _], Name) :-
    Variable == Candidate,
    !.
cst_variable_name(Variable, [_ | Rest], Name) :-
    cst_variable_name(Variable, Rest, Name).
body_item(Item, Vars, Vars, S0, S) :-
    surface(Name/0, _, _, word(_), _),
    atom_codes(Name, NameCodes),
    word(NameCodes, S0, S), !,
    Item = Name.
body_item(Item, Vars0, Vars, S0, S) :-
    bind_item(Item, Vars0, Vars, S0, S), !.
body_item(Item, Vars0, Vars, S0, S) :-
    comparison_item(Item, Vars0, Vars, S0, S), !.
body_item(not(Atom), Vars0, Vars, S0, S) :-
    lit_dcg(`!`, S0, S1), ident(Name, S1, S2), ws0(S2, S3), lit_dcg(`(`, S3, S4),
    args_positional(Args, Vars0, Vars, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S), !,
    Atom =.. [Name | Args].
body_item(Item, Vars0, Vars, S0, S) :-
    relatom_item(Item, Vars0, Vars, S0, S).

% recognizes `keyword ( ... balanced ... )` and returns the raw codes inside
% the parens (unparsed), so each specific handler above re-parses that inner
% text with its own sub-grammar. Consuming as balanced-paren text first
% (rather than parsing args in place) sidesteps the fact that `only`'s inner
% shape (finalize(Atom) or a bare atom) differs from `decode`'s (two
% comma-separated exprs).
keyword_call(Keyword, InnerCodes, S0, S) :-
    atom_codes(Keyword, KeywordCodes),
    word(KeywordCodes, S0, S1),
    ws0(S1, S2),
    lit_dcg(`(`, S2, S3),
    balanced_parens(S3, InnerCodes, S).

balanced_parens(S0, Inner, S) :-
    balanced_parens_(S0, 0, [], RevInner, S),
    reverse(RevInner, Inner).

balanced_parens_([0'( | Rest], Depth, Acc, Out, S) :- !,
    Depth1 is Depth + 1, balanced_parens_(Rest, Depth1, [0'( | Acc], Out, S).
balanced_parens_([0') | Rest], 0, Acc, Acc, Rest) :- !.
balanced_parens_([0') | Rest], Depth, Acc, Out, S) :- !,
    Depth1 is Depth - 1, balanced_parens_(Rest, Depth1, [0') | Acc], Out, S).
balanced_parens_([C | Rest], Depth, Acc, Out, S) :- !,
    balanced_parens_(Rest, Depth, [C | Acc], Out, S).

parse_full(Goal, Codes) :-
    call(Goal, Codes, Left),
    skip_ws(Left, Left2),
    ( Left2 == [] -> true ; throw(dl_parse_error(trailing_input(Left2))) ).

rel_atom_term(Term, Vars0, Vars, S0, S) :-
    ident(Name, S0, S1), ws0(S1, S2), lit_dcg(`(`, S2, S3),
    args_positional(Args, Vars0, Vars, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S),
    Term =.. [Name | Args].

parse_two_args(Codes, A, B, Vars0, Vars) :-
    skip_ws(Codes, Codes1),
    expr(A, Vars0, Vars1, Codes1, Rest1),
    skip_ws(Rest1, Rest2),
    lit_dcg(`,`, Rest2, Rest3),
    skip_ws(Rest3, Rest4),
    expr(B, Vars1, Vars, Rest4, Rest5),
    skip_ws(Rest5, Rest6),
    ( Rest6 == [] -> true ; throw(dl_parse_error(trailing_input(Rest6))) ).

parse_atom_list(Codes, Atoms, Vars0, Vars) :-
    parse_full(atom_list(Atoms, Vars0, Vars), Codes).

atom_list([Atom | Rest], Vars0, Vars, S0, S) :-
    rel_atom_term(Atom, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`,`, S2, S3)
    -> ws0(S3, S4), atom_list(Rest, Vars1, Vars, S4, S)
    ; Rest = [], Vars = Vars1, S = S2
    ).

combine_body([Atom], Atom) :- !.
combine_body([Left | Rest], Body) :-
    combine_body(Rest, Right), Body = (Left, Right).

parse_surface_wrapper(rel_atom, 1, Codes, [Atom], Vars0, Vars) :-
    parse_full(rel_atom_term(Atom, Vars0, Vars), Codes).
parse_surface_wrapper(atom_list, Arity, Codes, Atoms, Vars0, Vars) :-
    parse_atom_list(Codes, Atoms, Vars0, Vars),
    surface_arity_matches(Arity, Atoms).
parse_surface_wrapper(body_item, 1, Codes, [Inner], Vars0, Vars) :-
    parse_full(body_item(Inner, Vars0, Vars), Codes).
parse_surface_wrapper(expr, 1, Codes, [Expr], Vars0, Vars) :-
    parse_full(expr(Expr, Vars0, Vars), Codes).
parse_surface_wrapper(expr_pair, 2, Codes, [Left, Right], Vars0, Vars) :-
    parse_two_args(Codes, Left, Right, Vars0, Vars).
% `coalesce(rel_atom(...), Default)`: a relation atom, then one value. Its own
% shape rather than expr_pair because the first argument must be a relation
% atom (expr would read `latest_commit(Repo, Commit)` as an expression call and
% the expander's coalesce_source_not_rel_atom would fire on a term the author
% spelled correctly), and its own shape rather than rel_atom because rel_atom
% takes no second argument. Reserving the WORD is the point: an unrecognised
% body word parses as an ordinary relation atom, which for a misspelled
% coalesce would be a silently empty EDB rather than a finding.
parse_surface_wrapper(rel_atom_default, 2, Codes, [Atom, Default], Vars0, Vars) :-
    parse_full(rel_atom_default_args(Atom, Default, Vars0, Vars), Codes).

rel_atom_default_args(Atom, Default, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    rel_atom_term(Atom, Vars0, Vars1, S1, S2),
    ws0(S2, S3),
    lit_dcg(`,`, S3, S4),
    ws0(S4, S5),
    expr(Default, Vars1, Vars, S5, S).

surface_arity_matches(variadic, _).
surface_arity_matches(Arity, Args) :- integer(Arity), length(Args, Arity).

% Every surface item keeps its own functor, splice rows included.
%
% Splice rows used to be desugared into a plain conjunction RIGHT HERE, which
% erased the spelling the author wrote:
%
%   printed    out(Left, Right) <- combine(a(Left), b(Right)).
%   reparsed   prog([], [(out(_A, _B) <- a(_A), b(_B))])
%   printed    out(Value) <- next(a(Value)).
%   reparsed   prog([], [(out(_A) <- a(_A))])
%
% Two things were wrong with that, and only the second is about round-tripping.
% The text door and the TERM door handed the two back ends different terms for
% the same source, and the term door's copy was the one nothing could execute
% (body.pl:solve/2 had no splice clause, engine.pl:trigger_items/2 did not
% splice), so `combine` answered rows through one door and silence through the
% other. Both doors carry the term now and both splice it, so the desugar has
% nothing left to buy and the printed program reparses to itself.
%
% combine_body/2 stays: the statement reader still folds a parsed goal LIST
% into a conjunction, which is a different job.
build_surface_item(Name, _, _, Args, Item) :-
    Item =.. [Name | Args].

args_positional([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
args_positional([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), expr(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> args_positional(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).

% ═══ bind item : Var := Expr  |  Var is Expr  ═══════════════════════════════

bind_item(BindTerm, Vars0, Vars, S0, S) :-
    expr(Lhs, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    registered_infix_op(bind, Op, S2, S3),
    ws0(S3, S4),
    expr(Rhs, Vars1, Vars, S4, S),
    BindTerm =.. [Op, Lhs, Rhs].

% ═══ comparison item : Expr Op Expr, Op in {<,=<,<=,>,>=,==,=,\==,!=} ═══════
% Canonical internal functor per SYNTAX.md's alias table: <= => =<, != =>
% \==, bare = => == (structural/value equality, the closest existing
% vocabulary slot -- see SYNTAX.md's comparison-alias row for the reasoning).

comparison_item(Term, Vars0, Vars, S0, S) :-
    expr(Lhs, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
comp_op(OpAtom, S2, S3),
    ws0(S3, S4),
    expr(Rhs, Vars1, Vars, S4, S),
    Term =.. [OpAtom, Lhs, Rhs].

comp_op(=<, S0, S) :- lit_dcg(`<=`, S0, S), !.
comp_op(\==, S0, S) :- lit_dcg(`!=`, S0, S), !.
comp_op(Op, S0, S) :- registered_infix_op(guard, Op, S0, S), !.
comp_op(==, S0, S) :- lit_dcg(`=`, S0, S), !.

registered_infix_op(Axis, Op, S0, S) :-
    findall(NegLength-(Codes-Candidate),
            ( surface(Candidate/2, Axis, no_refs, infix(_), _),
              atom_codes(Candidate, Codes),
              length(Codes, Length),
              NegLength is -Length
            ),
            Candidates),
    keysort(Candidates, Ordered),
    member(_-(Codes-Op), Ordered),
    operator_codes(Codes, S0, S).

operator_codes(Codes, S0, S) :-
    Codes = [First | _],
    ( code_type(First, alpha)
    -> word(Codes, S0, S)
    ; lit_dcg(Codes, S0, S)
    ).

% ═══ plain host / relation / mutation atom ══════════════════════════════════
%
% One RHS spelling:
%
%   fetch(Ep, Status)
%
% If `fetch` resolves to an sh declaration, it becomes the existing
% internal probe/4 term. Otherwise it remains an ordinary relation atom.
% Top-level `? result(...)` is still query_stmt/5.

split_probe_values(InputCount, Values, InputValues, OutputValues) :-
    length(Values, ValueCount),
    ( ValueCount >= InputCount
    -> length(InputValues, InputCount),
       append(InputValues, OutputValues, Values)
    ; InputValues = Values,
      OutputValues = []
    ).

partition_host_input_values(Columns, Values, Roles, IdentityValues, Salts) :-
    ( same_length(Columns, Values)
    -> partition_host_input_values_(Columns, Values, Roles,
                                    IdentityValues, Salts)
    ; IdentityValues = Values,
      Salts = []
    ).

partition_host_input_values_([], [], [], [], []).
partition_host_input_values_([col(Name, _) | Columns], [Value | Values],
                             [Role | Roles], IdentityValues, Salts) :-
    ( Role == identity
    -> IdentityValues = [Value | IdentityRest],
       Salts = SaltRest
    ; Role == freshness
    -> Salts = [salt(Name, Value) | SaltRest],
       IdentityValues = IdentityRest
    ),
    partition_host_input_values_(Columns, Values, Roles,
                                 IdentityRest, SaltRest).

relatom_item(Item, Vars0, Vars, S0, S) :-
    dotted_path(Segments, S0, S1), last(Segments, Name), ws0(S1, S2),
    ( peek(0'!, S2, S2)
    -> lit_dcg(`!`, S2, S2a), ws0(S2a, S3), lit_dcg(`(`, S3, S4),
       head_args(Args, Vars0, Vars, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S),
       length(Args, Arity), record_finding(unsupported_surface(mutation(Name/Arity))),
       resolve_named_args(body, Name, Args, Positional), Item =.. [Name | Positional]
    ; lit_dcg(`(`, S2, S3),
      head_args(Args, Vars0, Vars1, S3, S4), ws0(S4, S5),
      lit_dcg(`)`, S5, S6),
      resolve_named_args(body, Name, Args, Positional),
      ( Segments = [_Single]
      -> host_or_relation_item(Name, Positional, Item, Vars1, Vars, S6, S)
      ;  Item = rel_path(Segments, Positional), Vars = Vars1, S = S6
      )
    ).

host_or_relation_item(Name, Values, Item, Vars0, Vars, S0, S) :-
    Vars = Vars0,
    S = S0,
    Item =.. [Name | Values].

% ═══ expressions : add/sub over mul/div/mod over parenthesized/atomic
% factors. Arithmetic (+,-,*,/,mod) is entirely EXT -- dl.langium's ArgTerm
% has no expression grammar at all (ArgTerm := Var | Literal | Wildcard,
% dl.langium:150-151) -- so this whole layer only fires when parsing dialect
% A text or the arithmetic-bearing conformance fixtures' printed form. ══════

expr(E, Vars0, Vars, S0, S) :- add_expr(E, Vars0, Vars, S0, S).

add_expr(E, Vars0, Vars, S0, S) :-
    mul_expr(E0, Vars0, Vars1, S0, S1),
    add_expr_rest(E0, E, Vars1, Vars, S1, S).

add_expr_rest(Acc, E, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    ( lit_dcg(`+`, S1, S2)
    -> ws0(S2, S3), mul_expr(Rhs, Vars0, Vars1, S3, S4),
       add_expr_rest(Acc + Rhs, E, Vars1, Vars, S4, S)
    ; lit_dcg(`-`, S1, S2)
    -> ws0(S2, S3), mul_expr(Rhs, Vars0, Vars1, S3, S4),
       add_expr_rest(Acc - Rhs, E, Vars1, Vars, S4, S)
    ; E = Acc, Vars = Vars0, S = S0
    ).

mul_expr(E, Vars0, Vars, S0, S) :-
    factor(E0, Vars0, Vars1, S0, S1),
    mul_expr_rest(E0, E, Vars1, Vars, S1, S).

mul_expr_rest(Acc, E, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    ( lit_dcg(`*`, S1, S2)
    -> ws0(S2, S3), factor(Rhs, Vars0, Vars1, S3, S4),
       mul_expr_rest(Acc * Rhs, E, Vars1, Vars, S4, S)
    ; lit_dcg(`/`, S1, S2)
    -> ws0(S2, S3), factor(Rhs, Vars0, Vars1, S3, S4),
       mul_expr_rest(Acc / Rhs, E, Vars1, Vars, S4, S)
    ; word(`mod`, S1, S2)
    -> ws0(S2, S3), factor(Rhs, Vars0, Vars1, S3, S4),
       mul_expr_rest(Acc mod Rhs, E, Vars1, Vars, S4, S)
    ; E = Acc, Vars = Vars0, S = S0
    ).

% factor: parenthesized expr | integer | quoted atom | string | braces |
% list | wildcard | ident(args) compound (incl. aggregate/concat/data ctor)
% | bare ident => variable (see module header's superseding decision).

factor(E, Vars0, Vars, S0, S) :-
    ws0(S0, S1),
    refuse_tagged_brace(S1),
    ( lit_dcg(`(`, S1, S2)
    -> ws0(S2, S3), expr(E, Vars0, Vars, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S)
    ; bool_lit(E, S1, S) -> Vars = Vars0
    ; float_lit(E, S1, S) -> Vars = Vars0
    ; integer_lit(E, S1, S) -> Vars = Vars0
    ; quoted_atom_lit(E, S1, S) -> Vars = Vars0
    ; string_lit(E, S1, S) -> Vars = Vars0
    ; dollar_var(E, Vars0, Vars, S1, S)
    ; braces_term(E, Vars0, Vars, S1, S)
    ; list_term(E, Vars0, Vars, S1, S)
    ; wildcard_var(E, S1, S) -> Vars = Vars0
    ; compound_or_var(E, Vars0, Vars, S1, S)
    ).

% CARD-BRACE-TAG, settled 2026-07-30 with a measured receipt rather than a
% preference (the coordinator asked "`_{}` -- or is that reserved?"; it is,
% and not by us):
%
%   term_to_atom(T, '{a: 1}')      ->  {a:1}          `{}`/1, curly term
%   term_to_atom(T, '_{a: 1}')     ->  _G{a:1}        SWI DICT
%   term_to_atom(T, 'point{x: 1}') ->  point{x:1}     SWI DICT
%
% The TERM door consults real Prolog, so `_{...}` and `Tag{...}` arrive there
% as dicts, a term shape `{}`/1 can never unify with -- body.pl's json_canon/2
% would never see them and the two doors could not agree without switching the
% `dicts` flag off language-wide. Bare `{...}` is the one spelling both doors
% already read the same way, so bare braces are the json literal and the tagged
% forms are RESERVED with this named refusal, which is also the future home the
% directive asked to keep open ("the `{` opening will later be abused beyond
% json").
%
% Refusing is what the position was missing, not what it gains: before this
% clause `_{a: 1}` consumed `_` as a wildcard and the leftover surfaced as
% `dl_parse_error(trailing_input([123,97,58,32,49,125]))`, naming neither the
% brace nor the tag.
refuse_tagged_brace(S) :-
    (   tagged_brace_name(S, Name)
    ->  throw(unsupported_construct(tagged_brace_reserved(Name)))
    ;   true
    ).

tagged_brace_name(S, Name) :-
    ident(Name, S, Rest),
    Rest = [0'{ | _].

% ruling json_key_hole_marker = dollar (2026-07-30). `$name` marks a hole in
% KEY position, where a bare identifier is a literal label. It is accepted in
% VALUE position too, where a bare identifier is already a variable, so that
% `{$key: $value}` reads uniformly on both planes -- an alias, never a second
% meaning. `$_` is a fresh anonymous variable, exactly like bare `_`.
dollar_var(Var, Vars0, Vars, S0, S) :-
    S0 = [0'$ | S1],
    ident(Name, S1, S),
    hole_var(Name, Vars0, Var, Vars).

hole_var('_', Vars, _Fresh, Vars) :- !.
hole_var(Name, Vars0, Var, Vars) :- get_or_make_var(Name, Vars0, Var, Vars).

bool_lit(bool_lit(true), S0, S) :- word(`true`, S0, S), !.
bool_lit(bool_lit(false), S0, S) :- word(`false`, S0, S).

wildcard_var(Var, S0, S) :-
    lit_dcg(`_`, S0, S1),
    \+ (S1 = [C | _], (code_type(C, alnum) ; C == 0'_)),
    Var = _,
    S = S1.

compound_or_var(E, Vars0, Vars, S0, S) :-
    ident(Name, S0, S1),
    ws0(S1, S2),
    ( peek(0'(, S2, S2)
    -> lit_dcg(`(`, S2, S3), call_args(Args, Vars0, Vars, S3, S4), ws0(S4, S5),
       lit_dcg(`)`, S5, S), E =.. [Name | Args]
    ; get_or_make_var(Name, Vars0, Receiver, Vars),
      dot_chain(Receiver, S1, S, E)
    ).

% Glued member access `Receiver.field` / `Receiver.a.b` on an identifier
% receiver. The dot is GLUED: no whitespace before it (the probe runs on S1,
% before ws0), and an identifier-start must follow. The statement-terminator
% dot (whitespace or EOF after it) and a spaced `x . y` both stay out of this
% route, and a float literal never arrives here (factor tries float_lit first,
% and the dot of `1.5` needs a digit after it). The chain builds a nested
% dot_get/2 term, desugared by the dot expansion phase in 1_expansion.
% Segments are KEPT, never joined: a joined atom would be a table name, and no
% module id exists to make that name collision-safe yet.
dotted_path([Segment | Rest], S0, S) :-
    ident(Segment, S0, S1),
    (   dot_then_ident(S1, S2)
    ->  dotted_path(Rest, S2, S)
    ;   Rest = [], S = S1
    ).

dot_chain(Receiver, S0, S, Final) :-
    ( dot_then_ident(S0, S1)
    -> ident(Field, S1, S2),
       dot_chain(dot_get(Receiver, Field), S2, S, Final)
    ; Final = Receiver, S = S0
    ).

% The cut commits on dot-plus-identifier-start together: an if-then-else on the
% dot alone would commit and then fail on a non-identifier follower, eating the
% terminator period after an identifier.
dot_then_ident([0'. | S1], S1) :-
    S1 = [C | _],
    ( code_type(C, alpha) ; C == 0'_ ),
    !.

call_args([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
call_args([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), expr(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> call_args(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).

% braces term `{ key: value, ... }` -> '{}'/1 wrapping a comma-conjunction of
% Key:Value pairs, matching exactly how plain Prolog reads the fixtures' own
% `{stars: 4, name: Name}` source (curly term + standard `:`/2 operator).
%
% ONE grammar, two roles (json_syntax lab §1 law (a)): the literal is this
% production minus holes. The empty object is the ATOM `{}`, not `{}`/1,
% because that is what the term door produces -- `term_to_atom(T, '{}')` reads
% an atom of arity 0, so making the text door mint `{}`/1 with an empty pair
% list would put the two doors on different terms for the same source.
%
% Keys are pattern-only by construction (lab §1 law (b)): JSON5 permits
% unquoted keys and forbids unquoted string values, so bareness is the literal
% marker on the KEY plane while quoting is the literal marker on the VALUE
% plane. A key that has to be COMPUTED is json_object(Key, Value), never a
% brace.
%
% Trailing commas are NOT accepted: ruling json5_subset = unquoted_keys_only
% takes bare identifier keys out of JSON5 and nothing else (no `,}` and no `#`
% comments inside a brace).

braces_term(Term, Vars0, Vars, S0, S) :-
    lit_dcg(`{`, S0, S1), !,
    ws0(S1, S2),
    (   peek(0'}, S2, S2)
    ->  Term = '{}', Vars = Vars0, S3 = S2
    ;   Term = '{}'(Pairs),
        brace_pairs(Pairs, Vars0, Vars, S2, S3)
    ),
    ws0(S3, S4), lit_dcg(`}`, S4, S).

brace_pairs((Pair, Rest), Vars0, Vars, S0, S) :-
    brace_pair(Pair, Vars0, Vars1, S0, S1), ws0(S1, S2),
    lit_dcg(`,`, S2, S3), !, ws0(S3, S4),
    brace_pairs(Rest, Vars1, Vars, S4, S).
brace_pairs(Pair, Vars0, Vars, S0, S) :-
    brace_pair(Pair, Vars0, Vars, S0, S).

brace_pair(Key:Typed, Vars0, Vars, S0, S) :-
    brace_key(Key, Vars0, Vars1, S0, S1), ws0(S1, S2),
    lit_dcg(`:`, S2, S3), ws0(S3, S4),
    expr(Value, Vars1, Vars, S4, S5),
    brace_value_type(Value, Typed, S5, S).

% The TYPED CAPTURE suffix, `{stars: Stars: int}`. The second colon is the
% same type marker the decl plane uses (ruling decl_column_spelling =
% colon_typed_ordered_columns), one level down, and it needs no term-door
% work at all: `:` is 600 xfy in SWI, so `stars: Stars: int` already reads as
% `:(stars, :(Stars, int))`.
%
% Unambiguous by position rather than by lookahead: inside a braces literal a
% value is always followed by `,` or `}`, so a colon after one can only be
% this. The type WORD is not checked here -- an unrecognised name is a named
% refusal at both doors (body.pl json_capture_type/2,
% lower.pl json_capture_json_type/2), which is where the message can say what
% the live types are.
brace_value_type(Value, Value : Type, S0, S) :-
    ws0(S0, S1), lit_dcg(`:`, S1, S2), !, ws0(S2, S3),
    ident(Type, S3, S).
brace_value_type(Value, Value, S, S).

% The key axis. Every form but the plain label is a MATCHER:
%
%   name        k_exact   the label itself
%   'name'      k_exact   ruling string_quote = both_parse; a quoted key is
%   "name"      k_exact   always literal, which is how a real "$ref" key in an
%                         OpenAPI document stays a key instead of a capture
%   $name       k_hole    ruling json_key_hole_marker = dollar; term `$`/1
%   **          k_descend ruling descent_depth_cap = uncapped; term `'**'`
%
% `**` and `$name` both have to survive the term door, which is why they carry
% those exact term shapes: `{**: ...}` is a Prolog syntax error (the reader
% wants an operand after the infix `**`) while `{'**': ...}` and `{$Key: V}`
% both read (`$`/1 is a standard SWI prefix operator).
brace_key('**', Vars, Vars, S0, S) :- lit_dcg(`**`, S0, S), !.
brace_key($(Var), Vars0, Vars, S0, S) :-
    S0 = [0'$ | S1], !,
    ident(Name, S1, S),
    hole_var(Name, Vars0, Var, Vars).
brace_key(Key, Vars, Vars, S0, S) :- quoted_atom_lit(Key, S0, S), !.
brace_key(Key, Vars, Vars, S0, S) :- string_lit(Text, S0, S), !, atom_string(Key, Text).
brace_key(Key, Vars, Vars, S0, S) :- ident(Key, S0, S).

% list term `[ e1, e2, ... ]`, plus the array SPREAD `[... pattern]`, which is
% the one array production the flagship needs: one row per element, siblings
% correlated (json_syntax lab receipt L2, `examples/gh-cache.dl:116-117`).
% Term form is `spread`/1 -- `[... P]` is a Prolog syntax error, so the term
% door needs a functor, and the printer renders it back to `[... P]`.

list_term(Term, Vars0, Vars, S0, S) :-
    lit_dcg(`[`, S0, S1), !,
    ws0(S1, S2),
    ( lit_dcg(`...`, S2, S3)
    -> ws0(S3, S4), expr(Element, Vars0, Vars, S4, S5),
       Term = spread(Element), S6 = S5
    ; peek(0'], S2, S2) -> Term = [], Vars = Vars0, S6 = S2
    ; list_items(Term, Vars0, Vars, S2, S6)
    ),
    ws0(S6, S7), lit_dcg(`]`, S7, S).

list_items([Item | Rest], Vars0, Vars, S0, S) :-
    expr(Item, Vars0, Vars1, S0, S1), ws0(S1, S2),
    ( lit_dcg(`,`, S2, S3) -> ws0(S3, S4), list_items(Rest, Vars1, Vars, S4, S)
    ; Rest = [], Vars = Vars1, S = S2
    ).
