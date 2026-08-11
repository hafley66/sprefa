:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% An enum variant field may name a declared relation as its type, same as
% any plain column; the read joins the tag rel and the retraction tracks it.
fixture(enum_variant_field_typed_as_rel_is_a_ref,
    prog(
        [type_decl(tree, [col(tree_id, int), col(name, text)]),
         col_type(tree/2, tree_id, int),
         col_type(tree/2, name, text),
         enum_decl(grade, (ripe(subject: tree) ; bruised(reason: text))),
         col_type(graded/2, id, int),
         col_type(graded/2, g, grade),
         col_type(graded_tag/2, id, int),
         col_type(graded_tag/2, tag, text)],
        [(graded_tag(Id, Tag) <- graded(Id, G), grade_tag(G, Tag))]),
    [],
    [
        [+grade_ripe(1, obj([tree_id-1000, name-"oak"]))],
        [+grade_bruised(2, "bruised")],
        [+graded(301, 1)],
        [+graded(302, 2)],
        [-grade_ripe(1, obj([tree_id-1000, name-"oak"]))]
    ],
    [
        final(grade_ripe/2, []),
        final(grade_bruised/2, [grade_bruised(2, "bruised")]),
        final(grade_tag/2, [grade_tag(2, bruised)]),
        final(graded_tag/2, [graded_tag(302, bruised)]),
        deltas(grade_tag/2, [
            [+grade_tag(1, ripe)],
            [+grade_tag(2, bruised)],
            [],
            [],
            [-grade_tag(1, ripe)]
        ]),
        deltas(graded_tag/2, [
            [],
            [],
            [+graded_tag(301, ripe)],
            [+graded_tag(302, bruised)],
            [-graded_tag(301, ripe)]
        ]),
        ticks(5)
    ]).

% An undeclared type name in a variant field stays a named error: no
% type_def is synthesized, so the shared plane throws column_type_unknown.
fixture(enum_variant_field_undeclared_type_still_throws,
    prog(
        [enum_decl(grade, (ripe(subject: treee) ; bruised(reason: text)))],
        []),
    [],
    [],
    [
        throws(column_type_unknown(treee))
    ]).
