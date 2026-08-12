% print.pl -- pretty printer for the dcg.pl term shape. Written by hand:
% reverse mode of dcg.pl does not terminate (see REPORT.md blocker list).

print_program(program(Decls, Rules, Queries), Text) :-
    with_output_to(string(Text), (
        emit_decls(Decls),
        emit_rules(Rules),
        emit_queries(Queries)
    )).

emit_decls([]).
emit_decls([D|Ds]) :- emit_decl(D), nl, emit_decls(Ds).

emit_rules([]).
emit_rules([R|Rs]) :- emit_rule(R), nl, emit_rules(Rs).

emit_queries([]).
emit_queries([Q|Qs]) :- emit_query(Q), nl, emit_queries(Qs).

% --- declarations ---------------------------------------------------------

emit_decl(rel(Name, Cols, Mods)) :-
    write('rel '), emit_name(Name), write('('),
    emit_decl_args(Cols), write(')'),
    emit_mods(Mods), write('.').
emit_decl(sh(Name, Ins, Outs, template(Tpl))) :-
    write('sh '), emit_name(Name), write('('),
    emit_decl_args(Ins), write(')'), write(' -> ('),
    emit_decl_args(Outs), write(')'), write(' = `'),
    write(Tpl), write('`.').
emit_decl(bind_decl(Name, Cols)) :-
    write('bind '), emit_name(Name), write('('),
    emit_decl_args(Cols), write(').').

emit_decl_args([]).
emit_decl_args([A]) :- emit_decl_arg(A).
emit_decl_args([A|As]) :- emit_decl_arg(A), write(', '), emit_decl_args(As).

emit_decl_arg(id(Name)) :- emit_name(Name).
emit_decl_arg(typed(Name, T)) :- emit_name(Name), write(': '), emit_decl_arg(T).
emit_decl_arg(applied(Name, Args)) :- emit_name(Name), write('('), emit_decl_args(Args), write(')').

emit_mods([]).
emit_mods([M|Ms]) :- emit_mod(M), emit_mods(Ms).

emit_mod(log) :- write(' log').
emit_mod(keep(all)) :- write(' keep(all)').
emit_mod(keep(count(N))) :- write(' keep(count('), write(N), write('))').
emit_mod(keyed(Ps)) :- write(' key('), emit_keyed(Ps), write(')').

emit_keyed([P]) :- write(P).
emit_keyed([P|Ps]) :- write(P), write(', '), emit_keyed(Ps).

% --- rules ----------------------------------------------------------------

emit_rule(level(Head, Body)) :- emit_atom(Head), write(' <- '), emit_goals(Body), write('.').
emit_rule(edge(Head, Body))  :- emit_atom(Head), write(' <+ '), emit_goals(Body), write('.').
emit_rule(fact(Head))        :- emit_atom(Head), write('.').
emit_rule(match(Source, Arms)) :-
    write('match '), emit_atom(Source), write(' ('),
    emit_arms(Arms), write(').').

emit_arms([A]) :- emit_arm(A).
emit_arms([A|As]) :- emit_arm(A), write(' ; '), emit_arms(As).

emit_arm(arm(Mode, Guards, Head)) :-
    emit_goals(Guards), arrow(Mode, A), write(A), write(' '), emit_atom(Head).
arrow(level, ' |->').
arrow(edge,  ' |+>').

emit_goals([]).
emit_goals([G]) :- emit_value(G).
emit_goals([G|Gs]) :- emit_value(G), write(', '), emit_goals(Gs).

% --- queries --------------------------------------------------------------

emit_query(query(Atom)) :- write('? '), emit_atom(Atom), write('.').

% --- atom calls -----------------------------------------------------------

emit_atom(call(Name, Args)) :-
    emit_name(Name), write('('),
    ( Args = [] -> true ; emit_goals(Args) ), write(')').

% --- values / expressions (precedence-aware) ------------------------------

emit_value(T) :- emit_expr(T, 0), !.

emit_expr(op(Name, L, R), Outer) :-
    prec(Name, P),
    ( P < Outer -> write('('), emit_op(Name, L, R, P), write(')')
    ; emit_op(Name, L, R, P) ).
emit_expr(T, _) :- emit_atomic(T).

emit_op(Name, L, R, P) :-
    emit_expr(L, P),
    write(' '), write(Name), write(' '),
    Q is P + 1,
    emit_expr(R, Q).

prec(':=', 30).
prec(is, 30).
prec('==', 60). prec('\\==', 60). prec('=:=', 60). prec('=\\=', 60).
prec('=<', 60). prec('<=', 60). prec('>=', 60). prec('>', 60).
prec('<', 60). prec('!=', 60). prec('=', 60).
prec('+', 300). prec('-', 300).
prec('*', 400). prec('/', 400). prec(mod, 400).

emit_atomic(var(Name)) :- emit_name(Name).
emit_atomic(wildcard) :- write('_').
emit_atomic(int(N)) :- write(N).
emit_atomic(float(F)) :- write_float(F).
emit_atomic(neg(A)) :- write('-'), emit_atomic(A).
emit_atomic(atom(Content)) :- write('\''), emit_escaped(0'', Content), write('\'').
emit_atomic(string(Content)) :- write('"'), emit_escaped(0'", Content), write('"').
emit_atomic(list(Items)) :-
    write('['),
    ( Items = [] -> true
      ; emit_list_items(Items) ),
    write(']').
emit_atomic(brace([])) :- write('{}').
emit_atomic(brace(Entries)) :- write('{'), emit_brace_entries(Entries), write('}').
emit_atomic(call(Name, Args)) :-
    emit_name(Name), write('('),
    ( Args = [] -> true ; emit_goals(Args) ), write(')').

emit_list_items([I]) :- emit_list_item(I).
emit_list_items([I|Is]) :- emit_list_item(I), write(', '), emit_list_items(Is).

emit_list_item(spread(E)) :- write('... '), emit_value(E).
emit_list_item(E) :- emit_value(E).

emit_brace_entries([E]) :- emit_brace_entry(E).
emit_brace_entries([E|Es]) :- emit_brace_entry(E), write(', '), emit_brace_entries(Es).

emit_brace_entry(entry(K, V)) :- emit_brace_key(K), write(': '), emit_value(V).
emit_brace_entry(entry(K, V, T)) :-
    emit_brace_key(K), write(': '), emit_value(V), write(': '), emit_decl_arg(T).

emit_brace_key(capture(Name)) :- write('$'), emit_name(Name).
emit_brace_key(descent) :- write('**').
emit_brace_key(qtkey(Content)) :- write('\''), emit_escaped(0'', Content), write('\'').
emit_brace_key(strkey(Content)) :- write('"'), emit_escaped(0'", Content), write('"').
emit_brace_key(key(Name)) :- emit_name(Name).
emit_brace_key(any) :- write('_').

% --- low-level ------------------------------------------------------------

emit_name(Name) :- write(Name).

write_float(F) :- with_output_to(string(S), format('~w', [F])), write(S).

emit_escaped(Delim, Content) :-
    string_codes(Content, Codes),
    emit_esc_codes(Delim, Codes).
emit_esc_codes(_, []).
emit_esc_codes(Delim, [C|Cs]) :-
    ( C =:= 0'\\ -> write('\\\\')
    ; C =:= Delim -> ( Delim =:= 0'' -> write('\\\'') ; write('\\"') )
    ; put_code(C) ),
    emit_esc_codes(Delim, Cs).
