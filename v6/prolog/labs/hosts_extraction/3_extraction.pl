:- module(hosts_extraction_fork,
          [ fork_grade/3,
            changed_file_delta/3,
            extraction_invocations/4,
            extraction_rx_lowering/2,
            worked_extract/4
          ]).

:- use_module(library(lists)).

% Canned extraction results shared by both candidate shapes.
worked_extract(callgraph, d1, q_calls, call(foo, bar, 0, 3)).
worked_extract(callgraph, d2, q_calls, call(baz, qux, 4, 9)).
worked_extract(callgraph, d3, q_calls, call(foo, zap, 0, 3)).
worked_extract(span_line, d1, q_lines, span_line(10, "old")).
worked_extract(span_line, d2, q_lines, span_line(20, "stable")).
worked_extract(span_line, d3, q_lines, span_line(11, "new")).

fork_grade(host, boundary, edb_arrivals).
fork_grade(host, identity, content_addressed(file_digest, query_digest)).
fork_grade(host, salt_sharing, across_rules_and_repos).
fork_grade(host, edge_feed, direct_from_edb_delta).
fork_grade(host, kernel_cost, zero_new_kernel_ops).
fork_grade(host, row_mint, world_boundary).

fork_grade(term_extract, boundary, rows_minted_in_rule).
fork_grade(term_extract, identity, rule_occurrence(file_digest, query_digest)).
fork_grade(term_extract, salt_sharing, via_named_shared_rel_only).
fork_grade(term_extract, edge_feed, via_materialized_level_rel).
fork_grade(term_extract, kernel_cost, decode_class_op).
fork_grade(term_extract, row_mint, rule_evaluation).

% One changed file replaces one old result with one new result for either
% worked extractor. The unchanged file contributes no boundary delta.
changed_file_delta(Kind, Shape,
                   [-OldRow, +NewRow]) :-
    memberchk(Shape, [host, term_extract]),
    query_for(Kind, Query),
    worked_extract(Kind, d1, Query, OldRow),
    worked_extract(Kind, d3, Query, NewRow).

query_for(callgraph, q_calls).
query_for(span_line, q_lines).

% Two rules ask for the same digest/query pair. The host cache deduplicates the
% request key. Two inline op occurrences evaluate independently.
extraction_invocations(host, Digest, Query, Count) :-
    sort([Digest-Query, Digest-Query], Unique),
    length(Unique, Count).
extraction_invocations(term_extract, _, _, 2).

extraction_rx_lowering(
    host,
    "demand$.pipe(groupBy(({fileDigest, queryDigest}) => fileDigest + ':' + queryDigest), mergeMap((group$) => group$.pipe(take(1), mergeMap(runHostExtract))), mergeMap(commitEdbArrival))").
extraction_rx_lowering(
    term_extract,
    "contentDelta$.pipe(mergeMap(({content, query}) => from(extract(content, query))), map(mintRuleRow))").
