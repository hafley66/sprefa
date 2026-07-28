:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(enum_decl_variant_rows_round_trip_through_tag_view,
    prog(
        [enum_decl(body, (page(view:view) ; redirect(to:text)))],
        []),
    [],
    [
        [+body_page(101, 7)],
        [+body_redirect(202, "/next")]
    ],
    [
        final(body_page/2, [body_page(101, 7)]),
        final(body_redirect/2, [body_redirect(202, "/next")]),
        final(body_tag/2, [body_tag(101, page), body_tag(202, redirect)]),
        deltas(body_tag/2, [
            [+body_tag(101, page)],
            [+body_tag(202, redirect)]
        ]),
        ticks(2)
    ]).

fixture(enum_decl_two_variants_union_in_tag_view,
    prog(
        [enum_decl(result, (ok(value:text) ; error(message:text)))],
        []),
    [],
    [
        [+result_ok(301, "ready"), +result_error(302, "failed")]
    ],
    [
        final(result_tag/2, [result_tag(301, ok), result_tag(302, error)]),
        deltas(result_tag/2, [
            [+result_tag(301, ok), +result_tag(302, error)]
        ]),
        ticks(1)
    ]).

fixture(enum_decl_variant_name_collision_is_refused,
    prog(
        [
            enum_decl(body, (page(view:view) ; redirect(to:text))),
            col_type(page/1, id, int)
        ],
        []),
    [],
    [],
    [
        throws(unsupported_construct(enum_variant_name_collision(page)))
    ]).
