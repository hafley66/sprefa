% print_dl.pl : phase D printer, the inverse of parse_dl.pl. Takes a
% prog(Decls, Rules) term plus its Bindings (Name=Var pairs, the same shape
% compile.pl:read_fixture_term/4 and parse_dl.pl both produce) and renders
% canonical .dl TEXT that parse_dl.pl re-parses back to a variant of the
% original term (G1's round-trip grade).
%
% Column names for decl lines are MINED, not reinvented: this file calls
% analyze.pl's exported rel_kind/3, decl_key/3, decl_keep/3, declared_refs/2,
% program_refs/2, rel_columns/4, snake_name/2 directly (read-only use of an
% already-owned module, not a duplicate implementation -- the repo style law
% against reinventing a mining pass that already exists elsewhere).
%
% Syntax choices here are canonical per SYNTAX.md: `<-`/`<+` arrows, latest/
% pre/departed/now/decode/json_each/:= as function-call-shaped body items,
% infix arithmetic (precedence-safe, parens added only where flattening
% would change meaning), atom literals single-quoted (every bare identifier
% is a variable in this grammar -- see parse_dl.pl's module header for the
% full reasoning), strings double-quoted, decls spelled
% `rel Name(cols) log|set [keep(...)] [key(...)].`.

:- module(print_dl, [ print_dl_program/3, print_dl_to_file/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(analyze, [ rel_columns/4 ]).
:- use_module(registry,
              [ body_surface_for_term/6,
                wrapper_lower_role/3
              ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ entry points ════════════════════════════════════════════════════════════

print_dl_to_file(prog(Decls, Rules), Bindings, FilePath) :-
    print_dl_program(prog(Decls, Rules), Bindings, Text),
    setup_call_cleanup(
        open(FilePath, write, Stream),
        format(Stream, "~w", [Text]),
        close(Stream)).

print_dl_program(prog(Decls, Rules), Bindings, Text) :-
    decl_ref_order(Decls, Refs),
    maplist(decl_line(Decls, Rules, Bindings), Refs, DeclLines),
    maplist(rule_line(Bindings), Rules, RuleLines),
    ( DeclLines == [] -> DeclBlock = "" ; atomic_list_concat(DeclLines, DeclBlock) ),
    ( RuleLines == [] -> RuleBlock = "" ; atomic_list_concat(RuleLines, RuleBlock) ),
    ( Rules == [] -> Sep = "" ; DeclLines == [] -> Sep = "" ; Sep = "\n" ),
    format(atom(Text), "~w~w~w", [DeclBlock, Sep, RuleBlock]).

% Decl lines print ONLY for refs that literally appear in Decls, in
% first-occurrence order -- NOT the declared_refs/program_refs union
% compile.pl:program_plan/2 computes for its own table-and-arrival purposes.
% A ref that is purely an arrival target with ZERO decl entries (Decls=[]
% level-only fixtures like expressions.pl are the extreme case) gets no line
% at all: synthesizing one would FABRICATE a kind(Ref,_) entry the original
% term never had, breaking G1's `=@=` LIST-structure check even though the
% two programs would mean the same thing under rel_kind/3's own fallback.
% The rule text alone already reveals the ref's shape (name + arity + the
% column names rel_columns/4 mines), so nothing is actually hidden.

decl_ref_order(Decls, Order) :-
    findall(Ref,
            ( member(Decl, Decls),
              ( Decl = kind(Ref, _) ; Decl = keyed(Ref, _) ; Decl = keep(Ref, _) )
            ), Refs0),
    dedup_preserve_order(Refs0, Order).

dedup_preserve_order(List, Deduped) :-
    dedup_preserve_order_(List, [], RevDeduped),
    reverse(RevDeduped, Deduped).
dedup_preserve_order_([], Acc, Acc).
dedup_preserve_order_([X | Xs], Acc, Out) :-
    ( memberchk(X, Acc) -> dedup_preserve_order_(Xs, Acc, Out)
    ; dedup_preserve_order_(Xs, [X | Acc], Out)
    ).

% ═══ decl line : `rel Name(cols) [log|set] [keep(policy)] [key(positions)].`
% Reproduces EXACTLY the literal decl/2 entries this ref has in the original
% Decls list, in their original relative order -- never rel_kind/3's or
% decl_keep/3's fallback-merged view, which would silently synthesize or
% drop an entry the round-trip needs bit-for-bit (see the module-level note
% above decl_ref_order/2). ═══════════════════════════════════════════════

decl_line(Decls, Rules, Bindings, Ref, Line) :-
    Ref = Name/_Arity,
    rel_columns(Rules, Bindings, Ref, Columns),
    atomic_list_concat(Columns, ', ', ColsText),
    findall(Decl, ( member(Decl, Decls), decl_is_for_ref(Decl, Ref) ), RefDecls),
    maplist(print_decl_modifier, RefDecls, ModifierTexts),
    ( ModifierTexts == []
    -> ModifiersText = '', Sep = ''
    ; atomic_list_concat(ModifierTexts, ' ', ModifiersText), Sep = ' '
    ),
    format(atom(Line), "rel ~w(~w)~w~w.~n", [Name, ColsText, Sep, ModifiersText]).

decl_is_for_ref(kind(Ref, _), Ref).
decl_is_for_ref(keep(Ref, _), Ref).
decl_is_for_ref(keyed(Ref, _), Ref).

print_decl_modifier(kind(_, Kind), Text) :- format(atom(Text), "~w", [Kind]).
print_decl_modifier(keep(_, all), Text) :- !, Text = 'keep(all)'.
print_decl_modifier(keep(_, count(N)), Text) :- !, format(atom(Text), "keep(count(~w))", [N]).
print_decl_modifier(keyed(_, Positions), Text) :-
    atomic_list_concat(Positions, ', ', PosText),
    format(atom(Text), "key(~w)", [PosText]).

% ═══ rule line : `HeadText <- BodyText.` / `HeadText <+ BodyText.` ══════════

rule_line(Bindings, (Head <- Body), Line) :- !,
    print_term(Head, Bindings, 0, top, HeadText),
    print_body(Body, Bindings, BodyText),
    format(atom(Line), "~w <- ~w.~n", [HeadText, BodyText]).
rule_line(Bindings, (Head <+ Body), Line) :- !,
    print_term(Head, Bindings, 0, top, HeadText),
    print_body(Body, Bindings, BodyText),
    format(atom(Line), "~w <+ ~w.~n", [HeadText, BodyText]).

% ═══ body : split the comma-conjunction, print each item, join with ", " ════

print_body((Left, Right), Bindings, Text) :- !,
    print_body_item(Left, Bindings, LeftText),
    print_body(Right, Bindings, RightText),
    format(atom(Text), "~w, ~w", [LeftText, RightText]).
print_body(Item, Bindings, Text) :-
    print_body_item(Item, Bindings, Text).

print_body_item(Term, Bindings, Text) :-
    body_surface_for_term(Term, _, _, _, LowerRole, _), !,
    print_surface_body_item(LowerRole, Term, Bindings, Text).
print_body_item(Term, Bindings, Text) :-
    print_term(Term, Bindings, 0, top, Text).

print_surface_body_item(LowerRole, Term, Bindings, Text) :-
    wrapper_lower_role(LowerRole, Shape, _), !,
    Term =.. [Name | Args],
    print_surface_wrapper_args(Shape, Args, Bindings, ArgTexts),
    atomic_list_concat(ArgTexts, ', ', ArgsText),
    format(atom(Text), "~w(~w)", [Name, ArgsText]).
print_surface_body_item(word(_), Term, _Bindings, Text) :-
    format(atom(Text), "~w", [Term]).
print_surface_body_item(infix(_), Term, Bindings, Text) :-
    Term =.. [Op, Left, Right],
    print_term(Left, Bindings, 0, top, LeftText),
    print_term(Right, Bindings, 0, top, RightText),
    format(atom(Text), "~w ~w ~w", [LeftText, Op, RightText]).

print_surface_wrapper_args(body_item, [Inner], Bindings, [InnerText]) :- !,
    print_body_item(Inner, Bindings, InnerText).
print_surface_wrapper_args(_, Args, Bindings, ArgTexts) :-
    maplist(print_arg(Bindings), Args, ArgTexts).

% ═══ general term printer : var (via Bindings) | int | atom (single-quoted)
% | string (double-quoted) | '{}'(Pairs) braces | list | arithmetic (infix,
% precedence-safe) | generic compound Name(Args...) ══════════════════════════

print_term(Term, Bindings, ParentPrec, Side, Text) :-
    ( var(Term)
    -> print_var(Term, Bindings, Text)
    ; integer(Term)
    -> format(atom(Text), "~w", [Term])
    ; string(Term)
    -> quote_value(Term, 0'", Text)
    ; is_list(Term)
    -> print_list(Term, Bindings, Text)
    ; Term = '{}'(Pairs)
    -> print_braces(Pairs, Bindings, PairsText), format(atom(Text), "{~w}", [PairsText])
    ; compound(Term), Term =.. [Op, Left, Right], arith_op(Op, MyPrec)
    -> print_term(Left, Bindings, MyPrec, left, LeftText),
       print_term(Right, Bindings, MyPrec, right, RightText),
       format(atom(Inner), "~w ~w ~w", [LeftText, Op, RightText]),
       ( needs_parens(MyPrec, Side, ParentPrec) -> format(atom(Text), "(~w)", [Inner]) ; Text = Inner )
    ; compound(Term)
    -> Term =.. [Name | Args],
       maplist(print_arg(Bindings), Args, ArgTexts),
       atomic_list_concat(ArgTexts, ', ', ArgsText),
       format(atom(Text), "~w(~w)", [Name, ArgsText])
    ; atom(Term)
    -> quote_value(Term, 0'\', Text)
    ; format(atom(Text), "~w", [Term])
    ).

print_arg(Bindings, Arg, Text) :- print_term(Arg, Bindings, 0, top, Text).

arith_op(+, 1). arith_op(-, 1). arith_op(*, 2). arith_op(/, 2). arith_op(mod, 2).

needs_parens(MyPrec, _, ParentPrec) :- MyPrec < ParentPrec, !.
needs_parens(MyPrec, right, ParentPrec) :- MyPrec =:= ParentPrec, !.

print_var(Var, Bindings, Text) :-
    ( member(Name = BoundVar, Bindings), BoundVar == Var
    -> format(atom(Text), "~w", [Name])
    ; Text = '_'
    ).

print_list([], _Bindings, "[]") :- !.
print_list(List, Bindings, Text) :-
    maplist(print_arg(Bindings), List, ItemTexts),
    atomic_list_concat(ItemTexts, ', ', Inner),
    format(atom(Text), "[~w]", [Inner]).

print_braces((Key:Value, Rest), Bindings, Text) :- !,
    print_term(Value, Bindings, 0, top, ValueText),
    print_braces(Rest, Bindings, RestText),
    format(atom(Text), "~w: ~w, ~w", [Key, ValueText, RestText]).
print_braces(Key:Value, Bindings, Text) :-
    print_term(Value, Bindings, 0, top, ValueText),
    format(atom(Text), "~w: ~w", [Key, ValueText]).

% ═══ quoting : always explicit, never Prolog's own ~q "quote only if
% necessary" logic -- see parse_dl.pl's module header for why every atom
% must be quoted (bare identifiers are always variables in this grammar) ═══

quote_value(Value, QuoteChar, Out) :-
    ( atom(Value) -> atom_codes(Value, Codes) ; string_codes(Value, Codes) ),
    escape_for_quote(Codes, QuoteChar, EscapedCodes),
    atom_codes(Body, EscapedCodes),
    char_code(QuoteCharAtom, QuoteChar),
    format(atom(Out), "~w~w~w", [QuoteCharAtom, Body, QuoteCharAtom]).

escape_for_quote([], _, []).
escape_for_quote([C | Cs], Q, Out) :-
    ( C == Q -> Out = [0'\\, C | More]
    ; C == 0'\\ -> Out = [0'\\, 0'\\ | More]
    ; C == 0'\n -> Out = [0'\\, 0'n | More]
    ; Out = [C | More]
    ),
    escape_for_quote(Cs, Q, More).
