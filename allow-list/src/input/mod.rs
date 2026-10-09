//! Input readers: turn one target's raw build evidence into a [`TargetGraph`].
//!
//! This is the only module that knows input formats. Today there is one reader, which combines
//! a cargo build log with two `cargo tree` outputs produced by the same invocation; see the
//! README for the producer recipe and for what the cross-checks below do and do not prove.

mod build_log;
mod cargo_tree;
mod pkgid;

use crate::graph::{PackageKey, Role, TargetGraph};
use anyhow::{anyhow, Context};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// File names used for each target in a directory of build evidence
const LOG_FILE: &str = "build.json";
const TREE_FILE: &str = "tree.txt";
const RUNTIME_TREE_FILE: &str = "runtime-tree.txt";

/// A package as compiled with a particular set of enabled features
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Variant {
    key: PackageKey,
    features: BTreeSet<String>,
}

/// The build evidence for one target
#[derive(Debug, Clone)]
pub(crate) enum TargetInput {
    /// A `cargo build --message-format json` log plus the full (`-e normal,build`) and runtime
    /// (`-e normal,no-proc-macro`) `cargo tree` outputs from the same invocation
    LogAndTree {
        log: PathBuf,
        tree: PathBuf,
        runtime_tree: PathBuf,
    },
}

impl std::fmt::Display for TargetInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LogAndTree { log, .. } => write!(f, "{}", log.display()),
        }
    }
}

impl TargetInput {
    /// The evidence for one target stored in a directory under the standard file names
    pub(crate) fn from_dir(dir: &Path) -> Self {
        Self::LogAndTree {
            log: dir.join(LOG_FILE),
            tree: dir.join(TREE_FILE),
            runtime_tree: dir.join(RUNTIME_TREE_FILE),
        }
    }

    /// Read the evidence and produce the graph of the build of `root`
    pub(crate) fn read(&self, root: &str) -> Result<TargetGraph, anyhow::Error> {
        match self {
            Self::LogAndTree {
                log,
                tree,
                runtime_tree,
            } => {
                let compiled = build_log::read(BufReader::new(open(log)?), root)
                    .with_context(|| format!("reading {}", log.display()))?;
                let tree = parse_tree(tree)?;
                let runtime = parse_tree(runtime_tree)?;
                graph_from_log_and_tree(root, &compiled, tree, &runtime)
            }
        }
    }
}

fn open(path: &Path) -> Result<File, anyhow::Error> {
    File::open(path).with_context(|| format!("opening {}", path.display()))
}

fn parse_tree(path: &Path) -> Result<cargo_tree::Tree, anyhow::Error> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    cargo_tree::parse(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Cross-check the build log against the trees, then assign roles from the runtime tree
fn graph_from_log_and_tree(
    root: &str,
    compiled: &BTreeSet<Variant>,
    tree: cargo_tree::Tree,
    runtime: &cargo_tree::Tree,
) -> Result<TargetGraph, anyhow::Error> {
    if tree.root.name != root {
        return Err(anyhow!(
            "cargo tree root is `{}`, expected `{root}`",
            tree.root.name
        ));
    }
    if runtime.root != tree.root {
        return Err(anyhow!(
            "the runtime tree's root {} differs from the full tree's root {}",
            runtime.root,
            tree.root
        ));
    }

    let compiled_packages: BTreeSet<&PackageKey> = compiled.iter().map(|v| &v.key).collect();
    let tree_packages = tree.packages();
    check_same(
        "packages",
        tree_packages
            .difference(&compiled_packages)
            .map(|k| k.to_string()),
        compiled_packages
            .difference(&tree_packages)
            .map(|k| k.to_string()),
    )?;
    check_same(
        "(package, features) variants",
        tree.variants.difference(compiled).map(Variant::to_string),
        compiled.difference(&tree.variants).map(Variant::to_string),
    )?;
    let not_in_tree: Vec<String> = runtime
        .variants
        .difference(&tree.variants)
        .map(Variant::to_string)
        .collect();
    if !not_in_tree.is_empty() {
        return Err(anyhow!(
            "the runtime tree has variants missing from the full tree (different arguments?): {}",
            not_in_tree.join(", ")
        ));
    }

    let runtime_packages = runtime.packages();
    let packages: BTreeMap<PackageKey, Role> = tree_packages
        .into_iter()
        .map(|key| {
            let role = if runtime_packages.contains(key) {
                Role::Runtime
            } else {
                Role::BuildTime
            };
            (key.clone(), role)
        })
        .collect();
    TargetGraph::new(tree.root, packages, tree.edges)
}

/// Fail with a description of both differences if either is non-empty
fn check_same(
    what: &str,
    only_in_tree: impl Iterator<Item = String>,
    only_compiled: impl Iterator<Item = String>,
) -> Result<(), anyhow::Error> {
    let only_in_tree: Vec<String> = only_in_tree.collect();
    let only_compiled: Vec<String> = only_compiled.collect();
    if only_in_tree.is_empty() && only_compiled.is_empty() {
        return Ok(());
    }
    Err(anyhow!(
        "the build log and cargo tree disagree on {what} (were they produced by the same \
         invocation, in the same environment?)\n  in cargo tree but not compiled: {}\n  \
         compiled but not in cargo tree: {}",
        only_in_tree.join(", "),
        only_compiled.join(", ")
    ))
}

impl std::fmt::Display for Variant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let features: Vec<&str> = self.features.iter().map(String::as_str).collect();
        write!(f, "{} [{}]", self.key, features.join(","))
    }
}
