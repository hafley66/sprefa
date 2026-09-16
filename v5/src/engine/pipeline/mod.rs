//! Production generation-pipeline boundaries.
//!
//! This façade is intentionally small. The current tick is not routed through
//! it yet; these types establish the legal phase order for that later split.

// The façade exports the future integration surface before tick is routed
// through it, so these two lints are expected only during the skeleton phase.
#![allow(dead_code, unused_imports)]

mod apply;
mod full_sources;
mod intent;
mod post_commit;
mod source_stage;
mod stage;

pub(crate) use full_sources::{source_stage_base, FullSourceStageBuilder, PreparedSourceFacts};
pub(crate) use intent::{
    CommittedGeneration, GenerationIntent, PreparedGeneration, ReadyGeneration,
};
pub(crate) use post_commit::PostCommit;

#[cfg(test)]
mod tests {
    #[test]
    fn facade_stays_small_and_cold() {
        let source = include_str!("mod.rs");
        let facade = source.split("#[cfg(test)]").next().unwrap();
        assert!(
            facade.lines().count() <= 220,
            "pipeline façade must stay navigable"
        );
        for forbidden in [
            "Box<dyn",
            "async_trait",
            "tokio",
            "rayon",
            "rusqlite",
            "std::fs",
            "Command::new",
            ".execute(",
            "HashMap",
            "Vec<",
        ] {
            assert!(
                !facade.contains(forbidden),
                "hot/runtime token `{forbidden}` leaked into façade"
            );
        }
    }
}
