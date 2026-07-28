:- module(hosts_extraction_terms,
          [ fixture/5,
            ghcacher_program/1,
            fetch_template/1,
            fetch_explicit_decl/1,
            fetch_inferred_decl/1,
            native_ts_query/1,
            ast_grep_example/1
          ]).

:- op(1150, xfx, <-).
:- discontiguous fixture/5.

fetch_template("template with {ep} and $prev").

fetch_explicit_decl(
    sh_decl(fetch,
            [col(ep, text), col(prev, text)],
            [col(status, int), col(tag, text), col(body, text)],
            template(Template))) :-
    fetch_template(Template).

fetch_inferred_decl(
    sh_decl_inferred(fetch,
                     [ col(ep, text), col(prev, text), col(status, int),
                       col(tag, text), col(body, text)
                     ],
                     template(Template))) :-
    fetch_template(Template).

% The worked ghcacher program uses only term shapes owned by this lab. The
% follow-up compiler arc can translate this program/3 value into its AST.
ghcacher_program(
  program(
    [ rel_decl(watch, [col(ep, text)]),
      rel_decl(interval, [col(period, int), col(bucket, int)]),
      bind_decl(interval, [col(period, int), col(bucket, int)]),
      rel_decl(etag, [col(ep, text), col(tag, text)]),
      rel_decl(poll, [col(ep, text), col(prev, text), col(bucket, int)]),
      rel_decl(resp, [ col(ep, text), col(bucket, int), col(status, int),
                       col(tag, text), col(body, text) ]),
      rel_decl(stars, [col(ep, text), col(n, int)]),
      rel_decl(full_name, [col(ep, text), col(name, text)]),
      rel_decl(pull_request,
               [ col(ep, text), col(num, int), col(title, text),
                 col(state, text), col(author, text) ]),
      rel_decl(change_log,
               [col(ep, text), col(kind, text), col(value, text)]),
      sh_decl(fetch,
              [col(ep, text), col(prev, text)],
              [col(status, int), col(tag, text), col(body, text)],
              template("template with {ep} and $prev"))
    ],
    [ fact(watch("repos/cli/cli")),
      fact(etag("repos/cli/cli", "")),
      rule(poll(Ep, Prev, Bucket),
           [watch(Ep), etag(Ep, Prev), interval(300, Bucket)]),
      rule(resp(Ep, Bucket, Status, Tag, Body),
           [ poll(Ep, Prev, Bucket),
             probe(fetch,
                   [Ep, Prev],
                   [Status, Tag, Body],
                   [salt(bucket, Bucket)])
           ]),
      rule(stars(Ep, N),
           [resp(Ep, _, 200, _, Body),
            decode(Body, obj([stargazers_count-N]))]),
      rule(full_name(Ep, Name),
           [resp(Ep, _, 200, _, Body),
            decode(Body, obj([full_name-Name]))]),
      rule(pull_request(Ep, Num, Title, State, Author),
           [ resp(Ep, _, 200, _, Body),
             json_each(Body, Item),
             decode(Item,
                    obj([ number-Num, state-State, title-Title,
                          user-obj([login-Author]) ]))
           ]),
      rule(change_log(Ep, "stars", N), [stars(Ep, N)]),
      rule(change_log(Ep, "full_name", Name), [full_name(Ep, Name)])
    ],
    [query(change_log(Ep, _Kind, _Value))])).

% This candidate runs directly against the landed conformance decode/json_each
% implementation. One array element is decoded once, so sibling and nested
% fields remain correlated.
fixture(ghcacher_json_normalization,
  prog([],
       [ (stars(Ep, N) <-
            (current_body(Ep, Body),
             decode(Body, {stargazers_count: N}))),
         (pull_request(Ep, Num, Title, State, Author) <-
            (current_body(Ep, Body),
             json_each(Body, Item),
             decode(Item,
                    {number: Num, title: Title, state: State,
                     user: {login: Author}})))
       ]),
  [ current_body(repo,
                 {full_name: cli, stargazers_count: 17}),
    current_body(pulls,
                 [ {number: 7, title: "seven", state: "open",
                    user: {login: "octo"}},
                   {number: 8, title: "eight", state: "closed",
                    user: {login: "hub"}}
                 ])
  ],
  [],
  [ final(stars/2, [stars(repo, 17)]),
    final(pull_request/5,
          [ pull_request(pulls, 7, "seven", "open", "octo"),
            pull_request(pulls, 8, "eight", "closed", "hub")
          ])
  ]).

% Candidate term for the complete tree-sitter feature receipt.
native_ts_query(
  ts_query(
    [ group(
        node(call_expression,
             [ field(function,
                     capture(callee, node(identifier, []))),
               field(arguments,
                     node(arguments,
                          [ quant(one_or_more,
                                  alternative(
                                    [ capture(arg, named_wildcard),
                                      anonymous(",")
                                    ]))
                          ]))
             ]),
        [ predicate(eq, capture_ref(callee), string("fetch")),
          predicate(match, capture_ref(arg), string("^[a-z]+$"))
        ]),
      quant(optional, node(comment, [])),
      quant(zero_or_more, wildcard)
    ])).

ast_grep_example(
    sg_pattern(language(rust),
               source("$RECEIVER.unwrap()"),
               captures([receiver]))).

% The remaining fixture/5 values are follow-up wiring candidates. They keep
% the conformance tuple shape while their expectations name lab-model checks.
fixture(ghcacher_host_program_term,
        Program,
        [world(watch("repos/cli/cli"))],
        [emit(interval(300, 1)), answer(fetch, [200, "t1", json_body])],
        [model_check(full_program_compiles), model_check(content_addressed)]) :-
    ghcacher_program(Program).

fixture(extraction_fork_callgraph,
        prog_candidate(callgraph_sg),
        [file(a, d1), file(b, d2)],
        [replace(file(a, d1), file(a, d3))],
        [model_check(equal_boundary_delta_size),
         model_check(host_shares_salt_across_rules)]).

fixture(extraction_fork_span_line,
        prog_candidate(span_line_scan),
        [file(a, d1)],
        [replace(file(a, d1), file(a, d3))],
        [model_check(equal_boundary_delta_size),
         model_check(term_needs_named_shared_rel)]).

fixture(native_ts_query_term,
        Query,
        [],
        [],
        [model_check(feature_complete), model_check(compiles_exactly)]) :-
    native_ts_query(Query).
