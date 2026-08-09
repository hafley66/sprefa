% 9_ordered_aggregates.pl : ordered aggregate surface and tick receipts.

:- op(1150, xfx, <-).

fixture(ordered_json_group_array_value,
  prog([], [ (value_sorted(Group, json_group_array(Value)) <- item(Group, Value)) ]),
  [ item(north, 'é"pear'), item(north, apple), item(north, orange) ],
  [],
  [ final(value_sorted/2,
          [ value_sorted(north, [apple, orange, 'é"pear']) ]) ]).

fixture(ordered_json_group_array_integer_values,
  prog([], [ (integer_sorted(Group, json_group_array(Value)) <- item(Group, Value)) ]),
  [ item(north, 2), item(north, 1) ],
  [],
  [ final(integer_sorted/2, [ integer_sorted(north, [1, 2]) ]) ]).

fixture(ordered_json_group_array_ordinal,
  prog([], [ (ordinal_sorted(Group, json_group_array(Value, Ordinal)) <-
              item(Group, Ordinal, Value)) ]),
  [ item(north, 2, orange), item(north, 1, pear), item(north, 3, apple) ],
  [],
  [ final(ordinal_sorted/2,
          [ ordinal_sorted(north, [pear, orange, apple]) ]) ]).

fixture(ordered_group_concat_value,
  prog([], [ (value_joined(Group, group_concat(Value, " > ")) <-
              item(Group, Value)) ]),
  [ item(north, pear), item(north, orange), item(north, apple) ],
  [],
  [ final(value_joined/2,
          [ value_joined(north, 'apple > orange > pear') ]) ]).

fixture(ordered_group_concat_ordinal,
  prog([], [ (ordinal_joined(Group, group_concat(Value, " > ", Ordinal)) <-
              item(Group, Ordinal, Value)) ]),
  [ item(north, 2, orange), item(north, 1, pear), item(north, 3, apple) ],
  [],
  [ final(ordinal_joined/2,
          [ ordinal_joined(north, 'pear > orange > apple') ]) ]).

fixture(ordered_group_concat_one_argument_defaults_to_comma,
  prog([], [ (simple_joined(Group, group_concat(Value)) <- item(Group, Value)) ]),
  [ item(north, pear), item(north, orange), item(north, apple) ],
  [],
  [ final(simple_joined/2, [ simple_joined(north, 'apple,orange,pear') ]) ]).

% group_concat(X) and group_concat(X, ',') are byte-identical: the one-argument
% spelling defaults its separator to SQLite's own comma.
fixture(ordered_group_concat_explicit_comma_matches_one_argument,
  prog([], [ (explicit_joined(Group, group_concat(Value, ",")) <- item(Group, Value)) ]),
  [ item(north, pear), item(north, orange), item(north, apple) ],
  [],
  [ final(explicit_joined/2, [ explicit_joined(north, 'apple,orange,pear') ]) ]).

fixture(ordered_aggregate_retraction_rebuild,
  prog([], [ (ordered_values(Group, json_group_array(Value, Ordinal)) <-
              item(Group, Ordinal, Value)) ]),
  [ item(north, 1, pear), item(north, 2, orange), item(north, 3, apple) ],
  [ [ -item(north, 1, pear) ],
    [ -item(north, 2, orange), -item(north, 3, apple) ],
    [ +item(north, 4, peach) ] ],
  [ deltas(ordered_values/2,
           [ [ -ordered_values(north, [pear, orange, apple]),
               +ordered_values(north, [orange, apple]) ],
             [ -ordered_values(north, [orange, apple]) ],
             [ +ordered_values(north, [peach]) ] ]),
    final(ordered_values/2, [ ordered_values(north, [peach]) ]) ]).

fixture(ordered_json_group_array_nested_json,
  prog([ col_type(child/2, group, text),
         col_type(child/2, payload, json) ],
       [ (nested(Group, json_group_array(Payload)) <- child(Group, Payload)) ]),
  [ child(north, obj([z-1, a-2])), child(north, obj([z-4, a-3])) ],
  [],
  [ final(nested/2,
          [ nested(north, [obj([a-2, z-1]), obj([a-3, z-4])]) ]) ]).

fixture(ordered_mermaid_line_assembly,
  prog([], [ (mermaid_text(FileName, group_concat(LineText, "\n", LineOrdinal)) <-
              mermaid_line(FileName, LineOrdinal, LineText)) ]),
  [ mermaid_line(chart, 2, '  b'), mermaid_line(chart, 1, 'a') ],
  [],
  [ final(mermaid_text/2, [ mermaid_text(chart, 'a\n  b') ]) ]).

fixture(ordered_fragment_line_assembly,
  prog([], [ (fragment_text(FragmentName, group_concat(LineText, "\n", LineOrdinal)) <-
              fragment_line(FragmentName, LineOrdinal, LineText)) ]),
  [ fragment_line(openapi, 2, '  paths'), fragment_line(openapi, 1, 'openapi: 3.1') ],
  [],
  [ final(fragment_text/2, [ fragment_text(openapi, 'openapi: 3.1\n  paths') ]) ]).

fixture(ordered_group_rels_v5_collect,
  prog([], [ (group_rels(GroupName, json_group_array(RelationName)) <-
              rel_catalog(RelationName, GroupName, _ColumnText, _DocumentationText)) ]),
  [ rel_catalog(alpha, cli, name, docs),
    rel_catalog(beta, cli, stars, docs),
    rel_catalog(gamma, cli, path, docs) ],
  [],
  [ final(group_rels/2, [ group_rels(cli, [alpha, beta, gamma]) ]) ]).

fixture(ordered_group_rels_json_head,
  prog([], [ (group_rels_json(GroupName, json_group_array(RelationName)) <-
              rel_catalog(RelationName, GroupName, _ColumnText, _DocumentationText)) ]),
  [ rel_catalog(alpha, cli, name, docs),
    rel_catalog(beta, cli, stars, docs),
    rel_catalog(gamma, cli, path, docs) ],
  [],
  [ final(group_rels_json/2,
          [ group_rels_json(cli, [alpha, beta, gamma]) ]) ]).
