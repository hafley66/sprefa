% dcg.pl -- clean-room SWI-Prolog DCG for .dl6, text to ground term.

:- module(dcg, [parse_dcg/2]).

parse_dcg(Text, program(Decls, Rules)) :-
    string_codes(Text, Codes),
    phrase(program(Decls, Rules), Codes).

program(Decls, Rules) -->
    ws, statements(Decls, Rules), ws.

statements(D, R) -->
    statement(S),
    statements(D0, R0),
    ( { S = decl(X) } -> { D = [X|D0], R = R0 }
    ; { S = rule_(X) } -> { D = D0, R = [X|R0] }
    ).
statements([], []) --> [].

statement(S) -->
    ws,
    ( kw(rel), ws, decl_rel(T), { S = decl(T) }
    ; kw(sh),  ws, decl_sh(T),  { S = decl(T) }
    ; kw(bind), ws, decl_bind(T), { S = decl(T) }
    ; literal("?"), ws, decl_query(T), { S = decl(T) }
    ; kw(match), ws, match_stmt(T), { S = rule_(T) }
    ; rule_stmt(T), { S = rule_(T) }
    ).

% ---- declarations -----------------------------------------------------------

decl_rel(rel_decl(Name, Cols, Mods)) -->
    name(Name), ws,
    literal("("), ws, decl_entries_opt(Entries), ws, literal(")"),
    ws, mods(Mods), ws, literal("."),
    ( { member(V, Entries), V = variant(_,_) } -> { Cols = enum(Entries) }
    ; { Cols = Entries }
    ).
decl_entries_opt(Es) --> decl_entries(Es).
decl_entries_opt([]) --> [].

decl_sh(sh_decl(Name, Ins, Outs, Template)) -->
    name(Name), ws,
    literal("("), ws, decl_entries(Ins), ws, literal(")"),
    ws, literal("->"), ws,
    literal("("), ws, decl_entries(Outs), ws, literal(")"),
    ws, literal("="), ws,
    literal("`"), tpl_chars(Cs), literal("`"), ws, literal("."),
    { atom_codes(Template, Cs) }.

decl_bind(bind_decl(Name, Cols)) -->
    name(Name), ws,
    literal("("), ws, decl_entries(Cols), ws, literal(")"), ws, literal(".").

decl_query(query(Name, Args)) -->
    name(Name), ws,
    literal("("), ws, arglist(Args), ws, literal(")"), ws, literal(".").

mods(Mods) --> mod(M), mods_tail(Mods0), { Mods = [M|Mods0] }.
mods([]) --> [].
mods_tail(Mods) --> ws, mod(M), mods_tail(Mods0), { Mods = [M|Mods0] }.
mods_tail([]) --> [].

mod(log) --> kw(log).
mod(keep(all)) --> kw(keep), ws, literal("("), ws, kw(all), ws, literal(")").
mod(keep(count(N))) -->
    kw(keep), ws, literal("("), ws, kw(count), ws, literal("("), ws,
    intlit(N), ws, literal(")"), ws, literal(")").
mod(key(Ns)) --> kw(key), ws, literal("("), ws, intlist(Ns), ws, literal(")").

intlist(Ns) --> intlit(N), intlist_tail(Ns0), { Ns = [N|Ns0] }.
intlist_tail(Ns) --> ws, literal(","), ws, intlit(N), intlist_tail(Ns0), { Ns = [N|Ns0] }.
intlist_tail([]) --> [].

% columns / enum variants, ; or , separated
decl_entries(Es) --> decl_entry(E), decl_entries_tail(Es0), { Es = [E|Es0] }.
decl_entries_tail(Es) --> ws, literal(","), ws, decl_entry(E), decl_entries_tail(Es0), { Es = [E|Es0] }.
decl_entries_tail(Es) --> ws, literal(";"), ws, decl_entry(E), decl_entries_tail(Es0), { Es = [E|Es0] }.
decl_entries_tail([]) --> [].

