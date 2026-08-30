//! Peak RSS of one `load_index` + cache build, machine receipt only.
use sprefa_extract::scip_decode::load_index;

fn rss() -> u64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    ru.ru_maxrss as u64
}

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("index path"));
    let index = load_index(&path).expect("load");
    eprintln!("after load: {} KB", rss() / 1024);
    let occs: usize = index.documents.iter().map(|d| d.occurrences.len()).sum();
    let syms: usize = index.documents.iter().map(|d| d.symbols.len()).sum();
    let doc_bytes: usize = index.documents.iter().map(|d| d.relative_path.len()).sum();
    let sym_str: usize = index
        .documents
        .iter()
        .map(|d| d.occurrences.iter().map(|o| o.symbol.len()).sum::<usize>())
        .sum();
    let info: usize = index
        .documents
        .iter()
        .map(|d| d.symbols.iter().map(|s| s.symbol.len()).sum::<usize>())
        .sum();
    eprintln!(
        "occurrences={occs} symbol_infos={syms} path_bytes={doc_bytes} occ_symbol_bytes={sym_str} info_symbol_bytes={info}"
    );
    eprintln!("end: {} KB", rss() / 1024);
}
