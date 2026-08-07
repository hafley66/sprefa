pub mod engine2;
pub mod engine_flat;
pub mod intern;
pub mod keys;
pub mod textinput;

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

// The shootout checksum: XOR over derived pairs of fnv1a64 of the two node ids
// little endian, so it is order independent and comparable to interp's.
pub fn pair_checksum(left: u32, right: u32) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&left.to_le_bytes());
    bytes[4..].copy_from_slice(&right.to_le_bytes());
    fnv1a64(&bytes)
}

pub unsafe fn peak_rss_kb() -> i64 {
    let mut usage: libc::rusage = std::mem::zeroed();
    libc::getrusage(libc::RUSAGE_SELF, &mut usage);
    // macOS reports ru_maxrss in bytes; divide by 1024 to get KiB.
    #[cfg(target_os = "macos")]
    {
        usage.ru_maxrss / 1024
    }
    #[cfg(not(target_os = "macos"))]
    {
        usage.ru_maxrss
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // interp's own test pins the same value for the chain 1->2->3->4 closure.
    #[test]
    fn checksum_matches_interps_hand_computed_three_edge_chain() {
        let pairs = [(1u32, 2u32), (2, 3), (3, 4), (1, 3), (2, 4), (1, 4)];
        let mut checksum = 0u64;
        for (left, right) in pairs {
            checksum ^= pair_checksum(left, right);
        }
        assert_eq!(checksum, 0x8e5a666c164d62c4);
    }
}
