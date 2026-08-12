% dcg.pl -- clean-room DCG for the .dl6 language.
%
% Operates over a code list (list of integer char codes). Text to term.
% Term shape (a finding; see REPORT.md):
%   program(Decls, Rules, Queries)
% Everything in the language is an expression tree; the body is a
% comma-separated list of exprs and needs no separate body grammar.

parse_program(Text, program(Decls, Rules, Queries)) :-
    string_codes(Text, Codes),
    phrase(program(program(Decls, Rules, Queries)), Codes).

% ---------------------------------------------------------------------------
% program / statement collection
% ---------------------------------------------------------------------------

program(P) --> ws, program_items(Items), ws,
    { program_kind(Items, P) }.

program_items([I|Is]) --> statement(I), program_items(Is).
program_items([]) --> [].

program_kind(Items, program(Decls, Rules, Queries)) :-
    classify_items(Items, Decls, Rules, Queries).

classify_items([], [], [], []).
classify_items([I|Is], [I|D], R, Q) :- decl_kind(I), classify_items(Is, D, R, Q).
classify_items([I|Is], D, [I|R], Q) :- rule_kind(I), classify_items(Is, D, R, Q).
classify_items([I|Is], D, R, [I|Q]) :- query_kind(I), classify_items(Is, D, R, Q).

decl_kind(rel(_,_,_)).
decl_kind(sh(_,_,_,_)).
decl_kind(bind_decl(_,_)).
rule_kind(level(_,_)).
rule_kind(edge(_,_)).
rule_kind(fact(_)).
rule_kind(match(_,_)).
query_kind(query(_)).

% ---------------------------------------------------------------------------
% statements
% ---------------------------------------------------------------------------

statement(Kind) --> ws, statement_kind(Kind).

statement_kind(rel(Name, Cols, Mods)) -->
    kw(rel), ws, name(Name), ws, "(", ws, decl_arglist(Cols), ws, ")",
    ws, decl_mods(Mods), ws, period.
statement_kind(sh(Name, Ins, Outs, template(Tpl))) -->
    kw(sh), ws, name(Name), ws, "(", ws, decl_arglist(Ins), ws, ")",
    ws, "->", ws, "(", ws, decl_arglist(Outs), ws, ")",
    ws, "=", ws, template_lit(Tpl), ws, period.
statement_kind(bind_decl(Name, Cols)) -->
    kw(bind), ws, name(Name), ws, "(", ws, decl_arglist(Cols), ws, ")",
    ws, period.
statement_kind(query(Atom)) -->
    "?", ws, atom_call(Atom), ws, period.
statement_kind(match(Source, Arms)) -->
    kw(match), ws, atom_call(Source), ws, "(", ws,
    opt_semi, ws, arm_list(Arms), ws, ")",
    ws, period.
statement_kind(Kind) --> rule_or_fact(Kind).

% ---------------------------------------------------------------------------
% rel declaration modifiers
% ---------------------------------------------------------------------------

decl_mods([]) --> [].
decl_mods([M|Ms]) -->
    ( kw(log), ws, { M = log }
    ; kw(keep), ws, "(", ws, keep_arg(K), ws, ")", ws, { M = keep(K) }
    ; kw(key), ws, "(", ws, key_positions(Ps), ws, ")", ws, { M = keyed(Ps) }
    ),
    decl_mods(Ms).

keep_arg(all) --> kw(all).
keep_arg(count(N)) --> kw(count), ws, "(", ws, integer(N), ws, ")".

key_positions([P]) --> integer(P).
key_positions([P|Ps]) --> integer(P), ws, ",", ws, key_positions(Ps).

% ---------------------------------------------------------------------------
% declaration argument list (columns / types / enum variants share one grammar)
% ---------------------------------------------------------------------------

decl_arglist([A]) --> decl_arg(A).
decl_arglist([A|As]) --> decl_arg(A), ws, decl_sep, ws, decl_arglist(As).
decl_arglist([]) --> [].

decl_sep --> ",".
decl_sep --> ";".

decl_arg(Term) -->
    name(Name), ws,
    ( "(", ws, decl_arglist(Args), ws, ")", { Term = applied(Name, Args) }
    ; ":", ws, decl_arg(Inner), { Term = typed(Name, Inner) }
    ; { Term = id(Name) }
    ).

% ---------------------------------------------------------------------------
% template literal (backticks)
% ---------------------------------------------------------------------------

