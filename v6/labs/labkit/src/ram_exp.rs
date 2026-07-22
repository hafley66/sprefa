//! RamZset — the resident weighted multiset. The proven oracle from frp-lab, i64-keyed.
//! A pure reducer: O(Δ) time per tick, O(facts) resident. No recompute, no history.

use crate::{mix, Complexity, Experiment};
use std::collections::HashMap;

#[derive(Default)]
pub struct RamZset {
    weight: HashMap<i64, i64>,
    writes: u64,
}

impl Experiment for RamZset {
    fn name(&self) -> &'static str {
        "ram-zset"
    }
    fn complexity(&self) -> Complexity {
        Complexity { time: "O(Δ)", space: "O(facts) resident" }
    }
    fn rationale(&self) -> &'static str {
        "the trusted oracle: a resident weighted multiset (frp-lab, i64-keyed). A pure reducer, no disk, no recompute. The floor every other engine is measured against."
    }
    fn reset(&mut self) {
        self.weight.clear();
        self.writes = 0;
    }
    fn setup(&mut self, base: usize) {
        for k in 0..base as i64 {
            self.weight.insert(k, 1);
        }
    }
    fn tick(&mut self, adds: &[i64], removes: &[i64]) {
        for &k in adds {
            *self.weight.entry(k).or_insert(0) += 1;
            self.writes += 1;
        }
        for &k in removes {
            if let Some(w) = self.weight.get_mut(&k) {
                *w -= 1;
                if *w <= 0 {
                    self.weight.remove(&k);
                }
            }
            self.writes += 1;
        }
    }
    fn digest(&self) -> i64 {
        self.weight.keys().fold(0i64, |a, &k| a ^ mix(k))
    }
    fn live(&self) -> u64 {
        self.weight.len() as u64
    }
    fn writes(&self) -> u64 {
        self.writes
    }
}
