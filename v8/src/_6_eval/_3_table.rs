//! One append-only table per relation. Row id is insertion position, so the
//! semi-naive delta is the slice `rows[frontier..]` and costs no copy. One
//! hash index per column, appended only for rows that were new.

use super::term::TermId;
use indexmap::IndexSet;
use std::collections::HashMap;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Range {
    /// rows[..frontier], the rows that existed before the last advance
    Old,
    /// rows[frontier..], the rows added by the last round
    Delta,
    /// every row
    All,
}

#[derive(Default, Debug)]
pub struct Table {
    pub rows: IndexSet<Box<[TermId]>>,
    pub frontier: usize,
    pub index: Vec<HashMap<TermId, Vec<u32>>>,
}

impl Table {
    pub fn insert(&mut self, row: Box<[TermId]>) -> bool {
        let arity = row.len();
        let (id, new) = self.rows.insert_full(row);
        if !new {
            return false;
        }
        if self.index.len() < arity {
            self.index.resize_with(arity, HashMap::new);
        }
        let row = &self.rows[id];
        for (c, cell) in row.iter().enumerate() {
            self.index[c].entry(*cell).or_default().push(id as u32);
        }
        true
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn contains(&self, row: &[TermId]) -> bool {
        self.rows.contains(row)
    }

    pub fn advance(&mut self) -> usize {
        let n = self.rows.len() - self.frontier;
        self.frontier = self.rows.len();
        n
    }

    pub fn bounds(&self, range: Range) -> (usize, usize) {
        match range {
            Range::Old => (0, self.frontier),
            Range::Delta => (self.frontier, self.rows.len()),
            Range::All => (0, self.rows.len()),
        }
    }

    /// Candidate row ids for a probe: the shortest posting list among the
    /// bound columns, or the whole range when nothing is bound.
    pub fn candidates(&self, bound: &[(usize, TermId)], range: Range) -> Vec<u32> {
        let (lo, hi) = self.bounds(range);
        let mut best: Option<&Vec<u32>> = None;
        for (col, value) in bound {
            let list = self.index.get(*col).and_then(|m| m.get(value));
            match list {
                None => return Vec::new(),
                Some(l) => {
                    if best.is_none_or(|b| l.len() < b.len()) {
                        best = Some(l);
                    }
                }
            }
        }
        match best {
            Some(list) => list
                .iter()
                .copied()
                .filter(|id| (*id as usize) >= lo && (*id as usize) < hi)
                .collect(),
            None => (lo as u32..hi as u32).collect(),
        }
    }

    pub fn row(&self, id: u32) -> &[TermId] {
        &self.rows[id as usize]
    }
}
