//! SalsaRows — salsa in the FACT role (the wrong role, shown quantitatively). It
//! memoizes the live key set as ROWS. Each tick re-sets the input and re-queries, so
//! it recomputes the whole digest over O(facts) every change and holds the rows
//! resident. recompute_units = salsa executes. This is the row that proves salsa
//! belongs in CONTROL (digests, O(rels)), not here.

use crate::{mix, Complexity, Experiment};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[salsa::db]
#[derive(Clone)]
struct Db {
    storage: salsa::Storage<Self>,
    execs: Arc<Mutex<u64>>,
}
impl Default for Db {
    fn default() -> Self {
        let execs = Arc::new(Mutex::new(0));
        let e2 = execs.clone();
        Self {
            storage: salsa::Storage::new(Some(Box::new(move |ev| {
                if matches!(ev.kind, salsa::EventKind::WillExecute { .. }) {
                    *e2.lock().unwrap() += 1;
                }
            }))),
            execs,
        }
    }
}
#[salsa::db]
impl salsa::Database for Db {}

#[salsa::input]
struct LiveSet {
    keys: Vec<i64>,
}

#[salsa::tracked]
fn live_digest(db: &dyn salsa::Database, ls: LiveSet) -> i64 {
    ls.keys(db).iter().fold(0i64, |a, &k| a ^ mix(k))
}

pub struct SalsaRows {
    db: Db,
    input: Option<LiveSet>,
    weight: HashMap<i64, i64>, // externally maintained; salsa memoizes over it
    last: i64,
}
impl Default for SalsaRows {
    fn default() -> Self {
        Self { db: Db::default(), input: None, weight: HashMap::new(), last: 0 }
    }
}

impl SalsaRows {
    fn push(&mut self) {
        use salsa::Setter;
        let mut keys: Vec<i64> = self.weight.keys().copied().collect();
        keys.sort_unstable();
        match self.input {
            Some(ls) => {
                ls.set_keys(&mut self.db).to(keys);
            }
            None => self.input = Some(LiveSet::new(&self.db, keys)),
        }
        self.last = *live_digest(&self.db, self.input.unwrap());
    }
}

impl Experiment for SalsaRows {
    fn name(&self) -> &'static str {
        "salsa-rows"
    }
    fn complexity(&self) -> Complexity {
        Complexity { time: "O(facts)/tick", space: "O(facts) resident" }
    }
    fn rationale(&self) -> &'static str {
        "salsa in the FACT role (deliberately wrong): memoize the live set as rows, recompute the digest over all facts each change. The row that proves salsa belongs in control, not facts."
    }
    fn reset(&mut self) {
        *self = SalsaRows::default();
    }
    fn setup(&mut self, base: usize) {
        for k in 0..base as i64 {
            self.weight.insert(k, 1);
        }
        self.push();
    }
    fn tick(&mut self, adds: &[i64], removes: &[i64]) {
        for &k in adds {
            *self.weight.entry(k).or_insert(0) += 1;
        }
        for &k in removes {
            if let Some(w) = self.weight.get_mut(&k) {
                *w -= 1;
                if *w <= 0 {
                    self.weight.remove(&k);
                }
            }
        }
        self.push(); // re-set input + re-query: salsa recomputes over ALL live facts
    }
    fn digest(&self) -> i64 {
        self.last
    }
    fn live(&self) -> u64 {
        self.weight.len() as u64
    }
    fn recompute_units(&self) -> u64 {
        *self.db.execs.lock().unwrap()
    }
}
