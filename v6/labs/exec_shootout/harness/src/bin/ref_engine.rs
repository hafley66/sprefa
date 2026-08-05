use exec_shootout_harness::refengine::{peak_rss_kb, read_input, ref_result};
use std::time::Instant;

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let mut input_path: Option<String> = None;
    let mut index = 1;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--input" {
            index += 1;
            input_path = Some(arguments[index].clone());
        } else {
            eprintln!("ref_engine: unknown argument '{}'", argument);
            std::process::exit(1);
        }
        index += 1;
    }
    let input_path = match input_path {
        Some(path) => path,
        None => {
            eprintln!("ref_engine: missing --input <path>");
            std::process::exit(1);
        }
    };

    let load_start = Instant::now();
    let (_, edge_count, edges) = read_input(&input_path);
    let load_ms = load_start.elapsed().as_millis() as u64;

    let fixpoint_start = Instant::now();
    let result = ref_result(&edges);
    let fixpoint_ms = fixpoint_start.elapsed().as_millis() as u64;

    let rss = peak_rss_kb();

    println!("{{\"event\":\"loaded\",\"edges\":{},\"ms\":{}}}", edge_count, load_ms);
    println!(
        "{{\"event\":\"fixpoint\",\"derived\":{},\"ms\":{}}}",
        result.derived, fixpoint_ms
    );
    println!(
        "{{\"event\":\"done\",\"checksum\":\"{:016x}\",\"peak_rss_kb\":{}}}",
        result.checksum, rss
    );
}
