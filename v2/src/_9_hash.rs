//! Pure hash helpers: cursor_hash, path_hash_static, content_hash.

use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

use crate::_0_types::{Cursor, PathSeg, SprfPath};

pub fn content_hash(bytes: &[u8]) -> u64 {
    let mut h = FxHasher::default();
    bytes.hash(&mut h);
    h.finish()
}

pub fn path_hash_static(path: &SprfPath) -> u64 {
    let mut h = FxHasher::default();
    for seg in path.0.iter() {
        match seg {
            PathSeg::Op      { name, parse_site, step } => {
                "Op".hash(&mut h);
                name.hash(&mut h);
                (&**parse_site as *const _ as usize).hash(&mut h);
                step.hash(&mut h);
            }
            PathSeg::Named   { name, key, parse_site } => {
                "Named".hash(&mut h);
                name.hash(&mut h); key.hash(&mut h);
                (&**parse_site as *const _ as usize).hash(&mut h);
            }
            PathSeg::ForkArm { index, parse_site } => {
                "ForkArm".hash(&mut h); index.hash(&mut h);
                (&**parse_site as *const _ as usize).hash(&mut h);
            }
            PathSeg::SwitchArm{ pat, parse_site } => {
                "SwitchArm".hash(&mut h); pat.hash(&mut h);
                (&**parse_site as *const _ as usize).hash(&mut h);
            }
            PathSeg::LeafArm { key, parse_site } => {
                "LeafArm".hash(&mut h); key.hash(&mut h);
                (&**parse_site as *const _ as usize).hash(&mut h);
            }
            PathSeg::Iter { .. } => {}           // observational, excluded
        }
    }
    h.finish()
}

pub fn cursor_hash(c: &Cursor) -> u64 {
    let mut h = FxHasher::default();
    c.repo.hash(&mut h);
    c.rev .hash(&mut h);
    if let Some(fs) = &c.fs {
        fs.0.as_os_str().hash(&mut h);
    } else {
        "<no-fs>".hash(&mut h);
    }
    let mut caps: Vec<_> = c.captures.iter().collect();
    caps.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in caps {
        k.hash(&mut h);
        match v.ref_id {
            Some(r) => r.0.hash(&mut h),
            None    => v.value.hash(&mut h),
        }
    }
    let mut fks: Vec<_> = c.fks.iter().collect();
    fks.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in fks {
        k.hash(&mut h); v.0.hash(&mut h);
    }
    path_hash_static(&c.path).hash(&mut h);
    h.finish()
}
