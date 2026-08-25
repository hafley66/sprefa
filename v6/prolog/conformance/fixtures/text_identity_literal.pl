% text_identity_literal.pl : `==` and `\==` on a text a rule COMPUTED, against
% a literal, plus the same literal in a HEAD position. Each fixture arrives its
% rows over ticks: a fixture with an empty schedule writes an empty tick log
% and the Rust door grades it byte-clean without running a rule.
%
% 16_string_affix_tests.pl cannot see either defect. Its affixes are also
% stated as facts, so the id lookup on both sides finds a row and the id
% compare agrees with the character compare by accident.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(text_identity_computed_prefix_equals_unstated_literal,
  prog([col_type(raw/1, text_value, text),
        col_type(quoted/1, text_value, text)],
       [ (quoted(Raw) <-
             raw(Raw), First := substr(Raw, 1, 1), First == '''') ]),
  [ raw('''abc''') ],
  [ [ +raw(plain) ], [ +raw('library(x)') ] ],
  [ deltas(quoted/1, [ [], [] ]),
    final(quoted/1,
          [ quoted('''abc''') ]) ]).

fixture(text_identity_computed_prefix_differs_from_unstated_literal,
  prog([col_type(raw/1, text_value, text),
        col_type(unquoted/1, text_value, text)],
       [ (unquoted(Raw) <-
             raw(Raw), First := substr(Raw, 1, 1), First \== '''') ]),
  [ raw('''abc''') ],
  [ [ +raw(plain) ], [ +raw('library(x)') ] ],
  [ deltas(unquoted/1, [ [ +unquoted(plain) ], [ +unquoted('library(x)') ] ]),
    final(unquoted/1,
          [ unquoted('library(x)'),
            unquoted(plain) ]) ]).

% No fact states '../', and no substr answer below is a stored content either:
% neither side of the compare has a dictionary row to find.
fixture(text_identity_unstated_literal_matches_no_computed_text,
  prog([col_type(raw/1, text_value, text),
        col_type(relative/1, text_value, text)],
       [ (relative(Raw) <-
             raw(Raw), Head := substr(Raw, 1, 3), Head == '../') ]),
  [ raw('''abc''') ],
  [ [ +raw(plain) ], [ +raw('library(x)') ] ],
  [ deltas(relative/1, [ [], [] ]),
    final(relative/1, []) ]).

% The WRITE side of the same literal: an interned head column holds an id, so
% the seed has to store the ONE character the rule spells.
fixture(text_identity_quote_literal_reaches_a_head_column,
  prog([col_type(raw/1, text_value, text),
        col_type(marked/2, text_value, text),
        col_type(marked/2, mark, text)],
       [ (marked(Raw, '''') <- raw(Raw)) ]),
  [ raw(plain) ],
  [ [ +raw('''abc''') ] ],
  [ deltas(marked/2, [ [ +marked('''abc''', '''') ] ]),
    final(marked/2,
          [ marked('''abc''', ''''),
            marked(plain, '''') ]) ]).
