//! Input readers: turn one target's raw build evidence into a [`TargetGraph`].
//!
//! This is the only module that knows input formats. Today there is one reader, which combines
//! a cargo build log with two `cargo tree` outputs, run as separate commands with identical
//! arguments in the build's environment; see the README for the producer recipe and for what
//! the cross-checks below do and do not prove.

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

#[cfg(test)]
impl Variant {
    fn new(key: PackageKey, features: &[&str]) -> Self {
        Self {
            key,
            features: features.iter().map(|f| f.to_string()).collect(),
        }
    }
}

/// The build evidence for one target
#[derive(Debug, Clone)]
pub(crate) enum TargetInput {
    /// A `cargo build --message-format json` log plus the full (`-e normal,build`) and runtime
    /// (`-e normal,no-proc-macro`) `cargo tree` outputs, run with the same arguments in the
    /// build's environment
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

/// The evidence of every target: one per immediate subdirectory of `dir`, in sorted order,
/// each holding the standard file names
pub(crate) fn target_dirs(dir: &Path) -> Result<Vec<TargetInput>, anyhow::Error> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    if dirs.is_empty() {
        return Err(anyhow!("{} has no target subdirectories", dir.display()));
    }
    dirs.sort();
    Ok(dirs.iter().map(|d| TargetInput::from_dir(d)).collect())
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
                let log = build_log::read(BufReader::new(open(log)?))
                    .with_context(|| format!("reading {}", log.display()))?;
                let tree = parse_tree(tree)?;
                let runtime = parse_tree(runtime_tree)?;
                graph_from_log_and_tree(root, &log, tree, &runtime)
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
    log: &build_log::BuildLog,
    tree: cargo_tree::Tree,
    runtime: &cargo_tree::Tree,
) -> Result<TargetGraph, anyhow::Error> {
    if tree.root.name != root {
        return Err(anyhow!(
            "cargo tree root is `{}`, expected `{root}`",
            tree.root.name
        ));
    }
    if !log.products.contains(&tree.root) {
        return Err(anyhow!(
            "the build log has no product artifact for {} (wrong -p, or `cargo check`?)",
            tree.root
        ));
    }
    if runtime.root != tree.root {
        return Err(anyhow!(
            "the runtime tree's root {} differs from the full tree's root {}",
            runtime.root,
            tree.root
        ));
    }

    let compiled = &log.variants;
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

#[cfg(test)]
mod tests {
    use super::build_log::BuildLog;
    use super::*;
    use crate::graph::test_util::{key, path_key};

    const TREE: &str = "0app v1.0.0 (/build/app)|default\n1serde v1.0.0|std\n1cc v1.0.0|\n";
    const RUNTIME: &str = "0app v1.0.0 (/build/app)|default\n1serde v1.0.0|std\n";

    fn log() -> BuildLog {
        BuildLog {
            variants: BTreeSet::from([
                Variant::new(path_key("app", "1.0.0"), &["default"]),
                Variant::new(key("serde", "1.0.0"), &["std"]),
                Variant::new(key("cc", "1.0.0"), &[]),
            ]),
            products: BTreeSet::from([path_key("app", "1.0.0")]),
        }
    }

    fn graph(log: &BuildLog, tree: &str, runtime: &str) -> Result<TargetGraph, String> {
        let tree = cargo_tree::parse(tree).unwrap();
        let runtime = cargo_tree::parse(runtime).unwrap();
        graph_from_log_and_tree("app", log, tree, &runtime).map_err(|e| e.to_string())
    }

    #[test]
    fn assigns_roles_from_the_runtime_tree() {
        let graph = graph(&log(), TREE, RUNTIME).unwrap();
        assert_eq!(graph.packages()[&key("serde", "1.0.0")], Role::Runtime);
        assert_eq!(graph.packages()[&key("cc", "1.0.0")], Role::BuildTime);
        assert_eq!(graph.edges().len(), 2);
    }

    #[test]
    fn rejects_a_package_missing_from_either_side() {
        let mut dropped = log();
        dropped
            .variants
            .remove(&Variant::new(key("cc", "1.0.0"), &[]));
        assert!(graph(&dropped, TREE, RUNTIME)
            .unwrap_err()
            .contains("cc@1.0.0"));
        let mut extra = log();
        extra
            .variants
            .insert(Variant::new(key("ring", "1.0.0"), &[]));
        assert!(graph(&extra, TREE, RUNTIME)
            .unwrap_err()
            .contains("ring@1.0.0"));
    }

    #[test]
    fn rejects_a_variant_with_different_features() {
        let mut other = log();
        other
            .variants
            .remove(&Variant::new(key("serde", "1.0.0"), &["std"]));
        other
            .variants
            .insert(Variant::new(key("serde", "1.0.0"), &["std", "derive"]));
        let err = graph(&other, TREE, RUNTIME).unwrap_err();
        assert!(err.contains("variants"), "{err}");
    }

    #[test]
    fn rejects_a_runtime_tree_from_different_arguments() {
        let runtime = "0app v1.0.0 (/build/app)|default\n1serde v1.0.0|std,alloc\n";
        let err = graph(&log(), TREE, runtime).unwrap_err();
        assert!(err.contains("runtime tree has variants missing"), "{err}");
    }

    #[test]
    fn rejects_mismatched_roots() {
        let runtime = "0other v1.0.0 (/build/other)|\n";
        assert!(graph(&log(), TREE, runtime).unwrap_err().contains("root"));
        let tree = cargo_tree::parse(TREE).unwrap();
        let runtime = cargo_tree::parse(RUNTIME).unwrap();
        assert!(graph_from_log_and_tree("other", &log(), tree, &runtime).is_err());
    }

    #[test]
    fn requires_a_product_artifact_for_the_root() {
        let mut check = log();
        check.products.clear();
        // a dependency with the same name does not count
        check.products.insert(key("app", "2.0.0"));
        assert!(graph(&check, TREE, RUNTIME)
            .unwrap_err()
            .contains("no product artifact"));
    }
}
