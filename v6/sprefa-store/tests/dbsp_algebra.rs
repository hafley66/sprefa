//! The Feldera/DBSP algebra, taught as executable theorems.
//!
//! Everything Feldera does is built on ONE data type — the **Z-set**: a
//! collection where every element carries an integer *weight* (multiplicity).
//! Positive means "present N times", negative means "a retraction", zero means
//! "absent". From this one idea the whole incremental-view-maintenance story
//! falls out, and each law below is a small, readable proof of it.
//!
//! This is the pure algebra (no DB). `tests/cascade.rs` is the SAME algebra
//! applied to SQLite rows on disk — weights in a column, retraction by
//! subtraction — which is why we can adopt DBSP's math without its resident
//! engine.

use std::collections::HashMap;
use std::hash::Hash;

/// A Z-set over `T`: element -> integer weight.
#[derive(Clone, Debug)]
struct Zset<T: Eq + Hash + Clone> {
    w: HashMap<T, i64>,
}

impl<T: Eq + Hash + Clone> Zset<T> {
    fn empty() -> Self {
        Self { w: HashMap::new() }
    }

    /// Build from `(element, weight)` pairs, summing duplicates.
    fn of(pairs: &[(T, i64)]) -> Self {
        let mut z = Self::empty();
        for (x, k) in pairs {
            *z.w.entry(x.clone()).or_insert(0) += *k;
        }
        z.normalized()
    }

    fn weight(&self, x: &T) -> i64 {
        *self.w.get(x).unwrap_or(&0)
    }

    /// Group addition: merge weights element-by-element. (`+` on Z-sets.)
    fn add(&self, other: &Self) -> Self {
        let mut w = self.w.clone();
        for (x, k) in &other.w {
            *w.entry(x.clone()).or_insert(0) += *k;
        }
        Self { w }.normalized()
    }

    /// Group inverse: negate every weight. This is what turns an INSERT into a
    /// RETRACTION — there is no separate delete operator.
    fn negate(&self) -> Self {
        Self {
            w: self.w.iter().map(|(x, k)| (x.clone(), -k)).collect(),
        }
        .normalized()
    }

    fn sub(&self, other: &Self) -> Self {
        self.add(&other.negate())
    }

