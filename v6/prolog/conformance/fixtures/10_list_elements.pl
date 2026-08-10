% Fixtures for the list-element widening arc.
%
% Each fixture states why it exists.
%   list_of_json_documents_round_trips    json_list(json) is spellable and survives a tick
%   nested_list_of_text_round_trips       json_list(json_list(text)) compiles, stores, renders byte-identically
%   non_array_value_at_list_column_is_refused   the arrival gate fires field_not_array
%   wrong_element_type_is_refused         the element guard fires; without it the column is untyped
%   list_of_relation_refs_still_refused   the identity law did not move; the compiler refuses it

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
