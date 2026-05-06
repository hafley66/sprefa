/**
 * tree-sitter-sprefa (v4) — host grammar for .sprf source files.
 *
 * v4 four-slot lowering shape:
 *   op = IDENT '?'? ('[' flow ']')? ('(' args ')')? ('`' dsl '`')? ('{' block '}')?
 *
 * Slots:
 *   - bracket = `flow` slot (cursor field override; defaults to `&.value`)
 *   - paren   = `args`  slot (comma-separated value/atom/sub-pipe positions)
 *   - dsl     = `` `dsl body` `` (opaque text, parsed at lower-time by op's parse_dsl)
 *   - brace   = `block` (sub-pipe)
 *
 * v4 DROPS from v3:
 *   - carveout_expr (`${...}` HostExpr) — sub-grammar interp is parse_dsl, not host
 *   - address_carveout (`&{...}`)       — same
 *   - shell_literal (`${{...}}`)        — collapsed into the dsl slot
 *   - term_ref (`$NAME` shorthand)      — at slot-text level, not grammar level
 *   - tag_* named rules                 — domain-neutral; "fact" is a normal op name
 *   - pipe_fork (naked-brace fork)      — TODO; reintroduce when needed
 *
 * v4 ADDS:
 *   - dsl_body — backtick-fenced text. Single-backtick fence; body is opaque
 *     to the host. (TODO: multi-backtick markdown-fence rule via external
 *     scanner if embedded backticks are needed.)
 *   - bare backtick at pipe-step position is a `str`-style naked dsl_body
 *     step; lower-time treats it as a `str` op call with no slots.
 *
 * Regen: `cd v4/crates/tree-sitter-sprefa && tree-sitter generate`.
 * If tree-sitter-cli is missing: `cargo install tree-sitter-cli`.
 *
 * Error recovery: tree-sitter ERROR/MISSING nodes; v4/src/compile/parse.rs
 * walks them into `effect_runtime::v2::Diag`.
 */

module.exports = grammar({
  name: 'sprefa',

  extras: $ => [
    /\s+/,
    $.line_comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    // `foo[bar]` inside a slot body: op_invocation vs identifier + _balanced_brackets.
    [$.op_invocation, $._slot_atom],
    // `foo(bar){baz}` nested: brace_slot extends inner op vs outer body atom.
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
      // Bare backtick at step position — naked string-literal step.
      // Lowers as `str` op with no slots; dsl_body is the backtick text.
      $.dsl_body,
    ),

    // ---- op invocation ----------------------------------------------------

    op_invocation: $ => prec(1, seq(
      field('name', $.identifier),
      // `?` op suffix flips push/pull. e.g. `fact?(...)`, `<rule>?(...)`.
      optional(field('predicate', '?')),
      repeat(field('bracket', $.bracket_slot)),
      optional(field('paren', $.paren_slot)),
      optional(field('dsl', $.dsl_body)),
      optional(field('brace', $.brace_slot)),
    )),

    bracket_slot: $ => seq('[', optional($._slot_body), ']'),
    paren_slot:   $ => seq('(', optional($._slot_body), ')'),
    brace_slot:   $ => seq('{', optional($._slot_body), '}'),

    // dsl_body — single-backtick fence. Body is opaque to host grammar.
    // No embedded backticks at this layer (markdown-fence with run-counting
    // is a TODO requiring an external scanner). At lower-time, the op's
    // `parse_dsl(raw)` re-parses the body for `${X}` interp etc.
    dsl_body: $ => token(seq(
      '`',
      /[^`]*/,
      '`',
    )),

    // Slot body is a sequence of opaque tokens that the walker classifies
    // per-arg via top-level comma split. Nested parens/braces/brackets stay
    // balanced; backticks are full dsl_body tokens.
    _slot_body: $ => repeat1($._slot_atom),

    _slot_atom: $ => choice(
      $.op_invocation,
      $.string_literal,
      $.raw_string_literal,
      $.atom_literal,
      $.dsl_body,
      $.identifier,
      $.number_literal,
      $._slot_punct,
      $._balanced_braces,
      $._balanced_parens,
      $._balanced_brackets,
    ),

    _balanced_braces:   $ => seq('{', optional($._slot_body), '}'),
    _balanced_parens:   $ => seq('(', optional($._slot_body), ')'),
    _balanced_brackets: $ => seq('[', optional($._slot_body), ']'),

    _slot_punct: $ => token(prec(-1, choice(
      ',', '.', ':', ';', '/', '\\',
      '+', '-', '*', '=', '!', '?', '|', '^', '%', '@', '~', '&',
      '<', '>',
      '$',
    ))),

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

    line_comment: $ => token(seq('#', /[^\n]*/)),
  },
});
