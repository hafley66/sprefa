//! One table per relation; the semi-naive delta is `rows[frontier..]`. A removal
//! fills its hole from the same side of `frontier`, so no row changes range.

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
    /// A row was appended since the evaluator last settled this table.
    pub grown: bool,
    /// A row was removed since the evaluator last settled this table.
    pub lost: bool,
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
        self.grown = true;
        true
    }

    /// Replace: rows whose key columns equal `row`'s are removed and returned;
    /// `row` is appended. A row already present removes nothing.
    pub fn replace(&mut self, keys: &[usize], row: Box<[TermId]>) -> Vec<Box<[TermId]>> {
        if self.rows.contains(&row) {
            return Vec::new();
        }
        let bound: Vec<(usize, TermId)> = keys.iter().map(|&k| (k, row[k])).collect();
        let displaced: Vec<Box<[TermId]>> = self
            .candidates(&bound, Range::All)
            .into_iter()
            .map(|id| self.row(id))
            .filter(|old| old.len() == row.len() && keys.iter().all(|&k| old[k] == row[k]))
            .map(|old| old.to_vec().into_boxed_slice())
            .collect();
        for old in &displaced {
            self.remove(old);
        }
        self.insert(row);
        displaced
    }

    /// Removal keeps the per-column index consistent; `frontier` is clamped.
    pub fn remove(&mut self, row: &[TermId]) -> bool {
        let Some(mut id) = self.rows.get_index_of(row) else {
            return false;
        };
        if id < self.frontier {
            let last_old = self.frontier - 1;
            if id != last_old {
                self.swap(id, last_old);
            }
            self.frontier = last_old;
            id = last_old;
        }
        let last = self.rows.len() - 1;
        if id != last {
            self.swap(id, last);
        }
        self.unindex(last);
        self.rows.pop();
        self.lost = true;
        true
    }

    /// Every row goes; the table reads as empty and lost if it held a row.
    pub fn clear(&mut self) {
        if !self.rows.is_empty() {
            self.lost = true;
        }
        self.rows.clear();
        self.index.clear();
        self.frontier = 0;
    }

    fn swap(&mut self, a: usize, b: usize) {
        self.unindex(a);
        self.unindex(b);
        self.rows.swap_indices(a, b);
        self.reindex(a);
        self.reindex(b);
    }

    fn unindex(&mut self, id: usize) {
        let row = &self.rows[id];
        for (c, cell) in row.iter().enumerate() {
            let postings = self.index[c].get_mut(cell).expect("indexed cell");
            let at = postings
                .iter()
                .position(|p| *p as usize == id)
                .expect("indexed row");
            postings.swap_remove(at);
            if postings.is_empty() {
                self.index[c].remove(cell);
            }
        }
    }

    fn reindex(&mut self, id: usize) {
        let row = &self.rows[id];
        for (c, cell) in row.iter().enumerate() {
            self.index[c].entry(*cell).or_default().push(id as u32);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[u32]) -> Box<[TermId]> {
        cells.iter().map(|c| TermId(*c)).collect()
    }

    /// Every posting names a row holding that cell, and every cell is posted
    /// exactly once.
    fn assert_index_consistent(table: &Table) {
        let mut postings = 0;
        for (c, by_cell) in table.index.iter().enumerate() {
            for (cell, ids) in by_cell {
                assert!(!ids.is_empty(), "empty posting list kept for {cell:?}");
                for id in ids {
                    assert_eq!(table.row(*id)[c], *cell, "column {c} posting {id}");
                }
                postings += ids.len();
            }
        }
        let cells: usize = table.rows.iter().map(|r| r.len()).sum();
        assert_eq!(postings, cells, "posting count");
        assert!(table.frontier <= table.len(), "frontier past the end");
    }

    fn keyed(rows: &[&[u32]]) -> Table {
        let mut table = Table::default();
        for r in rows {
            table.insert(row(r));
        }
        table
    }

    #[test]
    fn replace_displaces_the_row_with_the_same_key() {
        let mut table = keyed(&[&[1, 10], &[2, 20], &[3, 30]]);
        let displaced = table.replace(&[0], row(&[2, 21]));
        assert_eq!(displaced, vec![row(&[2, 20])]);
        assert!(table.contains(&row(&[2, 21])));
        assert!(!table.contains(&row(&[2, 20])));
        assert_eq!(table.len(), 3);
        assert_eq!(table.candidates(&[(0, TermId(2))], Range::All).len(), 1);
        assert!(table.candidates(&[(1, TermId(20))], Range::All).is_empty());
        assert_index_consistent(&table);
    }

    #[test]
    fn replace_with_a_present_row_changes_nothing() {
        let mut table = keyed(&[&[1, 10]]);
        table.grown = false;
        assert!(table.replace(&[0], row(&[1, 10])).is_empty());
        assert!(!table.grown && !table.lost);
        assert_index_consistent(&table);
    }

    #[test]
    fn composite_keys_compare_every_key_column() {
        let mut table = keyed(&[&[1, 5, 10], &[1, 6, 11]]);
        let displaced = table.replace(&[0, 1], row(&[1, 6, 12]));
        assert_eq!(displaced, vec![row(&[1, 6, 11])]);
        assert!(table.contains(&row(&[1, 5, 10])));
        assert_index_consistent(&table);
    }

    #[test]
    fn removing_an_old_row_keeps_the_delta_rows_delta() {
        let mut table = keyed(&[&[1, 10], &[2, 20], &[3, 30]]);
        table.advance();
        table.insert(row(&[4, 40]));
        table.insert(row(&[5, 50]));
        assert!(table.remove(&row(&[1, 10])));
        let delta: Vec<Box<[TermId]>> = table
            .candidates(&[], Range::Delta)
            .into_iter()
            .map(|id| table.row(id).into())
            .collect();
        let old: Vec<Box<[TermId]>> = table
            .candidates(&[], Range::Old)
            .into_iter()
            .map(|id| table.row(id).into())
            .collect();
        assert_eq!(old.len(), 2);
        assert!(old.contains(&row(&[2, 20])) && old.contains(&row(&[3, 30])));
        assert_eq!(delta.len(), 2);
        assert!(delta.contains(&row(&[4, 40])) && delta.contains(&row(&[5, 50])));
        assert!(table.lost);
        assert_index_consistent(&table);
    }

    #[test]
    fn removing_a_delta_row_keeps_the_old_rows_old() {
        let mut table = keyed(&[&[1, 10], &[2, 20]]);
        table.advance();
        table.insert(row(&[3, 30]));
        table.insert(row(&[4, 40]));
        assert!(table.remove(&row(&[3, 30])));
        assert_eq!(table.frontier, 2);
        assert_eq!(table.bounds(Range::Delta), (2, 3));
        assert_eq!(table.row(2), &*row(&[4, 40]));
        assert_index_consistent(&table);
    }

    #[test]
    fn removing_every_row_in_any_order_empties_the_index() {
        let rows: Vec<Box<[TermId]>> = (0..12).map(|i| row(&[i % 3, i, 7])).collect();
        let mut table = Table::default();
        for (i, r) in rows.iter().enumerate() {
            table.insert(r.clone());
            if i == 5 {
                table.advance();
            }
        }
        for i in [7, 0, 11, 3, 5, 6, 1, 10, 2, 9, 4, 8] {
            assert!(table.remove(&rows[i]));
            assert!(!table.remove(&rows[i]), "row {i} removed twice");
            assert_index_consistent(&table);
        }
        assert!(table.is_empty());
        assert!(table.index.iter().all(|m| m.is_empty()));
        assert_eq!(table.frontier, 0);
    }

    #[test]
    fn clear_marks_lost_only_when_a_row_was_held() {
        let mut empty = Table::default();
        empty.clear();
        assert!(!empty.lost);
        let mut table = keyed(&[&[1, 10]]);
        table.clear();
        assert!(table.lost && table.is_empty() && table.index.is_empty());
    }
}
