The path-constant corpus: a moved file re-aims the relative strings it hands to
`new URL`, `resolve`, `join` and `fileURLToPath`, and leaves every other string
alone. `tests/helpers/` co-moves with `tests/1_helpers.test.ts`, so that file's
literals must come out byte-identical.
