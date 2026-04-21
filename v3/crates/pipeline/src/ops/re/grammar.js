// tree-sitter-sprefa_re — sub-grammar for the `re(...)` paren body.
//
// Spec §14.2 / §14.7. Author runs `just regen-op re` after any edit.
//
// Minimal Phase 1 surface: literal bytes OR `$NAME` (term_ref).
// The `regex` crate handles its own syntax (quantifiers, char classes,
// alternations, native `(?<X>...)` named groups) as literal bytes; the
// only grammar-level distinction we need is term_ref so LSP hover
// + binds_captures can see the sugar.
//
//   re(TODO\($WHO\))
//            ^^^^
//            `- term_ref; compile rewrites to `(?P<WHO>.*?)`.

const shared = require('../../../../tree-sitter-sprefa/shared/tokens');

module.exports = grammar({
  name: 'sprefa_re',

  rules: {
    source_file: $ => repeat($._atom),

    _atom: $ => choice(
      $.term_ref,
      $.literal,
    ),

    // Any run of non-dollar bytes.
    literal: $ => /[^$]+/,

    ...shared,
  },
});
