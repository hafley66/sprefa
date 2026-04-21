//! Minimal oxc single-file parser. Shell-mode target for
//! `oxc_v3_bench` — benchmarks spawn this once per file to measure
//! the process-fork cost against in-process dispatch.

use std::path::PathBuf;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: oxc_parse_one <path> [<path> ...]");
        std::process::exit(2);
    }
    let mut total_bytes: u64 = 0;
    let mut total_errors: u64 = 0;
    for a in args {
        let p = PathBuf::from(a);
        let Ok(src) = std::fs::read_to_string(&p) else { continue };
        total_bytes += src.len() as u64;
        let alloc = Allocator::default();
        let st = SourceType::from_path(&p).unwrap_or_default();
        let ret = Parser::new(&alloc, &src, st).parse();
        total_errors += ret.errors.len() as u64;
    }
    println!("bytes={} errors={}", total_bytes, total_errors);
}
