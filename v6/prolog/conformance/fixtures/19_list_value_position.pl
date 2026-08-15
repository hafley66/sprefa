% 19_list_value_position.pl : a list(T) value in expression position IS the
% surrogate id of a content-interned list.
%
% The elements rest in the minted member rel keyed (list_id, idx); the column
% holds the entity's id; two rules computing the same content share one entity
% row and therefore one id. json is transport, never the resting shape.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(split_value_is_the_interned_list_id,
  prog([col_type(row_text/2, name, text),
        col_type(row_text/2, body, text),
        col_type(row_parts/2, name, text),
        col_type(row_parts/2, parts, list(text))],
       [ (row_parts(Name, Parts) <- row_text(Name, Body), Parts := split(Body, '/')) ]),
  [ row_text(path, 'usr/local/bin') ],
  [],
  [ final(row_parts/2, [ row_parts(path, 1) ]),
    final('__gen__list_text_df210f232c1299bd'/1,
          [ '__gen__list_text_df210f232c1299bd'('["usr","local","bin"]') ]),
    final('__gen__list_text_df210f232c1299bd__member'/3,
          [ '__gen__list_text_df210f232c1299bd__member'(1, 0, usr),
            '__gen__list_text_df210f232c1299bd__member'(1, 1, local),
            '__gen__list_text_df210f232c1299bd__member'(1, 2, bin) ]) ]).

% Content interning is the whole point of the id: two producers that compute
% the same parts name ONE entity row, so equality of two list values is `=`.
fixture(two_producers_share_one_interned_list_id,
  prog([col_type(left_text/2, name, text),
        col_type(left_text/2, body, text),
        col_type(right_text/2, name, text),
        col_type(right_text/2, body, text),
        col_type(left_parts/2, name, text),
        col_type(left_parts/2, parts, list(text)),
        col_type(right_parts/2, name, text),
        col_type(right_parts/2, parts, list(text))],
       [ (left_parts(Name, Parts) <- left_text(Name, Body), Parts := split(Body, ',')),
         (right_parts(Name, Parts) <- right_text(Name, Body), Parts := split(Body, ',')) ]),
  [ left_text(one, 'alpha,beta'), right_text(two, 'alpha,beta') ],
  [],
  [ final(left_parts/2, [ left_parts(one, 1) ]),
    final(right_parts/2, [ right_parts(two, 1) ]),
    final('__gen__list_text_df210f232c1299bd'/1,
          [ '__gen__list_text_df210f232c1299bd'('["alpha","beta"]') ]),
    final('__gen__list_text_df210f232c1299bd__member'/3,
          [ '__gen__list_text_df210f232c1299bd__member'(1, 0, alpha),
            '__gen__list_text_df210f232c1299bd__member'(1, 1, beta) ]) ]).

% The element type survives the spread: `Part` reaches initcap as text, not as
% a json value that would need a re-parse to be one.
fixture(list_element_type_flows_through_spread,
  prog([col_type(symbol/1, name, text),
        col_type(symbol_parts/2, name, text),
        col_type(symbol_parts/2, parts, list(text)),
        col_type(symbol_word/2, name, text),
        col_type(symbol_word/2, word, text)],
       [ (symbol_parts(Name, Parts) <- symbol(Name), Parts := split(Name, '_')),
         (symbol_word(Name, Word) <-
             symbol_parts(Name, Parts), decode(Parts, spread(Part)),
             Word := initcap(Part)) ]),
  [ symbol(emit_rust) ],
  [],
  [ final(symbol_parts/2, [ symbol_parts(emit_rust, 1) ]),
    final(symbol_word/2,
          [ symbol_word(emit_rust, 'Emit'),
            symbol_word(emit_rust, 'Rust') ]),
    final('__gen__list_text_df210f232c1299bd__member'/3,
          [ '__gen__list_text_df210f232c1299bd__member'(1, 0, emit),
            '__gen__list_text_df210f232c1299bd__member'(1, 1, rust) ]) ]).

% An empty separator answers the whole text as one part, unchanged by the id
% plane: the list is one element long and its id is an ordinary surrogate.
fixture(empty_separator_interns_a_one_element_list,
  prog([col_type(line/2, name, text),
        col_type(line/2, body, text),
        col_type(line_parts/2, name, text),
        col_type(line_parts/2, parts, list(text))],
       [ (line_parts(Name, Parts) <- line(Name, Body), Parts := split(Body, '')) ]),
  [ line(word, 'a,b') ],
  [],
  [ final(line_parts/2, [ line_parts(word, 1) ]),
    final('__gen__list_text_df210f232c1299bd'/1,
          [ '__gen__list_text_df210f232c1299bd'('["a,b"]') ]),
    final('__gen__list_text_df210f232c1299bd__member'/3,
          [ '__gen__list_text_df210f232c1299bd__member'(1, 0, 'a,b') ]) ]).
