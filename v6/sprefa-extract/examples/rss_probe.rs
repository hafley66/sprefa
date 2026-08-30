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
    // Interned ids hold no text; the table IS the symbol bytes now.
    let occ_symbol_bytes: usize = index.symbols.iter().map(|s| s.len()).sum();
    let sym_str = 0;
    let info = 0;
    eprintln!(
        "occurrences={occs} symbol_infos={syms} path_bytes={doc_bytes} occ_symbol_bytes={occ_symbol_bytes} info_symbol_bytes={info}"
    );
    eprintln!("end: {} KB", rss() / 1024);
}
