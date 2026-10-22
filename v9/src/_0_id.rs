//! Ids: a 64-bit content hash per byte string. Rows in the dataflow and in SQLite hold only ids;
//! bytes live once, in the dictionary.

use std::collections::HashMap;

/// The hash of a byte string. Stable across processes, so it is also the stored key.
pub fn id_of(bytes: &[u8]) -> u64 {
    let hash = blake3::hash(bytes);
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 is 32 bytes"))
}

/// Id to bytes. A second byte string under one id is a hash collision and stops the run.
#[derive(Default)]
pub struct Dict {
    pub bytes: HashMap<u64, Box<[u8]>>,
    /// Ids interned since the last `take_new`: what storage has yet to write.
    pub fresh: Vec<u64>,
}

impl Dict {
    pub fn intern(&mut self, bytes: &[u8]) -> u64 {
        let id = id_of(bytes);
        match self.bytes.get(&id) {
            Some(held) if &**held == bytes => {}
            Some(held) => panic!(
                "id collision: {id:#x} holds {:?} and {:?}",
                String::from_utf8_lossy(held),
                String::from_utf8_lossy(bytes)
            ),
            None => {
                self.bytes.insert(id, bytes.into());
                self.fresh.push(id);
            }
        }
        id
    }

    /// Loads a stored entry without marking it for writing.
    pub fn load(&mut self, id: u64, bytes: Box<[u8]>) {
        self.bytes.insert(id, bytes);
    }

    pub fn take_new(&mut self) -> Vec<(u64, &[u8])> {
        let fresh = std::mem::take(&mut self.fresh);
        fresh.into_iter().map(|id| (id, &*self.bytes[&id])).collect()
    }
}
