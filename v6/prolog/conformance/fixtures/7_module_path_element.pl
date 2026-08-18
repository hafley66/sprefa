% fixtures/7_module_path_element.pl : a module path in ELEMENT position, i.e.
% inside list(...) / option(...) / json_list(...).
%
% A dotted path in COLUMN position already resolved
% (module_path_and_option_column_coexist), and a dotted path in FUNCTOR
% position already resolved (7_module_path_wrapper.pl). The element slot of a
% type wrapper was the one place left where the same spelling did not: generic
% expansion (1_expansion.pl phase 5) mints a list artifact NAME from the
% element type, and it ran before the dot phase (order 44), so it met an
% unresolved type_path/1 and 0_generic_expand.pl:canonical_type_encoding/2 had
% no clause for the empty list inside it and simply FAILED.
%
% FAIL-FIRST, compile_dl6/2 on .dl6 text, before 1_expansion.pl ran
% dot_expand:resolve_qualified_types/2 ahead of the fold:
%
%   rel orchard.tree(name: text, url: text).
%   rel grove(id: int, trees: list(orchard.tree)) key(1).
%     -> exit 1, ZERO output on stdout and stderr
%        (swipl -q prints nothing for a failed -g goal)
%
%   rel grove(id: int, tree: option(orchard.tree)) key(1).
%     -> exit 2, unsupported_construct: option_element_type_unknown
%
%   rel grove(id: int, trees: list(list(orchard.tree))) key(1).
%     -> exit 1, ZERO output
%
% The same programs spelled with a same-module `tree` all compiled rc=0.
%
% json_list is the fourth fixture and is the PARITY case, not a widening: a rel
% element inside the json carrier is a decided stop (0_type_plane.pl:125), and
% the dotted spelling hits the same one the bare `json_list(tree)` hits. Its
% unsupported construct names `orchard__tree`, which proves the path resolved
% before the stop.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ list(orchard.tree) ═════════════════════════════════════════════════════
% rx: members$ = listMember$.pipe(filter(m => m.listId === id), map(m => m.value))
% over the SAME minted member rel the bare `list(tree)` spelling mints.
fixture(module_path_list_element_round_trips,
  prog([ col_type(orchard__tree/2, name, text),
         col_type(orchard__tree/2, url, text),
         rel_path_decl(orchard__tree/2, [orchard, tree]),
         col_type(grove/2, id, int),
         col_type(grove/2, members, list(type_path([orchard, tree]))),
         keyed(grove/2, [1]) ],
       []),
  [],
  [ [ +grove(1, 100),
      +'__gen__list_orchard__tree_5aa19b249953237f'('[{"name":"ada","url":"ada.io"},{"name":"bo","url":"bo.io"}]'),
      +'__gen__list_orchard__tree_5aa19b249953237f__member'(100, 0, obj([name-ada, url-'ada.io'])),
      +'__gen__list_orchard__tree_5aa19b249953237f__member'(100, 1, obj([name-bo, url-'bo.io'])) ] ],
  [ final(grove/2, [ grove(1, [ obj([name-ada, url-'ada.io']),
                                obj([name-bo, url-'bo.io']) ]) ]),
    final('__gen__list_orchard__tree_5aa19b249953237f__member'/3,
          ['__gen__list_orchard__tree_5aa19b249953237f__member'(100, 0, obj([name-ada, url-'ada.io'])),
           '__gen__list_orchard__tree_5aa19b249953237f__member'(100, 1, obj([name-bo, url-'bo.io']))]),
    final(orchard__tree/2,
          [ orchard__tree(ada, 'ada.io'), orchard__tree(bo, 'bo.io') ]),
    ticks(1) ]).

