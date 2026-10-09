use crate::commands::{Evidence, SbomOptions};
use crate::input::target_dirs;
use clap::{Args, Parser, Subcommand};
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
    /// outputs a human-readable report of the 3rd party licenses that ship in the product
    Licenses {
        #[clap(flatten)]
        evidence: EvidenceArgs,
    },
    /// outputs a CycloneDX 1.5 JSON SBOM of the product
    Sbom {
        #[clap(flatten)]
        evidence: EvidenceArgs,
        #[clap(flatten)]
        sbom: SbomArgs,
    },
}

/// Arguments shared by every command
#[derive(Args)]
pub(crate) struct EvidenceArgs {
    /// directory with one subdirectory per target the product is built for, each containing
    /// `build.json`, `tree.txt` and `runtime-tree.txt`
    #[clap(value_parser, long, short = 'e')]
    evidence_dir: PathBuf,
    /// name of the workspace package that is the product (e.g. `dnp3-ffi`)
    #[clap(long)]
    root_package: String,
    /// path to `cargo metadata --format-version 1 --all-features` JSON output
    #[clap(value_parser, long, short = 'm')]
    metadata_path: PathBuf,
    /// path to the JSON configuration (the allow list, e.g. `allowed.json`)
    #[clap(value_parser, long, short = 'c')]
    config_path: PathBuf,
}

impl EvidenceArgs {
    pub(crate) fn evidence(self) -> Result<Evidence, anyhow::Error> {
        Ok(Evidence {
            targets: target_dirs(&self.evidence_dir)?,
            root_package: self.root_package,
            metadata: self.metadata_path,
            config: self.config_path,
        })
    }
}

/// Options of the SBOM command
#[derive(Args)]
pub(crate) struct SbomArgs {
    /// path to the `Cargo.lock`, to add SHA-256 hashes of crates.io packages
    #[clap(value_parser, long)]
    lockfile: Option<PathBuf>,
    /// omit the random serial number, for reproducible output (with `SOURCE_DATE_EPOCH`)
    #[clap(long)]
    omit_serial_number: bool,
}

impl From<SbomArgs> for SbomOptions {
    fn from(args: SbomArgs) -> Self {
        Self {
            lockfile: args.lockfile,
            omit_serial_number: args.omit_serial_number,
        }
    }
}
