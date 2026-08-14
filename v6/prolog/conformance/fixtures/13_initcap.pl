% 13_initcap.pl : SQL INITCAP over the canonical snake_case naming scheme.
% Word boundary is any non-alnum and the fold is ASCII-only, both matching
% the emitted recursive-CTE rendering. The last fixture is the motivating
% idiom: replace(initcap(x), '_', '') projects snake_case to PascalCase.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(initcap_capitalizes_each_word_and_lowers_the_rest,
  prog([], [ (titled(Text, Out) <- text(Text), Out := initcap(Text)) ]),
  [ text('emit_types'), text('emit_types v2'), text('HELLO'), text('') ],
  [],
  [ final(titled/2,
          [ titled('', ''),
            titled('HELLO', 'Hello'),
            titled('emit_types', 'Emit_Types'),
            titled('emit_types v2', 'Emit_Types V2') ]) ]).

fixture(initcap_replace_projects_snake_to_pascal,
  prog([], [ (pascal_of(Text, Out) <-
                text(Text), Out := replace(initcap(Text), '_', '')) ]),
  [ text('emit_types'), text('rendered_interface'), text('edge') ],
  [],
  [ final(pascal_of/2,
          [ pascal_of('edge', 'Edge'),
            pascal_of('emit_types', 'EmitTypes'),
            pascal_of('rendered_interface', 'RenderedInterface') ]) ]).
