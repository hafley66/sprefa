//! git blob-walking throughput: git2 vs shell-out to git ls-tree +
//! cat-file --batch.
//!
//! Question the bench answers: for v3's content-read leg against a
//! repo's HEAD tree, which path is faster?
//!
//!   A. `git2::Repository::head().tree().walk()` + `odb.read(oid)` per
//!      blob. Holds a `Mutex<Repository>` across the walk because
//!      `git2::Repository` is `Send` yet `!Sync`.
//!   B. Shell out. `git ls-tree -r --long <ref>` prints one line per
//!      blob. Feed OIDs into `git cat-file --batch` via a pipe. Parse
//!      the "oid size\n<bytes>\n" frames off stdout.
//!
//! Both paths enumerate every blob reachable from HEAD, read its
//! bytes, and accumulate (blob_count, total_bytes, match_count).
//! Match accounting uses the same byte-contains prefilter the probe
//! uses so the workload is apples-to-apples with the ast-grep scan.
//!
//! Pattern: simple byte-literal search for "printk" so that hit-count
//! correlates with the ast-grep scan number (ast-grep found 16,627
//! printk() CALL SITES; this bench counts the byte occurrences of
//! the literal "printk" across all blobs, which is strictly ≥ that
//! because it includes the literal inside comments, the symbol in
//! #define DEBUG_printk, header prototypes, etc.).
//!
//! Measurements emitted: wall, bytes/s, blobs/s, RSS peak.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use effect_runtime::batchers::Passthrough;
use effect_runtime::{EffectKind, RtCtxBuilder};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Effect kind used by both backends. Input: which repo to walk and
/// what literal to count. Response: (blobs, bytes, matches).
///
/// The backends (`run_git2`, `run_shell_out`) become two different
/// `Batcher<WalkRepo>` impls registered against this same kind. Op
/// code stays `ctx.put(WalkRepo { .. }).await` in both cases; the
/// topology and backend choice live in the wiring.
struct WalkRepo {
    repo: PathBuf,
    needle: Arc<Vec<u8>>,
}

impl EffectKind for WalkRepo {
    type Response = (u64, u64, u64); // (blobs, bytes, matches)
    /// Attribute throughput on the response side: the bytes figure
    /// is what defines "git blob-walk MB/s".
    fn response_bytes(r: &Self::Response) -> Option<usize> { Some(r.1 as usize) }
}

fn rss_peak_kb() -> u64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 { return 0; }
        #[cfg(target_os = "macos")]
        { (u.ru_maxrss as u64) / 1024 }
        #[cfg(not(target_os = "macos"))]
        { u.ru_maxrss as u64 }
    }
}

// --- path A: git2 tree walk + odb.read per blob ---------------------------
//
// This mirrors what `GitBlobReader` in v2/src/readers/_2_git.rs does:
// hold one `Mutex<Repository>` for the whole walk, iterate tree
// entries, and call `odb.read(oid)` inside the lock. ODB read inflates
// the pack entry (zlib). Counting hits happens outside the critical
// section after cloning the bytes out of the git2 `OdbObject`.
fn run_git2(repo_path: &Path, needle: &[u8]) -> (u64, u64, u64) {
    let repo = git2::Repository::open(repo_path).expect("open repo");
    // Collect oids in a scope so the Reference + Tree borrows of
    // `repo` end before we move `repo` into a Mutex below.
    let oids: Vec<git2::Oid> = {
        let head = repo.head().expect("head");
        let tree = head.peel_to_tree().expect("peel tree");
        let mut v: Vec<git2::Oid> = Vec::with_capacity(1 << 16);
        tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                v.push(entry.id());
            }
            git2::TreeWalkResult::Ok
        })
        .expect("tree walk");
        v
    };

    // ODB handle borrows from the repo, so it must live as long as
    // the guard. Scope each read inside its own block so the borrow
    // and the guard drop together at the end of the iteration.
    let repo_mu = Mutex::new(repo);
    let mut blob_count: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut match_count: u64 = 0;
    for oid in oids {
        let guard = repo_mu.lock().unwrap();
        let odb = guard.odb().expect("odb");
        let obj = odb.read(oid).expect("odb read");
        let data = obj.data();
        total_bytes += data.len() as u64;
        blob_count += 1;
        match_count += memchr_count(needle, data);
        // `obj`, `odb`, `guard` all drop here in reverse order.
    }
    (blob_count, total_bytes, match_count)
}

