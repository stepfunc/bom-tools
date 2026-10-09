//! Parser for `cargo tree --prefix depth --color never --format '{p}|{f}'` output
//!
//! Every line has the form
//!
//! ```text
//! <depth><name> v<version>[ (proc-macro)][ (<source>)]|<features, comma separated>[ (*)]
//! ```
//!
//! where `(*)` marks a node whose dependencies were already listed earlier. Any other line is an
//! error, so a change in Cargo's output fails loudly instead of silently dropping packages.

use super::Variant;
use crate::graph::{PackageKey, SourceKind};
use anyhow::anyhow;
use semver::Version;
use std::collections::BTreeSet;

/// A parsed dependency tree
#[derive(Debug)]
pub(crate) struct Tree {
    /// The package at depth 0
    pub(crate) root: PackageKey,
    /// Every (package, enabled features) node in the tree
    pub(crate) variants: BTreeSet<Variant>,
    /// Package-level dependency edges, from every expansion in the tree
    pub(crate) edges: BTreeSet<(PackageKey, PackageKey)>,
}

impl Tree {
    /// The distinct packages in the tree
    pub(crate) fn packages(&self) -> BTreeSet<&PackageKey> {
        self.variants.iter().map(|v| &v.key).collect()
    }
}

/// Parse the full text of a `cargo tree` invocation with a single root
pub(crate) fn parse(text: &str) -> Result<Tree, anyhow::Error> {
    let mut root = None;
    let mut variants = BTreeSet::new();
    let mut edges = BTreeSet::new();
    // ancestors[d] is the package most recently seen at depth d
    let mut ancestors: Vec<PackageKey> = Vec::new();
    // depth of the previous line if it was a `(*)` node, whose children are never listed
    let mut previous_deduplicated: Option<usize> = None;

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let (depth, variant, deduplicated) = parse_line(line)
            .map_err(|err| anyhow!("cargo tree line {line_number}: {err}: {line:?}"))?;

        if previous_deduplicated.is_some_and(|d| depth > d) {
            return Err(anyhow!(
                "cargo tree line {line_number}: children listed under a `(*)` node"
            ));
        }
        if depth == 0 {
            if root.is_some() {
                return Err(anyhow!("cargo tree line {line_number}: more than one root"));
            }
            root = Some(variant.key.clone());
        } else {
            let parent = ancestors.get(depth - 1).ok_or_else(|| {
                anyhow!("cargo tree line {line_number}: depth {depth} has no parent")
            })?;
            edges.insert((parent.clone(), variant.key.clone()));
        }
        ancestors.truncate(depth);
        ancestors.push(variant.key.clone());
        previous_deduplicated = deduplicated.then_some(depth);
        variants.insert(variant);
    }

    let root = root.ok_or_else(|| anyhow!("cargo tree output is empty"))?;
    Ok(Tree {
        root,
        variants,
        edges,
    })
}

/// Parse one line into its depth, variant, and whether it is a deduplicated `(*)` node
fn parse_line(line: &str) -> Result<(usize, Variant, bool), String> {
    let (line, deduplicated) = match line.strip_suffix(" (*)") {
        Some(rest) => (rest, true),
        None => (line, false),
    };
    let digits = line
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(line.len());
    if digits == 0 {
        return Err("missing depth prefix (expected `--prefix depth`)".to_string());
    }
    let depth = line[..digits]
        .parse()
        .map_err(|err| format!("invalid depth: {err}"))?;
    let (package, features) = line[digits..]
        .rsplit_once('|')
        .ok_or("missing `|<features>` (expected `--format '{p}|{f}'`)")?;
    // Cargo prints the marker between the version and the source
    let package = package.replacen(" (proc-macro)", "", 1);
    let (name, rest) = package
        .split_once(" v")
        .ok_or("expected `<name> v<version>`")?;
    let (version, source) = match rest.split_once(" (") {
        None => (rest, SourceKind::CratesIo),
        Some((version, source)) => {
            let source = source.strip_suffix(')').ok_or("unterminated source")?;
            (version, source_kind(source))
        }
    };
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Err("invalid package name".to_string());
    }
    let version = Version::parse(version).map_err(|err| format!("invalid version: {err}"))?;
    let features = features
        .split(',')
        .filter(|f| !f.is_empty())
        .map(str::to_string)
        .collect();
    let key = PackageKey {
        name: name.to_string(),
        version,
        source,
    };
    Ok((depth, Variant { key, features }, deduplicated))
}

