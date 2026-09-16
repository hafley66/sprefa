//! One reducer shape for the four fixpoints: `(state, event) -> output`, with
//! effects leaving only through `fx`.

/// `Effect = Never` is a compile-time proof the reducer is pure.
pub trait Slice {
    type State;
    type Event;
    type Output;
    type Effect;
    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        fx: &mut dyn FnMut(Self::Effect),
    ) -> Self::Output;
}

/// Uninhabited: no value of this type can ever reach a sink.
pub enum Never {}

/// Collect the reduction's effects into `effects`, then hand each to `apply`
/// against the same state. The reducer never sees the applier.
pub fn reduce_then_apply<R: Slice>(
    st: &mut R::State,
    ev: R::Event,
    effects: &mut Vec<R::Effect>,
    mut apply: impl FnMut(&mut R::State, R::Effect),
) -> R::Output {
    let out = R::reduce(st, ev, &mut |effect| effects.push(effect));
    for effect in effects.drain(..) {
        apply(st, effect);
    }
    out
}
