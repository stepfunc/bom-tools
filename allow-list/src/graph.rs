//! The format-neutral contract between input readers and the core.
//!
//! An input reader turns one target's raw build evidence into a [`TargetGraph`]. Nothing after
//! a reader knows any input format, so new readers (e.g. Cargo's SBOM precursor files) can be
//! added without touching the core.

use anyhow::anyhow;
use semver::Version;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Source ids under which Cargo refers to crates.io
const CRATES_IO_SOURCES: [&str; 2] = [
    "registry+https://github.com/rust-lang/crates.io-index",
    "sparse+https://index.crates.io/",
];

/// Whether a Cargo source id (as in package ids and `cargo metadata`) refers to crates.io
pub(crate) fn is_crates_io(source: &str) -> bool {
    CRATES_IO_SOURCES.contains(&source)
}

/// Where a package came from, as every reader can state it
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SourceKind {
    /// The crates.io registry
    CratesIo,
    /// A local path (first-party code, matched to workspace members by the core)
    Path,
    /// Any other source (git, alternate registry), kept verbatim for error messages
    Other(String),
}

/// Identity of a package, independent of any input format
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PackageKey {
    pub(crate) name: String,
    pub(crate) version: Version,
    pub(crate) source: SourceKind,
}

impl fmt::Display for PackageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)?;
        match &self.source {
            SourceKind::CratesIo => Ok(()),
            SourceKind::Path => write!(f, " (path)"),
            SourceKind::Other(source) => write!(f, " ({source})"),
        }
    }
}

/// How a package participates in the product
///
/// Ordered so that the maximum of two roles is the one that wins when merging.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Role {
    /// Only used to build the product (build scripts, proc-macros and their dependencies)
    BuildTime,
    /// Linked into the product
    Runtime,
}

/// What one build of one target proved
#[derive(Debug)]
pub(crate) struct TargetGraph {
    root: PackageKey,
    packages: BTreeMap<PackageKey, Role>,
    edges: BTreeSet<(PackageKey, PackageKey)>,
}

impl TargetGraph {
    /// Construct a graph, enforcing the contract's invariants: the root is a member, every edge
    /// connects members, and there are no self-edges (they are dropped).
    pub(crate) fn new(
        root: PackageKey,
        packages: BTreeMap<PackageKey, Role>,
        edges: BTreeSet<(PackageKey, PackageKey)>,
    ) -> Result<Self, anyhow::Error> {
        if !packages.contains_key(&root) {
            return Err(anyhow!("root package {root} is not a member of its graph"));
        }
        if let Some((from, to)) = edges
            .iter()
            .find(|(from, to)| !packages.contains_key(from) || !packages.contains_key(to))
        {
            return Err(anyhow!(
                "dependency edge {from} -> {to} has an unknown endpoint"
            ));
        }
        let edges = edges.into_iter().filter(|(from, to)| from != to).collect();
        Ok(Self {
            root,
            packages,
            edges,
        })
    }

    pub(crate) fn root(&self) -> &PackageKey {
        &self.root
    }

    pub(crate) fn packages(&self) -> &BTreeMap<PackageKey, Role> {
        &self.packages
    }

    pub(crate) fn edges(&self) -> &BTreeSet<(PackageKey, PackageKey)> {
        &self.edges
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;

    /// A crates.io key, for tests
    pub(crate) fn key(name: &str, version: &str) -> PackageKey {
        PackageKey {
            name: name.to_string(),
            version: Version::parse(version).unwrap(),
            source: SourceKind::CratesIo,
        }
    }

    /// A path (workspace) key, for tests
    pub(crate) fn path_key(name: &str, version: &str) -> PackageKey {
        PackageKey {
            source: SourceKind::Path,
            ..key(name, version)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;

    #[test]
    fn rejects_root_outside_graph() {
        let err = TargetGraph::new(key("a", "1.0.0"), BTreeMap::new(), BTreeSet::new());
        assert!(err.is_err());
    }

    #[test]
    fn rejects_edge_with_unknown_endpoint() {
        let root = key("a", "1.0.0");
        let packages = BTreeMap::from([(root.clone(), Role::Runtime)]);
        let edges = BTreeSet::from([(root.clone(), key("b", "1.0.0"))]);
        assert!(TargetGraph::new(root, packages, edges).is_err());
    }

    #[test]
    fn drops_self_edges() {
        let root = key("a", "1.0.0");
        let packages = BTreeMap::from([(root.clone(), Role::Runtime)]);
        let edges = BTreeSet::from([(root.clone(), root.clone())]);
        let graph = TargetGraph::new(root, packages, edges).unwrap();
        assert!(graph.edges().is_empty());
    }

    #[test]
    fn runtime_wins_when_merging_roles() {
        assert_eq!(Role::BuildTime.max(Role::Runtime), Role::Runtime);
    }
}
