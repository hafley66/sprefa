mod checksum;
mod engines;
mod gen;

use gen::{Family, Params};
use std::process::exit;

struct Case {
    name: &'static str,
    family: Family,
    params: Params,
    expected_edges: u32,
    expected_derived: u64,
    expected_checksum: &'static str,
}

const CASES: [Case; 3] = [
    Case {
        name: "grid_10000",
        family: Family::Grid,
        params: Params::Grid { rows: 45, cols: 45 },
        expected_edges: 3_960,
        expected_derived: 1_069_200,
        expected_checksum: "9d7239568960d6a8",
    },
    Case {
        name: "layered_10000",
        family: Family::Layered,
        params: Params::Layered {
            layers: 193,
            width: 26,
            fanout: 2,
        },
        expected_edges: 9_984,
        expected_derived: 9_951_396,
        expected_checksum: "addcf85b5162b9da",
    },
    Case {
        name: "chain_10000",
        family: Family::Chain,
        params: Params::Chain { segment_len: 2_582 },
        expected_edges: 7_743,
        expected_derived: 9_996_213,
        expected_checksum: "df09b2f409f8b9a8",
    },
];

fn peak_rss_kb() -> u64 {
    #[repr(C)]
    struct RUsage {
        ru_utime: [i64; 2],
        ru_stime: [i64; 2],
        ru_maxrss: i64,
        ru_ixrss: i64,
        ru_idrss: i64,
        ru_isrss: i64,
        ru_minflt: i64,
        ru_majflt: i64,
        ru_nswap: i64,
        ru_inblock: i64,
        ru_oublock: i64,
        ru_msgsnd: i64,
        ru_msgrcv: i64,
        ru_nsignals: i64,
        ru_nvcsw: i64,
        ru_nivcsw: i64,
    }
    unsafe {
        let mut usage = std::mem::zeroed::<RUsage>();
        extern "C" {
            fn getrusage(who: i32, usage: *mut RUsage) -> i32;
        }
        getrusage(0, &mut usage);
        // ru_maxrss is bytes on macOS, KiB on Linux.
        let maxrss = usage.ru_maxrss;
        if cfg!(target_os = "macos") {
            (maxrss / 1024) as u64
        } else {
            maxrss as u64
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--list-cases") {
        for c in &CASES {
            println!("{}", c.name);
        }
        return;
    }
    let case_name = flag(&args, "--case");
    let variant = flag(&args, "--variant").unwrap_or_else(|| {
        eprintln!("sqlite_baseline: --variant <naive|tuned_range|tuned_wave> is required");
        exit(2);
    });
    let runs = flag(&args, "--runs")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(3);

    let case = CASES
        .iter()
        .find(|c| Some(c.name.to_string()) == case_name)
        .unwrap_or_else(|| {
            eprintln!("sqlite_baseline: unknown --case; use --list-cases");
            exit(2);
        });

    let scale = 10_000u32;
    let seed = gen::seed_for_scale(scale);
    let generated = gen::generate(case.family, case.params, scale, seed);
    if generated.edge_count != case.expected_edges {
        eprintln!(
            "sqlite_baseline: generator port BROKEN for {}: got {} edges, expected {}",
            case.name, generated.edge_count, case.expected_edges
        );
        exit(1);
    }
    let edges = generated.edges;

    let run = |variant: &str| -> (u128, u64, u64, u64, u64) {
        let (result, measures) = match variant {
            "naive" => engines::run_naive(&edges),
            "tuned_range" => engines::run_tuned_range(&edges),
            "tuned_wave" => engines::run_tuned_wave(&edges),
            other => {
                eprintln!("sqlite_baseline: unknown --variant {other}");
                exit(2);
            }
        };
        (
            measures.fixpoint_ms,
            result.derived,
            result.checksum,
            result.rounds,
            result.statements,
        )
    };

    let mut best_ms = u128::MAX;
    let mut best_result: Option<(u64, u64, u64, u64)> = None;
    let mut per_run: Vec<u128> = Vec::new();
    for _ in 0..runs {
        let (ms, derived, checksum, rounds, statements) = run(&variant);
        per_run.push(ms);
        if derived != case.expected_derived {
            eprintln!(
                "INVALID: {} {} derived {derived}, expected {}",
                case.name, variant, case.expected_derived
            );
            exit(1);
        }
        let checksum_hex = format!("{checksum:016x}");
        if checksum_hex != case.expected_checksum {
            eprintln!(
                "INVALID: {} {} checksum {checksum_hex}, expected {}",
                case.name, variant, case.expected_checksum
            );
            exit(1);
        }
        if ms < best_ms {
            best_ms = ms;
            best_result = Some((derived, checksum, rounds, statements));
        }
    }
    let (derived, checksum, rounds, statements) = best_result.unwrap();
    let peak_rss_kb = peak_rss_kb();

    let all_ms = per_run
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{ \"case\": \"{cn}\", \"variant\": \"{vr}\", \"runs\": {rn}, \
          \"best_fixpoint_ms\": {bm}, \"all_fixpoint_ms\": [{am}], \
          \"derived\": {dv}, \"checksum\": \"{cs}\", \"rounds\": {rd}, \
          \"statements\": {st}, \"peak_rss_kb\": {pk} }}",
        cn = case.name,
        vr = variant,
        rn = runs,
        bm = best_ms,
        am = all_ms,
        dv = derived,
        cs = format!("{checksum:016x}"),
        rd = rounds,
        st = statements,
        pk = peak_rss_kb,
    );
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).cloned()
}
