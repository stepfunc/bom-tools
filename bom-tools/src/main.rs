use cyclonedx_bom::models::component::Classification;
use cyclonedx_bom::models::license::{License, LicenseChoice, LicenseIdentifier, Licenses};
use cyclonedx_bom::models::organization::OrganizationalEntity;
use cyclonedx_bom::prelude::{Component, NormalizedString};
use semver::Version;
use std::fs::File;
use std::io::stdout;
use std::path::Path;

use cargo_bom::config::{Config, Package, Source};
use cargo_bom::log::BuildLog;

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
        Commands::GenLicensesDir {
            dir,
            file_name,
            config_path,
        } => gen_licenses_from_dir(&dir, &file_name, &config_path),
        Commands::GenBom {
            subject_name,
            log_path,
            config_path,
        } => gen_bom(&subject_name, &log_path, &config_path),
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
    let log = BuildLog::read_file(path)?;
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
    let log = BuildLog::read_file(log_path)?;
    let tree = tree::parse_tree(File::open(tree_path)?)?;

    let mut config = Config {
        targets: Default::default(),
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
    let log = BuildLog::read_file(log_path)?;
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

fn gen_licenses_from_dir(
    dir: &Path,
    file_name: &str,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let log = BuildLog::read_files_recursively(dir, file_name)?;
    cargo_bom::licenses::gen_licenses(log, config_path, stdout())?;
    Ok(())
}

fn gen_licenses(log_path: &Path, config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let log = BuildLog::read_file(log_path)?;
    cargo_bom::licenses::gen_licenses(log, config_path, stdout())?;
    Ok(())
}

fn get_component(
    name: &str,
    version: &Version,
    licenses: Vec<LicenseChoice>,
    copyright: Option<NormalizedString>,
) -> Component {
    use cyclonedx_bom::prelude::*;

    Component {
        component_type: Classification::Library,
        mime_type: None,
        bom_ref: None,
        supplier: None,
        author: None,
        publisher: None,
        group: None,
        name: NormalizedString::new(name),
        version: NormalizedString::new(&version.to_string()),
        description: None,
        scope: None,
        hashes: None,
        licenses: Some(Licenses(licenses)),
        copyright,
        cpe: None,
        purl: Some(Purl::new("cargo", name, &version.to_string()).unwrap()),
        swid: None,
        modified: None,
        pedigree: None,
        external_references: None,
        properties: None,
        components: None,
        evidence: None,
    }
}

fn gen_bom(
    target: &str,
    log_path: &Path,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use cyclonedx_bom::prelude::*;

    let mut log = BuildLog::read_file(log_path)?;
    let config: Config = serde_json::from_reader(File::open(config_path)?)?;

    let target = match config.targets.get(target) {
        None => return Err(format!("Target not found in config file: {target}").into()),
        Some(x) => x,
    };

    log.remove_build_deps(&config);
    let mut components: Vec<Component> = Default::default();
    for (name, usage) in log.packages.iter() {
        if name == &target.name {
            continue;
        }

        match config.third_party.get(name) {
            Some(package) => {
                for version in usage.versions.values() {
                    components.push(get_component(
                        name,
                        version,
                        package.licenses()?,
                        package.copyright(),
                    ));
                }
            }
            None => match config.vendor.get(name) {
                None => return Err(format!("Unknown dependency: {name}").into()),
                Some(_) => {
                    for version in usage.versions.values() {
                        components.push(get_component(
                            name,
                            version,
                            target.vendor_licenses()?,
                            Some(NormalizedString::new("Copyright Step Function I/O LLC")),
                        ));
                    }
                }
            },
        }
    }

    let bom = Bom {
        version: 1,
        serial_number: Some(UrnUuid::generate()),
        metadata: Some(Metadata {
            timestamp: Some(DateTime::now()?),
            tools: None,
            authors: None,
            component: Some(Component {
                component_type: Classification::Library,
                mime_type: None,
                bom_ref: None,
                supplier: Some(OrganizationalEntity {
                    name: Some(NormalizedString::new("Step Function I/O LLC")),
                    url: Some(vec![Uri::try_from("https://stepfunc.io".to_string())?]),
                    contact: None,
                }),
                author: None,
                publisher: None,
                group: Some(NormalizedString::new("io.stepfunc")),
                name: NormalizedString::new(&target.name),
                version: NormalizedString::new(&target.version),
                description: None,
                scope: None,
                hashes: None,
                licenses: Some(Licenses(vec![LicenseChoice::License(License {
                    license_identifier: LicenseIdentifier::Name(NormalizedString::new(
                        "Custom non-commercial license",
                    )),
                    text: None,
                    url: Some(Uri::try_from(target.license_url.clone())?),
                })])),
                copyright: Some(NormalizedString::new("Step Function I/O LLC")),
                cpe: None,
                purl: None,
                swid: None,
                modified: None,
                pedigree: None,
                external_references: None,
                properties: None,
                components: None,
                evidence: None,
            }),
            manufacture: None,
            supplier: None,
            licenses: None,
            properties: None,
        }),
        components: Some(Components(components)),
        services: None,
        external_references: None,
        dependencies: None,
        compositions: None,
        properties: None,
    };

    bom.output_as_json_v1_3(&mut std::io::BufWriter::new(std::io::stdout()))?;
    Ok(())
}
