//! v6's clock rules (`v6/prolog/compile/registry.pl:206`) as Rust types; rustc is the checker.
//! Cases: `tests/pass/*.rs` must compile, `tests/fail/*.rs` must not.
use std::marker::PhantomData;

/// Weight "present" (v6 ring B): is the row in the set now.
pub struct Present;
/// Weight "change" (v6 ring Z): this tick's signed changes, +1 added, -1 removed.
pub struct Change;

pub struct Tick;
/// One tick before `Clock`.
pub struct Prev<Clock>(PhantomData<Clock>);
/// Inside a recursion that runs to its fixpoint within `Clock`.
pub struct Inner<Clock>(PhantomData<Clock>);

pub struct Rel<Weight, Clock>(PhantomData<(Weight, Clock)>);

/// A clock whose relations are finished; a recursion's `Inner` clock never is.
pub trait Finished {}
impl Finished for Tick {}
impl<Clock: Finished> Finished for Prev<Clock> {}

pub fn input<Weight, Clock>() -> Rel<Weight, Clock> {
    Rel(PhantomData)
}

pub fn join<Weight, Clock>(_left: &Rel<Weight, Clock>, _right: &Rel<Weight, Clock>) -> Rel<Weight, Clock> {
    Rel(PhantomData)
}

pub fn negate<Clock: Finished>(_set: &Rel<Present, Clock>) -> Rel<Present, Clock> {
    Rel(PhantomData)
}

pub fn pre<Weight, Clock>(_now: &Rel<Weight, Clock>) -> Rel<Weight, Prev<Clock>> {
    Rel(PhantomData)
}

pub fn sample<Clock>(_changes: &Rel<Change, Clock>) -> Rel<Present, Clock> {
    Rel(PhantomData)
}

pub fn enter<Weight, Clock>(_outer: &Rel<Weight, Clock>) -> Rel<Weight, Inner<Clock>> {
    Rel(PhantomData)
}

pub fn iterate<Clock>(
    _seed: &Rel<Present, Clock>,
    step: impl FnOnce(&Rel<Present, Inner<Clock>>) -> Rel<Present, Inner<Clock>>,
) -> Rel<Present, Clock> {
    let _ = step(&Rel(PhantomData));
    Rel(PhantomData)
}
