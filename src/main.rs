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
    /// reports the differences between a log file and the contents of cargo tree
    DiffTree {
        /// path to the log file
        #[clap(value_parser)]
        log_path: std::path::PathBuf,
        /// path to the output of cargo tree
        #[clap(value_parser)]
        tree_path: std::path::PathBuf,
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
        Commands::DiffTree {
            log_path,
            tree_path,
        } => {
            let packages = log::read_packages(&log_path)?;
            let tree = tree::parse_tree(File::open(tree_path)?)?;

            // first, make sure that everything in tree is in packages
            for dep in tree.iter() {
                match packages.get(&dep.id) {
                    None => {
                        eprintln!(
                            "Tree contains dependency {} not found in build log!",
                            dep.id
                        );
                    }
                    Some(x) => {
                        if !x.versions.contains(&dep.version) {
                            eprintln!(
                                "Tree contains dependency {} version {} not found in build log!",
                                dep.id, dep.version
                            );
                        }
                    }
                }
            }

            // then tell us what's in log that isn't in the tree
            for (id, usage) in packages.iter() {
                for version in usage.versions.values() {
                    if !tree
                        .iter()
                        .any(|dep| &dep.id == id && &dep.version == version)
                    {
                        eprintln!(
                            "Log contains dependency {} version {} not found in the tree",
                            id, version
                        );
                    }
                }
            }
        }
    }

    Ok(())
}