template_lit(Text) -->
    "`", tpl_body(Codes), "`",
    { string_codes(Text, Codes) }.

tpl_body([C|Cs]) --> [C], { C =\= 0'` }, tpl_body(Cs).
tpl_body([]) --> [].

% ---------------------------------------------------------------------------
% rules and facts
% ---------------------------------------------------------------------------

rule_or_fact(Kind) -->
    atom_call(Head), ws,
    ( "<-", ws, body(Body), ws, period, { Kind = level(Head, Body) }
    ; "<+", ws, body(Body), ws, period, { Kind = edge(Head, Body) }
    ; period, { Kind = fact(Head) }
    ).

body(Goals) --> goal_list(Goals).

goal_list([G]) --> expr(G).
goal_list([G|Gs]) --> expr(G), ws, ",", ws, goal_list(Gs).

% ---------------------------------------------------------------------------
% match
% ---------------------------------------------------------------------------

opt_semi --> ( ";" -> [] ; [] ).

arm_list([A]) --> arm(A).
arm_list([A|As]) --> arm(A), ws, ";", ws, arm_list(As).

arm(arm(Mode, Guards, Head)) -->
    goal_list(Guards), ws, arrow(Mode), ws, atom_call(Head).

arrow(level) --> "|->".
arrow(edge)  --> "|+>".

% ---------------------------------------------------------------------------
% atom call (name(args))
% ---------------------------------------------------------------------------

atom_call(call(Name, Args)) -->
    name(Name), ws, "(", ws, arg_list(Args), ws, ")".

arg_list([A]) --> expr(A).
arg_list([A|As]) --> expr(A), ws, ",", ws, arg_list(As).
arg_list([]) --> [].

% ---------------------------------------------------------------------------
% expressions: precedence climbing over code chars
% ---------------------------------------------------------------------------

expr(T) --> expr_(0, T).

expr_(MinPrec, T) -->
    primary(First),
    expr_tail(MinPrec, First, T).

expr_tail(MinPrec, Left, T) -->
    binop(Name, Prec),
    { Prec >= MinPrec },
    !,
    { NextMin is Prec + 1 },
    expr_(NextMin, Right),
    expr_tail(MinPrec, op(Name, Left, Right), T).
expr_tail(_, Left, Left) --> [].

% ---------------------------------------------------------------------------
% primary values
% ---------------------------------------------------------------------------

primary(P) --> ws, primary_tok(P).

primary_tok(neg(X)) --> "-", ws, number(X).
primary_tok(X) --> number(X).
primary_tok(atom(Content)) --> squote(Content).
primary_tok(string(Content)) --> dquote(Content).
primary_tok(T) --> list_expr(T).
primary_tok(T) --> brace_expr(T).
primary_tok(call(Name, Args)) --> atom_call(call(Name, Args)).
primary_tok(wildcard) --> wildcard_tok.
primary_tok(var(Name)) --> name(Name).

number(X) --> float_lit(X), !.
number(X) --> int_lit(X).

float_lit(float(F)) -->
    digits(IntCodes), [0'.], digits(FracCodes), expo(ExpCodes),
    { append(IntCodes, [0'.|FracCodes], A), append(A, ExpCodes, All),
      number_codes(F, All) }.

int_lit(int(N)) --> digits(Digits), { number_codes(N, Digits) }.

expo([]) --> [].
expo([E|Rest]) --> [E], { E =:= 0'e ; E =:= 0'E }, expo_digits(Rest).
expo_digits(D) --> "+", digits(D).
expo_digits(D) --> "-", digits(D).
expo_digits(D) --> digits(D).

digits([C|Cs]) --> [C], { digit_code(C) }, more_digits(Cs).
more_digits([C|Cs]) --> [C], { digit_code(C) }, more_digits(Cs).
more_digits([]) --> [].

integer(N) --> ws, int_lit(int(N)).

wildcard_tok --> [0'_], wildcard_rest.
wildcard_rest --> [C], { ident_char(C) }, !, wildcard_tail.
wildcard_rest --> [].
wildcard_tail --> [C], { ident_char(C) }, wildcard_tail.
wildcard_tail --> [].

% ---------------------------------------------------------------------------
% lists and braces
% ---------------------------------------------------------------------------

list_expr(list([])) --> "[", ws, "]".
list_expr(list(Items)) --> "[", ws, list_items(Items), ws, "]".

list_items([I]) --> list_item(I).
list_items([I|Is]) --> list_item(I), ws, ",", ws, list_items(Is).

list_item(spread(E)) --> "...", ws, list_element(E).
list_item(E) --> list_element(E).

list_element(E) --> expr(E).

brace_expr(brace([])) --> "{", ws, "}".
brace_expr(brace(Entries)) --> "{", ws, brace_entries(Entries), ws, "}".

brace_entries([E]) --> brace_entry(E).
brace_entries([E|Es]) --> brace_entry(E), ws, ",", ws, brace_entries(Es).

brace_entry(Entry) -->
    brace_key(Key), ws, ":", ws, brace_value(Value),
    ( ws, ":", ws, decl_arg(Type) -> { Entry = entry(Key, Value, Type) }
    ; { Entry = entry(Key, Value) } ).

brace_value(Value) --> expr(Value).

brace_key(capture(Name)) --> "$", ws, name(Name).
brace_key(descent) --> "**".
brace_key(qtkey(Content)) --> squote(Content).
brace_key(strkey(Content)) --> dquote(Content).
brace_key(key(Name)) --> name(Name).
brace_key(any) --> "_", not_ident_follow.

% ---------------------------------------------------------------------------
% binary operators (higher precedence number = binds tighter)
% ---------------------------------------------------------------------------

binop(Name, Prec) --> ws, binop_tok(Name, Prec).

binop_tok(':=', 30) --> ":", "=".
binop_tok('==', 60) --> "=", "=".
binop_tok('\\==', 60) --> "\\", "=", "=".
binop_tok('=:=', 60) --> "=", ":", "=".
binop_tok('=\\=', 60) --> "=", "\\", "=".
binop_tok('=<', 60) --> "=", "<".
binop_tok('>=', 60) --> ">", "=".
binop_tok('<=', 60) --> "<", "=".
binop_tok('!=', 60) --> "!", "=".
binop_tok('>', 60) --> ">".
binop_tok('<', 60) --> "<", not_follow_arrow.
binop_tok('+', 300) --> "+".
binop_tok('-', 300) --> "-".
binop_tok('*', 400) --> "*".
binop_tok('/', 400) --> "/".
binop_tok(mod, 400) --> "mod", not_ident_follow.
binop_tok('=', 60) --> "=", not_more_eq.

not_follow_arrow --> \+ ( [C], { member(C, [0'-, 0'+, 0'=]) } ), [].
not_more_eq --> \+ ( [C], { member(C, [0'=, 0'<, 0':, 0'\\, 0'>]) } ), [].
not_ident_follow --> \+ ( [C], { ident_char(C) } ), [].

% ---------------------------------------------------------------------------
% names and quoted literals
% ---------------------------------------------------------------------------

name(Name) --> ws, ident(Name).

ident(Name) -->
    [C0], { ident_start(C0) },
    ident_rest([C0], Name).

ident_rest(Acc, Name) -->
    [C], { ident_char(C) },
    !,
    ident_rest([C|Acc], Name).
ident_rest(Acc, Name) -->
    [D], { D =:= 0'. },
    [C2], { ident_start(C2) },
    !,
    ident_rest([C2, D|Acc], Name).
ident_rest(Acc, Name) -->
    { reverse(Acc, Codes), string_codes(Str, Codes), atom_string(Name, Str) }.

squote(Content) --> quote(0'', Content).
dquote(Content) --> quote(0'", Content).

quote(Delim, Content) -->
    ws, [Delim], quote_body(Delim, Codes), [Delim],
    { string_codes(Content, Codes) }.

quote_body(Delim, [C|Cs]) -->
    ( escape_sequence(C)
    ; [C], { C \== Delim, C =\= 0'\\ }
    ),
    quote_body(Delim, Cs).
quote_body(_, []) --> [].

escape_sequence(C) --> [0'\\], [C].

% ---------------------------------------------------------------------------
% keywords and character classes
% ---------------------------------------------------------------------------

kw(Word) -->
    { atom_codes(Word, Chars) },
    chars(Chars),
    not_ident_follow.

chars([]) --> [].
chars([C|Cs]) --> [C], chars(Cs).

ws --> wspace.
wspace --> ( "\n" ; "\t" ; " " ; "\r" ), wspace, !.
wspace --> [].

period --> ws, ".", ws.

ident_start(C) :- ( code_type(C, alpha) ; C =:= 0'_ ).
ident_char(C) :- ( code_type(C, alnum) ; C =:= 0'_ ).
digit_code(C) :- code_type(C, digit).
