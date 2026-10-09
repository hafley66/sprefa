//! v6's clock rules (`v6/prolog/compile/registry.pl:206`) as Rust types; rustc is the checker.
//! Cases: `tests/pass/*.rs` must compile, `tests/fail/*.rs` must not.
use std::marker::PhantomData;

/// Ring B: the current set.
pub struct B;
/// Ring Z: this tick's signed changes.
pub struct Z;

pub struct Tick;
/// One tick before `C`.
pub struct Prev<C>(PhantomData<C>);
/// Inside a recursion that runs to its fixpoint within `C`.
pub struct Inner<C>(PhantomData<C>);

pub struct Rel<R, C>(PhantomData<(R, C)>);

/// A clock whose relations are finished; a recursion's `Inner` clock never is.
pub trait Finished {}
impl Finished for Tick {}
impl<C: Finished> Finished for Prev<C> {}

pub fn input<R, C>() -> Rel<R, C> {
    Rel(PhantomData)
}

pub fn join<R, C>(_left: &Rel<R, C>, _right: &Rel<R, C>) -> Rel<R, C> {
    Rel(PhantomData)
}

pub fn negate<C: Finished>(_set: &Rel<B, C>) -> Rel<B, C> {
    Rel(PhantomData)
}

pub fn pre<R, C>(_now: &Rel<R, C>) -> Rel<R, Prev<C>> {
    Rel(PhantomData)
}

pub fn sample<C>(_changes: &Rel<Z, C>) -> Rel<B, C> {
    Rel(PhantomData)
}

pub fn enter<R, C>(_outer: &Rel<R, C>) -> Rel<R, Inner<C>> {
    Rel(PhantomData)
}

pub fn iterate<C>(
    _seed: &Rel<B, C>,
    step: impl FnOnce(&Rel<B, Inner<C>>) -> Rel<B, Inner<C>>,
) -> Rel<B, C> {
    let _ = step(&Rel(PhantomData));
    Rel(PhantomData)
}
