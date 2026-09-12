//! The DL7 Lisp reader. Port of v7 `src/0_reader/0_parser.pl` `read_dl7/5`;
//! the frozen oracle under `v8/oracle/read/` is v7's own output.

// `DL8_READ_PATH` overrides the path written into `reader_node(Path, Index)`.
// Integers wider than i64 raise `integer_out_of_range`; v7 has bignums.

#[path = "_0_grammar.rs"]
pub mod grammar;
#[path = "_3_cli.rs"]
pub mod read_cli;
#[path = "_2_reader.rs"]
pub mod reader;
#[path = "_1_tokens.rs"]
pub mod tokens;

pub use read_cli::cli;
pub use reader::{read, Pos, Read, Reader};
