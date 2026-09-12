//! The two lookup stores. `ReservationIndex` replaces the JITI reservation
//! arena (`0_lowerer.pl:1502-1605`); `EdgeIndex` replaces the graph store
//! (`0_graph_lookup.pl`). Both are built once, read many, and dropped with the
//! lowering frame, so `close_reservation_arena/0` and `close_graph_store/0`
//! have no Rust analogue.

use crate::_6_eval::term::{TermId, Universe};
use indexmap::IndexSet;
use std::collections::HashMap;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct OwnerId(pub u32);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct NameId(pub u32);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum View {
    Promoted,
    Visible,
}

#[derive(Clone, Debug)]
pub struct Reservation {
    pub row: TermId,
    pub owner: TermId,
    pub name: TermId,
    pub target: TermId,
    pub kind: TermId,
}

/// `reservation(Owner, Name, Target, Kind)`.
pub fn reservation_parts(u: &Universe, row: TermId) -> Option<Reservation> {
    let (name, args) = u.functor(row)?;
    if name != "reservation" || args.len() != 4 {
        return None;
    }
    Some(Reservation {
        row,
        owner: args[0],
        name: args[1],
        target: args[2],
        kind: args[3],
    })
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub row: TermId,
    pub owner: TermId,
    pub name: TermId,
    pub target: TermId,
    pub index: i64,
}

/// `pending_edge(Owner, Name, Target, Index)`.
pub fn edge_parts(u: &Universe, row: TermId) -> Option<Edge> {
    let (name, args) = u.functor(row)?;
    if name != "pending_edge" || args.len() != 4 {
        return None;
    }
    Some(Edge {
        row,
        owner: args[0],
        name: args[1],
        target: args[2],
        index: u.as_int(args[3])?,
    })
}

#[derive(Default)]
pub struct Dictionary {
    pub terms: IndexSet<TermId>,
}

impl Dictionary {
    pub fn intern(&mut self, term: TermId) -> u32 {
        let (i, _) = self.terms.insert_full(term);
        i as u32
    }
    pub fn lookup(&self, term: TermId) -> Option<u32> {
        self.terms.get_index_of(&term).map(|i| i as u32)
    }
}

#[derive(Default)]
pub struct ReservationIndex {
    pub owners: Dictionary,
    pub names: Dictionary,
    pub rows: Vec<Reservation>,
    pub by_owner_name: HashMap<(u32, u32, u8), Vec<usize>>,
    pub product_by_owner_name: HashMap<(u32, u32, u8), usize>,
    pub parent_by_target: HashMap<(u32, u8), u32>,
}

fn view_key(view: View) -> u8 {
    match view {
        View::Promoted => 0,
        View::Visible => 1,
    }
}

impl ReservationIndex {
    /// `:1510`, the `visible` view.
    pub fn build(u: &Universe, rows: &[TermId]) -> Self {
        let mut index = ReservationIndex::default();
        index.install(u, rows, View::Visible);
        index
    }

    /// `:1530`, the `promoted` view, consulted first.
    pub fn install_promoted(&mut self, u: &Universe, rows: &[TermId]) {
        self.install(u, rows, View::Promoted);
    }

    fn install(&mut self, u: &Universe, rows: &[TermId], view: View) {
        let v = view_key(view);
        for row in rows {
            let Some(parts) = reservation_parts(u, *row) else {
                continue;
            };
            let owner = self.owners.intern(parts.owner);
            let name = self.names.intern(parts.name);
            let position = self.rows.len();
            let is_product = u
                .functor_or_atom(parts.kind)
                .is_some_and(|(n, a)| n == "product" && a.is_empty());
            if is_product {
                if let Some(target) = u.unary(parts.target, "target") {
                    let target_owner = self.owners.intern(target);
                    self.parent_by_target
                        .entry((target_owner, v))
                        .or_insert(owner);
                    self.product_by_owner_name
                        .entry((owner, name, v))
                        .or_insert(position);
                }
            }
            self.by_owner_name
                .entry((owner, name, v))
                .or_default()
                .push(position);
            self.rows.push(parts);
        }
    }

    fn probe(&self, owner: u32, name: u32, view: View) -> Option<&Reservation> {
        self.by_owner_name
            .get(&(owner, name, view_key(view)))
            .and_then(|rows| rows.first())
            .map(|i| &self.rows[*i])
    }

    fn probe_product(&self, owner: u32, name: u32, view: View) -> Option<&Reservation> {
        self.product_by_owner_name
            .get(&(owner, name, view_key(view)))
            .map(|i| &self.rows[*i])
    }

