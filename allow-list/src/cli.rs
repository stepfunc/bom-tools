use crate::commands::{Evidence, SbomOptions};
use crate::input::{target_dirs, TargetInput};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub(crate) command: Commands,
}

// variant names are the CLI subcommand names, so they keep their common `Gen` prefix
#[allow(clippy::enum_variant_names)]
#[derive(Subcommand)]
pub(crate) enum Commands {
    /// outputs a human-readable report of the 3rd party licenses that ship in the product,
    /// from the build evidence of one target
    GenLicensesLog {
        #[clap(flatten)]
        target: TargetFiles,
        #[clap(flatten)]
        common: Common,
    },
    /// outputs a human-readable report of the 3rd party licenses that ship in the product,
    /// from the build evidence of every target in the subdirectories of a directory
    GenLicensesLogDir {
        #[clap(flatten)]
        targets: TargetDir,
        #[clap(flatten)]
        common: Common,
    },
    /// outputs a CycloneDX 1.5 JSON SBOM of the product, from the build evidence of one target
    GenSbomLog {
        #[clap(flatten)]
        target: TargetFiles,
        #[clap(flatten)]
        common: Common,
        #[clap(flatten)]
        sbom: SbomArgs,
    },
    /// outputs a CycloneDX 1.5 JSON SBOM of the product, from the build evidence of every
    /// target in the subdirectories of a directory
    GenSbomLogDir {
        #[clap(flatten)]
        targets: TargetDir,
        #[clap(flatten)]
        common: Common,
        #[clap(flatten)]
        sbom: SbomArgs,
    },
}

/// Options of the SBOM commands
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

/// The build evidence of one target
#[derive(Args)]
pub(crate) struct TargetFiles {
    /// path to the `cargo build --message-format json` log
    #[clap(value_parser, long, short = 'g', alias = "log-path")]
    log: PathBuf,
    /// path to the `cargo tree -e normal,build --prefix depth --format '{p}|{f}'` output
    #[clap(value_parser, long)]
    tree: PathBuf,
    /// path to the `cargo tree -e normal,no-proc-macro --prefix depth --format '{p}|{f}'` output
    #[clap(value_parser, long)]
    runtime_tree: PathBuf,
}

/// The build evidence of several targets
#[derive(Args)]
pub(crate) struct TargetDir {
    /// directory with one subdirectory per target, each containing `build.json`, `tree.txt`
    /// and `runtime-tree.txt`
    #[clap(value_parser, long, short = 'l')]
    list_dir: PathBuf,
}

/// Arguments shared by every command that reads build evidence
#[derive(Args)]
pub(crate) struct Common {
    /// name of the workspace package that is the product (e.g. `dnp3-ffi`)
    #[clap(long)]
    root_package: String,
    /// path to `cargo metadata --format-version 1 --all-features` JSON output
    #[clap(value_parser, long, short = 'm')]
    metadata_path: PathBuf,
    /// path to the JSON configuration (allow-list)
    #[clap(value_parser, long, short = 'c')]
    config_path: PathBuf,
}

impl TargetFiles {
    pub(crate) fn evidence(self, common: Common) -> Evidence {
        let target = TargetInput::LogAndTree {
            log: self.log,
            tree: self.tree,
            runtime_tree: self.runtime_tree,
        };
        common.evidence(vec![target])
    }
}

impl TargetDir {
    pub(crate) fn evidence(self, common: Common) -> Result<Evidence, anyhow::Error> {
        Ok(common.evidence(target_dirs(&self.list_dir)?))
    }
}

impl Common {
    fn evidence(self, targets: Vec<TargetInput>) -> Evidence {
        Evidence {
            targets,
            root_package: self.root_package,
            metadata: self.metadata_path,
            config: self.config_path,
        }
    }
}