decl_entry(E) --> name(N), decl_entry_tail(N, E).
decl_entry_tail(N, col(N, T)) --> ws, literal(":"), ws, type_spec(T).
decl_entry_tail(N, variant(N, F)) --> ws, literal("("), ws, opt_variant_fields(F), ws, literal(")").
decl_entry_tail(N, col(N)) --> [].
opt_variant_fields(F) --> decl_entries(F).
opt_variant_fields([]) --> [].

type_spec(type(N)) --> name(N).
type_spec(type(N, Ts)) --> name(N), ws, literal("("), ws, type_list(Ts), ws, literal(")").
type_list(Ts) --> type_spec(T), type_list_tail(Ts0), { Ts = [T|Ts0] }.
type_list_tail(Ts) --> ws, literal(","), ws, type_spec(T), type_list_tail(Ts0), { Ts = [T|Ts0] }.
type_list_tail([]) --> [].

% ---- match ------------------------------------------------------------------

match_stmt(match(Source, Arms)) -->
    source_atom(Source), ws,
    literal("("), ws, opt_sep, arms(Arms), ws, literal(")"), ws, literal(".").

opt_sep --> literal(";"), ws, opt_sep.
opt_sep --> [].

source_atom(call(Name, Args)) --> name(Name), ws, literal("("), ws, arglist(Args), ws, literal(")").

arms(Arms) --> arm(A), arms_tail(Arms0), { Arms = [A|Arms0] }.
arms_tail(As) --> ws, literal(";"), ws, arm(A), arms_tail(As0), { As = [A|As0] }.
arms_tail([]) --> [].

arm(arm(Mode, Head, Body)) --> ws, body(Body), ws, arrow(Mode), ws, head(Head).
arrow(level) --> literal("|->").
arrow(edge) --> literal("|+>").

% ---- rules ------------------------------------------------------------------

rule_stmt(rule(Head, Mode, Body)) -->
    head(Head), ws, rule_arrow(Mode, Body), ws, literal(".").
rule_stmt(rule(Head, level, [])) -->
    head(Head), ws, literal(".").

rule_arrow(level, Body) --> literal("<-"), ws, body(Body).
rule_arrow(edge, Body)  --> literal("<+"), ws, body(Body).

head(call(Name, Args)) --> name(Name), ws, literal("("), ws, arglist(Args), ws, literal(")").

body(Items) --> item(I), body_tail(Items0), { Items = [I|Items0] }.
body_tail(Is) --> ws, literal(","), ws, item(I), body_tail(Is0), { Is = [I|Is0] }.
body_tail([]) --> [].

item(I) --> ws, name(not), ws, literal("("), ws, item(G), ws, literal(")"), { I = call(not, [G]) }.
item(I) --> expr(E), item_tail(E, I).
item_tail(E, bind(Op, LHS, RHS)) --> ws, bindop(Op), ws, expr(RHS), { LHS = E }.
item_tail(E, cmp(Op, A, B)) --> ws, cmpop(Op), ws, expr(B), { A = E }.
item_tail(E, E) --> [].

bindop(:=) --> literal(":=").
bindop(is) --> kw(is).

cmpop('\\==') --> literal("\\==").
cmpop('=\\=') --> literal("=\\=").
cmpop('=:=') --> literal("=:=").
cmpop('>=')  --> literal(">=").
cmpop('=<')  --> literal("=<").
cmpop('==')  --> literal("==").
cmpop('>')   --> literal(">").
cmpop('<')   --> literal("<").

% ---- expressions ------------------------------------------------------------

expr(E) --> aexpr(E).

aexpr(E) --> mexpr(T), arest(T, E).
arest(Acc, Res) --> ws, literal("+"), ws, mexpr(T), arest(plus(Acc,T), Res).
arest(Acc, Res) --> ws, literal("-"), ws, mexpr(T), arest(minus(Acc,T), Res).
arest(E, E) --> [].

mexpr(E) --> pexpr(T), mrest(T, E).
mrest(Acc, Res) --> ws, literal("*"), ws, pexpr(T), mrest(times(Acc,T), Res).
mrest(Acc, Res) --> ws, literal("/"), ws, pexpr(T), mrest(div(Acc,T), Res).
mrest(Acc, Res) --> ws, kw(mod), ws, pexpr(T), mrest(mod(Acc,T), Res).
mrest(E, E) --> [].

