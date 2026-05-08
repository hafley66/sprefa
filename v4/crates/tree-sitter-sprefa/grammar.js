/**
 * tree-sitter-sprefa (v4) — host grammar for .sprf source files.
 *
 * v4 four-slot lowering shape:
 *   op = IDENT '?'? '.'? ('[' flow ']')? ('(' args ')')? ('`' dsl '`')? ('{' block '}')?
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

    // Top-level statements are `;`-terminated. Pipes inside `( … )`
    // grouping or `{ … }` blocks are NOT terminated — only the outer
    // top-level statement is.
    _stmt: $ => seq(
      $.pipe,
      ';',
    ),

    pipe: $ => seq(
      $._pipe_step,
      repeat(seq('>', $._pipe_step)),
    ),

    _pipe_step: $ => choice(
      $.op_invocation,
      $.parenthesized,
      // Bare backtick at step position — naked string-literal step.
      // Lowers as `str` op with no slots; dsl_body is the backtick text.
      $.dsl_body,
    ),

    // Parenthesized pipe — host-level grouping. `(a > b > c)` parses as
    // a sub-pipe and inlines into the outer pipe at lower-time. No `;`
    // permitted inside; `;` is a top-level statement terminator only.
    parenthesized: $ => seq('(', $.pipe, ')'),

    // ---- op invocation ----------------------------------------------------

    // Slot openers (`?`, `[`, `(`, `` ` ``) attach IMMEDIATELY to the op
    // name — no whitespace allowed in between. Block `{ … }` is the only
    // slot that may follow whitespace.
    //
    // This forbids `re \`...\`` and `fact (…)`. `re\`...\``, `fact(…)`, and
    // `rule(:foo) { … }` all lex correctly.
    op_invocation: $ => prec(1, seq(
      field('name', $.identifier),
      optional(field('predicate', token.immediate('?'))),
      optional(field('apply', token.immediate('.'))),
      repeat(field('bracket', $.bracket_slot)),
      optional(field('paren', $.paren_slot)),
      optional(field('dsl', alias($._dsl_body_attached, $.dsl_body))),
      optional(field('brace', $.brace_slot)),
    )),

    bracket_slot: $ => seq(token.immediate('['), optional($._slot_body), ']'),
    paren_slot:   $ => seq(token.immediate('('), optional($._slot_body), ')'),
    brace_slot:   $ => seq('{', optional($._slot_body), '}'),

    // dsl_body — single-backtick fence.
    //
    // Two flavors:
    //   - `dsl_body`             : extras may precede. Used at pipe-step
    //                              position (`` `hello` `` lowers to str).
    //   - `_dsl_body_attached`   : `token.immediate`; used inside an
    //                              op_invocation's `dsl` field so that
    //                              `re\`…\`` is allowed but `re \`…\``
    //                              fails to parse as a re-with-dsl.
    //
    // The op_invocation rule aliases `_dsl_body_attached` back to
    // `dsl_body` so downstream consumers see one node kind.
    //
    // The body regex accepts one level of `${...}` carveout containing a
    // single backtick-fenced sub-body. That covers `${ str`hi` }` /
    // `${ ast`x` }` style sub-pipe holes (#10). Deeper nesting still
    // requires an external scanner.
    //   - any non-backtick / non-`$` char, OR
    //   - `$` not followed by `{`, OR
    //   - `${` ... `}` where the body is non-`}`/non-backtick chars or a
    //     balanced `` `…` `` (one level)
    dsl_body:           $ => token(seq('`', /([^`$]|\$[^{]|\$\{([^}`]|`[^`]*`)*\})*/, '`')),
    _dsl_body_attached: $ => token.immediate(seq('`', /([^`$]|\$[^{]|\$\{([^}`]|`[^`]*`)*\})*/, '`')),

    // Slot body is a sequence of opaque tokens that the walker classifies
    // per-arg via top-level comma split. Nested parens/braces/brackets stay
    // balanced; backticks are full dsl_body tokens.
    _slot_body: $ => repeat1($._slot_atom),

    _slot_atom: $ => choice(
      $.op_invocation,
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

    // v4 has NO `"…"` / `r#"…"#` strings. All string-shaped values use
    // backticks (`` `text` ``). At pipe-step position the bare backtick
    // lowers to `str`; in arg-slot position it remains a `dsl_body` value
    // that the walker classifies as a constant-string sub-pipe.

    number_literal: $ => token(choice(
      /-?\d+/,
      /-?\d+\.\d+/,
    )),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    line_comment: $ => token(seq('#', /[^\n]*/)),
  },
});
