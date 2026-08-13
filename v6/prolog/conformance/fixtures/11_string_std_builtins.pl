% 11_string_std_builtins.pl : the string standard library rows landed from
% plans/2026-08-12-string-std-builtins.md. Every row is all-text-operand, so
% the Rendering equals the SQLite scalar name and lowers with no glue: empty
% string, non-ASCII, and default-charset cases are the edge assertions.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(upper_folds_ascii_untouches_nonascii,
  prog([], [ (shouted(Text, Out) <- text(Text), Out := upper(Text)) ]),
  [ text('hello'), text('a1b_'), text(''), text('é') ],
  [],
  [ final(shouted/2,
          [ shouted('', ''),
            shouted('a1b_', 'A1B_'),
            shouted('hello', 'HELLO'),
            shouted('é', 'é') ]) ]).

fixture(lower_folds_ascii_untouches_nonascii,
  prog([], [ (mumbled(Text, Out) <- text(Text), Out := lower(Text)) ]),
  [ text('HeLLo'), text('A1B'), text('Éẞ') ],
  [],
  [ final(mumbled/2,
          [ mumbled('A1B', 'a1b'),
            mumbled('HeLLo', 'hello'),
            mumbled('Éẞ', 'Éẞ') ]) ]).

fixture(trim_default_strips_only_space,
  prog([], [ (padded(Text, Out) <- text(Text), Out := trim(Text)) ]),
  [ text('  x  '), text('unpadded'), text('\t x \n') ],
  [],
  [ final(padded/2,
          [ padded('\t x \n', '\t x \n'),
            padded('  x  ', 'x'),
            padded('unpadded', 'unpadded') ]) ]).

fixture(trim_charset_strips_both_ends,
  prog([], [ (curled(Text, Out) <- text(Text), Out := trim(Text, 'x')) ]),
  [ text('xxabxx'), text('ab'), text('xa'), text('bx') ],
  [],
  [ final(curled/2,
          [ curled('ab', 'ab'),
            curled('bx', 'b'),
            curled('xa', 'a'),
            curled('xxabxx', 'ab') ]) ]).

fixture(ltrim_strips_leading_default_and_charset,
  prog([], [ (led(Text, Out) <- text(Text), Out := ltrim(Text, '0')) ]),
  [ text('007'), text('00'), text('abc') ],
  [],
  [ final(led/2,
          [ led('00', ''),
            led('007', '7'),
            led('abc', 'abc') ]) ]).

fixture(rtrim_one_arg_strips_trailing_whitespace,
  prog([], [ (written(Text, Out) <- text(Text), Out := rtrim(Text)) ]),
  [ text('ab  '), text('x'), text('  ') ],
  [],
  [ final(written/2,
          [ written('  ', ''),
            written('ab  ', 'ab'),
            written('x', 'x') ]) ]).

fixture(reverse_reverses_characters,
  prog([], [ (mirrored(Text, Out) <- text(Text), Out := reverse(Text)) ]),
  [ text('abc'), text(''), text('中é') ],
  [],
  [ final(mirrored/2,
          [ mirrored('', ''),
            mirrored('abc', 'cba'),
            mirrored('中é', 'é中') ]) ]).
