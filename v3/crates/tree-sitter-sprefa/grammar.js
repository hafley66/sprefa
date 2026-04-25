/**
 * tree-sitter-sprefa — host grammar for .sprf source files.
 *
 * Covers the v3 surface locked in v3/crates/sprefa/src/parse.md:
 *   - program = stmt*
 *   - stmt    = pipe `;`?
 *   - pipe    = pipe_step (`>` pipe_step)*
 *   - step    = op_invocation
 *   - op      = IDENT ('[' … ']')* ('(' … ')')? ('{' … '}')?
 *   - `${...}` = carveout_expr (HostExpr)         — §8.3
 *   - `&{...}` = carveout_expr (Address)          — §8.3
 *   - `${{...}}` = shell_literal                  — §8.4
 *   - `$IDENT` = term_ref (shorthand for `${IDENT}`)
 *   - `:name` = atom_literal
 *   - `"..."` = string_literal
 *   - `r"..."` / `r#"..."#` = raw_string_literal
 *   - `# to EOL` = line_comment
 *
 * Carveouts are first-class nodes. Ops that want sub-grammar injection
 * (ast_grep, regex, json, sh, ...) walk their slot bodies, collect the
 * `carveout_expr` ranges, and feed complement ranges to their child
 * parser via `set_included_ranges` (§8.3.3 strip_carveouts semantics).
 *
 * Error recovery: rely on tree-sitter's built-in ERROR/MISSING nodes.
 * sprefa_parse wraps this with two entry points: `host_parse` (strict —
 * errors fail the parse) and `host_parse_tolerant` (LSP — errors become
 * diagnostics, partial tree keeps working).
 */

const shared = require('./shared/tokens');

