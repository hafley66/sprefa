use_item(Item) -->
    ws,
    ( ~`pub` -> ws, ~`use`, { Visibility = pub_use } ; ~`use`, { Visibility = use } ),
    ws,
    ( string_lit(Text) -> { Target = Text, F = Visibility }
    ; ident(Module), { Target = Module, module_use_functor(Visibility, F) }
    ),
    ws,
    ( ~`as`, ws, ident(Alias)
    -> { Item =.. [F, Target, Alias] }
    ; { Item =.. [F, Target] }
    ),
    ws, [0'.].

module_use_functor(use, use_mod).
module_use_functor(pub_use, pub_use_mod).


% import "spec": attaches an external spec; records its source span (0-based,
% end inclusive, the hover_note convention) via the existing col machinery.
import_stmt(import_decl(File, Line, Col, EndLine, EndCol)) -->
    here(Start), ~`import`, ws, string_lit(File), ws, [0'.], here(End),
    { length(Start, R0), length(End, R1),
      remaining_line_column(R0, L1, C1),
      remaining_line_column(R1, L2, C2),
      Line is L1 - 1, Col is C1 - 1,
      ( L2 == L1 -> EndLine is L1 - 1, EndCol is C2 - 2
      ; EndLine is L2 - 1, EndCol is C2 - 1 ) }.


statement(Kind, Item, Sites) -->
    ws,
    ( removed_world_decl_stmt(Ds)
    -> { Kind = decl_list, Item = Ds, Sites = [] }
    ; interface_stmt(D)
    -> { Kind = decl_list, Item = [D], Sites = [] }
    ; rel_stmt(Ds, Sites)
    -> { Kind = decl_list, Item = Ds }
    ; import_stmt(D)
    -> { Kind = decl_list, Item = [D], Sites = [] }
    ; query_stmt(Q)
    -> { Kind = query, Item = Q, Sites = [] }
    ; ( match_stmt(R) -> [] ; rule_stmt(R) -> [] ),
      { Kind = rule, Sites = [],
        b_getval(dl_vars, Vars), annotate_cst_item(Vars, R, Item) }
    ).


% parameterized nonterminals via call//N; one arity now that dl_vars is global
sep(P, [X | Xs]) -->
    call(P, X), ws,
    ( @`,` -> ws, sep(P, Xs) ; { Xs = [] } ).
args(P, Xs) --> ws, ( peek(0')) -> { Xs = [] } ; sep(P, Xs) ).
#Cs --> ws, @Cs.