/// `cargo tree` prints local paths for path packages, and URLs (or `registry ...`) otherwise
fn source_kind(source: &str) -> SourceKind {
    let windows_drive = source.as_bytes().get(1..3) == Some(b":\\".as_slice());
    if source.starts_with('/') || source.starts_with("\\\\") || windows_drive {
        SourceKind::Path
    } else {
        SourceKind::Other(source.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_util::{key, path_key};

    #[test]
    fn parses_every_line_form() {
        let tree = parse(concat!(
            "0app v1.0.0 (/build/app)|default,std\n",
            "1serde v1.0.228|std\n",
            "1serde_derive v1.0.228 (proc-macro)|default\n",
            "2serde v1.0.228|std (*)\n",
            "1local_macro v0.1.0 (proc-macro) (/build/local_macro)|\n",
            "1local v0.1.0 (C:\\build\\local)|\n",
            "1gitdep v0.2.0 (https://github.com/x/y#abc123)|\n",
        ))
        .unwrap();
        assert_eq!(tree.root, path_key("app", "1.0.0"));
        assert!(tree
            .variants
            .contains(&Variant::new(path_key("app", "1.0.0"), &["default", "std"])));
        assert!(tree
            .variants
            .contains(&Variant::new(key("serde_derive", "1.0.228"), &["default"])));
        assert!(tree
            .variants
            .contains(&Variant::new(path_key("local", "0.1.0"), &[])));
        let git = tree
            .packages()
            .into_iter()
            .find(|k| k.name == "gitdep")
            .unwrap()
            .clone();
        assert_eq!(
            git.source,
            SourceKind::Other("https://github.com/x/y#abc123".to_string())
        );
        assert!(tree
            .variants
            .contains(&Variant::new(path_key("local_macro", "0.1.0"), &[])));
        assert_eq!(tree.variants.len(), 6);
        assert!(tree
            .edges
            .contains(&(key("serde_derive", "1.0.228"), key("serde", "1.0.228"))));
        assert_eq!(tree.edges.len(), 6);
    }

    #[test]
    fn repeated_expansions_with_different_children_all_contribute_edges() {
        // e.g. a host and a target variant of the same package with different dependencies
        let tree = parse(concat!(
            "0app v1.0.0|\n",
            "1dep v1.0.0|target\n",
            "2a v1.0.0|\n",
            "1tool v1.0.0|\n",
            "2dep v1.0.0|host\n",
            "3b v1.0.0|\n",
        ))
        .unwrap();
        assert!(tree
            .edges
            .contains(&(key("dep", "1.0.0"), key("a", "1.0.0"))));
        assert!(tree
            .edges
            .contains(&(key("dep", "1.0.0"), key("b", "1.0.0"))));
        assert_eq!(tree.variants.len(), 6);
        assert_eq!(tree.packages().len(), 5);
    }

    #[test]
    fn rejects_malformed_output() {
        for bad in [
            "",
            "app v1.0.0|\n",                              // no depth prefix
            "0app v1.0.0\n",                              // no features
            "├── app v1.0.0|\n",                          // indent prefix
            "0app v1.0.0|\n0other v1.0.0|\n",             // two roots
            "0app v1.0.0|\n2deep v1.0.0|\n",              // skipped a level
            "0app v1.0.0|\n1a v1.0.0| (*)\n2b v1.0.0|\n", // children under (*)
            "0app vnot-a-version|\n",
            "0app v1.0.0 (/x|\n",
            "0app v1.0.0|\n\n", // blank line
        ] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
    }
}
