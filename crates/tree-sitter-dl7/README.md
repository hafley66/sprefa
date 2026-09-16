# DL7 Tree-sitter grammar

`grammar.js` defines the DL7 concrete syntax. `tree-sitter generate` writes the
generated C parser and grammar metadata under `src/`. Generated files are
checked in so compiler builds do not require JavaScript or grammar generation.

The generated parser exports one C ABI function:

```c
const TSLanguage *tree_sitter_dl7(void);
```

The declaration in `bindings/c/tree-sitter-dl7.h` can be included by C and
C++, or imported by Zig with `@cImport`. A later adapter will walk Tree-sitter
nodes and construct the canonical DL7 syntax, source, and diagnostic values.

From `v7/`:

```text
just tree-sitter-generate
just tree-sitter-test
just tree-sitter-parse test/fixtures/0_minimal.dl7
just build
```
