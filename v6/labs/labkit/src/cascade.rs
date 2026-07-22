//! Store-backed Z-set retraction experiment.
use crate::store_db::StoreDb;
use crate::{Complexity, Experiment};
use std::collections::HashMap;

pub struct CascadeZset {
    db: StoreDb,
    path: std::path::PathBuf,
    stmts: u64,
    rounds: u64,
    live_weight: HashMap<i64, i64>,
    rows: u64,
}
static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
impl Default for CascadeZset { fn default() -> Self { Self::new() } }
impl Drop for CascadeZset { fn drop(&mut self) { let _ = std::fs::remove_file(&self.path); let _ = std::fs::remove_file(format!("{}-wal", self.path.display())); let _ = std::fs::remove_file(format!("{}-shm", self.path.display())); } }
impl CascadeZset {
    pub fn new() -> Self {
        let id = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("labkit_czset_{}_{}.db", std::process::id(), id));
        let _ = std::fs::remove_file(&path);
        Self { db: StoreDb::file(&path), path, stmts: 0, rounds: 0, live_weight: HashMap::new(), rows: 0 }
    }
    pub fn insert_rows(&mut self, rows: &[(i64, i64)]) { self.db.runtime.block_on(self.db.store.add_rows(&rows.iter().map(|&(k,w)| (0,k,w)).collect::<Vec<_>>())).unwrap(); self.stmts += 1; }
    pub fn insert_deps(&mut self, edges: &[(i64, i64)]) { self.db.runtime.block_on(self.db.store.add_deps(&edges.iter().map(|&(p,c)| (0,p,0,c)).collect::<Vec<_>>())).unwrap(); self.stmts += 1; }
    pub fn retract(&mut self, seeds: &[i64]) -> u64 { self.rounds = self.db.runtime.block_on(self.db.store.retract(&seeds.iter().map(|&k| (0,k)).collect::<Vec<_>>())).unwrap(); self.stmts += 1; self.rounds }
    pub fn assert(&mut self, seeds: &[i64]) -> u64 { self.rounds = self.db.runtime.block_on(self.db.store.assert(&seeds.iter().map(|&k| (0,k)).collect::<Vec<_>>())).unwrap(); self.stmts += 1; self.rounds }
    pub fn survivors(&self) -> (i64, u64) { let keys = self.db.runtime.block_on(self.db.store.alive_keys()).unwrap(); (keys.into_iter().fold(0, |a,k| a ^ crate::mix(k)), self.db.runtime.block_on(self.db.store.alive()).unwrap() as u64) }
    pub fn statements(&self) -> u64 { self.stmts }
    pub fn rounds(&self) -> u64 { self.rounds }
    pub fn db_size_mb(&self) -> f64 { std::fs::metadata(&self.path).map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0) }
}

impl Experiment for CascadeZset {
    fn name(&self) -> &'static str { "cascade-zset" }
    fn complexity(&self) -> Complexity { Complexity { time: "O(Δ + cascade)", space: "O(working set) RSS" } }
    fn rationale(&self) -> &'static str { "store-backed counted Z-set, with an empty dependency graph for the live-set workload" }
    fn reset(&mut self) { *self = Self::default(); }
    fn setup(&mut self, base: usize) {
        let pool = (base as i64 * 2).max(2);
        self.insert_rows(&(0..pool).map(|k| (k, if k < base as i64 { 1 } else { 0 })).collect::<Vec<_>>());
        self.rows = pool as u64;
        self.live_weight.extend((0..base as i64).map(|k| (k, 1)));
    }
    fn tick(&mut self, adds: &[i64], removes: &[i64]) {
        let mut asserts = Vec::new();
        for &key in adds {
            let weight = self.live_weight.entry(key).or_insert(0);
            if *weight == 0 { asserts.push(key); }
            *weight += 1;
        }
        if !asserts.is_empty() { self.assert(&asserts); }
        let mut retracts = Vec::new();
        for &key in removes {
            if let Some(weight) = self.live_weight.get_mut(&key) {
                *weight -= 1;
                if *weight <= 0 {
                    self.live_weight.remove(&key);
                    retracts.push(key);
                }
            }
        }
        if !retracts.is_empty() { self.retract(&retracts); }
    }
    fn digest(&self) -> i64 { self.survivors().0 }
    fn live(&self) -> u64 { self.survivors().1 }
    fn total_rows(&self) -> u64 { self.rows }
    fn recompute_units(&self) -> u64 { self.rounds }
    fn writes(&self) -> u64 { self.stmts }
}

pub fn cascade_oracle(weights: &[(i64,i64)], deps: &[(i64,i64)], seeds: &[i64]) -> (i64,u64) {
    use std::collections::HashMap;
    let mut w: HashMap<i64,i64> = weights.iter().copied().collect(); let mut children: HashMap<i64,Vec<i64>> = HashMap::new();
    for &(p,c) in deps { children.entry(p).or_default().push(c); }
    let mut frontier = Vec::new(); for &s in seeds { if let Some(v)=w.get_mut(&s) { *v-=1; if *v<=0 { frontier.push(s); } } }
    while !frontier.is_empty() { let mut hits=HashMap::new(); for f in &frontier { for &c in children.get(f).into_iter().flatten() { *hits.entry(c).or_insert(0)+=1; } } let mut next=Vec::new(); for (c,d) in hits { let before=*w.get(&c).unwrap_or(&0); let after=before-d; w.insert(c,after); if after<=0 && before>0 { next.push(c); } } frontier=next; }
    w.into_iter().filter(|(_,v)|*v>0).fold((0,0),|(d,n),(k,_)|(d^crate::mix(k),n+1))
}