    fn parent(&self, owner: u32) -> Option<u32> {
        self.parent_by_target
            .get(&(owner, view_key(View::Promoted)))
            .or_else(|| self.parent_by_target.get(&(owner, view_key(View::Visible))))
            .copied()
    }

    /// `scoped_reservation/5` at `:1550` through `scoped_reservation_arena/5`
    /// at `:1563`. The product-kind probe runs before the any-kind probe, and
    /// the promoted view before the visible view, in both.
    pub fn scoped(&self, owner_term: TermId, name_term: TermId) -> Option<&Reservation> {
        let mut owner = self.owners.lookup(owner_term)?;
        let name = self.names.lookup(name_term)?;
        let mut visited: Vec<u32> = Vec::new();
        loop {
            if visited.contains(&owner) {
                return None;
            }
            if let Some(row) = self
                .probe_product(owner, name, View::Promoted)
                .or_else(|| self.probe_product(owner, name, View::Visible))
            {
                return Some(row);
            }
            if let Some(row) = self
                .probe(owner, name, View::Promoted)
                .or_else(|| self.probe(owner, name, View::Visible))
            {
                return Some(row);
            }
            visited.push(owner);
            owner = self.parent(owner)?;
        }
    }
}

#[derive(Default)]
pub struct EdgeIndex {
    pub owners: Dictionary,
    pub names: Dictionary,
    pub rows: Vec<Edge>,
    pub by_owner_name: HashMap<(u32, u32), Vec<usize>>,
    pub by_owner_index: HashMap<(u32, i64), Vec<usize>>,
    pub by_target_owner: HashMap<u32, Vec<usize>>,
}

impl EdgeIndex {
    /// `0_graph_lookup.pl:38`. Source order is insertion order, so `first()`
    /// reproduces `memberchk/2` and `once/1`.
    pub fn build(u: &Universe, rows: &[TermId]) -> Self {
        let mut index = EdgeIndex::default();
        for row in rows {
            let Some(parts) = edge_parts(u, *row) else {
                continue;
            };
            let owner = index.owners.intern(parts.owner);
            let name = index.names.intern(parts.name);
            let position = index.rows.len();
            index
                .by_owner_name
                .entry((owner, name))
                .or_default()
                .push(position);
            index
                .by_owner_index
                .entry((owner, parts.index))
                .or_default()
                .push(position);
            if let Some(target) = u.unary(parts.target, "target") {
                let target_owner = index.owners.intern(target);
                index
                    .by_target_owner
                    .entry(target_owner)
                    .or_default()
                    .push(position);
            }
            index.rows.push(parts);
        }
        index
    }

    /// `0_graph_lookup.pl:93`.
    pub fn forward(&self, owner: TermId, name: TermId) -> Option<&Edge> {
        let owner = self.owners.lookup(owner)?;
        let name = self.names.lookup(name)?;
        self.by_owner_name
            .get(&(owner, name))
            .and_then(|rows| rows.first())
            .map(|i| &self.rows[*i])
    }

    /// `0_graph_lookup.pl:113`. The first edge at that ordinal wins even when
    /// its name is not an atom; the clause cut does not backtrack.
    pub fn callable_slot(&self, u: &Universe, callable: TermId, index: i64) -> Option<TermId> {
        let owner = self.owners.lookup(callable)?;
        let first = self
            .by_owner_index
            .get(&(owner, index))
            .and_then(|rows| rows.first())
            .map(|i| &self.rows[*i])?;
        match u.get(first.name) {
            crate::_6_eval::term::Term::Atom(_) => Some(first.name),
            _ => None,
        }
    }

    /// `:1882`. Sorted, deduplicated ordinals of the callable's `return` edges.
    pub fn return_indices(&self, callable: TermId, return_atom: TermId) -> Vec<i64> {
        let (Some(owner), Some(name)) =
            (self.owners.lookup(callable), self.names.lookup(return_atom))
        else {
            return Vec::new();
        };
        let mut out: Vec<i64> = self
            .by_owner_name
            .get(&(owner, name))
            .map(|rows| rows.iter().map(|i| self.rows[*i].index).collect())
            .unwrap_or_default();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Every edge owned by `owner`, in source order.
    pub fn owned(&self, owner: TermId) -> Vec<&Edge> {
        let Some(owner) = self.owners.lookup(owner) else {
            return Vec::new();
        };
        let mut positions: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, e)| self.owners.lookup(e.owner) == Some(owner))
            .map(|(i, _)| i)
            .collect();
        positions.sort_unstable();
        positions.into_iter().map(|i| &self.rows[i]).collect()
    }
}
