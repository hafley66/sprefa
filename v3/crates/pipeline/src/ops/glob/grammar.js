// tree-sitter-sprefa_glob — sub-grammar for the `glob(...)` paren body.
//
// Spec §14.2 / §14.7. Author runs `just regen-op glob` after any edit;
// the committed src/parser.c + src/grammar.json + src/node-types.json
// carry the parser state into `pipeline/build.rs` via `cc`.
//
//   glob(A/B/$DIR/file.txt)    where A and B are star/double-star/chars
//            ^^^^
//            `- term_ref (hole, binds $DIR)
//
// Only `term_ref` is shared with the host grammar (via shared/tokens.js).
// `carveout_expr` is deferred — Phase 2 will reintroduce it when the
// host-recurse hover path is wired.

const shared = require('../../../../tree-sitter-sprefa/shared/tokens');

module.exports = grammar({
  name: 'sprefa_glob',

  rules: {
    source_file: $ => repeat($._segment),

    _segment: $ => choice(
      $.double_star,
      $.star,
      $.question,
      $.slash,
      $.brace_alt,
      $.term_ref,
      $.literal,
    ),

    double_star: $ => '**',
    star:        $ => '*',
    question:    $ => '?',
    slash:       $ => '/',

    // Brace alternation: `{c,h}` matches `c` or `h`; `**/*.{ts,tsx}`
    // matches both. Each alternative is a literal byte run with no
    // nested glob features (commas and braces are the only specials).
    brace_alt: $ => seq(
      '{',
      $.brace_alt_item,
      repeat(seq(',', $.brace_alt_item)),
      '}',
    ),
    brace_alt_item: $ => /[^,}]+/,

    // Any run of non-special bytes.
    literal: $ => /[^*?/${}]+/,

    ...shared,
  },
});
