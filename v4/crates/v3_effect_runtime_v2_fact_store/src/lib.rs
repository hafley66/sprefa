//! Friendly re-export wrapper. Maps the long, self-describing crate
//! name v4 tests use (`v3_effect_runtime_v2_fact_store`) to the
//! sqlite-flavored surface of `effect_runtime::v2::fact_store`.

pub use effect_runtime::v2::fact_store::{register_sprf_udfs, PragmaProfile, SqliteFactStore};
