/**
 * Shared tree-sitter rule fragments used across pattern-op sub-grammars
 * (spec §14.3, v3/crates/sprefa_parse/parse.md).
 *
 * Pattern sub-grammars spread this into their `rules` object so that
 * `term_ref` (`$NAME`) tokenizes identically everywhere. Host grammar
 * dogfoods the same spread.
 *
 * Usage:
 *   const shared = require('./shared/tokens');         // host
 *   const shared = require('../../../tree-sitter-sprefa/shared/tokens'); // pattern op
 *
 *   module.exports = grammar({
 *     name: '...',
 *     rules: {
 *       // ...op-specific rules...
 *       ...shared,
 *     },
 *   });
 *
 * `carveout_expr` is NOT in this fragment. Its body rule differs per
 * grammar (host re-enters $.pipe; pattern ops treat the body as opaque
 * bytes for injection re-parse). Each grammar defines its own
 * `carveout_expr` with `${...}` / `&{...}` delimiters; only `term_ref`
 * is truly shared.
 */

module.exports = {
  term_ref: $ => token(seq('$', /[A-Za-z_][A-Za-z0-9_]*/, optional('?'))),
};
