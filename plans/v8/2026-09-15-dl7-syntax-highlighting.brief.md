# Brief: dl7 syntax highlighting, one grammar source, every surface the book and editors need

## Job
Make `.dl7` highlight everywhere it is read: the mdbook (highlight.js), VS Code (TextMate), and GitHub (linguist reads the same TextMate grammar). One hand-written TextMate grammar is the source; the highlight.js language is derived from it by a script so the two never drift. Tests run through real tools: `mdbook build` and a node check that the highlight.js language tokenizes every book probe without an `illegal` token.

## Base and first action
- Base sha: `824831a8d594d3e985a3d268eeaca76a1a09ef92` (`origin/main`). Branch `feat/dl7-syntax-highlighting`.
- FIRST command: `git merge --ff-only 824831a8d594d3e985a3d268eeaca76a1a09ef92`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- `mdbook 0.5.4` and `mdbook-mermaid` are installed; `node` and `npx` exist.

## Ownership
You own, all new: `v8/editors/dl7.tmLanguage.json`, `v8/editors/vscode-dl7/` (a minimal extension: `package.json` with `contributes.languages` and `contributes.grammars`, `language-configuration.json` with `;` line comments and `()` brackets), `v8/editors/gen_hljs.mjs` (TextMate to highlight.js), `v8/book/dl7.hljs.js` (generated output, committed), `v8/book/check_highlight.mjs`, and two lines in `v8/book/book.toml` (`additional-js = [..., "dl7.hljs.js"]`; keep the mermaid entries). `v8/tests/_22_book.rs`: add one test `dl7_blocks_highlight_without_illegal_tokens` that runs `node book/check_highlight.mjs` and asserts exit 0. Forbidden: `v8/src/**`, `v8/fixtures/**`, `v8/oracle/**`, `v8/book/src/**` (chapter text), `tree-sitter-dl/**` (that is the v5 `.dl` grammar, a different language), `v8/book/mermaid*.js`. Never spawn subagents. Never `--no-verify`.

## The language, what to tokenize (read these, do not guess)
| token | spelling | source of truth |
|---|---|---|
| comment | `;` to end of line | `v8/src/_0_read/_1_tokens.rs` |
| declaration head | `(:` | `v8/book/src/2_declare.md` |
| rule head | `(<-`, macro `(<+` | `v8/book/src/3_rules.md`, `8_macros.md` |
| product, sum | `(*`, `(+` | `2_declare.md` |
| variable | `?Name`, `?_` | `_0_read/_2_reader.rs:355-367` |
| string | `"..."` with escapes the reader accepts | `_1_tokens.rs` |
| number | int and float, `-?digits`, `digits.digits` | `_1_tokens.rs:45-49` |
| bool | `true`, `false` | `v8/fixtures/literals/1_bool.dl7` |
| primitive type | `int float bool text any type` | `v8/src/_3_check/_5_kernel.rs:31` `primitive_name` |
| kernel relation | the 20 names in `KERNEL_RELATIONS` | `_5_kernel.rs:16-38` |
| infix colon label | `(name : T)` and `(name: T)` | `binding_symmetry/10_infix_colon_spelling.dl7` |
| relation name | any other identifier in call position after `(` | identifier chars at `_1_tokens.rs:16-17` (letters, digits, `_`, `-`, `.`) |
Scope names follow TextMate convention: `comment.line.semicolon.dl7`, `keyword.declaration.dl7`, `variable.other.dl7`, `string.quoted.double.dl7`, `constant.numeric.dl7`, `constant.language.boolean.dl7`, `support.type.primitive.dl7`, `support.function.kernel.dl7`, `entity.name.function.dl7`, `punctuation.section.parens.dl7`.

## Deliverables in order
1. `dl7.tmLanguage.json`, `scopeName: source.dl7`, `fileTypes: ["dl7"]`.
2. `gen_hljs.mjs`: reads the TextMate JSON, emits `dl7.hljs.js` registering `hljs.registerLanguage('dl7', ...)` with `contains` rules for each pattern above (keywords from the same lists, so the kernel names appear once, in the TextMate file). The generated file starts with a comment naming the generator and the grammar path.
3. `book.toml` wiring; `mdbook build book`; open `book/book/3_rules.html` and confirm `<span class="hljs-` inside a `language-dl7` block (grep count > 0, paste it).
4. `check_highlight.mjs`: loads `dl7.hljs.js` against highlight.js (`npx -y highlight.js` or a vendored copy under `v8/book/`; say which), runs `hljs.highlight(src, {language:'dl7'})` over every `v8/book/src/probes/*.dl7` and every `v8/fixtures/**/*.dl7`, fails on any `illegal` or on zero spans.
5. `vscode-dl7/` extension folder; `code --install-extension` is NOT run; the PR body shows the `package.json` contributes block.
6. Test in `_22_book.rs`.

## Validation
```bash
cd v8 && node book/check_highlight.mjs                      # rc 0, prints files checked
mdbook build book && grep -c 'hljs-' book/book/3_rules.html # > 0
cargo test --locked --offline --test _22_book               # all pass
git diff --stat origin/main...HEAD                          # owned files only
```
10 s per operation; over the cap is reported by name, never waited out.

## Style laws
No em dashes. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth, refusal, support (say refCount), honest, distill, "here is", "below is", "the following". Comment budget: constraints only. Interfaces carry `I`. Follow each file's existing style.

## Reporting
PR title `feat(v8): dl7 syntax highlighting, TextMate source, highlight.js for the book`, base `main`. Body: token table with the scope each got, the grep count from step 3, `check_highlight` output. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "dl7 highlighting: PR #<n>, files checked <k>, illegal 0, hljs spans <count>, _22_book <pass>/<total>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
