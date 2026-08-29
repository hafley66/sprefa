/**
 * Tree-sitter grammar for the DL7 prefix syntax.
 *
 * This grammar owns character recognition, parenthesized nesting, concrete
 * syntax spans, and recoverable syntax errors. Bare-token classification,
 * semantic identities, variable sharing, literal decoding, and compiler
 * diagnostics belong to the adapter above the generated C parser.
 */

module.exports = grammar({
  name: 'dl7',

  extras: $ => [/[\s]+/, $.line_comment],

  rules: {
    source_file: $ => repeat($._expression),

    _expression: $ => choice(
      $.parenthesized_expression,
      $.string_literal,
      $.bare_token,
    ),

    parenthesized_expression: $ => seq(
      '(',
      repeat($._expression),
      ')',
    ),

    string_literal: _ => token(seq(
      '"',
      repeat(choice(/[^"\\]/, /\\./)),
      '"',
    )),

    // One maximal run between DL7 delimiters. The adapter classifies the
    // complete text as an integer, symbol, variable, name, or diagnostic.
    // Keeping this boundary in the grammar prevents malformed text such as
    // `-12abc` or `'a'b` from becoming two adjacent valid expressions.
    bare_token: _ => token(/[^\s();"]+/),

    line_comment: _ => token(seq(';', /[^\n]*/)),
  },
});
