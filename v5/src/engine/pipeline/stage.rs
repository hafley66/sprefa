//! Protocol for moving prepared intent into the transaction-ready state.

use super::{PreparedGeneration, ReadyGeneration};

/// Staging consumes the prepared token. A caller cannot apply the same
/// generation twice or skip directly from preparation to committed state.
pub(super) trait StageGeneration: Sized {
    fn stage(self) -> ReadyGeneration;
}

impl StageGeneration for PreparedGeneration {
    fn stage(self) -> ReadyGeneration {
        self.into_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::pipeline::GenerationIntent;

    #[test]
    fn legal_prepare_then_stage_transition_preserves_intent() {
        let prepared = GenerationIntent::new().prepare();
        let ready: ReadyGeneration = prepared.stage();
        // Staging is a typestate move, not a mutation: the intent that entered
        // preparation must survive staging and the commit that follows it,
        // recoverable unchanged at the end of the chain.
        let recovered = ready.into_committed().into_intent();
        assert_eq!(recovered, GenerationIntent::new());
    }
}
