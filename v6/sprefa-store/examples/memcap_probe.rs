//! Empirical probe: does the memcap guardrail actually stop a large allocation
//! on THIS machine? Caps to <cap_mb>, then tries to touch <alloc_mb> of memory.
//! Prints "SURVIVED" if the allocation succeeded (cap did NOT bite) or the
//! process aborts / exits nonzero if the cap fired. Small numbers only.
//! `cargo run --release --example memcap_probe -- <cap_mb> <alloc_mb>`

use sprefa_store::memcap;

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cap_mb: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(128);
    let alloc_mb: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(512);

    memcap::cap_address_space_mb(cap_mb);

    // Allocate and TOUCH every page so the OS must actually back it (defeats
    // lazy/overcommit): if the cap works, this is where we die.
    let bytes = alloc_mb * 1024 * 1024;
    let mut v: Vec<u8> = Vec::with_capacity(bytes);
    for i in 0..bytes {
        v.push((i & 0xff) as u8);
    }
    // Prevent the optimizer from dropping the allocation before we report.
    let checksum: u64 = v.iter().step_by(4096).map(|&b| b as u64).sum();
    println!("SURVIVED cap={cap_mb}MB alloc={alloc_mb}MB checksum={checksum}");
}
