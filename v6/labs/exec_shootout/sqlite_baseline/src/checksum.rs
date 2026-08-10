const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub fn fnv1a64(source: u32, target: u32) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[0..4].copy_from_slice(&source.to_le_bytes());
    bytes[4..8].copy_from_slice(&target.to_le_bytes());
    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub fn fold(iter: impl Iterator<Item = (u32, u32)>) -> u64 {
    let mut fold = 0u64;
    for (source, target) in iter {
        fold ^= fnv1a64(source, target);
    }
    fold
}
