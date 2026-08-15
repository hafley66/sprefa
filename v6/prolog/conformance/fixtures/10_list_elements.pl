% Fixtures for the list-element widening arc.
%
% Each fixture states why it exists.
%   list_of_json_documents_round_trips    json_list(json) is spellable and survives a tick
%   nested_list_of_text_round_trips       json_list(json_list(text)) compiles, stores, renders byte-identically
%   non_array_value_at_list_column_is_refused   the arrival gate fires field_not_array
%   wrong_element_type_is_refused         the element guard fires; without it the column is untyped
%   list_of_relation_refs_still_refused   the identity law did not move; the compiler refuses it
%   rel_element_list_round_trips          a rel type as the relational list(T) element: the minted
%                                         member value column is the rel type, arrivals decompose
%                                         each element into its own rel, and a member retraction
%                                         leaves the fighter row in place
%   nested_rel_element_list_round_trips   list(list(fighter_summary)) mints both levels through the
%                                         finding-2 discovery fixpoint
%   list_interned_set_relation_element_refused  the interned-set value dictionary is redundant for a
%                                         rel element (the rel row already interns it); named, not
%                                         forced, by the generic expansion

fixture(list_of_json_documents_round_trips,
  prog([ col_type(batch/2, id, int),
         col_type(batch/2, payloads, json_list(json)) ],
       [ (carry(Id, Payloads) <- batch(Id, Payloads)) ]),
  [ batch(1, [obj([a-1]), 42]) ],
  [],
  [ final(carry/2, [ carry(1, [obj([a-1]), 42]) ]) ]).

fixture(nested_list_of_text_round_trips,
  prog([ col_type(grid/2, id, int),
         col_type(grid/2, rows, json_list(json_list(text))) ],
       [ (carry(Id, Rows) <- grid(Id, Rows)) ]),
  [ grid(1, [[a, b], [c]]) ],
  [],
  [ final(carry/2, [ carry(1, [[a, b], [c]]) ]) ]).

fixture(non_array_value_at_list_column_is_refused,
  prog([ col_type(batch/2, id, int),
         col_type(batch/2, payloads, json_list(json)) ],
       []),
  [ batch(1, 42) ],
  [],
  [ throws(type_arrival_shape_mismatch(batch/2, payloads, json_list(json),
                                       field_not_array(42))) ]).

fixture(wrong_element_type_is_refused,
  prog([ col_type(batch/2, id, int),
         col_type(batch/2, payloads, json_list(text)) ],
       []),
  [ batch(1, [alpha, 42]) ],
  [],
  [ throws(type_arrival_shape_mismatch(
             batch/2, payloads, json_list(text),
             list_element_shape(2, field_not_text(42)))) ]).

fixture(list_of_relation_refs_still_refused,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(holder/1, item, span),
         col_type(doc/2, id, int),
         col_type(doc/2, spans, json_list(span)) ],
       []),
  [ doc(1, [obj([start-1, end-2])]) ],
  [],
  [ final(doc/2, [ doc(1, [obj([start-1, end-2])]) ]) ]).

% A rel type as the relational list(T) element: the minted member value column
% is the rel type, arrivals post each element into its own rel, and a member
% retraction leaves the fighter row in place (the shared-child law).
fixture(rel_element_list_round_trips,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         col_type(squad/2, id, int),
         col_type(squad/2, members, list(fighter_summary)),
         keyed(squad/2, [1]) ],
       []),
  [],
  [ [ +squad(1, 100),
      +'__gen__list_fighter_summary_b424a4b49951eef7'('[{"name":"ada","url":"ada.io"},{"name":"bo","url":"bo.io"}]'),
      +'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 0, obj([name-ada, url-'ada.io'])),
      +'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 1, obj([name-bo, url-'bo.io'])) ],
    [ +'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 2, obj([name-cat, url-'cat.io'])) ],
    [ -'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 1, obj([name-bo, url-'bo.io'])) ] ],
  [ final(squad/2, [ squad(1, [ obj([name-ada, url-'ada.io']),
                                obj([name-cat, url-'cat.io']) ]) ]),
    final('__gen__list_fighter_summary_b424a4b49951eef7'/1,
          ['__gen__list_fighter_summary_b424a4b49951eef7'('[{"name":"ada","url":"ada.io"},{"name":"bo","url":"bo.io"}]')]),
    final('__gen__list_fighter_summary_b424a4b49951eef7__member'/3,
          ['__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 0, obj([name-ada, url-'ada.io'])),
           '__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 2, obj([name-cat, url-'cat.io']))]),
    final(fighter_summary/2,
          [ fighter_summary(ada, 'ada.io'),
            fighter_summary(bo, 'bo.io'),
            fighter_summary(cat, 'cat.io') ]),
    ticks(3) ]).

% Nesting: the outer member's value column is itself a list, so the finding-2
% fixpoint mints the inner list(fighter_summary) as well.
fixture(nested_rel_element_list_round_trips,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         col_type(squad/2, id, int),
         col_type(squad/2, members, list(list(fighter_summary))),
         keyed(squad/2, [1]) ],
       []),
  [],
  [ [ +squad(1, 900),
      +'__gen__list_list_fighter_summary_de80754d44b67a64'('[[{"name":"ada","url":"ada.io"}]]'),
      +'__gen__list_list_fighter_summary_de80754d44b67a64__member'(900, 0, 100),
      +'__gen__list_fighter_summary_b424a4b49951eef7'('[{"name":"ada","url":"ada.io"}]'),
      +'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 0, obj([name-ada, url-'ada.io'])) ] ],
  [ final(squad/2, [ squad(1, [ [obj([name-ada, url-'ada.io'])] ]) ]),
    final('__gen__list_list_fighter_summary_de80754d44b67a64'/1,
          ['__gen__list_list_fighter_summary_de80754d44b67a64'('[[{"name":"ada","url":"ada.io"}]]')]),
    final('__gen__list_list_fighter_summary_de80754d44b67a64__member'/3,
          ['__gen__list_list_fighter_summary_de80754d44b67a64__member'(900, 0,
              [obj([name-ada, url-'ada.io'])])]),
    final('__gen__list_fighter_summary_b424a4b49951eef7'/1,
          ['__gen__list_fighter_summary_b424a4b49951eef7'('[{"name":"ada","url":"ada.io"}]')]),
    final('__gen__list_fighter_summary_b424a4b49951eef7__member'/3,
          ['__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 0, obj([name-ada, url-'ada.io']))]),
    final(fighter_summary/2, [ fighter_summary(ada, 'ada.io') ]),
    ticks(1) ]).

% The interned-set value dictionary keys on scalar content; a rel element is
% already interned by its own rel row.  Named by generic expansion.
fixture(list_interned_set_relation_element_refused,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         col_type(squad/2, id, int),
         col_type(squad/2, members, list_interned_set(fighter_summary)),
         keyed(squad/2, [1]) ],
       []),
  [],
  [],
  [ throws(unsupported_construct(
             list_interned_set_relation_element(fighter_summary))) ]).
