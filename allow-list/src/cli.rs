use clap::{Parser, Subcommand};

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
        bom_path: std::path::PathBuf,
        /// path to the JSON configuration (allow-list)
        #[clap(value_parser, long, short = 'c')]
        config_path: std::path::PathBuf,
    },
    /// outputs a human-readable report of all 3rd party licenses
    GenLicensesDir {
        /// list all the directories in this directory
        #[clap(value_parser, long, short = 'l')]
        list_dir: std::path::PathBuf,
        /// name of the BOM file in each directory
        #[clap(value_parser, long, short = 'b')]
        bom_file: String,
        /// path to the JSON configuration (allow-list)
        #[clap(value_parser, long, short = 'c')]
        config_path: std::path::PathBuf,
    },
}
