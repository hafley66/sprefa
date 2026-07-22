//! Scale + family-skew stress, ONE scenario per process so peak RSS is honest.
//! (First cut measured peak-across-scenarios in a single process — the Z-set's RSS
//! was contaminated by an earlier full-recompute in the same run. Fixed by argv
//! dispatch; drive it with examples/scale.sh.)
//!
//!   cargo run --release --example core_scale -- full  80000
//!   cargo run --release --example core_scale -- zset  80000
//!   cargo run --release --example core_scale -- dense 80000
//!
//! The question I am trying to BREAK: at ~10M edges with skewed family sizes, does a
//! resident Z-set delta hold, or is it the store lab's on-disk-cascade case? Doubt
//! log inline. Expected break: the string-`Edge` map overflows a 1.5GB budget; the
//! dense-id map (store lab E1) is what actually fits.

use frp_lab::{derive_family_batch, edges_of, Edge, FamilyZSet, File};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

fn peak_rss_mb() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    ru.ru_maxrss as f64 / (1024.0 * 1024.0) // darwin: bytes
}

/// Skewed corpus: Rust giant (many big files), Ts medium, Other a scatter of tiny
/// files that all share one edge. `rust_files` sets the giant family's size.
fn skewed_corpus(rust_files: usize) -> (Vec<File>, usize) {
    let mut files = Vec::new();
    let mut total = 0usize;
    for i in 0..rust_files {
        let mut text = String::with_capacity(2048);
        for j in 0..120 {
            text.push_str(&format!("r{i}_s{j} r{i}_s{}\n", j + 1));
            total += 1;
        }
        files.push(File { path: format!("rust/f{i}.rs"), text });
    }
    for i in 0..(rust_files / 8).max(1) {
        let mut text = String::with_capacity(512);
        for j in 0..40 {
            text.push_str(&format!("t{i}_s{j} t{i}_s{}\n", j + 1));
            total += 1;
        }
        files.push(File { path: format!("ts/f{i}.ts"), text });
    }
    for i in 0..(rust_files / 2).max(1) {
        let text = format!("o{i}_a o{i}_b\no{i}_b o{i}_c\nshared_root shared_leaf\n");
        total += 3;
        files.push(File { path: format!("other/f{i}.md"), text });
    }
    (files, total)
}

fn run_full(rust_files: usize) {
    let (files, total) = skewed_corpus(rust_files);
    let nfiles = files.len();
    let t = Instant::now();
    let edges = derive_family_batch(&files);
    let build = t.elapsed();
    println!(
        "full   {:>9} edges  {:>7} files  build {:>8.2?}  peakRSS {:>7.0} MB  (live {})",
        total, nfiles, build, peak_rss_mb(), edges.len()
    );
}

fn run_zset(rust_files: usize) {
    let (files, total) = skewed_corpus(rust_files);
    let mut z = FamilyZSet::default();
    let t = Instant::now();
    for f in &files {
        z.upsert(f);
    }
    let build = t.elapsed();
    // delta by changed-file size, against the resident ~10M map.
    let tiny = File { path: "other/f0.md".into(), text: "o0_a o0_CHANGED\nshared_root shared_leaf\n".into() };
    let t = Instant::now();
    z.upsert(&tiny);
    let d_tiny = t.elapsed();
    let mut gt = String::new();
    for j in 0..120 {
        gt.push_str(&format!("r0_s{j} r0_CH{}\n", j + 1));
    }
    let t = Instant::now();
    z.upsert(&File { path: "rust/f0.rs".into(), text: gt });
    let d_giant = t.elapsed();
    let shared = Edge { from: "shared_root".into(), to: "shared_leaf".into() };
    assert!(z.edges().contains(&shared), "Z-set dropped a fact 40k files still assert");
    println!(
        "zset   {:>9} edges  string Edge   build {:>8.2?}  peakRSS {:>7.0} MB  delta tiny {:>8.2?} / giant {:>8.2?}",
        total, build, peak_rss_mb(), d_tiny, d_giant
    );
}

// ---- Dense-id Z-set: the store lab's E1 (encode the coordinate, don't store the
// string per row). Symbols interned once into a table; the Edge becomes (u32,u32).
struct DenseZSet {
    intern: HashMap<String, u32>,
    weight: BTreeMap<(u32, u32), i32>,
    per_file: HashMap<String, Vec<(u32, u32)>>,
}
impl DenseZSet {
    fn new() -> Self {
        Self { intern: HashMap::new(), weight: BTreeMap::new(), per_file: HashMap::new() }
    }
    fn id(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.intern.get(s) {
            return id;
        }
        let id = self.intern.len() as u32;
        self.intern.insert(s.to_string(), id);
        id
    }
    fn upsert(&mut self, file: &File) {
        if let Some(old) = self.per_file.remove(&file.path) {
            for e in old {
                if let Some(w) = self.weight.get_mut(&e) {
                    *w -= 1;
                    if *w == 0 {
                        self.weight.remove(&e);
                    }
                }
            }
        }
        let fresh: Vec<(u32, u32)> = edges_of(file)
            .into_iter()
            .map(|e| (self.id(&e.from), self.id(&e.to)))
            .collect();
        for &e in &fresh {
            *self.weight.entry(e).or_default() += 1;
        }
        self.per_file.insert(file.path.clone(), fresh);
    }
}

fn run_dense(rust_files: usize) {
    let (files, total) = skewed_corpus(rust_files);
    let mut z = DenseZSet::new();
    let t = Instant::now();
    for f in &files {
        z.upsert(f);
    }
    let build = t.elapsed();
    let tiny = File { path: "other/f0.md".into(), text: "o0_a o0_CHANGED\nshared_root shared_leaf\n".into() };
    let t = Instant::now();
    z.upsert(&tiny);
    let d_tiny = t.elapsed();
    println!(
        "dense  {:>9} edges  (u32,u32)     build {:>8.2?}  peakRSS {:>7.0} MB  delta tiny {:>8.2?}  (interned {} syms)",
        total, build, peak_rss_mb(), d_tiny, z.intern.len()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("full");
    let rust_files: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(80_000);
    match mode {
        "full" => run_full(rust_files),
        "zset" => run_zset(rust_files),
        "dense" => run_dense(rust_files),
        other => eprintln!("unknown mode {other:?}; use full|zset|dense <rust_files>"),
    }
}
