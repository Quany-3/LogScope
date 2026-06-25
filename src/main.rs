//! Binary entrypoint — delegates to the CLI runner.

fn main() -> anyhow::Result<()> {
    log_scope::cli::run()
}
