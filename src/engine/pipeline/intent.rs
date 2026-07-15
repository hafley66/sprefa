//! Intent and typestate tokens for one semantic generation.

/// A request to execute one generation.
///
/// It deliberately carries no corpus rows or queued work. Those belong to the
/// concrete runtime integration, not to this phase-ordering skeleton.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct GenerationIntent {
    _private: (),
}

impl GenerationIntent {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }

    pub(super) fn prepare(self) -> PreparedGeneration {
        PreparedGeneration { intent: self }
    }
}

/// Preflight completed; staging may begin.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PreparedGeneration {
    intent: GenerationIntent,
}

impl PreparedGeneration {
    pub(super) fn into_ready(self) -> ReadyGeneration {
        ReadyGeneration {
            intent: self.intent,
        }
    }
}

/// All database-bound work is ready to apply atomically.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadyGeneration {
    intent: GenerationIntent,
}

impl ReadyGeneration {
    pub(super) fn into_committed(self) -> CommittedGeneration {
        CommittedGeneration {
            intent: self.intent,
        }
    }
}

/// The semantic SQLite transaction committed successfully.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CommittedGeneration {
    intent: GenerationIntent,
}

impl CommittedGeneration {
    pub(super) fn into_intent(self) -> GenerationIntent {
        self.intent
    }
}
