% 12_string_substr_instr.pl : the typed string scalars. substr/instr/length
% mix text and int operands and return mixed results, so they ride the
% typed_scalar registry family rather than text_only text_scalar. Semantics
% are pinned against sqlite3 directly: 1-based positions, negative substr
% start counts from the end, instr misses answer 0.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(substr_positive_start_is_one_based,
  prog([], [ (clipped(Text, Out) <- text(Text), Out := substr(Text, 2)) ]),
  [ text('hello'), text('ab'), text('') ],
  [],
  [ final(clipped/2,
          [ clipped('', ''),
            clipped('ab', 'b'),
            clipped('hello', 'ello') ]) ]).

fixture(substr_negative_start_counts_from_the_end,
  prog([], [ (tail_two(Text, Out) <- text(Text), Out := substr(Text, -2)) ]),
  [ text('hello'), text('ab') ],
  [],
  [ final(tail_two/2,
          [ tail_two('ab', 'ab'),
            tail_two('hello', 'lo') ]) ]).

fixture(substr_with_span_takes_span_characters,
  prog([], [ (middle(Text, Out) <- text(Text), Out := substr(Text, 2, 2)) ]),
  [ text('hello'), text('ab') ],
  [],
  [ final(middle/2,
          [ middle('ab', 'b'),
            middle('hello', 'el') ]) ]).

fixture(instr_answers_first_position_or_zero,
  prog([], [ (found_at(Text, Out) <- text(Text), Out := instr(Text, 'l')) ]),
  [ text('hello'), text('ab') ],
  [],
  [ final(found_at/2,
          [ found_at('ab', 0),
            found_at('hello', 3) ]) ]).

fixture(length_counts_characters,
  prog([], [ (measured(Text, Out) <- text(Text), Out := length(Text)) ]),
  [ text('hello'), text(''), text('é') ],
  [],
  [ final(measured/2,
          [ measured('', 0),
            measured('hello', 5),
            measured('é', 1) ]) ]).

fixture(length_result_joins_a_comparison,
  prog([], [ (long(Text) <- text(Text), Len := length(Text), Len > 3) ]),
  [ text('hello'), text('ab') ],
  [],
  [ final(long/1,
          [ long('hello') ]) ]).

fixture(instr_result_feeds_a_substr_operand,
  prog([], [ (after_underscore(Text, Out) <-
                text(Text), Out := substr(Text, instr(Text, '_') + 1)) ]),
  [ text('emit_types'), text('plain') ],
  [],
  [ final(after_underscore/2,
          [ after_underscore('emit_types', 'types'),
            after_underscore('plain', 'plain') ]) ]).