    /// Drop zero-weight elements so equality is canonical.
    fn normalized(&self) -> Self {
        Self {
            w: self
                .w
                .iter()
                .filter(|(_, k)| **k != 0)
                .map(|(x, k)| (x.clone(), *k))
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.normalized().w.is_empty()
    }

    /// `map` is LINEAR: apply `f` per element, weight carried through
    /// (collisions add).
    fn map<U: Eq + Hash + Clone>(&self, f: impl Fn(&T) -> U) -> Zset<U> {
        let mut w: HashMap<U, i64> = HashMap::new();
        for (x, k) in &self.w {
            *w.entry(f(x)).or_insert(0) += *k;
        }
        Zset { w }.normalized()
    }

    /// `filter` is LINEAR: keep matching elements, weight carried through.
    fn filter(&self, p: impl Fn(&T) -> bool) -> Self {
        Self {
            w: self
                .w
                .iter()
                .filter(|(x, _)| p(x))
                .map(|(x, k)| (x.clone(), *k))
                .collect(),
        }
        .normalized()
    }

    /// `distinct`: clamp positive weights to 1 (a bag becomes a set). NOT
    /// linear — it needs the whole accumulated weight to decide presence.
    fn distinct(&self) -> Self {
        Self {
            w: self
                .w
                .iter()
                .filter(|(_, k)| **k > 0)
                .map(|(x, _)| (x.clone(), 1))
                .collect(),
        }
    }
}

impl<T: Eq + Hash + Clone> PartialEq for Zset<T> {
    fn eq(&self, other: &Self) -> bool {
        self.normalized().w == other.normalized().w
    }
}

/// `join`: on matching keys, weights **multiply**. This bilinearity is the
/// engine of incremental joins.
fn join<T, U, K, V>(
    a: &Zset<T>,
    b: &Zset<U>,
    ka: impl Fn(&T) -> K,
    kb: impl Fn(&U) -> K,
    out: impl Fn(&T, &U) -> V,
) -> Zset<V>
where
    T: Eq + Hash + Clone,
    U: Eq + Hash + Clone,
    K: Eq + Hash,
    V: Eq + Hash + Clone,
{
    let mut w: HashMap<V, i64> = HashMap::new();
    for (x, wx) in &a.w {
        for (y, wy) in &b.w {
            if ka(x) == kb(y) {
                *w.entry(out(x, y)).or_insert(0) += wx * wy;
            }
        }
    }
    Zset { w }.normalized()
}

// ---------------------------------------------------------------------------
// The theorems.
// ---------------------------------------------------------------------------

/// THEOREM 1 — Z-sets form an abelian group under `+`.
/// (associative, commutative, an identity `0`, and every element has an inverse).
/// This is the foundation: because it is a group, "undo" always exists.
#[test]
fn theorem_1_zset_is_an_abelian_group() {
    let a = Zset::of(&[("x", 1), ("y", 2)]);
    let b = Zset::of(&[("y", 1), ("z", -1)]);
    let c = Zset::of(&[("x", -1)]);

    assert_eq!(a.add(&b).add(&c), a.add(&b.add(&c)), "associative");
    assert_eq!(a.add(&b), b.add(&a), "commutative");
    assert_eq!(a.add(&Zset::empty()), a, "identity: empty is 0");
    assert!(a.add(&a.negate()).is_empty(), "inverse: a + (-a) = 0");
}

/// THEOREM 2 — Retraction IS subtraction.
/// There is no delete operator. Deleting a row is adding it with weight -1, and
/// insert-then-retract annihilates to nothing. This is the whole reason the
/// SQLite `weight` column gives turnkey retraction.
#[test]
fn theorem_2_retraction_is_subtraction() {
    let insert = Zset::of(&[("row", 1)]);
    let retract = Zset::of(&[("row", -1)]);

    assert!(insert.add(&retract).is_empty(), "insert + retract = nothing");
    assert_eq!(retract, insert.negate(), "a retraction is just a negated insert");
}

/// THEOREM 3 — Weight refcounts derivations; a row survives while any support
/// remains. A fact derived two independent ways has weight 2; retracting one
/// derivation leaves weight 1 and the fact stays. No DRed, no re-derivation.
/// This is exactly what `tests/cascade.rs` proves on disk.
#[test]
fn theorem_3_weight_refcounts_derivations() {
    let derived_twice = Zset::of(&[("fact", 1)]).add(&Zset::of(&[("fact", 1)]));
    assert_eq!(derived_twice.weight(&"fact"), 2, "two supports -> weight 2");

    let retract_one = derived_twice.add(&Zset::of(&[("fact", -1)]));
    assert_eq!(retract_one.weight(&"fact"), 1, "one support left -> survives");
    assert!(!retract_one.is_empty());

    let retract_last = retract_one.add(&Zset::of(&[("fact", -1)]));
    assert!(retract_last.is_empty(), "last support gone -> row dies");
}

/// THEOREM 4 — `map` and `filter` are LINEAR: `f(a + b) = f(a) + f(b)`.
/// Linearity is what lets a query run on a delta alone.
#[test]
fn theorem_4_map_and_filter_are_linear() {
    let a = Zset::of(&[(1, 1), (2, 1)]);
    let b = Zset::of(&[(2, 1), (3, -1)]);

    let parity = |n: &i32| n % 2;
    assert_eq!(a.add(&b).map(parity), a.map(parity).add(&b.map(parity)), "map linear");

    let big = |n: &i32| *n > 1;
    assert_eq!(a.add(&b).filter(big), a.filter(big).add(&b.filter(big)), "filter linear");
}

/// THEOREM 5 — The linear IVM theorem: for a linear query Q,
/// `Q(D + ΔD) = Q(D) + Q(ΔD)`, so the view updates by `Q(ΔD)` ALONE and the
/// base is never re-scanned. This is why incremental beats recompute.
#[test]
fn theorem_5_linear_queries_never_recompute() {
    // Q = keep evens, then scale by 10 — a composition of linear ops, so linear.
    let q = |z: &Zset<i32>| z.filter(|n| n % 2 == 0).map(|n| n * 10);

    let base = Zset::of(&[(2, 1), (4, 1), (5, 1)]);
    let delta = Zset::of(&[(6, 1), (2, -1)]); // insert 6, retract one 2

    let recompute_from_scratch = q(&base.add(&delta));
    let incremental = q(&base).add(&q(&delta)); // old view + Q(delta) only

    assert_eq!(recompute_from_scratch, incremental, "view += Q(delta); base untouched");
}

/// THEOREM 6 — `join` is BILINEAR, giving the delta rule of incremental joins:
/// `Δ(a ⋈ b) = Δa ⋈ b + a ⋈ Δb + Δa ⋈ Δb`.
/// (weights multiply, so the product expands exactly like `(a+da)(b+db)`.)
#[test]
fn theorem_6_join_is_bilinear() {
    let key = |t: &(i32, char)| t.0;
    let jn = |x: &Zset<(i32, char)>, y: &Zset<(i32, char)>| {
        join(x, y, key, key, |t, u| (t.1, u.1))
    };

    let a = Zset::of(&[((1, 'a'), 1), ((1, 'b'), 1)]);
    let b = Zset::of(&[((1, 'x'), 1)]);
    let da = Zset::of(&[((1, 'c'), 1), ((1, 'a'), -1)]); // insert c, retract a
    let db = Zset::of(&[((1, 'y'), 1)]);

    let delta_full = jn(&a.add(&da), &b.add(&db)).sub(&jn(&a, &b));
    let chain_rule = jn(&da, &b).add(&jn(&a, &db)).add(&jn(&da, &db));

    assert_eq!(delta_full, chain_rule, "Δ(a⋈b) = Δa⋈b + a⋈Δb + Δa⋈Δb");
}

/// THEOREM 7 — `distinct` BREAKS linearity, which is why set-semantics needs
/// special incremental handling: `distinct(a+b) != distinct(a) + distinct(b)`.
/// It is, however, idempotent. Knowing where linearity fails is as important as
/// knowing where it holds.
#[test]
fn theorem_7_distinct_breaks_linearity_but_is_idempotent() {
    let a = Zset::of(&[("r", 1)]);
    let b = Zset::of(&[("r", 1)]);

    // distinct(a+b) = {r:1}, but distinct(a)+distinct(b) = {r:2}
    assert_ne!(
        a.add(&b).distinct(),
        a.distinct().add(&b.distinct()),
        "distinct is NOT linear"
    );

    let z = Zset::of(&[("r", 3), ("s", -1)]);
    assert_eq!(z.distinct().distinct(), z.distinct(), "distinct is idempotent");
}
