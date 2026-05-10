//! Store: path-keyed cache abstraction.
//!
//! - `get` / `put` / `delete_prefix` / `iter_prefix`
//! - Cache invalidation = cascading delete by prefix
//! - Lib ships `MemStore`. SQLite-backed impl lives outside.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::cst::path::{Path, PathKey};

pub trait Store: Send + Sync {
    fn get(&self, path: &Path) -> Option<Arc<[u8]>>;
    fn put(&self, path: &Path, value: Arc<[u8]>);
    fn delete_prefix(&self, prefix: &Path) -> usize;
    fn iter_prefix<'a>(&'a self, prefix: &Path)
        -> Box<dyn Iterator<Item = (Path, Arc<[u8]>)> + 'a>;
}

/// Simple in-memory Store. Uses a `BTreeMap` keyed by a stringified path so
/// `delete_prefix` and `iter_prefix` can use ordered range scans. The string
/// rendering is via `crate::cst::path::render_uri(path, "", "/")`.
pub struct MemStore {
    inner: RwLock<BTreeMap<String, (PathKey, Arc<[u8]>)>>,
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(BTreeMap::new()),
        }
    }
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn key_of(path: &Path) -> String {
    crate::cst::path::render_uri(path, "", "/")
}

impl Store for MemStore {
    fn get(&self, path: &Path) -> Option<Arc<[u8]>> {
        let k = key_of(path);
        let g = self.inner.read().unwrap();
        g.get(&k).map(|(_, v)| v.clone())
    }

    fn put(&self, path: &Path, value: Arc<[u8]>) {
        let k = key_of(path);
        let mut g = self.inner.write().unwrap();
        g.insert(k, (PathKey(path.clone()), value));
    }

    fn delete_prefix(&self, prefix: &Path) -> usize {
        let pk = key_of(prefix);
        let mut g = self.inner.write().unwrap();
        let to_drop: Vec<String> = g
            .range(pk.clone()..)
            .take_while(|(k, _)| k.as_str() == pk || k.starts_with(&format!("{pk}/")))
            .map(|(k, _)| k.clone())
            .collect();
        for k in &to_drop {
            g.remove(k);
        }
        to_drop.len()
    }

    fn iter_prefix<'a>(
        &'a self,
        prefix: &Path,
    ) -> Box<dyn Iterator<Item = (Path, Arc<[u8]>)> + 'a> {
        let pk = key_of(prefix);
        let g = self.inner.read().unwrap();
        let pk_slash = format!("{pk}/");
        let collected: Vec<(Path, Arc<[u8]>)> = g
            .range(pk.clone()..)
            .take_while(move |(k, _)| k.as_str() == pk || k.starts_with(&pk_slash))
            .map(|(_, (path_key, v))| (path_key.0.clone(), v.clone()))
            .collect();
        Box::new(collected.into_iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::path::{items::*, PathBuilder, PathItem};

    fn p<I: IntoIterator<Item = Box<dyn PathItem>>>(items: I) -> Path {
        let mut b = PathBuilder::new();
        for it in items {
            b.push_boxed(it);
        }
        b.build()
    }

    #[test]
    fn put_get_delete_prefix() {
        let s = MemStore::new();
        let a = p([Box::new(Name::new("a")) as _]);
        let ab = p([Box::new(Name::new("a")) as _, Box::new(Name::new("b")) as _]);
        let abc = p([
            Box::new(Name::new("a")) as _,
            Box::new(Name::new("b")) as _,
            Box::new(Name::new("c")) as _,
        ]);
        let z = p([Box::new(Name::new("z")) as _]);

        s.put(&a, Arc::from([1u8].as_slice()));
        s.put(&ab, Arc::from([2u8].as_slice()));
        s.put(&abc, Arc::from([3u8].as_slice()));
        s.put(&z, Arc::from([9u8].as_slice()));

        assert_eq!(s.get(&a).unwrap()[0], 1);
        assert_eq!(s.get(&abc).unwrap()[0], 3);

        let dropped = s.delete_prefix(&a);
        assert_eq!(dropped, 3);
        assert!(s.get(&a).is_none());
        assert!(s.get(&abc).is_none());
        assert_eq!(s.get(&z).unwrap()[0], 9);
    }

    #[test]
    fn iter_prefix_returns_only_under_prefix() {
        let s = MemStore::new();
        let a = p([Box::new(Name::new("a")) as _]);
        let ab = p([Box::new(Name::new("a")) as _, Box::new(Name::new("b")) as _]);
        let abc = p([Box::new(Name::new("abc")) as _]); // sibling prefix-string but not path-prefix
        s.put(&a, Arc::from([1u8].as_slice()));
        s.put(&ab, Arc::from([2u8].as_slice()));
        s.put(&abc, Arc::from([3u8].as_slice()));

        let count = s.iter_prefix(&a).count();
        assert_eq!(count, 2, "expected only `a` and `a/b`, not `abc`");
    }
}
