//! Command glue: load the evidence once, run the core, render an output

use crate::approval::{self, ValidatedInventory};
use crate::config::Config;
use crate::input::TargetInput;
use crate::sbom::{self, Checksums};
use crate::{inventory, licenses};
use anyhow::{anyhow, Context};
use cargo_metadata::Metadata;
use cyclonedx_bom::prelude::DateTime;
use serde::de::DeserializeOwned;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Everything needed to describe one product build
pub(crate) struct Evidence {
    /// One entry per target the product is built for
    pub(crate) targets: Vec<TargetInput>,
    /// Name of the workspace package that is the product
    pub(crate) root_package: String,
    /// `cargo metadata --format-version 1 --all-features` output
    pub(crate) metadata: PathBuf,
    /// The allow-list configuration
    pub(crate) config: PathBuf,
}

/// Write the human-readable report of the open-source licenses that ship in the product
pub(crate) fn gen_licenses<W: Write>(evidence: &Evidence, w: W) -> Result<(), anyhow::Error> {
    let config: Config = load_json(&evidence.config)?;
    let metadata: Metadata = load_json(&evidence.metadata)?;
    let validated = validate(evidence, &metadata, &config)?;
    Ok(licenses::write(&validated, w)?)
}

/// Options of the SBOM commands
pub(crate) struct SbomOptions {
    /// `Cargo.lock` from which to take SHA-256 hashes of crates.io packages
    pub(crate) lockfile: Option<PathBuf>,
    /// Omit the random serial number, for reproducible output
    pub(crate) omit_serial_number: bool,
}

/// Write a CycloneDX SBOM of the product
pub(crate) fn gen_sbom<W: Write>(
    evidence: &Evidence,
    options: &SbomOptions,
    w: W,
) -> Result<(), anyhow::Error> {
    let config: Config = load_json(&evidence.config)?;
    let metadata: Metadata = load_json(&evidence.metadata)?;
    let validated = validate(evidence, &metadata, &config)?;
    let checksums = match &options.lockfile {
        Some(path) => Some(
            Checksums::from_lockfile(&std::fs::read_to_string(path)?)
                .with_context(|| format!("parsing {}", path.display()))?,
        ),
        None => None,
    };
    let options = sbom::Options {
        checksums: checksums.as_ref(),
        omit_serial_number: options.omit_serial_number,
        timestamp: timestamp()?,
    };
    sbom::write(&validated, config.commercial_license.as_ref(), &options, w)
}

/// `SOURCE_DATE_EPOCH` (for reproducible builds) if set, otherwise now
fn timestamp() -> Result<DateTime, anyhow::Error> {
    match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(epoch) => {
            let epoch: i64 = epoch
                .trim()
                .parse()
                .with_context(|| format!("invalid SOURCE_DATE_EPOCH {epoch:?}"))?;
            let time = OffsetDateTime::from_unix_timestamp(epoch)?.format(&Rfc3339)?;
            Ok(DateTime::try_from(time)?)
        }
        Err(std::env::VarError::NotPresent) => Ok(DateTime::now()?),
        Err(err) => Err(anyhow!("invalid SOURCE_DATE_EPOCH: {err}")),
    }
}

/// Read every target's evidence and validate the product's inventory against the configuration
pub(crate) fn validate<'m, 'c>(
    evidence: &Evidence,
    metadata: &'m Metadata,
    config: &'c Config,
) -> Result<ValidatedInventory<'m, 'c>, anyhow::Error> {
    let graphs = evidence
        .targets
        .iter()
        .map(|target| {
            target
                .read(&evidence.root_package)
                .with_context(|| format!("reading build evidence: {target}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let inventory = inventory::build(&graphs, metadata, |name| config.is_embedded(name))?;
    approval::validate(inventory, config)
}

/// Load a JSON file: the allow-list configuration, or the `cargo metadata --format-version 1`
/// output (which is only a dictionary of package details: license, source, workspace
/// membership; it never determines scope or roles)
fn load_json<T: DeserializeOwned>(path: &Path) -> Result<T, anyhow::Error> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parsing {}", path.display()))
}
