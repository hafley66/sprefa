mod opt1;
mod opt2;

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("opt2-probe") {
        println!("hir_expand linked, HirFileId size = {}", opt2::probe());
        return;
    }
    match args.get(1).map(String::as_str) {
        Some("fixture") => {
            // fixture: for each path, expand to fixpoint (max 5 passes; nested
            // invocations surface on later passes) and print stats TSV
            let out_dir = std::env::var("LAB_OUT_DIR").unwrap_or_else(|_| ".".into());
            for path in &args[2..] {
                let mut cur = path.clone();
                let mut rows = Vec::new();
                for _ in 0..5 {
                    match opt1::expand_file_to(&cur, &out_dir) {
                        Ok((row, _p)) => {
                            let inv = row.split('\t').find(|f| f.starts_with("invocations=")).map(String::from).unwrap_or_default();
                            let exp = row.split('\t').find(|f| f.starts_with("expanded=")).map(String::from).unwrap_or_default();
                            let path_of = row.split('\t').next().unwrap_or("").to_string();
                            let tail = row.split('\t').skip(1).collect::<Vec<_>>().join(" ");
                            let label = std::path::Path::new(&path_of).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            rows.push(format!("{}|{}", label, tail));
                            if exp == "expanded=0" || inv == "invocations=0" { break; }
                            cur = format!("{}.expanded.rs", path_of);
                        }
                        Err(e) => {
                            rows.push(format!("ERR {}", e));
                            break;
                        }
                    }
                }
                println!("{}\t{}", path, rows.join(" ;; "));
            }
        }
        Some("corpus") => {
            // corpus: one file, stats only, expanded text to <out_dir>/<name>.expanded.rs
            let path = &args[2];
            let out_dir = &args[3];
            match opt1::expand_file_to(path, out_dir) {
                Ok((row, expanded_path)) => println!("{}\t{}", row, expanded_path),
                Err(e) => println!("{}\tERR\t{}", path, e),
            }
        }
        Some("spans") => {
            let mut out = String::new();
            for path in &args[2..] {
                let _ = opt1::dump_spans(path, &mut out);
            }
            print!("{}", out);
        }
        _ => {
            eprintln!("usage: macro_expand_lab fixture FILE... | corpus FILE OUT_DIR");
            exit(2);
        }
    }
}
