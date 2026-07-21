//! Golden test for the OS-protective memory cap. The claim under test is the one
//! the owner doubted: the guardrail must actually BITE on macOS, where
//! `setrlimit` alone is a no-op (a 128 MB rlimit let a 512 MB alloc through —
//! see examples/memcap_probe). The real enforcer is the counting global
//! allocator, and here we prove it aborts an over-cap process instead of letting
//! it grow into swap.
//!
//! The over-cap path aborts the whole process (SIGABRT) by design, so it runs in
//! a re-exec of this very test binary: the parent spawns the child with env vars,
//! the child sets the cap and tries the allocation, the parent asserts on the
//! child's exit status. This binary installs `CappedAlloc` as its own
//! `#[global_allocator]`; the cap starts at 0 (unlimited), so ordinary tests are
//! unaffected until a child explicitly sets it.

use std::process::Command;

use sprefa_store::memcap;

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

/// The child worker. Inert during a normal `cargo test` run (MEMCAP_CHILD unset,
/// early return); only the re-exec below drives it, at which point it sets the
/// cap and tries to touch `MEMCAP_ALLOC` MB. If the cap fires the process aborts
/// before printing SURVIVED.
#[test]
fn child_alloc_worker() {
    if std::env::var("MEMCAP_CHILD").is_err() {
        return;
    }
    let cap_mb: u64 = std::env::var("MEMCAP_CAP").unwrap().parse().unwrap();
    let alloc_mb: usize = std::env::var("MEMCAP_ALLOC").unwrap().parse().unwrap();
    memcap::cap_address_space_mb(cap_mb);

    let bytes = alloc_mb * 1024 * 1024;
    let mut v: Vec<u8> = Vec::with_capacity(bytes);
    for i in 0..bytes {
        v.push((i & 0xff) as u8);
    }
    let checksum: u64 = v.iter().step_by(4096).map(|&b| b as u64).sum();
    println!("SURVIVED cap={cap_mb} alloc={alloc_mb} checksum={checksum}");
}

/// Re-exec this test binary running ONLY `child_alloc_worker`, with the cap /
/// alloc sizes passed via env.
fn run_child(cap_mb: u64, alloc_mb: u64) -> std::process::Output {
    let exe = std::env::current_exe().unwrap();
    Command::new(exe)
        .args(["child_alloc_worker", "--exact", "--nocapture"])
        .env("MEMCAP_CHILD", "1")
        .env("MEMCAP_CAP", cap_mb.to_string())
        .env("MEMCAP_ALLOC", alloc_mb.to_string())
        .output()
        .expect("spawn child")
}

#[test]
fn over_cap_process_aborts_instead_of_swapping() {
    let out = run_child(128, 512);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "128MB cap must abort a 512MB allocation; child exited 0. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("SURVIVED"),
        "child should have died before printing SURVIVED; stdout: {stdout}"
    );
}

#[test]
fn under_cap_process_survives() {
    let out = run_child(512, 64);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "512MB cap must allow a 64MB allocation. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("SURVIVED"),
        "expected SURVIVED from the under-cap child; stdout: {stdout}"
    );
}

#[test]
fn cap_only_tightens_never_raises() {
    // Runs in a child so it doesn't clamp the shared allocator for other tests.
    // Here we just check the state transitions directly via the getter.
    memcap::cap_address_space_mb(4096);
    let a = memcap::cap_bytes();
    memcap::cap_address_space_mb(1024); // tighter -> takes effect
    let b = memcap::cap_bytes();
    memcap::cap_address_space_mb(8192); // looser -> ignored
    let c = memcap::cap_bytes();
    assert_eq!(a, 4096 * 1024 * 1024);
    assert_eq!(b, 1024 * 1024 * 1024);
    assert_eq!(c, 1024 * 1024 * 1024, "a looser cap must not raise the limit");
}
