:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% A generic list declaration must preserve the author's column order in the
% unrelated struct reference used by tree_label/2.
fixture(declaration_order_preserves_struct_refs,
    prog([ type_decl(plot, [col(row, int), col(col, int)]),
           col_type(plot/2, row, int),
           col_type(plot/2, col, int),
           type_decl(patch, [col(label, text), col(at, plot)]),
           col_type(patch/2, label, text),
           col_type(patch/2, at, plot),
           col_type(tree/3, tree_id, int),
           col_type(tree/3, species, text),
           col_type(tree/3, site, patch),
           col_type(tree_label/2, tree_id, int),
           col_type(tree_label/2, label, text),
           col_type(box_list/2, tree_id, int),
           col_type(box_list/2, items, list(text)) ],
         [ (tree_label(TreeId, Label) <-
                tree(TreeId, _Species, Site),
                decode(Site, {label: Label})) ]),
    [],
    [],
    [ final(tree_label/2, []) ]).
