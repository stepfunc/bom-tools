use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// outputs a human-readable report of all 3rd party licenses
    GenLicenses {
        /// path to the cyclonedx JSON
        #[clap(value_parser, long, short = 'b')]
        bom_path: PathBuf,
        /// path to the JSON configuration (allow-list)
        #[clap(value_parser, long, short = 'c')]
        config_path: PathBuf,
    },
    /// outputs a human-readable report of all 3rd party licenses
    GenLicensesDir {
        /// list all the directories in this directory
        #[clap(value_parser, long, short = 'l')]
        list_dir: PathBuf,
        /// name of the BOM file in each directory
        #[clap(value_parser, long, short = 'b')]
        bom_file: String,
        /// path to the JSON configuration (allow-list)
        #[clap(value_parser, long, short = 'c')]
        config_path: PathBuf,
    },
    /// outputs a human-readable report of all 3rd party licenses from a cargo
    /// build log (`cargo build --message-format=json`), scoped to what actually compiled
    GenLicensesLog {
        /// path to the cargo build log (JSON message stream)
        #[clap(value_parser, long, short = 'g')]
        log_path: PathBuf,
        /// path to `cargo metadata --format-version 1` JSON output
        #[clap(value_parser, long, short = 'm')]
        metadata_path: PathBuf,
        /// path to the JSON configuration (allow-list)
        #[clap(value_parser, long, short = 'c')]
        config_path: PathBuf,
    },
    /// outputs a human-readable report of all 3rd party licenses from cargo
    /// build logs in subdirectories, scoped to what actually compiled
    GenLicensesLogDir {
        /// list all the directories in this directory
        #[clap(value_parser, long, short = 'l')]
        list_dir: PathBuf,
        /// name of the build log file in each directory
        #[clap(value_parser, long, short = 'g')]
        log_file: String,
        /// path to `cargo metadata --format-version 1` JSON output
        #[clap(value_parser, long, short = 'm')]
        metadata_path: PathBuf,
        /// path to the JSON configuration (allow-list)
        #[clap(value_parser, long, short = 'c')]
        config_path: PathBuf,
    },
}
