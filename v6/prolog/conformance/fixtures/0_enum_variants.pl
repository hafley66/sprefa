:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(enum_decl_variant_rows_round_trip_through_tag_view,
    prog(
        [enum_decl(body, (page(view:int) ; redirect(to:text)))],
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

% FAIL-PRE-FIX: emitted `PRIMARY KEY ()` and could not boot. Surface text spells
% the arm `none()`; the parser yields the bare atom a term fixture writes.
fixture(enum_nullary_variant_boots_and_tags,
    prog(
        [enum_decl(maybe_text, (none ; some(value:text)))],
        []),
    [],
    [
        [+maybe_text_none(1)],
        [+maybe_text_some(2, "hi")],
        [-maybe_text_some(2, "hi")]
    ],
    [
        final(maybe_text_none/1, [maybe_text_none(1)]),
        final(maybe_text_some/2, []),
        final(maybe_text_tag/2, [maybe_text_tag(1, none)]),
        deltas(maybe_text_tag/2, [
            [+maybe_text_tag(1, none)],
            [+maybe_text_tag(2, some)],
            [-maybe_text_tag(2, some)]
        ]),
        ticks(3)
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

% An enum column carries the instance id, so the variant is read by joining the
% tag rel. picked_tag/2 is the receipt that the reference is READABLE, and the
% retraction tick proves the join tracks the variant leaving.
fixture(enum_name_is_a_column_type,
    prog(
        [enum_decl(grade, (ripe(sugar: int) ; green(days: int))),
         col_type(picked/2, id, int),
         col_type(picked/2, g, grade),
         col_type(picked_tag/2, id, int),
         col_type(picked_tag/2, tag, text)],
        [(picked_tag(Id, Tag) <- picked(Id, G), grade_tag(G, Tag))]),
    [],
    [
        [+grade_ripe(401, 12)],
        [+picked(101, 401)],
        [-grade_ripe(401, 12)]
    ],
    [
        final(grade_tag/2, []),
        final(picked/2, [picked(101, 401)]),
        final(picked_tag/2, []),
        deltas(picked_tag/2, [
            [],
            [+picked_tag(101, ripe)],
            [-picked_tag(101, ripe)]
        ]),
        ticks(3)
    ]).

% FAIL-PRE-FIX: expansion stamped keyed on every variant rel, so deriving a
% variant from a level rule threw keyed_level_head(review_done/2). Sabotage:
% restore the unconditional keyed mint in 0_enum_expand.pl expand_variant/5
% and this fixture refuses to compile again.
fixture(enum_variant_derived_by_level_rule,
    prog(
        [ enum_decl(review, (pending ; done(verdict:text))),
          col_type(submission/2, id, int),
          col_type(submission/2, verdict, text),
          keyed(submission/2, [1]) ],
        [ (review_done(SubmissionId, Verdict) <-
               submission(SubmissionId, Verdict)) ]),
    [],
    [
        [+submission(1, "ship")],
        [+submission(2, "hold")]
    ],
    [
        final(review_done/2, [review_done(1, "ship"), review_done(2, "hold")]),
        final(review_tag/2, [review_tag(1, done), review_tag(2, done)]),
        ticks(2)
    ]).

% The enum origin-row pass runs late on the completed program in BOTH doors.
% Sabotage: wire merge_enum_type_rows/3 into one door only and the twin plunit
% test both_doors_mint_the_same_enum_origin_rows goes red while this stays green.
fixture(enum_variants_run_unchanged_under_the_origin_row_pass,
    prog(
        [enum_decl(shape, (circle(radius:int) ; square(side:int)))],
        []),
    [],
    [
        [+shape_circle(11, 3)],
        [+shape_square(12, 4)]
    ],
    [
        final(shape_circle/2, [shape_circle(11, 3)]),
        final(shape_square/2, [shape_square(12, 4)]),
        final(shape_tag/2, [shape_tag(11, circle), shape_tag(12, square)]),
        deltas(shape_tag/2, [
            [+shape_tag(11, circle)],
            [+shape_tag(12, square)]
        ]),
        ticks(2)
    ]).
