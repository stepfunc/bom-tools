use crate::cli::{Cli, Commands};
use std::io::stdout;

/// approval of the inventory against the allow-list configuration
pub(crate) mod approval;
pub(crate) mod cli;
/// command glue: load build evidence, run the core, render an output
pub(crate) mod commands;
/// json configuration structures
pub mod config;
#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod golden_tests;
/// the format-neutral contract between input readers and the core
pub(crate) mod graph;
/// readers that turn build evidence into a per-target graph
pub(crate) mod input;
/// merge per-target graphs and resolve them against the cargo metadata
pub(crate) mod inventory;
/// license report renderer
pub(crate) mod licenses;
/// CycloneDX SBOM renderer
pub(crate) mod sbom;

fn main() -> Result<(), anyhow::Error> {
    use clap::Parser;

    let cli = Cli::parse();

    match cli.command {
        Commands::Licenses { evidence } => commands::gen_licenses(&evidence.evidence()?, stdout()),
        Commands::Sbom { evidence, sbom } => {
            commands::gen_sbom(&evidence.evidence()?, &sbom.into(), stdout())
        }
    }
}
