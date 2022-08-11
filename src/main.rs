extern crate core;

use crate::config::{Config, LicenseInfo, Package, Source};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

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
}

/// json configuration structures
pub(crate) mod config;
/// read cargo log files for dependency information
pub(crate) mod log;
/// parse the output of cargo tree
pub(crate) mod tree;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::PrintLog { path } => print_log(&path),
        Commands::PrintTree { path } => print_tree(&path),
        Commands::DiffTree {
            log_path,
            tree_path,
        } => diff_tree(&log_path, &tree_path),
        Commands::GenConfig {
            log_path,
            tree_path,
            output_path,
        } => generate_config(&log_path, &tree_path, &output_path),
        Commands::GenLicenses {
            log_path,
            config_path,
        } => gen_licenses(&log_path, &config_path),
    }
}

fn print_tree(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let deps = tree::parse_tree(file)?;
    for dep in deps {
        println!("{} {:?}", dep.id, dep.version);
    }
    Ok(())
}

fn print_log(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let packages = log::read_packages(path)?;
    for (id, usage) in packages {
        println!("{} {}", id, usage.versions)
    }
    Ok(())
}

fn generate_config(
    log_path: &Path,
    tree_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let packages = log::read_packages(log_path)?;
    let tree = tree::parse_tree(File::open(tree_path)?)?;

    let mut config = Config {
        build_only: Default::default(),
        vendor: Default::default(),
        third_party: Default::default(),
    };

    // then tell us what's in log that isn't in the tree
    for (id, usage) in packages.iter() {
        for version in usage.versions.values() {
            if tree
                .iter()
                .any(|dep| &dep.id == id && &dep.version == version)
            {
                config.third_party.insert(
                    id.clone(),
                    Package {
                        id: id.clone(),
                        source: Source::CratesIo,
                        licenses: Vec::new(),
                    },
                );
            } else {
                // it's a build only dependency
                config.build_only.insert(id.clone());
            }
        }
    }

    let writer = std::io::BufWriter::new(File::create(output_path)?);
    serde_json::to_writer_pretty(writer, &config)?;
    Ok(())
}

fn diff_tree(log_path: &Path, tree_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let packages = log::read_packages(log_path)?;
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

    Ok(())
}

fn gen_licenses(log_path: &Path, config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut log = log::read_packages(log_path)?;
    let config: Config = serde_json::from_reader(File::open(config_path)?)?;

    // remove build-only and vendor dependencies
    log.retain(|id, _| !(config.build_only.contains(id) || config.vendor.contains(id)));

    // first summarize the licenses
    let mut licenses: BTreeMap<&'static str, LicenseInfo> = BTreeMap::new();
    for (id, _) in log.iter() {
        let pkg = config
            .third_party
            .get(id)
            .ok_or_else(|| format!("3rd party package {} not in the allow list", id))?;
        for license in pkg.licenses.iter() {
            licenses.insert(license.spdx_short(), license.info());
        }
    }

    println!("This binary contains open source dependencies under the following licenses:");
    println!();
    for (spdx, info) in licenses.iter() {
        println!("  * {}", spdx);
        println!("      - {}", info.url);
    }
    println!();
    println!("Copies of these licenses are provided at the end of this document. They may also be obtained from the URLs above.");
    println!();

    for id in log.keys() {
        let pkg = config
            .third_party
            .get(id)
            .ok_or_else(|| format!("3rd party package {} not in the allow list", id))?;
        println!("crate: {}", pkg.id);
        println!("url: {}", pkg.url());

        if pkg.licenses.is_empty() {
            return Err(format!("No license specified for {}", id).into());
        }

        let licenses: Vec<String> = pkg
            .licenses
            .iter()
            .map(|x| x.spdx_short().to_string())
            .collect();
        println!("licenses: {}", licenses.join(" AND "));

        // write out copyright statements
        for lic in pkg.licenses.iter() {
            if let Some(lines) = lic.copyright() {
                for line in lines {
                    println!("{}", line);
                }
            }
        }

        println!();
    }

    for info in licenses.values() {
        println!("{}", info.text);
        println!();
    }

    Ok(())
}
