% 15_string_split.pl : split/2, the multi-row string primitive.
%
% split does NOT introduce a table-valued construct. It returns the json array
% carrier that `decode(Parts, [... Part])` already fans out, so the multi-row
% half is the spread that json_arm.pl:159 already grades and the scalar half
% is one typed_scalar row. The two-rule idiom is the whole surface: one rule
% binds the array into a json_list(text) column, the next rule spreads it.
%
% Semantics are JS String.prototype.split with a non-empty separator, probed
% against sqlite3 3.43.2: N separators give N+1 parts, so an empty text gives
% one empty part, a trailing separator gives a trailing empty part, and a text
% with no separator gives itself. An EMPTY separator does not walk characters;
% it answers the whole text as a single part, because a character walk would
% make the separator scan non-terminating.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(split_parts_carry_as_a_json_array,
  prog([col_type(line/2, name, text),
        col_type(line/2, body, text),
        col_type(line_parts/2, name, text),
        col_type(line_parts/2, parts, json_list(text))],
       [ (line_parts(Name, Parts) <- line(Name, Body), Parts := split(Body, ',')) ]),
  [ line(header, 'id,name,age'), line(single, 'solo') ],
  [],
  [ final(line_parts/2,
          [ line_parts(header, [id, name, age]),
            line_parts(single, [solo]) ]) ]).

fixture(split_fans_out_through_spread,
  prog([col_type(line/2, name, text),
        col_type(line/2, body, text),
        col_type(line_parts/2, name, text),
        col_type(line_parts/2, parts, json_list(text)),
        col_type(field/2, name, text),
        col_type(field/2, field, text)],
       [ (line_parts(Name, Parts) <- line(Name, Body), Parts := split(Body, ',')),
         (field(Name, Field) <- line_parts(Name, Parts), decode(Parts, spread(Field))) ]),
  [ line(header, 'id,name,age'), line(single, 'solo') ],
  [],
  [ final(field/2,
          [ field(header, age),
            field(header, id),
            field(header, name),
            field(single, solo) ]) ]).

% N separators, N+1 parts: the empty text is one empty part and a trailing
% separator leaves a trailing empty part. Neither is dropped.
fixture(split_keeps_empty_parts,
  prog([col_type(line/2, name, text),
        col_type(line/2, body, text),
        col_type(line_parts/2, name, text),
        col_type(line_parts/2, parts, json_list(text))],
       [ (line_parts(Name, Parts) <- line(Name, Body), Parts := split(Body, ',')) ]),
  [ line(blank, ''), line(trailing, 'a,'), line(inner, 'a,,b') ],
  [],
  [ final(line_parts/2,
          [ line_parts(blank, ['']),
            line_parts(inner, [a, '', b]),
            line_parts(trailing, [a, '']) ]) ]).

fixture(split_takes_a_multi_character_separator,
  prog([col_type(line/2, name, text),
        col_type(line/2, body, text),
        col_type(line_parts/2, name, text),
        col_type(line_parts/2, parts, json_list(text))],
       [ (line_parts(Name, Parts) <- line(Name, Body), Parts := split(Body, '::')) ]),
  [ line(path, 'std::io::Read'), line(bare, 'plain') ],
  [],
  [ final(line_parts/2,
          [ line_parts(bare, [plain]),
            line_parts(path, [std, io, 'Read']) ]) ]).

% substr/length are character based, so split must be too: a multi-byte part
% survives whole rather than splitting at a continuation byte.
fixture(split_parts_keep_non_ascii_characters,
  prog([col_type(line/2, name, text),
        col_type(line/2, body, text),
        col_type(line_parts/2, name, text),
        col_type(line_parts/2, parts, json_list(text))],
       [ (line_parts(Name, Parts) <- line(Name, Body), Parts := split(Body, ',')) ]),
  [ line(accented, 'café,naïve') ],
  [],
  [ final(line_parts/2,
          [ line_parts(accented, ['café', 'naïve']) ]) ]).

% An empty separator has no scan position to advance past, so it answers the
% whole text as one part rather than walking characters.
fixture(split_on_an_empty_separator_answers_the_whole_text,
  prog([col_type(line/2, name, text),
        col_type(line/2, body, text),
        col_type(line_parts/2, name, text),
        col_type(line_parts/2, parts, json_list(text))],
       [ (line_parts(Name, Parts) <- line(Name, Body), Parts := split(Body, '')) ]),
  [ line(word, 'abc'), line(blank, '') ],
  [],
  [ final(line_parts/2,
          [ line_parts(blank, ['']),
            line_parts(word, [abc]) ]) ]).

% The renderer shape this arc exists for: a snake_case name splits, each part
% initcaps, and the parts fold back through an ordered group_concat. This is
% the PascalCase idiom spelled end to end with landed primitives only.
fixture(split_initcap_and_fold_render_pascal_case,
  prog([col_type(sym/1, name, text),
        col_type(sym_parts/2, name, text),
        col_type(sym_parts/2, parts, json_list(text)),
        col_type(sym_word/2, name, text),
        col_type(sym_word/2, word, text),
        col_type(pascal/2, name, text),
        col_type(pascal/2, rendered, text)],
       [ (sym_parts(Name, Parts) <- sym(Name), Parts := split(Name, '_')),
         (sym_word(Name, Word) <-
             sym_parts(Name, Parts), decode(Parts, spread(Part)),
             Word := initcap(Part)),
         (pascal(Name, group_concat(Word, '')) <- sym_word(Name, Word)) ]),
  [ sym(emit_types), sym(plain) ],
  [],
  [ final(pascal/2,
          [ pascal(emit_types, 'EmitTypes'),
            pascal(plain, 'Plain') ]) ]).
