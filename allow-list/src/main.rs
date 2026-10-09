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
pub(crate) mod licenses;
/// CycloneDX SBOM renderer
pub(crate) mod sbom;

fn main() -> Result<(), anyhow::Error> {
    use clap::Parser;

    let cli = Cli::parse();

    match cli.command {
        Commands::GenLicenses {
            bom_path,
            config_path,
        } => licenses::gen_licenses(&bom_path, &config_path, stdout()),
        Commands::GenLicensesDir {
            list_dir,
            bom_file,
            config_path,
        } => licenses::gen_licenses_in_dirs(&list_dir, &bom_file, &config_path, stdout()),
        Commands::GenLicensesLog { target, common } => {
            commands::gen_licenses(&target.evidence(common), stdout())
        }
        Commands::GenLicensesLogDir { targets, common } => {
            commands::gen_licenses(&targets.evidence(common)?, stdout())
        }
        Commands::GenSbomLog {
            target,
            common,
            sbom,
        } => commands::gen_sbom(&target.evidence(common), &sbom.into(), stdout()),
        Commands::GenSbomLogDir {
            targets,
            common,
            sbom,
        } => commands::gen_sbom(&targets.evidence(common)?, &sbom.into(), stdout()),
    }
}