// Byte occurrences of `needle` inside `hay`. memchr narrows to the
// needle's first byte, then memcmp at each candidate.
fn memchr_count(needle: &[u8], hay: &[u8]) -> u64 {
    if needle.is_empty() || hay.len() < needle.len() {
        return 0;
    }
    let first = needle[0];
    let mut n: u64 = 0;
    let mut i = 0;
    while let Some(off) = memchr::memchr(first, &hay[i..]) {
        let p = i + off;
        if p + needle.len() > hay.len() { break; }
        if &hay[p..p + needle.len()] == needle {
            n += 1;
            i = p + needle.len();
        } else {
            i = p + 1;
        }
    }
    n
}

// --- path B: shell out to git ls-tree | git cat-file --batch -------------
//
// `git ls-tree -r HEAD` prints one line per blob:
//     <mode> SP <type> SP <oid> TAB <path>
// Feed the oids into `git cat-file --batch`, which reads "oid\n" on
// stdin and writes frames on stdout:
//     <oid> SP <type> SP <size>\n
//     <size bytes>\n
// Parse frames off stdout while simultaneously feeding oids into
// stdin. Two threads, zero intermediate materialization.
fn run_shell_out(repo_path: &Path, needle: &[u8]) -> (u64, u64, u64) {
    // 1. git ls-tree -r HEAD: stream of oids, kept entirely in a
    //    memory buffer because the kernel tree is ~85k blobs and
    //    each line is <200 bytes → ~17 MB text.
    let ls = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("ls-tree")
        .arg("-r")
        .arg("HEAD")
        .output()
        .expect("ls-tree");
    assert!(ls.status.success(), "git ls-tree failed: {:?}", ls.status);

    let mut oids: Vec<String> = Vec::with_capacity(1 << 16);
    for line in ls.stdout.split(|b| *b == b'\n') {
        if line.is_empty() { continue; }
        // <mode> SP <type> SP <oid> TAB <path>
        // split on first whitespace runs to isolate the oid column.
        let mut parts = line.splitn(3, |b| *b == b' ' || *b == b'\t');
        let _mode = parts.next();
        let obj_type = parts.next();
        let rest = parts.next();
        if !matches!(obj_type, Some(t) if t == b"blob") { continue; }
        let Some(rest) = rest else { continue };
        // rest = "<oid>\t<path>". Take up to the TAB.
        let oid_end = rest
            .iter()
            .position(|b| *b == b'\t')
            .unwrap_or(rest.len());
        let oid = std::str::from_utf8(&rest[..oid_end])
            .expect("ascii oid")
            .to_string();
        oids.push(oid);
    }

    // 2. git cat-file --batch: one subprocess, long-lived, stdin+stdout.
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("cat-file")
        .arg("--batch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn cat-file --batch");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");

    // Writer thread: feed all oids to stdin, then drop stdin to let
    // cat-file exit its read loop.
    let writer = std::thread::spawn(move || {
        for oid in &oids {
            stdin.write_all(oid.as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
        drop(stdin);
        oids.len() as u64
    });

    // Reader thread (main): parse frames one by one. Frame header
    // is a single line "oid type size\n". Body is `size` bytes then
    // a trailing '\n'. Reuse one buffer for bodies.
    let mut reader = BufReader::with_capacity(1 << 20, stdout);
    let mut blob_count: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut match_count: u64 = 0;
    let mut body_buf: Vec<u8> = Vec::with_capacity(1 << 16);
    let mut header_line = String::new();
    loop {
        header_line.clear();
        let n = match reader.read_line(&mut header_line) {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 { break; }
        let header = header_line.trim_end_matches('\n');
        // "<oid> missing" handling: shouldn't happen for our own
        // ls-tree output, but bail gracefully.
        if header.ends_with("missing") { continue; }
        let mut parts = header.split_ascii_whitespace();
        let _oid = parts.next();
        let obj_type = parts.next();
        let size_str = parts.next();
        let Some(size_str) = size_str else { continue };
        let size: usize = size_str.parse().expect("size parse");
        body_buf.resize(size, 0);
        reader.read_exact(&mut body_buf).expect("body read");
        // Eat trailing '\n' between frames.
        let mut lf = [0u8; 1];
        reader.read_exact(&mut lf).ok();
        if !matches!(obj_type, Some("blob")) { continue; }
        total_bytes += size as u64;
        blob_count += 1;
        match_count += memchr_count(needle, &body_buf);
    }
    // Make sure child exits cleanly.
    let _ = writer.join();
    let _ = child.wait();
    (blob_count, total_bytes, match_count)
}

// --- main -----------------------------------------------------------------

fn main() {
    let mut repo_path = PathBuf::from("tests/smoke/.fixtures/linux");
    let mut trials: usize = 3;
    let mut needle_str: String = String::from("printk");
    let mut modes: Vec<String> = vec!["git2".into(), "shell".into()];

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo"   => { repo_path = PathBuf::from(&args[i + 1]); i += 2; }
            "--trials" => { trials = args[i + 1].parse::<usize>().unwrap().max(1); i += 2; }
            "--needle" => { needle_str = args[i + 1].clone(); i += 2; }
            "--modes"  => {
                modes = args[i + 1].split(',').map(|s| s.to_string()).collect();
                i += 2;
            }
            other => panic!("unknown arg: {}", other),
        }
    }
    let needle: Arc<Vec<u8>> = Arc::new(needle_str.as_bytes().to_vec());

    eprintln!(
        "# config: repo={:?} trials={} needle={:?} modes={:?}",
        repo_path, trials, needle_str, modes,
    );

    // Tokio runtime so we can `.await` the ctx.put futures. Two
    // worker threads are plenty; the heavy work sits in one
    // synchronous backend call per put.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    for mode in &modes {
        // Register one of two backend batchers against the WalkRepo
        // effect kind. The choice of backend is the only thing that
        // differs between modes; call site stays ctx.put(WalkRepo).
        let ctx = match mode.as_str() {
            "git2" => RtCtxBuilder::new()
                .register::<WalkRepo, _>(Passthrough::<WalkRepo, _>::new(
                    move |e: WalkRepo| -> (u64, u64, u64) {
                        run_git2(&e.repo, &e.needle)
                    },
                ))
                .build(),
            "shell" => RtCtxBuilder::new()
                .register::<WalkRepo, _>(Passthrough::<WalkRepo, _>::new(
                    move |e: WalkRepo| -> (u64, u64, u64) {
                        run_shell_out(&e.repo, &e.needle)
                    },
                ))
                .build(),
            other => panic!("unknown mode: {}", other),
        };

        // Warmup put outside the timed loop; primes git's pack
        // caches and whatever subprocess fork costs.
        let _ = rt.block_on(ctx.put(WalkRepo {
            repo: repo_path.clone(),
            needle: needle.clone(),
        }));
        eprintln!("# warm rss_kb={}  mode={}", rss_peak_kb(), mode);

        let mut walls_ms: Vec<f64> = Vec::with_capacity(trials);
        let mut last_blobs: u64 = 0;
        let mut last_bytes: u64 = 0;
        let mut last_matches: u64 = 0;
        for t in 0..trials {
            ctx.telemetry().clear();
            let t0 = Instant::now();
            let (blobs, bytes, matches) = rt.block_on(ctx.put(WalkRepo {
                repo: repo_path.clone(),
                needle: needle.clone(),
            }));
            let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let rss_kb = rss_peak_kb();
            eprintln!(
                "# trial {}/{} mode={}: end_to_end_wall_ms={:.1} blobs={} bytes={} matches={} rss_kb={}",
                t + 1, trials, mode, wall_ms, blobs, bytes, matches, rss_kb,
            );
            eprintln!("{}", ctx.telemetry().summary());
            walls_ms.push(wall_ms);
            last_blobs = blobs; last_bytes = bytes; last_matches = matches;
        }

        let mut sorted = walls_ms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = sorted[sorted.len() / 2];
        let wmin = sorted.first().copied().unwrap();
        let wmax = sorted.last().copied().unwrap();
        println!(
            "git-mode={}\tblobs={}\tbytes={}\tmatches={}\twall_min_ms={:.1}\twall_p50_ms={:.1}\twall_max_ms={:.1}",
            mode, last_blobs, last_bytes, last_matches, wmin, p50, wmax,
        );
    }
}