% ═══ option(orchard.tree) ═══════════════════════════════════════════════════
% The option desugars to a companion split rel holding the endpoint id, so an
% absent tree is a missing companion row. Grove 1 is present, grove 2 absent.
% rx: tree$ = grove$.pipe(switchMap(g => companion$.pipe(
%       filter(c => c.groveId === g.id), defaultIfEmpty(none))))
fixture(module_path_option_element_round_trips,
  prog([ col_type(orchard__tree/2, id, int),
         col_type(orchard__tree/2, name, text),
         rel_path_decl(orchard__tree/2, [orchard, tree]),
         keyed(orchard__tree/2, [1]),
         col_type(grove/2, id, int),
         col_type(grove/2, tree, option(type_path([orchard, tree]))),
         keyed(grove/2, [1]),
         col_type(planted/2, grove_id, int),
         col_type(planted/2, tree_name, text) ],
       [ (planted(GroveId, TreeName) <-
              grove__tree(GroveId, TreeId),
              rel_path([orchard, tree], [TreeId, TreeName])) ]),
  [],
  [ [ +orchard__tree(7, "ada"), +grove(1), +grove(2) ],
    [ +grove__tree(1, 7) ] ],
  [ final(grove/1, [ grove(1), grove(2) ]),
    final(grove__tree/2, [ grove__tree(1, 7) ]),
    final(planted/2, [ planted(1, "ada") ]),
    ticks(2) ]).

% ═══ list(list(orchard.tree)) ═══════════════════════════════════════════════
% The outer member's value column is itself a list, so the discovery fixpoint
% has to mint BOTH levels off one resolved element name.
% rx: the outer member stream carries the inner list id, one flatMap per level.
fixture(module_path_nested_list_element_round_trips,
  prog([ col_type(orchard__tree/2, name, text),
         col_type(orchard__tree/2, url, text),
         rel_path_decl(orchard__tree/2, [orchard, tree]),
         col_type(grove/2, id, int),
         col_type(grove/2, members, list(list(type_path([orchard, tree])))),
         keyed(grove/2, [1]) ],
       []),
  [],
  [ [ +grove(1, 900),
      +'__gen__list_list_orchard__tree_5d13be82b434dc70'('[[{"name":"ada","url":"ada.io"}]]'),
      +'__gen__list_list_orchard__tree_5d13be82b434dc70__member'(900, 0, 100),
      +'__gen__list_orchard__tree_5aa19b249953237f'('[{"name":"ada","url":"ada.io"}]'),
      +'__gen__list_orchard__tree_5aa19b249953237f__member'(100, 0, obj([name-ada, url-'ada.io'])) ] ],
  [ final(grove/2, [ grove(1, [ [obj([name-ada, url-'ada.io'])] ]) ]),
    final('__gen__list_list_orchard__tree_5d13be82b434dc70__member'/3,
          ['__gen__list_list_orchard__tree_5d13be82b434dc70__member'(900, 0,
              [obj([name-ada, url-'ada.io'])])]),
    final('__gen__list_orchard__tree_5aa19b249953237f__member'/3,
          ['__gen__list_orchard__tree_5aa19b249953237f__member'(100, 0, obj([name-ada, url-'ada.io']))]),
    final(orchard__tree/2, [ orchard__tree(ada, 'ada.io') ]),
    ticks(1) ]).

% ═══ json_list(orchard.tree) ════════════════════════════════════════════════
% Parity with the same-module spelling: both stop at list_element_not_scalar.
% The oracle carries the document through untouched; the compiler stops, and
% names the RESOLVED rel.
% rx: the column is one string in one row, identity in and out.
fixture(module_path_json_list_element_keeps_its_stop,
  prog([ col_type(orchard__tree/2, name, text),
         col_type(orchard__tree/2, url, text),
         rel_path_decl(orchard__tree/2, [orchard, tree]),
         col_type(grove/2, id, int),
         col_type(grove/2, trees, json_list(type_path([orchard, tree]))) ],
       []),
  [ grove(1, [obj([name-ada, url-'ada.io'])]) ],
  [],
  [ final(grove/2, [ grove(1, [obj([name-ada, url-'ada.io'])]) ]) ]).