module.exports = grammar({
  name: 'sprefa',

  extras: $ => [
    /\s+/,
    $.line_comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    // `foo[bar]` inside a slot body: op_invocation vs identifier + _balanced_brackets.
    // GLR explores both; lowerer picks op_invocation when the token after the
    // identifier is `[` / `(` / `{` (op-head shape). See parse.md §14.5.
    [$.op_invocation, $._slot_atom],
    // `foo(bar){baz}` nested: brace_slot extends the inner op, or the `{baz}`
    // is a `_balanced_braces` atom of the outer body. GLR produces both; the
    // lowerer sees op_invocation with brace_slot and takes it.
    [$.op_invocation],
  ],

  rules: {
    source_file: $ => repeat($._stmt),

    _stmt: $ => seq(
      $.pipe,
      optional(';'),
    ),

    pipe: $ => seq(
      $._pipe_step,
      repeat(seq('>', $._pipe_step)),
    ),

    _pipe_step: $ => choice(
      $.op_invocation,
    ),

    // ---- op invocation ----------------------------------------------------

    op_invocation: $ => prec(1, seq(
      field('name', $.identifier),
      repeat(field('bracket', $.bracket_slot)),
      optional(field('paren', $.paren_slot)),
      optional(field('brace', $.brace_slot)),
    )),

    bracket_slot: $ => seq('[', optional($._slot_body), ']'),
    paren_slot:   $ => seq('(', optional($._slot_body), ')'),
    brace_slot:   $ => seq('{', optional($._slot_body), '}'),

    // Slot body is a sequence of tokens that the op will re-parse with its
    // own sub-grammar. We only need to recognize the shapes that affect
    // balanced-brace scanning — carveouts, shell literals, strings, comments,
    // and raw braces. Everything else flows through as opaque atoms.
    _slot_body: $ => repeat1($._slot_atom),

    _slot_atom: $ => choice(
      $.carveout_expr,
      $.shell_literal,
      $.op_invocation,
      $.string_literal,
      $.raw_string_literal,
      $.atom_literal,
      $.term_ref,
      $.identifier,
      $.number_literal,
      $._slot_punct,
      $._balanced_braces,
      $._balanced_parens,
      $._balanced_brackets,
    ),

    // Nested `{ ... }` / `( ... )` / `[ ... ]` inside a slot body —
    // balanced, stay inside the body. Ops parse their own slot bodies so
    // the host only needs to preserve delimiter structure.
    _balanced_braces:   $ => seq('{', optional($._slot_body), '}'),
    _balanced_parens:   $ => seq('(', optional($._slot_body), ')'),
    _balanced_brackets: $ => seq('[', optional($._slot_body), ']'),

    // Punctuation that's opaque to the host but needs to be recognized so
    // it doesn't match a sub-rule token by accident. We explicitly list the
    // common ones; anything else reaches `_slot_token` as a fallback.
    // Slot bodies are opaque from the host's perspective — they're
    // re-parsed by each op's sub-grammar. The host only needs to keep the
    // balanced-brace/bracket/paren scopes intact. Anything else is a
    // literal byte. Listing `>` here so a `>` inside `{ ... }` doesn't
    // trip the outer pipe separator.
    _slot_punct: $ => token(prec(-1, choice(
      ',', '.', ':', ';', '/', '\\',
      '+', '-', '*', '=', '!', '?', '|', '^', '%', '@', '~',
      '<', '>',
      // Bare `$` (not followed by an ident or `{`) is opaque to the host
      // — it carries the ast-grep multi-node prefix in `$$${VAR?}` and
      // any other sub-grammar's metavar conventions. `$NAME` and `${...}`
      // still claim higher-precedence tokens.
      '$',
    ))),

    // ---- carveouts --------------------------------------------------------

    // `${...}` — host expression carveout. Body re-enters the grammar as a
    // pipe so `${op(x)}` and `${X?}` term-shorthand both parse.
    carveout_expr: $ => seq(
      '${', field('body', $._carveout_host_body), '}',
    ),

    // Body forms inside `${...}` / `&{...}`:
    //   - `${IDENT?}` — unbound term shorthand. Lexes as a single token to
    //     disambiguate from `IDENT` followed by punct `?`. Aliased to
    //     `term_ref` so downstream consumers see the same node kind as
    //     bare `$IDENT`. Mode (Read vs Unbound) is recovered downstream by
    //     inspecting whether the byte range ends in `?`.
    //   - any pipe — including a bare IDENT which the lowerer recognizes
    //     as a term-shorthand (Read mode) when it's a single op_invocation
    //     with no slots.
    _carveout_host_body: $ => choice(
      alias($._brace_term_unbound, $.term_ref),
      $.pipe,
    ),

    _brace_term_unbound: $ => token(seq(
      /[A-Za-z_][A-Za-z0-9_]*/,
      '?',
    )),

    // `${{...}}` — atomic shell literal. Body is opaque bytes terminated by
    // the first `}}` that isn't inside a nested balanced pair.
    shell_literal: $ => seq(
      '${{',
      field('body', alias($._shell_body, $.shell_body)),
      '}}',
    ),

    _shell_body: $ => repeat1($._shell_atom),

    _shell_atom: $ => choice(
      /[^}]+/,
      // Single `}` not followed by another `}` is body text.
      prec(-1, '}'),
    ),

    // ---- terms ------------------------------------------------------------
    // term_ref lives in shared/tokens.js and is spread into this rules
    // block below (§14.3); pattern-op sub-grammars spread the same fragment
    // so $NAME tokenizes identically everywhere. The host grammar only
    // sees term_ref inside slot bodies (paren/brace/bracket); those bodies
    // are re-parsed by each op's sub-grammar. At pipe-step position only
    // op_invocation is legal (sprefa-r6k, brace-mandatory).

    // ---- literals ---------------------------------------------------------

    atom_literal: $ => token(seq(':', /[A-Za-z_][A-Za-z0-9_]*/)),

    string_literal: $ => seq(
      '"',
      repeat(choice(
        /[^"\\]+/,
        seq('\\', /./),
      )),
      '"',
    ),

    raw_string_literal: $ => token(choice(
      seq('r"', /[^"]*/, '"'),
      seq('r#"', /([^"]|"[^#])*/, '"#'),
      seq('r##"', /([^"]|"[^#]|"#[^#])*/, '"##'),
    )),

    number_literal: $ => token(choice(
      /-?\d+/,
      /-?\d+\.\d+/,
    )),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    // ---- comments ---------------------------------------------------------

    line_comment: $ => token(seq('#', /[^\n]*/)),

    // ---- shared fragments -------------------------------------------------
    // §14.3: term_ref shared across host + pattern sub-grammars.
    ...shared,
  },
});
