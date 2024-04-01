use crate::cli::*;
use std::io::stdout;

pub(crate) mod cli;
/// json configuration structures
pub mod config;
pub(crate) mod licenses;

fn main() -> Result<(), anyhow::Error> {
    use clap::Parser;

    let cli = crate::Cli::parse();

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
    }
}
