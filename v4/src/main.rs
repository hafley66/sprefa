// sprefa v4 prototype — min DD wiring, all 7 layers, fs + ast-grep only.
//
// hardcoded "script":
//   fs(**/*.rs) > ast("fn $NAME") > tag(:fns, $name, $path:lo..hi)
//   tag?(:fns, ${A?}, ${B?}) > filter(name.len >= 8) > print     // long fns
//   tag?(:fns, ${A?}, ${B?}) antijoin tag?(:long_names, ${A?})    // short fns
//
// L0 SOURCE       — InputSession<Gen, Cursor, ±1>; rounds drive retraction
// L1 PARSE+LOWER  — input seam: rayon-side parse with ast-grep, push rows
// L2 RULE STORE   — DD's traces ARE the per-collection store (memo + retract)
// L3 SLICE RUNTIME — Slice = Collection<G, Cursor> → Collection<G, Cursor>
// L4 SAGA DISPATCH — inspect_batch on output collections drains effects
// L5 SUBSTRATE    — timely worker + DD scope; FsSubstrate for filesystem
// L6 OS           — std::fs

use std::path::PathBuf;

use ast_grep_core::{source::StrDoc, AstGrep, Pattern};
use ast_grep_language::SupportLang;

use differential_dataflow::input::InputSession;
use differential_dataflow::operators::{Join, Threshold};
use timely::dataflow::operators::probe::Handle;

// ─────────────────────────────────────────────────────────── shared types ──

type Gen = u64;

/// Cursor row in DD. Kept tiny + Hash/Ord/Clone-friendly.
/// Real v4 will carry an Arc<bytes> handle + richer captures.
type Cursor = (
    String,            // path
    u32,               // lo
    u32,               // hi
    String,            // capture: NAME (empty if none)
);

// ─────────────────────────────────────────────────────────── L6/L5 substrate ──

fn walk_rs(root: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().map_or(false, |n| n == "target" || n.to_string_lossy().starts_with('.')) {
                continue;
            }
            walk_rs(&p, out);
        } else if p.extension().map_or(false, |x| x == "rs") {
            out.push(p);
        }
    }
}

// ─────────────────────────────────────────────────────────── L1 input seam ──
// rayon-side equivalent: parse one file with ast-grep, return Vec<Cursor>.

fn extract_fns(path: &PathBuf, pattern: &Pattern<SupportLang>) -> Vec<Cursor> {
    let Ok(src) = std::fs::read_to_string(path) else { return vec![] };
    let grep: AstGrep<StrDoc<SupportLang>> = AstGrep::new(&src, SupportLang::Rust);
    let mut out = Vec::new();
    let path_s = path.display().to_string();
    for nm in grep.root().find_all(pattern) {
        let r = nm.range();
        let env = nm.get_env();
        let name = env.get_match("NAME").map(|n| n.text().to_string()).unwrap_or_default();
        out.push((path_s.clone(), r.start as u32, r.end as u32, name));
    }
    out
}

// ─────────────────────────────────────────────────────────── L0 events ──

enum InputEvent {
    /// Initial seed.
    Run { root: PathBuf },
    /// Drop one file's rows (simulates fs-watcher delete).
    Forget { path: String },
}

// ─────────────────────────────────────────────────────────── main ──

fn main() {
    let root = std::env::args().nth(1)
        .map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

    let events = vec![
        InputEvent::Run { root: root.clone() },
        InputEvent::Forget { path: format!("{}/src/main.rs", root.display()) },
    ];

    timely::execute_directly(move |worker| {
        // L0 InputSession — the dam between rayon side and DD side.
        let mut input: InputSession<Gen, Cursor, isize> = InputSession::new();
        let mut probe = Handle::new();

        // L3+L4: build the slice graph once, hook sinks via inspect_batch.
        worker.dataflow::<Gen, _, _>(|scope| {
            let cursors = input.to_collection(scope);

            // tag(:fns, $name, (path, lo, hi))   keyed by NAME for joins
            let fns_by_name = cursors.map(|(p, lo, hi, n)| (n, (p, lo, hi)));

            // filter slice — long-named fns
            let long = fns_by_name
                .filter(|(n, _)| n.len() >= 8);

            // tag?(:long_names, ${A?})  derived collection of just keys
            let long_keys = long.map(|(n, _)| n).distinct();

            // antijoin slice — fns whose name is NOT in long_keys (i.e. short)
            let short = fns_by_name.antijoin(&long_keys);

            // sinks (L4): drain to stdout. Diff sign reflects insert vs retract.
            long.inspect_batch(|t, batch| {
                for ((name, (path, lo, hi)), _, diff) in batch {
                    let sign = if *diff > 0 { '+' } else { '-' };
                    println!("[{}] long  gen={} {}:{}..{}  fn {}", sign, t, path, lo, hi, name);
                }
            }).probe_with(&mut probe);

            short.inspect_batch(|t, batch| {
                for ((name, (path, lo, hi)), _, diff) in batch {
                    let sign = if *diff > 0 { '+' } else { '-' };
                    println!("[{}] short gen={} {}:{}..{}  fn {}", sign, t, path, lo, hi, name);
                }
            }).probe_with(&mut probe);
        });

        // shared pattern compiled once.
        let pat = Pattern::new("fn $NAME", SupportLang::Rust);

        // remember per-path inserted rows so Forget can issue exact retractions.
        let mut by_path: std::collections::HashMap<String, Vec<Cursor>> = Default::default();

        let mut t: Gen = 0;
        for ev in events {
            t += 1;
            match ev {
                InputEvent::Run { root } => {
                    let mut files = Vec::new();
                    walk_rs(&root, &mut files);
                    for f in files {
                        let rows = extract_fns(&f, &pat);
                        let key = f.display().to_string();
                        for r in &rows { input.insert(r.clone()); }
                        by_path.insert(key, rows);
                    }
                }
                InputEvent::Forget { path } => {
                    if let Some(rows) = by_path.remove(&path) {
                        for r in rows { input.remove(r); }
                    } else {
                        eprintln!("(forget: no rows seen for {})", path);
                    }
                }
            }
            input.advance_to(t + 1);
            input.flush();
            worker.step_while(|| probe.less_than(input.time()));
            eprintln!("── gen {} drained.", t);
        }
    });
}
