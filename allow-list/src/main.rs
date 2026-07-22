use crate::cli::{Cli, Commands};
use std::io::stdout;

pub(crate) mod cli;
/// json configuration structures
pub mod config;
pub(crate) mod licenses;
/// read cargo build logs (message-format=json) for the actually-compiled dependency set
pub(crate) mod log;

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
        Commands::GenLicensesLog {
            log_path,
            metadata_path,
            config_path,
        } => log::gen_licenses_from_log(&log_path, &metadata_path, &config_path, stdout()),
        Commands::GenLicensesLogDir {
            list_dir,
            log_file,
            metadata_path,
            config_path,
        } => log::gen_licenses_from_log_dir(&list_dir, &log_file, &metadata_path, &config_path, stdout()),
    }
}
