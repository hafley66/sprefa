% fixtures/2_hosts_wiring.pl: phase-1 host answers arrive in the schedule.
% sibling folded 2026-08-20 (same throw level_body_goal/json_each):
% ghcacher_json_normalization. See git.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

fixture(ghcacher_host_program_term,
  program(
    [ col_type(watch/1, ep, text),
      col_type(etag/2, ep, text),
      col_type(etag/2, tag, text),
      col_type(interval/2, period, int),
      col_type(interval/2, bucket, int),
      sh_decl(fetch,
              [col(ep, text), col(prev, text), col(bucket, int)],
              [col(status, int), col(tag, text), col(body, json)],
              template(""))
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
          0, repo, "", 200, "t1",
          {full_name: "cli", stargazers_count: 17}),
      +'__host_response_fetch'(
          'witness|fetch|ep:text=pulls|prev:text=|bucket=1',
          0, pulls, "", 200, "t2",
          [ {number: 7, title: "seven", state: "open",
             user: {login: "octo"}},
            {number: 8, title: "eight", state: "closed",
             user: {login: "hub"}}
          ])
    ],
    [ +'__host_response_fetch'(
          'witness|fetch|ep:text=repo|prev:text=|bucket=1',
          0, repo, "", 200, "t1",
          {full_name: "cli", stargazers_count: 17}),
      +'__host_response_fetch'(
          'witness|fetch|ep:text=repo|prev:text=|bucket=1',
          0, repo, "", 200, "t1b",
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
              template(""))
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
          0, d1, q_calls, foo, bar, 0, 3)
    ],
    [ -file(a, d1),
      +file(a, d3),
      +'__host_response_sg'(
          'witness|sg|file_digest:text=d3|query_digest:text=q_calls',
          0, d3, q_calls, foo, zap, 0, 3)
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
              template(""))
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
          0, d1, q_lines, 10, "old")
    ],
    [ -file(a, d1),
      +file(a, d3),
      +'__host_response_span_scan'(
          'witness|span_scan|file_digest:text=d3|query_digest:text=q_lines',
          0, d3, q_lines, 11, "new")
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
              template("")),
      col_type(interval/2, period, int),
      col_type(interval/2, bucket, int)
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

% A host declaring a column the RUNTIME already puts on the same relation.
%
% `ordinal` is the response row index, the column that makes an N-row host
% answer N rows instead of one, and `witness_digest` and `identity_digest`
% are the demand and response keys. All three are generated by
% 1_host_expand.pl:generated_host_decls/7 and filled BY LITERAL NAME by
% v6/tsv2/serve/1_hosts.ts:project(), which tests the runtime names FIRST.
%
% FAIL-FIRST RECEIPT for this exact declaration, before the unsupported construct existed:
% it prepared clean and produced a response relation of ARITY 4 carrying
% THREE column names,
%
%   col __host_response_look/4 . witness_digest : text
%   col __host_response_look/4 . ordinal        : int
%   col __host_response_look/4 . path           : text
%
% where an ordinary host of the same shape carries four. The author's column
% did not clash and lose, it VANISHED: two identical col_type/3 terms folded
% into one while the arity kept the slot. Then project()'s
% `if (column === "ordinal") return ordinal;` would have written the answer's
% row index into it, so the declared output could never carry its own value.
% No error at any door.
fixture(host_output_column_shadows_runtime_ordinal,
  program(
    [ sh_decl(look, [col(path, text)], [col(ordinal, int)],
              template("")) ],
    [ (found(Path, Ordinal) <- probe(look, [Path], [Ordinal], [])) ],
    []),
  [],
  [],
  [ throws(host_column_shadows_runtime(look, output, ordinal)) ]).

% The same law on the INPUT side. An input named witness_digest would have
% been handed the demand's own digest by project()'s first name test rather
% than the value the rule bound.
fixture(host_input_column_shadows_runtime_witness,
  program(
    [ sh_decl(peek, [col(witness_digest, text)], [col(line, text)],
              template("")) ],
    [ (found(Digest, Line) <- probe(peek, [Digest], [Line], [])) ],
    []),
  [],
  [],
  [ throws(host_column_shadows_runtime(peek, input, witness_digest)) ]).

% ── the name IS the key ─────────────────────────────────────────────────────
%
% Two `sh` declarations under one name. FAIL-FIRST: before
% 1_host_expand.pl:no_duplicate_host_names/1 this program compiled CLEAN on
% both doors (ARCH row host_arity_overload_miscompile) -- host_relation_refs/3
% derives __host_demand_look and __host_response_look from the NAME, so the two
% plans share one demand relation and one response relation, and the second
% declaration's `repo` column has nowhere to live at all.
%
% This is the enforcement half of ruling repo_column_spelling =
% distinct_name_hosts: the repo-scoped case gets its own NAME (repo_files,
% repo_files_at, repo_extract), and an author who reaches for the overload
% instead is told why rather than handed a silently merged relation.
fixture(duplicate_host_name_is_refused,
  program(
    [ sh_decl(look, [col(path, text)], [col(line, text)],
              template("")),
      sh_decl(look, [col(repo, text), col(path, text)], [col(line, text)],
              template(""))
    ],
    [ (first(Path, Line) <- probe(look, [Path], [Line], [])) ],
    []),
  [],
  [],
  [ throws(duplicate_host_decl(look)) ]).

% ── `repo` is not a bind column ─────────────────────────────────────────────
%
% RULING sh_bind_surface_removed: an arrival rel that is ALSO rule-headed is
% two sources writing one rel. Before this stop the arrow was silently
% ignored (compile.pl subtracts DerivedRefs from ArrivalTargets), so the
% author's demand plane never existed and nothing said so.
fixture(arrival_rel_with_rule_head_is_refused,
  program(
    [ sh_decl(watch, [col(glob, text)],
              [col(path, text), col(digest, text)], template("")) ],
    [ (watch(Glob, Path, Digest) <-
         seed(Glob, Path, Digest)) ],
    []),
  [],
  [],
  [ throws(host_and_rule_head(watch)) ]).

% One name is one declaration: the probe rewrite used to win silently and the
% declared table was created, writable, and never read (plan risk 2).
fixture(arrival_rel_and_plain_rel_share_name_is_refused,
  program(
    [ col_type(files/3, glob, text),
      col_type(files/3, path, text),
      col_type(files/3, digest, text),
      sh_decl(files, [col(glob, text)],
              [col(path, text), col(digest, text)], template("")) ],
    [ (seen(Path) <- probe(files, [g], [Path, _Digest], [])) ],
    []),
  [],
  [],
  [ throws(host_and_rel_share_name(files)) ]).

