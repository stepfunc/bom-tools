use clap::{Parser, Subcommand};
use std::fs::File;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Adds files to myapp
    PrintLog {
        /// Path to a cargo log file
        #[clap(value_parser)]
        path: std::path::PathBuf,
    },
    PrintTree {
        /// path to the output of cargo tree
        #[clap(value_parser)]
        path: std::path::PathBuf,
    },
}

/// read cargo log files for dependency information
pub(crate) mod log;
/// parse the output of cargo tree
pub(crate) mod tree;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::PrintLog { path } => {
            let packages = log::read_packages(&path)?;

            for (id, usage) in packages {
                println!("{} {}", id, usage.versions)
            }
        }
        Commands::PrintTree { path } => {
            let file = File::open(path)?;
            let deps = tree::parse_tree(file)?;
            for dep in deps {
                println!("{} {:?}", dep.id, dep.version);
            }
        }
    }

    Ok(())
}
