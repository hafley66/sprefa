% seq_wire.pl : the surface and hand-desugared grading pair.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(seq_wire_surface,
  prog([ col_type(arrival/1, payload, text),
         col_type(numbered/2, ordinal, int),
         col_type(numbered/2, payload, text),
         kind(numbered/2, log),
         keep(numbered/2, all) ],
       [ (numbered(Ordinal, Payload) <+
             (arrival(Payload), Ordinal := seq('q'))) ]),
  [],
  [ [ +arrival(one) ], [ +arrival(two) ], [ +arrival(three) ] ],
  [ deltas(numbered/2,
           [ [ +numbered(1, one) ],
             [ +numbered(2, two) ],
             [ +numbered(3, three) ],
             [] ]),
    ticks(4) ]).

fixture(seq_wire_hand,
  prog([ col_type(arrival/1, payload, text),
         col_type(numbered/2, ordinal, int),
         col_type(numbered/2, payload, text),
         kind(numbered/2, log),
         keep(numbered/2, all),
         col_type(seq_numbered_1/2, partition, text),
         col_type(seq_numbered_1/2, at, int),
         keyed(seq_numbered_1/2, [1]) ],
       [ (seq_numbered_1('q', 1) <+
             (arrival(Payload), not(seq_numbered_1('q', _)))),
         (seq_numbered_1('q', Advanced) <+
             (arrival(Payload), pre(seq_numbered_1('q', At)),
              Advanced := At + 1)),
         (numbered(1, Payload) <+
             (arrival(Payload), not(seq_numbered_1('q', _)))),
         (numbered(AdvancedOrdinal, Payload) <+
             (arrival(Payload), pre(seq_numbered_1('q', AtHead)),
              AdvancedOrdinal := AtHead + 1)) ]),
  [],
  [ [ +arrival(one) ], [ +arrival(two) ], [ +arrival(three) ] ],
  [ deltas(numbered/2,
           [ [ +numbered(1, one) ],
             [ +numbered(2, two) ],
             [ +numbered(3, three) ],
             [] ]),
    ticks(4) ]).
