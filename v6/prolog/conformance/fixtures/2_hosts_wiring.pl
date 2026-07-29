% fixtures/2_hosts_wiring.pl: phase-1 host answers arrive in the schedule.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

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
  [ current_body(repo, {full_name: cli, stargazers_count: 17}),
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

fixture(ghcacher_host_program_term,
  program(
    [ col_type(watch/1, ep, text),
      col_type(etag/2, ep, text),
      col_type(etag/2, tag, text),
      bind_decl(interval, [col(period, int), col(bucket, int)]),
      sh_decl(fetch,
              [col(ep, text), col(prev, text)],
              [col(status, int), col(tag, text), col(body, json)],
              template("fetch {ep} with $prev"))
    ],
    [ (poll(Ep, Prev, Bucket) <-
         (watch(Ep), etag(Ep, Prev), interval(300, Bucket))),
      (resp(Ep, Bucket, Status, Tag, Body) <-
         (poll(Ep, Prev, Bucket),
          probe(fetch, [Ep, Prev], [Status, Tag, Body],
                [salt(bucket, Bucket)]))),
      (stars(Ep, N) <-
         (resp(Ep, _, 200, _, Body),
          decode(Body, {stargazers_count: N}))),
      (full_name(Ep, Name) <-
         (resp(Ep, _, 200, _, Body),
          decode(Body, {full_name: Name}))),
      (pull_request(Ep, Num, Title, State, Author) <-
         (resp(Ep, _, 200, _, Body),
          json_each(Body, Item),
          decode(Item,
                 {number: Num, title: Title, state: State,
                  user: {login: Author}}))),
      (change_log(Ep, "stars", N) <- stars(Ep, N)),
      (change_log(Ep, "full_name", Name) <- full_name(Ep, Name))
    ],
    [query(change_log(Ep, _Kind, _Value))]),
  [ watch(repo),
    watch(pulls),
    etag(repo, ""),
    etag(pulls, "")
  ],
  [ [+interval(300, 1)],
    [ +'__host_response_fetch'(
          'witness|fetch|ep:text=repo|prev:text=|bucket=1',
          repo, "", 200, "t1",
          {full_name: "cli", stargazers_count: 17}),
      +'__host_response_fetch'(
          'witness|fetch|ep:text=pulls|prev:text=|bucket=1',
          pulls, "", 200, "t2",
          [ {number: 7, title: "seven", state: "open",
             user: {login: "octo"}},
            {number: 8, title: "eight", state: "closed",
             user: {login: "hub"}}
          ])
    ],
    [ +'__host_response_fetch'(
          'witness|fetch|ep:text=repo|prev:text=|bucket=1',
          repo, "", 200, "t1",
          {full_name: "cli", stargazers_count: 17}),
      +'__host_response_fetch'(
          'witness|fetch|ep:text=repo|prev:text=|bucket=1',
          repo, "", 200, "t1b",
          {full_name: "cli", stargazers_count: 18})
    ]
  ],
  [ final(stars/2, [stars(repo, 18)]),
    final(full_name/2, [full_name(repo, "cli")]),
    final(pull_request/5,
          [ pull_request(pulls, 7, "seven", "open", "octo"),
            pull_request(pulls, 8, "eight", "closed", "hub")
          ]),
    final(change_log/3,
          [ change_log(repo, "full_name", "cli"),
            change_log(repo, "stars", 18)
          ]),
    final('__host_demand_fetch'/5,
          [ '__host_demand_fetch'(
                'identity|fetch|ep:text=pulls|prev:text=',
                'witness|fetch|ep:text=pulls|prev:text=|bucket=1',
                pulls, "", 1),
            '__host_demand_fetch'(
                'identity|fetch|ep:text=repo|prev:text=',
                'witness|fetch|ep:text=repo|prev:text=|bucket=1',
                repo, "", 1)
          ])
  ]).

fixture(extraction_fork_callgraph,
  program(
    [ sh_decl(sg,
              [col(file_digest, text), col(query_digest, text)],
              [ col(caller, text), col(callee, text),
                col(start_byte, int), col(end_byte, int)
              ],
              template("sg {file_digest} $query_digest"))
    ],
    [ (call_edge(File, Caller, Callee, Start, End) <-
         (file(File, FileDigest),
          query_value(QueryDigest),
          probe(sg, [FileDigest, QueryDigest],
                [Caller, Callee, Start, End], []))),
      (call_site(File, Caller, Callee) <-
         (file(File, FileDigest),
          query_value(QueryDigest),
          probe(sg, [FileDigest, QueryDigest],
                [Caller, Callee, _, _], [])))
    ],
    [query(call_edge(File, Caller, Callee, Start, End))]),
  [file(a, d1), query_value(q_calls)],
  [ [ +'__host_response_sg'(
          'witness|sg|file_digest:text=d1|query_digest:text=q_calls',
          d1, q_calls, foo, bar, 0, 3)
    ],
    [ -file(a, d1),
      +file(a, d3),
      +'__host_response_sg'(
          'witness|sg|file_digest:text=d3|query_digest:text=q_calls',
          d3, q_calls, foo, zap, 0, 3)
    ]
  ],
  [ final(call_edge/5, [call_edge(a, foo, zap, 0, 3)]),
    final(call_site/3, [call_site(a, foo, zap)]),
    deltas(call_edge/5,
           [ [+call_edge(a, foo, bar, 0, 3)],
             [-call_edge(a, foo, bar, 0, 3),
              +call_edge(a, foo, zap, 0, 3)]
           ]),
    final('__host_demand_sg'/4,
          [ '__host_demand_sg'(
                'identity|sg|file_digest:text=d3|query_digest:text=q_calls',
                'witness|sg|file_digest:text=d3|query_digest:text=q_calls',
                d3, q_calls)
          ])
  ]).

fixture(extraction_fork_span_line,
  program(
    [ sh_decl(span_scan,
              [col(file_digest, text), col(query_digest, text)],
              [col(line, int), col(text, text)],
              template("span {file_digest} $query_digest"))
    ],
    [ (span_line(File, Line, Text) <-
         (file(File, FileDigest),
          query_value(QueryDigest),
          probe(span_scan, [FileDigest, QueryDigest],
                [Line, Text], [])))
    ],
    [query(span_line(File, Line, Text))]),
  [file(a, d1), query_value(q_lines)],
  [ [ +'__host_response_span_scan'(
          'witness|span_scan|file_digest:text=d1|query_digest:text=q_lines',
          d1, q_lines, 10, "old")
    ],
    [ -file(a, d1),
      +file(a, d3),
      +'__host_response_span_scan'(
          'witness|span_scan|file_digest:text=d3|query_digest:text=q_lines',
          d3, q_lines, 11, "new")
    ]
  ],
  [ final(span_line/3, [span_line(a, 11, "new")]),
    deltas(span_line/3,
           [ [+span_line(a, 10, "old")],
             [-span_line(a, 10, "old"),
              +span_line(a, 11, "new")]
           ])
  ]).

fixture(native_ts_query_term,
  program(
    [ sh_decl(tree_sitter,
              [col(file_digest, text), col(query, text)],
              [col(capture, text)],
              template("tree-sitter {file_digest} $query")),
      bind_decl(interval, [col(period, int), col(bucket, int)])
    ],
    [ (query_value(
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
            ])) <- query_source(unit)),
      (captured(Capture) <-
         (file_digest(FileDigest),
          query_value(Query),
          probe(tree_sitter, [FileDigest, Query], [Capture], [])))
    ],
    [query(captured(Capture))]),
  [file_digest(d1), query_source(unit), interval(300, 1)],
  [],
  [ final(captured/1, []),
    final(query_value/1,
          [ query_value(
              "((call_expression function: (identifier) @callee arguments: (arguments [(_) @arg \",\"]+)) (#eq? @callee \"fetch\") (#match? @arg \"^[a-z]+$\"))\n(comment)?\n_*")
          ])
  ]).
