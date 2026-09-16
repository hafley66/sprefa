//! The `dl` binary is a thin shell: all argument parsing and dispatch lives in
//! `sprefa_v5::cli`. See `src/cli/` for the `Cli` struct, the pre-clap
//! subcommand intercepts, and the run-mode dispatch.

// ARCH {"url":"00-main","role":"entry-point"}
fn main() -> anyhow::Result<()> {
    sprefa_v5::cli::run()
}
