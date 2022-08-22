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
    /// prints the output of cargo log
    PrintLog {
        /// Path to a cargo log file
        #[clap(value_parser)]
        path: std::path::PathBuf,
    },
    /// prints the output of cargo tree
    PrintTree {
        /// path to the output of cargo tree
        #[clap(value_parser)]
        path: std::path::PathBuf,
    },
    /// generates the skeleton of a configuration using the build log and the output of cargo tree
    GenConfig {
        /// path to the log file
        #[clap(value_parser)]
        log_path: std::path::PathBuf,
        /// path to the output of cargo tree
        #[clap(value_parser)]
        tree_path: std::path::PathBuf,
        /// path to the output config file
        #[clap(value_parser)]
        output_path: std::path::PathBuf,
    },
    /// reports the differences between a log file and the contents of cargo tree
    DiffTree {
        /// path to the log file
        #[clap(value_parser)]
        log_path: std::path::PathBuf,
        /// path to the output of cargo tree
        #[clap(value_parser)]
        tree_path: std::path::PathBuf,
    },
    /// outputs a human readable report of all 3rd party licenses
    GenLicenses {
        /// path to the log file
        #[clap(value_parser)]
        log_path: std::path::PathBuf,
        /// path to the JSON configuration file
        #[clap(value_parser)]
        config_path: std::path::PathBuf,
    },
    /// generate a license report by reading a directory recursively
    GenLicensesDir {
        /// directory to scan for files matching a name
        #[clap(value_parser)]
        dir: std::path::PathBuf,
        #[clap(value_parser)]
        file_name: String,
        /// path to the JSON configuration file
        #[clap(value_parser)]
        config_path: std::path::PathBuf,
    },
    /// writes a bill-of-materials as JSON
    GenBom {
        /// subject of the BOM
        #[clap(value_parser)]
        subject_name: String,
        /// path to the log file
        #[clap(value_parser)]
        log_path: std::path::PathBuf,
        /// path to the JSON configuration file
        #[clap(value_parser)]
        config_path: std::path::PathBuf,
        /// path to the output JSON bom file
        #[clap(value_parser)]
        output_path: std::path::PathBuf,
    },
}
