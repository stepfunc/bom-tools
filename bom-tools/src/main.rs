use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use cargo_bom::config::{Config, LicenseInfo, Package, Source};
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
    let mut log = log::read_log(log_path)?;
    let config: Config = serde_json::from_reader(File::open(config_path)?)?;

    // we don't care about these for license purposes, just the OSS that is linked into the library
    log.remove_build_deps(&config);
    log.remove_vendor_deps(&config);

    // first summarize the licenses
    let mut licenses: BTreeMap<&'static str, LicenseInfo> = BTreeMap::new();
    for (id, _) in log.packages.iter() {
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

    for id in log.packages.keys() {
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