pexpr(E) --> atom_expr(E).

atom_expr(E) --> literal("("), ws, expr(E0), ws, literal(")"), { E = paren(E0) }.
atom_expr(E) --> listlit(E).
atom_expr(E) --> bracelit(E).
atom_expr(E) --> qatom(E).
atom_expr(E) --> qstr(E).
atom_expr(E) --> number(E).
atom_expr(E) --> ident_or_call(E).

listlit(list(Items)) --> literal("["), ws, listitems(Items), ws, literal("]").
listitems([]) --> [].
listitems(Is) --> listitem(I), listitem_tail(Is0), { Is = [I|Is0] }.
listitem(spread(E)) --> literal("..."), ws, expr(E).
listitem(E) --> expr(E).
listitem_tail(Is) --> ws, literal(","), ws, listitem(I), listitem_tail(Is0), { Is = [I|Is0] }.
listitem_tail([]) --> [].

bracelit(obj([])) --> literal("{"), ws, literal("}").
bracelit(obj(Pairs)) --> literal("{"), ws, pair(P), pair_tail(P0), ws, literal("}"), { Pairs = [P|P0] }.
pair_tail(Ps) --> ws, literal(","), ws, pair(P), pair_tail(P0), { Ps = [P|P0] }.
pair_tail([]) --> [].

pair(pair(Key, Val)) --> key(Key), ws, literal(":"), ws, value(Val).

key(descent)      --> literal("**").
key(capture(N))   --> literal("$"), ident(N).
key(atom(T))      --> qatom_text(T).
key(string(T))    --> qstr_text(T).
key(name(N))      --> name(N).

value(V) --> expr(E), typed_tail(E, V).
typed_tail(var(N), typed(N, T)) --> ws, literal(":"), ws, type_spec(T).
typed_tail(E, E) --> [].

% number
% number (float tried first so "0.2" is a float, not int 0 plus leftover)
number(float(F)) --> sign_code(M), digits(D), literal("."), digits(E),
    { append(D, [0'.|E], D0), ( M = s -> C = [0'-|D0] ; C = D0 ), number_codes(F, C) }.
