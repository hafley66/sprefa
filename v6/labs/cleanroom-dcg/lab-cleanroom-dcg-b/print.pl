% print.pl -- hand-written printer for the .dl6 AST from dcg.pl.

:- module(printpi, [print_program/2]).

print_program(program(Decls, Rules), Text) :-
    with_output_to(string(Text), (print_decls(Decls), print_rules(Rules))).

print_decls([]).
print_decls([D|Ds]) :- print_decl(D), nl, print_decls(Ds).

print_rules([]).
print_rules([R|Rs]) :- print_rule(R), nl, print_rules(Rs).

print_decl(rel_decl(Name, Cols, Mods)) :-
    write('rel '), write(Name), write('('),
    print_cols(Cols), write(')'),
    print_mods(Mods), write('.').
print_decl(sh_decl(Name, Ins, Outs, Template)) :-
    write('sh '), write(Name), write('('), print_cols(Ins), write(') -> ('),
    print_cols(Outs), write(') = `'), write(Template), write('`.').
print_decl(bind_decl(Name, Cols)) :-
    write('bind '), write(Name), write('('), print_cols(Cols), write(').').
print_decl(query(Name, Args)) :-
    write('? '), write(Name), write('('), print_args(Args), write(').').

print_cols(enum(Vs)) :- print_variants(Vs).
print_cols(Cols) :- print_variants(Cols).

print_variants([]).
print_variants([V|Vs]) :- print_col(V), print_variants_tail(Vs).
print_variants_tail([]).
print_variants_tail([V|Vs]) :- write(' ; '), print_col(V), print_variants_tail(Vs).

print_col(col(N)) :- write(N).
print_col(col(N, T)) :- write(N), write(': '), print_type(T).
print_col(variant(N, F)) :- write(N), write('('), print_variants(F), write(')').

print_type(type(N)) :- write(N).
print_type(type(N, Ts)) :- write(N), write('('), print_types(Ts), write(')').
print_types([]).
print_types([T|Ts]) :- print_type(T), print_types_tail(Ts).
print_types_tail([]).
print_types_tail([T|Ts]) :- write(', '), print_type(T), print_types_tail(Ts).

print_mods([]).
print_mods([M|Ms]) :- print_mod(M), print_mods(Ms).
print_mod(log) :- write(' log').
print_mod(keep(all)) :- write(' keep(all)').
print_mod(keep(count(N))) :- write(' keep(count('), write(N), write('))').
print_mod(key(Ns)) :- write(' key('), print_intlist(Ns), write(')').
print_intlist([]).
print_intlist([N|Ns]) :- write(N), print_intlist_tail(Ns).
print_intlist_tail([]).
print_intlist_tail([N|Ns]) :- write(', '), write(N), print_intlist_tail(Ns).

print_rule(rule(Head, Mode, Body)) :-
    print_expr(Head), write(' '), ( Mode = level -> write('<-') ; write('<+') ),
    write(' '), print_items(Body), write('.').
print_rule(match(Source, Arms)) :-
    write('match '), print_expr(Source), write(' ('),
    print_arms(Arms),
    write(' ).').

print_arms([]).
print_arms([A|As]) :- write(' ; '), print_arm(A), print_arms_tail(As).
print_arms_tail([]).
print_arms_tail([A|As]) :- write(' ; '), print_arm(A), print_arms_tail(As).
print_arm(arm(Mode, Head, Body)) :-
    print_items(Body), write(' '),
    ( Mode = level -> write('|->') ; write('|+>') ),
    write(' '), print_expr(Head).

print_items([]).
print_items([I|Is]) :- print_item(I), print_items_tail(Is).
print_items_tail([]).
print_items_tail([I|Is]) :- write(', '), print_item(I), print_items_tail(Is).

print_item(I) :- print_expr(I).

% ---- expression printing ----------------------------------------------------
print_args([]).
print_args([A|As]) :- print_expr(A), print_args_tail(As).
print_args_tail([]).
print_args_tail([A|As]) :- write(', '), print_expr(A), print_args_tail(As).

print_expr(var(N)) :- write(N).
print_expr(atom(T)) :- qatom(T).
print_expr(string(T)) :- qstr(T).
print_expr(int(N)) :- write(N).
print_expr(float(F)) :- write(F).
print_expr(list(Items)) :- write('['), print_listitems(Items), write(']').
print_expr(obj(Pairs)) :- obj(Pairs).
print_expr(call(N, Args)) :- write(N), write('('), print_args(Args), write(')').
print_expr(paren(E)) :- write('('), print_expr(E), write(')').
print_expr(path(V, Fs)) :- print_expr(V), print_path(Fs).
print_expr(plus(A,B)) :- print_binop(A, '+', B).
print_expr(minus(A,B)) :- print_binop(A, '-', B).
print_expr(times(A,B)) :- print_binop(A, '*', B).
print_expr(div(A,B)) :- print_binop(A, '/', B).
print_expr(mod(A,B)) :- print_binop(A, 'mod', B).

print_binop(A, Op, B) :- print_expr(A), write(' '), write(Op), write(' '), print_expr(B).

print_path([]).
print_path([F|Fs]) :- write('.'), write(F), print_path(Fs).

print_listitems([]).
print_listitems([I|Is]) :- print_listitem(I), print_listitems_tail(Is).
print_listitems_tail([]).
print_listitems_tail([I|Is]) :- write(', '), print_listitem(I), print_listitems_tail(Is).
print_listitem(spread(E)) :- write('... '), print_expr(E).
print_listitem(E) :- print_expr(E).

obj([]) :- write('{}').
obj(Pairs) :- write('{'), print_pairs(Pairs), write('}').

print_pairs([]).
print_pairs([P|Ps]) :- print_pair(P), print_pairs_tail(Ps).
print_pairs_tail([]).
print_pairs_tail([P|Ps]) :- write(', '), print_pair(P), print_pairs_tail(Ps).

print_pair(pair(Key, Val)) :- print_key(Key), write(': '), print_value(Val).

print_key(descent) :- write('**').
print_key(capture(N)) :- write('$'), write(N).
print_key(atom(T)) :- qatom(T).
print_key(string(T)) :- qstr(T).
print_key(name(N)) :- write(N).

print_value(V) :- print_expr(V).

% cmp/bind are printed as infix
print_expr(cmp(Op, A, B)) :- print_expr(A), write(' '), write(Op), write(' '), print_expr(B).
print_expr(bind(Op, A, B)) :- print_expr(A), write(' '), write(Op), write(' '), print_expr(B).
print_expr(typed(N, T)) :- write(N), write(': '), print_type(T).

% ---- quoting ----------------------------------------------------------------
qatom(T) :- atom_codes(T, Cs), esc_q(Cs, Esc, 0''), write('\''), put_esc(Esc), write('\'').
qstr(T) :- string_codes(T, Cs), esc_q(Cs, Esc, 0'"), write('"'), put_esc(Esc), write('"').

esc_q([], [], _).
esc_q([C|Cs], Esc, Q) :-
    ( C =:= Q -> Esc = [0'\\, C|Rest]
    ; C =:= 0'\\ -> Esc = [0'\\, C|Rest]
    ; C =:= 10 -> Esc = [0'\\, 0'n|Rest]
    ; C =:= 9 -> Esc = [0'\\, 0't|Rest]
    ; Esc = [C|Rest] ),
    esc_q(Cs, Rest, Q).

put_esc([]).
put_esc([C|Cs]) :- put_char(C), put_esc(Cs).
