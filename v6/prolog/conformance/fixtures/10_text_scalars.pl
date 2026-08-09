% 10_text_scalars.pl : the str-stratum minimum, rtrim/2 and replace/3, the
% dirname idiom Dir := rtrim(Path, replace(Path, '/', '')), no UDF, no split.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(rtrim_strips_trailing_chars,
  prog([], [ (trimmed(Path, Out) <- path(Path), Out := rtrim(Path, '/')) ]),
  [ path('src/'), path('a/b/'), path('repo/src') ],
  [],
  [ final(trimmed/2,
          [ trimmed('a/b/', 'a/b'),
            trimmed('repo/src', 'repo/src'),
            trimmed('src/', 'src') ]) ]).

fixture(replace_rewrites_all_occurrences,
  prog([], [ (slashed(Text, Out) <- text(Text), Out := replace(Text, '/', '_')) ]),
  [ text('a/b/c'), text('no_slash') ],
  [],
  [ final(slashed/2,
          [ slashed('a/b/c', 'a_b_c'),
            slashed('no_slash', 'no_slash') ]) ]).

fixture(derive_directory_prefix_via_rtrim_replace,
  prog([], [ (directory(File, Dir) <- file_path(File),
              Dir := rtrim(File, replace(File, '/', ''))) ]),
  [ file_path('src/a.ts'), file_path('README.md') ],
  [],
  [ final(directory/2,
          [ directory('README.md', ''),
            directory('src/a.ts', 'src/') ]) ]).