number(int(N)) --> sign_code(M), digits(D),
    { ( M = s -> C = [0'-|D] ; C = D ), number_codes(N, C) }.
sign_code(s) --> literal("-").
sign_code(n) --> [].
digits([C|Cs]) --> [C], { code_type(C, digit) }, digits_rest(Cs).
digits_rest([C|Cs]) --> [C], { code_type(C, digit) }, digits_rest(Cs).
digits_rest([]) --> [].

intlit(N) --> digits(D), { number_codes(N, D) }.

% quoted atom / string (decode escapes, forward)
qatom(atom(Text)) --> qatom_text(Text).
qatom_text(Text) --> literal("'"), sq_codes(Cs), literal("'"), { atom_codes(Text, Cs) }.
qstr(string(Text)) --> qstr_text(Text).
qstr_text(Text) --> literal("\""), dq_codes(Cs), literal("\""), { string_codes(Text, Cs) }.

sq_codes(Cs) --> sq_char(C), sq_codes(Cs0), { Cs = [C|Cs0] }.
sq_codes([]) --> [].
sq_char(C) --> literal("\\'"),  { C = 0'' }.
sq_char(C) --> literal("\\\\"), { C = 0'\\ }.
sq_char(C) --> [C], { C =\= 0'', C =\= 0'\\, C =\= 10, C =\= 13 }.

dq_codes(Cs) --> dq_char(C), dq_codes(Cs0), { Cs = [C|Cs0] }.
dq_codes([]) --> [].
dq_char(C) --> literal("\\\""),  { C = 0'" }.
dq_char(C) --> literal("\\\\"),  { C = 0'\\ }.
dq_char(C) --> literal("\\n"),   { C = 10 }.
dq_char(C) --> literal("\\t"),   { C = 9 }.
dq_char(C) --> [C], { C =\= 0'", C =\= 0'\\, C =\= 10, C =\= 13 }.

% shell template raw text until backtick
tpl_chars(Cs) --> tpl_char(C), tpl_chars(Cs0), { Cs = [C|Cs0] }.
tpl_chars([]) --> [].
tpl_char(C) --> [C], { C =\= 0'` }.

% identifier handling
ident_or_call(T) --> base(B, K), ioc_tail(B, K, T).
% uppercase/underscore base: field-access path or plain variable
ioc_tail(B, upper, path(var(B), Fs)) --> dotfields(Fs).
ioc_tail(B, upper, var(B)) --> [].
% lowercase base: dotted name, either a call or (rare) a bare variable
ioc_tail(B, lower, call(Dotted, Args)) --> dotname(Ds), ws, literal("("), ws, arglist(Args), ws, literal(")"),
    { join_dots([B|Ds], Dotted) }.
ioc_tail(B, lower, var(B)) --> dotname(Ds), { Ds = [] }.

% dotted name for decl/call/keyword position
name(A) --> bident(B), name_dots(Ds), { join_dots([B|Ds], A) }.
name_dots(Ds) --> literal("."), bident(D), name_dots(Ds0), { Ds = [D|Ds0] }.
name_dots([]) --> [].

dotname(Ds) --> literal("."), bident(D), dotname(Ds0), { Ds = [D|Ds0] }.
dotname([]) --> [].
dotfields([F|Fs]) --> literal("."), bident(F), dotfields_rest(Fs).
dotfields_rest([]) --> [].
dotfields_rest([F|Fs]) --> literal("."), bident(F), dotfields_rest(Fs).

join_dots([H], H) :- !.
join_dots([H|T], A) :- join_dots(T, A0), atom_concat(H, '.', H1), atom_concat(H1, A0, A).

arglist([]) --> [].
arglist(Args) --> expr(E), arglist_tail(Args0), { Args = [E|Args0] }.
arglist_tail(Args) --> ws, literal(","), ws, expr(E), arglist_tail(Args0), { Args = [E|Args0] }.
arglist_tail([]) --> [].

% and the original bare-identifier reading (split: first char then rest)
% and the original bare-identifier reading (split: first char then rest)
base(A, K) --> ident(A), { kind_of(A, K) }.
kind_of(A, upper) :- sig_letter(A, C), C \= none, code_type(C, upper).
kind_of(A, lower) :- sig_letter(A, C), C \= none, code_type(C, lower).
kind_of(A, upper) :- sig_letter(A, none).
sig_letter(A, C) :- atom_codes(A, Cs), strip_us(Cs, Cs1),
                    ( Cs1 = [C0|_] -> C = C0 ; C = none ).
strip_us([], []).
strip_us([H|T], R) :- ( H =:= 0'_ -> strip_us(T, R) ; R = [H|T] ).
base_rest(Cs) --> [C], { id_char(C) }, base_rest(Cs0), { Cs = [C|Cs0] }.
base_rest([]) --> [].
id_start(C) :- code_type(C, alpha) ; C =:= 0'_.
id_char(C) :- code_type(C, alnum) ; C =:= 0'_.

% bare identifier -> atom (any casing), first char id start
ident(A) --> [C0], { id_start(C0) }, ident_rest(Cs), { atom_codes(A, [C0|Cs]) }.
ident_rest(Cs) --> [C], { id_char(C) }, ident_rest(Cs0), { Cs = [C|Cs0] }.
ident_rest([]) --> [].

bident(A) --> ident(A).

% keyword (matches full identifier of the same atom)
kw(W) --> ident(A), { A = W }.

% ---- whitespace and literal -------------------------------------------------
ws --> [C], { code_type(C, space) }, ws.
ws --> [].

literal(S) --> { string_codes(S, Cs) }, lit_codes(Cs).
lit_codes([]) --> [].
lit_codes([C|Cs]) --> [C], lit_codes(Cs).
