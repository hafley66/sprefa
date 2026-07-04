# sprefa dl slideshow

A terminal slideshow (`presenterm`) showing off `dl`, sprefa's reactive
datalog-over-code engine, with real dl syntax highlighting.

## Run it

```bash
presenterm -X deck/sprefa.md
```

`-X` (or `snippet.exec_replace.enable: true` in presenterm's config file)
is required: the dl code slides are `bash +exec_replace` blocks that shell
out to `bat` to render dl with real highlighting, and presenterm refuses
`+exec_replace` blocks unless you opt in.

Navigate with the arrow keys / `hjkl` / page up-down, per presenterm's
normal key bindings.

## How dl highlighting is wired

presenterm has no runtime mechanism for adding a custom syntax of its own:
its `SyntaxSet` is a fixed set of syntaxes baked into the binary at compile
time from a pinned snapshot of bat's asset bundle
(`src/code/highlighting.rs`'s `include_bytes!("../../bat/syntaxes.bin")`,
refreshed by `bat/update.sh` against a specific bat git hash). The language
a fenced code block maps to is a closed Rust enum (`SnippetLanguage`); a
fence tag it doesn't recognize becomes `Unknown` and is highlighted as
plain text. There is no config directory, no bat-cache read, no
`.sublime-syntax` drop-in for `presenterm` itself. (Custom *themes* are
the one thing it does load at runtime from a directory, via
`HighlightThemeSet::register_from_directory` — syntaxes are not.)

So dl code runs through `bat` instead, which genuinely does support custom
`.sublime-syntax` files, and presenterm's own docs document exactly this
combination for languages it doesn't natively support: a `bash
+exec_replace` block that invokes `bat --color always` and lets
presenterm's ANSI passthrough render the result in place of the block.

Concretely:

1. `deck/dl.sublime-syntax` — hand-converted from
   `editors/vscode-dl/syntaxes/dl.tmLanguage.json` (comments, `rel`/`scan`/
   `match`/`ast`/`sg`/`json`/`cmd` keywords, the `<-` rule arrow, the `?`
   query marker, strings + regex literals with their `${}`/`{}`/`$X`/`$$$X`
   placeholder sub-tokens, `:atom`/`:lang` tags, `fs:`/`glob:` schemes, kwarg
   and type-annotation colons, comparison operators, upper-vs-lower-case
   variables), plus one addition the tmLanguage doesn't have: an explicit
   `@qualifier` context so `@in`/`@out`/`@async`/`@stream`/`@next`/
   `@recompute` and `@@mark` read as annotations instead of flat text.
2. Installed into `~/.config/bat/syntaxes/dl.sublime-syntax`, then
   `bat cache --build` compiled it into `~/.cache/bat/syntaxes.bin` (and
   `~/.cache/bat/themes.bin`, bat's normal cache layout — nothing outside
   those two dirs was touched). Confirmed with `bat --list-languages | grep
   dl` → `dl:dl`.
3. Each dl code slide is a `bash +exec_replace +no_background` block that
   runs `bat --color always --style=plain --paging=never --language=dl
   deck/snippets/<name>.dl` against a real dl program under
   `deck/snippets/`.

## Regenerating the syntax when the grammar evolves

Re-run the hand conversion from `editors/vscode-dl/syntaxes/dl.tmLanguage.json`
into `deck/dl.sublime-syntax` (the mapping is documented in a comment at the
top of that file), then:

```bash
cp deck/dl.sublime-syntax ~/.config/bat/syntaxes/dl.sublime-syntax
bat cache --build
```
