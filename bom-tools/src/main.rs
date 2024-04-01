use crate::cli::*;
use std::io::stdout;
use std::path::Path;

pub(crate) mod cli;
pub(crate) mod licenses;

fn main() -> Result<(), anyhow::Error> {
    use clap::Parser;

    let cli = crate::Cli::parse();

    match cli.command {
        Commands::GenLicenses {
            bom_path,
            config_path,
        } => gen_licenses(&bom_path, &config_path),
    }
}

fn gen_licenses(bom_path: &Path, config_path: &Path) -> Result<(), anyhow::Error> {
    licenses::gen_licenses(bom_path, config_path, stdout())
}
