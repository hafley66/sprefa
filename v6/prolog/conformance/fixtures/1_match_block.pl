:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(match_classify_response,
    prog(
        [
            col_type(resp_raw/4, endpoint, text),
            col_type(resp_raw/4, status, int),
            col_type(resp_raw/4, tag, text),
            col_type(resp_raw/4, body, text),
            kind(resp_raw/4, log),
            keep(resp_raw/4, all)
        ],
        [
            match(
                resp_raw(Endpoint, Status, Tag, Body),
                (
                    (fetch_result_fresh(Endpoint, Tag, Body) <- Status == 200)
                  ; (fetch_result_unchanged(Endpoint) <- Status == 304)
                  ; (fetch_result_error(Endpoint, Status) <- Status >= 400)
                ))
        ]),
    [],
    [
        [
            +resp_raw(api, 200, etag_1, payload_1),
            +resp_raw(api, 304, etag_1, payload_1),
            +resp_raw(worker, 503, none, unavailable)
        ]
    ],
    [
        final(fetch_result_fresh/3,
              [fetch_result_fresh(api, etag_1, payload_1)]),
        final(fetch_result_unchanged/1,
              [fetch_result_unchanged(api)]),
        final(fetch_result_error/2,
              [fetch_result_error(worker, 503)])
    ]).

fixture(match_classify_response_desugared,
    prog(
        [
            col_type(resp_raw/4, endpoint, text),
            col_type(resp_raw/4, status, int),
            col_type(resp_raw/4, tag, text),
            col_type(resp_raw/4, body, text),
            kind(resp_raw/4, log),
            keep(resp_raw/4, all)
        ],
        [
            (fetch_result_fresh(Endpoint, Tag, Body) <-
                resp_raw(Endpoint, Status, Tag, Body), Status == 200),
            (fetch_result_unchanged(Endpoint) <-
                resp_raw(Endpoint, Status, Tag, Body), Status == 304),
            (fetch_result_error(Endpoint, Status) <-
                resp_raw(Endpoint, Status, Tag, Body), Status >= 400)
        ]),
    [],
    [
        [
            +resp_raw(api, 200, etag_1, payload_1),
            +resp_raw(api, 304, etag_1, payload_1),
            +resp_raw(worker, 503, none, unavailable)
        ]
    ],
    [
        final(fetch_result_fresh/3,
              [fetch_result_fresh(api, etag_1, payload_1)]),
        final(fetch_result_unchanged/1,
              [fetch_result_unchanged(api)]),
        final(fetch_result_error/2,
              [fetch_result_error(worker, 503)])
    ]).

fixture(match_edge_arm_keeps_edge_semantics,
    prog(
        [
            kind(poll_result/2, log),
            keep(poll_result/2, all),
            keyed(cache/2, [1])
        ],
        [
            match(
                poll_result(Key, Value),
                (cache(Key, Value) <+ true))
        ]),
    [],
    [
        [+poll_result(cli, v1)],
        [+poll_result(cli, v2)]
    ],
    [
        final(cache/2, [cache(cli, v2)]),
        deltas(cache/2,
               [[+cache(cli, v1)], [-cache(cli, v1), +cache(cli, v2)], []])
    ]).

fixture(match_enum_nonexhaustive_is_refused,
    prog(
        [enum_decl(body, (page(view:text) ; redirect(to:text)))],
        [
            match(
                decoded_body(Id, Tag, Value),
                (body_page(Id, Value) <+ Tag == page))
        ]),
    [],
    [],
    [
        throws(unsupported_construct(
            match_nonexhaustive(body, redirect)))
    ]).

fixture(keyed_level_head_is_refused,
    prog(
        [keyed(current_value/2, [1])],
        [
            (current_value(Key, Value) <- source_value(Key, Value))
        ]),
    [
        source_value(cli, v1),
        source_value(cli, v2)
    ],
    [],
    [
        throws(keyed_level_head(current_value/2))
    ]).

fixture(keyed_edge_head_still_replaces,
    prog(
        [
            kind(update_value/2, log),
            keep(update_value/2, all),
            keyed(current_value/2, [1])
        ],
        [
            (current_value(Key, Value) <+ update_value(Key, Value))
        ]),
    [],
    [
        [+update_value(cli, v1)],
        [+update_value(cli, v2)]
    ],
    [
        final(current_value/2, [current_value(cli, v2)]),
        deltas(current_value/2,
               [[+current_value(cli, v1)],
                [-current_value(cli, v1), +current_value(cli, v2)],
                []])
    ]).
