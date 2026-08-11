% Text-door spellings for the four list constructors (ruling
% list_flavor_set_v1). Each fixture: a carrier rel whose column carries one
% constructor spelling, a pass-through rule, and arrivals + a retraction.
% list(some_rel) (a rel element) is skipped: the rel-element engine path is
% still refused on main (list_of_relation_refs_still_refused), so only the
% nested spelling list(list(text)) is graded here.

fixture(list_bare_text_door,
  prog([ col_type(box/2, id, int),
         col_type(box/2, items, list(text)),
         keyed(box/2, [1]) ],
       [ (carry(Id, Items) <- box(Id, Items)) ]),
  [],
  [[+box(1, 100)], [-box(1, 100)]],
  [ final(box/2, []), ticks(2) ]).

fixture(list_dense_sequence_text_door,
  prog([ col_type(box/2, id, int),
         col_type(box/2, items, list_entity_dense_sequence(text)),
         keyed(box/2, [1]) ],
       [ (carry(Id, Items) <- box(Id, Items)) ]),
  [],
  [[+box(1, 100)], [-box(1, 100)]],
  [ final(box/2, []), ticks(2) ]).

fixture(list_interned_set_text_door,
  prog([ col_type(box/2, id, int),
         col_type(box/2, items, list_interned_set(text)),
         keyed(box/2, [1]) ],
       [ (carry(Id, Items) <- box(Id, Items)) ]),
  [],
  [[+box(1, 200)], [-box(1, 200)]],
  [ final(box/2, []), ticks(2) ]).

fixture(list_linked_sequence_text_door,
  prog([ col_type(box/2, id, int),
         col_type(box/2, items, list_entity_linked_sequence(text)),
         keyed(box/2, [1]) ],
       [ (carry(Id, Items) <- box(Id, Items)) ]),
  [],
  [[+box(1, 300)], [-box(1, 300)]],
  [ final(box/2, []), ticks(2) ]).

fixture(nested_list_text_door,
  prog([ col_type(grid/2, id, int),
         col_type(grid/2, rows, list(list(text))),
         keyed(grid/2, [1]) ],
       [ (carry(Id, Rows) <- grid(Id, Rows)) ]),
  [],
  [[+grid(1, 400)], [-grid(1, 400)]],
  [ final(grid/2, []), ticks(2) ]).
