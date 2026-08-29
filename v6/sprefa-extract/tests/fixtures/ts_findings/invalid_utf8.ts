// Corpus finding, NOT FIXED: a file that is not valid UTF-8 exits 0 with an
// empty stream. `extract --help` EXIT CODES documents 1 for exactly this
// case ("could not read the input file (I/O or UTF-8)"), so the contract and
// the behaviour disagree and a caller reads the file as fact-free.
//
// Measured on microsoft/TypeScript @9a8581c3: exactly 1 of 12967 testdata
// sources is invalid UTF-8,
// tsc/testdata/tests/cases/compiler/regexInvalidUtf8WithUnicodeFlag.ts.
//
// The lone 0x80 byte in the regex literal below is the invalid sequence.
//
// Repro:
//   extract --family call invalid_utf8.ts; echo $?
// Expected: rc 1, or a named row saying the encoding stopped the parse.
// Observed: rc 0, zero lines, empty stderr.
export const pattern = /€/u;
