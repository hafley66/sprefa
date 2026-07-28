% parse_dl.pl : phase D parser. Plain SWI-Prolog DCG-and-recursive-descent
% over codes, .dl TEXT in, the fixture term form out: prog(Decls, Rules) with
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
%       (print_dl.pl) emits into dl_view/*.dl -- <-/<+ arrows, latest/pre/
%       departed/now/decode/json_each/:= as function-call-shaped body items,
%       arithmetic infix, `rel Name(cols) log|set [keep(...)] [key(...)].`
%       decls.
%   (b) the existing v6/dl surface (v6/dl/grammar/dl.langium), read here so
%       v6/dl/fixtures/ghcacher.dl and conformance.dl keep parsing: `rel
%       Name(col: type, ...).` decls, `?`/`!` postfix probe/mutation atoms,
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
% than filed as a finding. Constructs the term form truly cannot hold (probe
% `?(...)`, mutation `!(...)`, `sh` host decls, `?` query lines, retention
% markers `rel(N)`) become unsupported_surface(...) findings, collected and
% returned, never silently dropped.

:- module(parse_dl, [ parse_dl/4, parse_dl_file/4 ]).

:- set_prolog_flag(back_quotes, codes).

:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- dynamic(finding_fact/1).
:- dynamic(rel_column_order_fact/2).

record_finding(F) :- assertz(finding_fact(F)).
record_column_order(Name, Cols) :-
    retractall(rel_column_order_fact(Name, _)),
    assertz(rel_column_order_fact(Name, Cols)).
lookup_column_order(Name, Cols) :- rel_column_order_fact(Name, Cols).

% ═══ entry points ════════════════════════════════════════════════════════════

parse_dl_file(FilePath, Prog, Bindings, Findings) :-
    read_file_to_codes(FilePath, Codes, []),
    parse_dl(Codes, Prog, Bindings, Findings).

parse_dl(Codes, prog(Decls, Rules), Bindings, Findings) :-
    retractall(finding_fact(_)),
    retractall(rel_column_order_fact(_, _)),
    statements(Codes, Left, [], VarsFinal, Decls, Rules),
    ( Left == [] -> true ; throw(dl_parse_error(trailing_input(Left))) ),
    maplist(swap_pair, VarsFinal, BindingsRev),
    reverse(BindingsRev, Bindings),
    findall(F, finding_fact(F), Findings).

swap_pair(Name-Var, Name=Var).

% ═══ top-level statement loop : one Vars accumulator threads across every
% decl and rule in the file (the whole-clause variable scope compile.pl's
% read_term call gets for free); Decls/Rules accumulate as the recursion
% unwinds, splicing in each decl_list statement's whole list at once ═══════

statements(S0, S, Vars0, Vars, Decls, Rules) :-
    skip_ws(S0, S1),
    ( S1 == []
    -> Decls = [], Rules = [], Vars = Vars0, S = S1
    ; ( statement(Kind, Item, Vars0, Vars1, S1, S2) -> true ; throw(dl_parse_error(statement, S1)) ),
      statements(S2, S, Vars1, Vars, Decls1, Rules1),
      ( Kind == decl_list -> append(Item, Decls1, Decls), Rules = Rules1
      ; Kind == rule -> Decls = Decls1, Rules = [Item | Rules1]
      ; Kind == skip -> Decls = Decls1, Rules = Rules1
      )
    ).

% ═══ whitespace + `#` line comments (plain predicate, not `-->`, since the
% rest of this parser is written with explicit S0/S args throughout) ════════

skip_ws(S0, S) :-
    ( S0 = [C | S1], (code_type(C, space) ; C == 0'\n ; C == 0'\r)
    -> skip_ws(S1, S)
    ; S0 = [0'# | S1]
    -> skip_to_eol(S1, S2), skip_ws(S2, S)
    ; S = S0
    ).

skip_to_eol(S0, S) :-
    ( S0 = [C | S1], C \== 0'\n -> skip_to_eol(S1, S)
    ; S0 = [0'\n | S1] -> S = S1
    ; S = S0
    ).

ws0(S0, S) :- skip_ws(S0, S).

% ═══ literal punctuation / keyword matching ════════════════════════════════

lit_dcg([]) --> [].
lit_dcg([C | Cs]) --> [C], lit_dcg(Cs).

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
    ( S0 = [0'- | S1] -> Neg = true, S2 = S1 ; Neg = false, S2 = S0 ),
    S2 = [D0 | _], code_type(D0, digit), !,
    digits(S2, Digits, S),
    number_codes(Magnitude, Digits),
    ( Neg == true -> Value is -Magnitude ; Value = Magnitude ).

digits([C | Cs], [C | More], S) :- code_type(C, digit), !, digits(Cs, More, S).
digits(S, [], S).

% ═══ quoted atom 'text' and string "text" literals ══════════════════════════
% Both support \' \" \\ \n \t escapes and the doubled-quote escape ('' inside
% '...' is one literal quote, the plain Prolog convention).

quoted_atom_lit(Atom, S0, S) :-
    S0 = [0'\' | S1], !,
    quoted_chars(0'\', S1, Codes, S),
    atom_codes(Atom, Codes).

string_lit(Str, S0, S) :-
    S0 = [0'" | S1], !,
    quoted_chars(0'", S1, Codes, S),
    string_codes(Str, Codes).

quoted_chars(Quote, [Quote, Quote | Rest], [Quote | More], S) :- !,
    quoted_chars(Quote, Rest, More, S).
quoted_chars(Quote, [Quote | Rest], [], Rest) :- !.
quoted_chars(Quote, [0'\\, Esc | Rest], [Code | More], S) :- !,
    escape_code(Esc, Code),
    quoted_chars(Quote, Rest, More, S).
quoted_chars(Quote, [C | Rest], [C | More], S) :-
    quoted_chars(Quote, Rest, More, S).

escape_code(0'n, 0'\n) :- !.
escape_code(0't, 0'\t) :- !.
escape_code(0'r, 0'\r) :- !.
escape_code(C, C).

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
    ( decl_a_stmt(Item0, S1, S2) -> Kind = decl_list, Item = Item0, Vars = Vars0, S = S2
    ; decl_b_stmt(Item0, S1, S2) -> Kind = decl_list, Item = Item0, Vars = Vars0, S = S2
    ; sh_decl_stmt(S1, S2) -> Kind = skip, Item = [], Vars = Vars0, S = S2
    ; query_stmt(S1, S2) -> Kind = skip, Item = [], Vars = Vars0, S = S2
    ; rule_stmt(Item0, Vars0, Vars1, S1, S2) -> Kind = rule, Item = Item0, Vars = Vars1, S = S2
    ).

% ═══ dialect-A decl: `rel Name(col, ...) [log|set] [keep(all|count(N))]
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

decl_a_stmt(DeclList, S0, S) :-
    word(`rel`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_a_columns(Cols, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    length(Cols, Arity),
    Ref = Name/Arity,
    record_column_order(Name, Cols),
    ws0(S8, S9),
    decl_a_modifiers(Ref, DeclList, S9, S10),
    ws0(S10, S11),
    lit_dcg(`.`, S11, S).

decl_a_modifiers(Ref, [Decl | Rest], S0, S) :-
    ( word(`log`, S0, S1) -> Decl = kind(Ref, log)
    ; word(`set`, S0, S1) -> Decl = kind(Ref, set)
    ; keep_clause(Policy, S0, S1) -> Decl = keep(Ref, Policy)
    ; key_clause(Positions, S0, S1) -> Decl = keyed(Ref, Positions)
    ), !,
    ws0(S1, S2),
    decl_a_modifiers(Ref, Rest, S2, S).
decl_a_modifiers(_, [], S, S).

decl_a_columns([], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
decl_a_columns([Name | Rest], S0, S) :-
    ws0(S0, S1), ident(Name, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> decl_a_columns(Rest, S4, S) ; Rest = [], S = S3 ).

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
% own comment, dl.langium:38); every dialect-B rel becomes kind(Ref, set)
% here, the closest already-modeled shape (plain membership, no keep policy
% required by decl_keep/3's own `all` fallback). ═══════════════════════════

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
    decl_b_columns(Name, Cols, S7, S8),
    ws0(S8, S9),
    lit_dcg(`)`, S9, S10),
    ws0(S10, S11),
    lit_dcg(`.`, S11, S),
    length(Cols, Arity),
    Ref = Name/Arity,
    record_column_order(Name, Cols),
    ( HasRetention == true -> record_finding(unsupported_surface(retention_marker(Ref, Retention))) ; true ),
    DeclList = [kind(Ref, set)].

% Wrapper column types (Key(text)/Min(int)/Max(int)) parse -- and their
% wrapper name is recorded as an unsupported_surface(column_type_wrapper(...))
% finding, since kind/keyed/keep carry no per-column type info at all in
% this term form and the wrapped type is silently discarded otherwise.
% dl.langium's own comment (grammar lines 38-39) already calls Key() semantically
% inert and Min()/Max() a load error ("frontier") in the real bridge -- this
% just makes the same gap visible on the tsv2 side instead of swallowing it.
% Neither ghcacher.dl nor conformance.dl actually uses any of the three
% (grepped), so this path is UNTESTED by G2's real files; kept here anyway
% since a future .dl file using them must not be silently mis-lowered.

decl_b_columns(_, [], S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
decl_b_columns(RelName, [ColName | Rest], S0, S) :-
    ws0(S0, S1), ident(ColName, S1, S2), ws0(S2, S3), lit_dcg(`:`, S3, S4),
    ws0(S4, S5), coltype(Wrapper, S5, S6), ws0(S6, S7),
    ( Wrapper == none
    -> true
    ; record_finding(unsupported_surface(column_type_wrapper(RelName, ColName, Wrapper)))
    ),
    ( lit_dcg(`,`, S7, S8) -> decl_b_columns(RelName, Rest, S8, S) ; Rest = [], S = S7 ).

coltype(Wrapper, S0, S) :-
    ( word(`Key`, S0, S1) -> Wrapper = 'Key'
    ; word(`Min`, S0, S1) -> Wrapper = 'Min'
    ; word(`Max`, S0, S1) -> Wrapper = 'Max'
    ), !,
    ws0(S1, S2), lit_dcg(`(`, S2, S3), ws0(S3, S4), ident(_, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S).
coltype(none, S0, S) :- ident(_, S0, S).

% ═══ `sh` host decl : unsupported_surface, no term-form shape holds it ═════

sh_decl_stmt(S0, S) :-
    word(`sh`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    decl_b_columns(Name, Cols, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`=`, S9, S10),
    ws0(S10, S11),
    template_lit(S11, S12),
    ws0(S12, S13),
    lit_dcg(`.`, S13, S),
    length(Cols, Arity),
    record_column_order(Name, Cols),
    record_finding(unsupported_surface(host_decl(Name/Arity))).

template_lit(S0, S) :-
    S0 = [0'` | S1], !,
    skip_backtick_body(S1, S).
skip_backtick_body([0'` | S], S) :- !.
skip_backtick_body([_ | Rest], S) :- skip_backtick_body(Rest, S).

% ═══ `?` query line : unsupported_surface, no term-form shape holds it ═════

query_stmt(S0, S) :-
    lit_dcg(`?`, S0, S1),
    ws0(S1, S2),
    ident(Name, S2, S3),
    ws0(S3, S4),
    lit_dcg(`(`, S4, S5),
    query_args(Args, [], _, S5, S6),
    ws0(S6, S7),
    lit_dcg(`)`, S7, S8),
    ws0(S8, S9),
    lit_dcg(`.`, S9, S),
    length(Args, Arity),
    record_finding(unsupported_surface(query(Name/Arity))).

query_args([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
query_args([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), atom_arg(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> query_args(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).

% ═══ rule / fact: `HeadAtom (<- | <+) Body.` or `HeadAtom.` (bare fact) ══════

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

head_atom(Term, Vars0, Vars, S0, S) :-
    ident(Name, S0, S1),
    ws0(S1, S2),
    lit_dcg(`(`, S2, S3),
    head_args(Args, Vars0, Vars, S3, S4),
    ws0(S4, S5),
    lit_dcg(`)`, S5, S),
    resolve_named_args(Name, Args, PositionalArgs),
    Term =.. [Name | PositionalArgs].

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
% corpus case: v6/dl/fixtures/conformance.dl's
% `proves_group_count(source, fanout: count(target))`). A mix is not a
% term-form gap (no new construct is needed to hold the result), so it
% resolves silently here rather than filing a finding: named args fill their
% column by name first, then the positional args fill whatever columns are
% left, in the order they were written.

resolve_named_args(_, [], []) :- !.
resolve_named_args(RelName, Args, Positional) :-
    ( forall(member(A, Args), A = pos(_))
    -> maplist(arg_value, Args, Positional)
    ; lookup_column_order(RelName, Cols)
    -> resolve_mixed_args(Args, Cols, Positional)
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
resolve_mixed_args(Args, Cols, Positional) :-
    length(Cols, N),
    length(Positional, N),
    place_named(Cols, 1, Args, Positional),
    findall(ColName, member(named(ColName, _), Args), NamedCols),
    findall(Idx, ( nth1(Idx, Cols, ColName), \+ memberchk(ColName, NamedCols) ), FreeIdxs),
    findall(V, member(pos(V), Args), PosValues),
    fill_free_slots(FreeIdxs, PosValues, Positional).

place_named([], _, _, _).
place_named([ColName | Cols], Idx, Args, Positional) :-
    ( member(named(ColName, V), Args) -> nth1(Idx, Positional, V) ; true ),
    Idx1 is Idx + 1,
    place_named(Cols, Idx1, Args, Positional).

fill_free_slots([], [], _).
fill_free_slots([Idx | Idxs], [V | Vs], Positional) :-
    nth1(Idx, Positional, V),
    fill_free_slots(Idxs, Vs, Positional).

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
% prefix negation (!rel(args)), then a plain/probe/mutation relation atom.

body_item(latest(Atom), Vars0, Vars, S0, S) :-
    keyword_call(latest, InnerCodes, S0, S),
    parse_full(rel_atom_term(Atom, Vars0, Vars), InnerCodes), !.
body_item(Body, Vars0, Vars, S0, S) :-
    keyword_call(combine, InnerCodes, S0, S),
    parse_atom_list(InnerCodes, Atoms, Vars0, Vars),
    combine_body(Atoms, Body), !.
body_item(Atom, Vars0, Vars, S0, S) :-
    keyword_call(next, InnerCodes, S0, S),
    parse_full(rel_atom_term(Atom, Vars0, Vars), InnerCodes), !.
body_item(zip(Left, Right), Vars0, Vars, S0, S) :-
    keyword_call(zip, InnerCodes, S0, S),
    parse_atom_list(InnerCodes, [Left, Right], Vars0, Vars), !.
body_item(finalize(Atom), Vars0, Vars, S0, S) :-
    keyword_call(finalize, InnerCodes, S0, S), !,
    parse_full(rel_atom_term(Atom, Vars0, Vars), InnerCodes).
body_item(pre(Atom), Vars0, Vars, S0, S) :-
    keyword_call(pre, InnerCodes, S0, S), !,
    parse_full(rel_atom_term(Atom, Vars0, Vars), InnerCodes).
body_item(now(Var), Vars0, Vars, S0, S) :-
    keyword_call(now, InnerCodes, S0, S), !,
    parse_full(expr(Var, Vars0, Vars), InnerCodes).
body_item(decode(Expr, Pattern), Vars0, Vars, S0, S) :-
    keyword_call(decode, InnerCodes, S0, S), !,
    parse_two_args(InnerCodes, Expr, Pattern, Vars0, Vars).
body_item(json_each(Expr, Elem), Vars0, Vars, S0, S) :-
    keyword_call(json_each, InnerCodes, S0, S), !,
    parse_two_args(InnerCodes, Expr, Elem, Vars0, Vars).
body_item(not(Inner), Vars0, Vars, S0, S) :-
    keyword_call(not, InnerCodes, S0, S), !,
    parse_full(body_item(Inner, Vars0, Vars), InnerCodes).
body_item(Item, Vars0, Vars, S0, S) :-
    lifecycle_arm_name(Name),
    keyword_call(Name, InnerCodes, S0, S),
    parse_full(rel_atom_term(Atom, Vars0, Vars), InnerCodes), !,
    Item =.. [Name, Atom].
body_item(true, Vars, Vars, S0, S) :-
    word(`true`, S0, S), !.
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

lifecycle_arm_name(unsubscribe).
lifecycle_arm_name(complete).
lifecycle_arm_name(subscribe).
lifecycle_arm_name(error).

args_positional([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
args_positional([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), expr(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> args_positional(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).

% ═══ bind item : Var := Expr  |  Var is Expr  ═══════════════════════════════

bind_item(BindTerm, Vars0, Vars, S0, S) :-
    expr(Lhs, Vars0, Vars1, S0, S1),
    ws0(S1, S2),
    ( lit_dcg(`:=`, S2, S3) -> Op = (:=)
    ; word(`is`, S2, S3) -> Op = is
    ),
    ws0(S3, S4),
    expr(Rhs, Vars1, Vars, S4, S),
    ( Op == (:=) -> BindTerm = (Lhs := Rhs) ; BindTerm = (Lhs is Rhs) ).

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

comp_op(=<, S0, S) :- lit_dcg(`=<`, S0, S), !.
comp_op(=<, S0, S) :- lit_dcg(`<=`, S0, S), !.
comp_op(==, S0, S) :- lit_dcg(`==`, S0, S), !.
comp_op(\==, S0, S) :- lit_dcg(`\\==`, S0, S), !.
comp_op(\==, S0, S) :- lit_dcg(`!=`, S0, S), !.
comp_op(>=, S0, S) :- lit_dcg(`>=`, S0, S), !.
comp_op(<, S0, S) :- lit_dcg(`<`, S0, S), !.
comp_op(>, S0, S) :- lit_dcg(`>`, S0, S), !.
comp_op(==, S0, S) :- lit_dcg(`=`, S0, S), !.

% ═══ plain / probe / mutation relation atom ═════════════════════════════════

relatom_item(Item, Vars0, Vars, S0, S) :-
    ident(Name, S0, S1), ws0(S1, S2),
    ( peek(0'?, S2, S2)
    -> lit_dcg(`?`, S2, S2a), ws0(S2a, S3), lit_dcg(`(`, S3, S4),
       head_args(Args, Vars0, Vars, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S),
       length(Args, Arity), record_finding(unsupported_surface(probe(Name/Arity))),
       resolve_named_args(Name, Args, Positional), Item =.. [Name | Positional]
    ; peek(0'!, S2, S2)
    -> lit_dcg(`!`, S2, S2a), ws0(S2a, S3), lit_dcg(`(`, S3, S4),
       head_args(Args, Vars0, Vars, S4, S5), ws0(S5, S6), lit_dcg(`)`, S6, S),
       length(Args, Arity), record_finding(unsupported_surface(mutation(Name/Arity))),
       resolve_named_args(Name, Args, Positional), Item =.. [Name | Positional]
    ; lit_dcg(`(`, S2, S3),
      head_args(Args, Vars0, Vars, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S),
      resolve_named_args(Name, Args, Positional), Item =.. [Name | Positional]
    ).

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
    ( lit_dcg(`(`, S1, S2)
    -> ws0(S2, S3), expr(E, Vars0, Vars, S3, S4), ws0(S4, S5), lit_dcg(`)`, S5, S)
    ; integer_lit(E, S1, S) -> Vars = Vars0
    ; quoted_atom_lit(E, S1, S) -> Vars = Vars0
    ; string_lit(E, S1, S) -> Vars = Vars0
    ; braces_term(E, Vars0, Vars, S1, S)
    ; list_term(E, Vars0, Vars, S1, S)
    ; wildcard_var(E, S1, S) -> Vars = Vars0
    ; compound_or_var(E, Vars0, Vars, S1, S)
    ).

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
    ; get_or_make_var(Name, Vars0, E, Vars), S = S1
    ).

call_args([], Vars, Vars, S0, S) :- ws0(S0, S1), peek(0'), S1, S), !.
call_args([Arg | Rest], Vars0, Vars, S0, S) :-
    ws0(S0, S1), expr(Arg, Vars0, Vars1, S1, S2), ws0(S2, S3),
    ( lit_dcg(`,`, S3, S4) -> call_args(Rest, Vars1, Vars, S4, S) ; Rest = [], Vars = Vars1, S = S3 ).

% braces term `{ key: value, ... }` -> '{}'/1 wrapping a comma-conjunction of
% Key:Value pairs, matching exactly how plain Prolog reads the fixtures' own
% `{stars: 4, name: Name}` source (curly term + standard `:`/2 operator).
% Key is a LABEL (bare ident taken literally, never quoted, never a var --
% the same lexical role as a relation functor name).

braces_term('{}'(Pairs), Vars0, Vars, S0, S) :-
    lit_dcg(`{`, S0, S1), !,
    ws0(S1, S2),
    brace_pairs(Pairs, Vars0, Vars, S2, S3),
    ws0(S3, S4), lit_dcg(`}`, S4, S).

brace_pairs((Key:Value, Rest), Vars0, Vars, S0, S) :-
    ident(Key, S0, S1), ws0(S1, S2), lit_dcg(`:`, S2, S3), ws0(S3, S4),
    expr(Value, Vars0, Vars1, S4, S5), ws0(S5, S6),
    lit_dcg(`,`, S6, S7), !, ws0(S7, S8),
    brace_pairs(Rest, Vars1, Vars, S8, S).
brace_pairs(Key:Value, Vars0, Vars, S0, S) :-
    ident(Key, S0, S1), ws0(S1, S2), lit_dcg(`:`, S2, S3), ws0(S3, S4),
    expr(Value, Vars0, Vars, S4, S).

% list term `[ e1, e2, ... ]` (only ever used as concat/1's argument in this
% corpus, kept general as a proper Prolog list of exprs).

list_term(List, Vars0, Vars, S0, S) :-
    lit_dcg(`[`, S0, S1), !,
    ws0(S1, S2),
    ( peek(0'], S2, S2) -> List = [], Vars = Vars0, S3 = S2
    ; list_items(List, Vars0, Vars, S2, S3)
    ),
    ws0(S3, S4), lit_dcg(`]`, S4, S).

list_items([Item | Rest], Vars0, Vars, S0, S) :-
    expr(Item, Vars0, Vars1, S0, S1), ws0(S1, S2),
    ( lit_dcg(`,`, S2, S3) -> ws0(S3, S4), list_items(Rest, Vars1, Vars, S4, S)
    ; Rest = [], Vars = Vars1, S = S2
    ).
