use std::fs::File;
use std::io::stdout;
use std::path::Path;

use cargo_bom::config::{Config, Package, Source};
use cargo_bom::{bom, log};

use crate::cli::*;

/// cli interface for the application
pub(crate) mod cli;
/// parse the output of cargo tree
pub(crate) mod tree;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser;

    let cli = crate::Cli::parse();

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
        Commands::GenBom {
            subject_name,
            log_path,
            config_path,
            output_path,
        } => gen_bom(subject_name, &log_path, &config_path, &output_path),
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
    let log = log::read_log(path)?;
    for (id, usage) in log.packages {
        println!("{} {}", id, usage.versions)
    }
    Ok(())
}

fn generate_config(
    log_path: &Path,
    tree_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let log = log::read_log(log_path)?;
    let tree = tree::parse_tree(File::open(tree_path)?)?;

    let mut config = Config {
        build_only: Default::default(),
        vendor: Default::default(),
        third_party: Default::default(),
    };

    // then tell us what's in log that isn't in the tree
    for (id, usage) in log.packages.iter() {
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
    let log = log::read_log(log_path)?;
    let tree = tree::parse_tree(File::open(tree_path)?)?;

    // first, make sure that everything in tree is in packages
    for dep in tree.iter() {
        match log.packages.get(&dep.id) {
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
    for (id, usage) in log.packages.iter() {
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
    cargo_bom::licenses::gen_licenses(log_path, config_path, stdout())?;
    Ok(())
}

fn gen_bom(
    subject: String,
    log_path: &Path,
    config_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let log = log::read_log(log_path)?;
    let config: Config = serde_json::from_reader(File::open(config_path)?)?;

    let bom = bom::create_bom(subject, log, config)?;

    serde_json::to_writer_pretty(
        std::io::BufWriter::new(std::fs::File::create(output_path)?),
        &bom,
    )?;
    Ok(())
}
