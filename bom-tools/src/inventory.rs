//! Core: merge the per-target graphs and resolve them against the `cargo metadata` dictionary

use crate::graph::{is_crates_io, PackageKey, Role, SourceKind, TargetGraph};
use anyhow::anyhow;
use cargo_metadata::{Metadata, Package, PackageId};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A package's declared `license` field, parsed and normalized once for every consumer
#[derive(Debug)]
pub(crate) enum DeclaredLicense {
    /// A valid SPDX expression, normalized (e.g. legacy `MIT/Apache-2.0` becomes `MIT OR Apache-2.0`)
    Valid(Box<spdx::Expression>),
    /// No `license` field (the package may only have a `license-file`)
    Missing,
    /// A `license` field that is not a valid SPDX expression, with the reason
    Invalid(String),
}

/// A package in the product's inventory
#[derive(Debug)]
pub(crate) struct Component<'m> {
    pub(crate) package: &'m Package,
    pub(crate) role: Role,
    /// A workspace member: our own code, needing no approval
    pub(crate) first_party: bool,
    pub(crate) license: DeclaredLicense,
}

/// Every package of the product across all targets, resolved against the metadata
#[derive(Debug)]
pub(crate) struct Inventory<'m> {
    /// The product itself
    pub(crate) root: &'m Package,
    /// All other packages, sorted by name, version and id
    pub(crate) components: Vec<Component<'m>>,
    /// Package-level dependency edges, including those from the root
    pub(crate) edges: BTreeSet<(&'m PackageId, &'m PackageId)>,
}

/// Merge the graphs of all targets and resolve every package against the metadata.
///
/// A package is runtime if it is runtime in any target, or if `embedded` says its code ships
/// even though it is only a build-time dependency.
pub(crate) fn build<'m>(
    graphs: &[TargetGraph],
    metadata: &'m Metadata,
    embedded: impl Fn(&str) -> bool,
) -> Result<Inventory<'m>, anyhow::Error> {
    let (first, rest) = graphs
        .split_first()
        .ok_or_else(|| anyhow!("no build evidence (no target directories?)"))?;
    let root_key = first.root();
    if let Some(other) = rest.iter().find(|g| g.root() != root_key) {
        return Err(anyhow!(
            "targets have different roots: {root_key} and {}",
            other.root()
        ));
    }

    let mut roles: BTreeMap<&PackageKey, Role> = BTreeMap::new();
    let mut key_edges = BTreeSet::new();
    for graph in graphs {
        for (key, role) in graph.packages() {
            let merged = roles.entry(key).or_insert(*role);
            *merged = (*merged).max(*role);
        }
        key_edges.extend(graph.edges());
    }

    let resolved = resolve(roles.keys().copied(), metadata)?;
    let root = resolved[root_key];
    let workspace: BTreeSet<&PackageId> = metadata.workspace_members.iter().collect();
    // the root is never approved or license-checked, so it must be our own code
    if !workspace.contains(&root.id) {
        return Err(anyhow!(
            "the root package {root_key} is not a workspace member"
        ));
    }

    let mut components: Vec<Component> = roles
        .iter()
        .filter(|(key, _)| *key != &root_key)
        .map(|(key, role)| {
            let package = resolved[key];
            let role = if embedded(&package.name) {
                Role::Runtime
            } else {
                *role
            };
            Component {
                package,
                role,
                first_party: workspace.contains(&package.id),
                license: declared_license(package),
            }
        })
        .collect();
    components.sort_by(|a, b| {
        (&a.package.name, &a.package.version, &a.package.id).cmp(&(
            &b.package.name,
            &b.package.version,
            &b.package.id,
        ))
    });

    let edges = key_edges
        .into_iter()
        .map(|(from, to)| (&resolved[from].id, &resolved[to].id))
        .collect();
    Ok(Inventory {
        root,
        components,
        edges,
    })
}

/// Resolve each key to exactly one metadata package, reporting every failure at once
fn resolve<'k, 'm>(
    keys: impl Iterator<Item = &'k PackageKey>,
    metadata: &'m Metadata,
) -> Result<HashMap<&'k PackageKey, &'m Package>, anyhow::Error> {
    let workspace: BTreeSet<&PackageId> = metadata.workspace_members.iter().collect();
    let mut by_name_version: HashMap<(&str, &semver::Version), Vec<&Package>> = HashMap::new();
    for package in &metadata.packages {
        by_name_version
            .entry((package.name.as_str(), &package.version))
            .or_default()
            .push(package);
    }

    let mut resolved = HashMap::new();
    let mut errors = Vec::new();
    for key in keys {
        let candidates = by_name_version
            .get(&(key.name.as_str(), &key.version))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let matching: Vec<&Package> = match &key.source {
            SourceKind::CratesIo => candidates
                .iter()
                .filter(|p| p.source.as_ref().is_some_and(|s| is_crates_io(&s.repr)))
                .copied()
                .collect(),
            SourceKind::Path => candidates
                .iter()
                .filter(|p| p.source.is_none())
                .copied()
                .collect(),
            SourceKind::Other(source) => {
                errors.push(format!(
                    "{key}: unsupported source `{source}` (only crates.io and workspace members are supported)"
                ));
                continue;
            }
        };
        match matching.as_slice() {
            [package] if key.source == SourceKind::Path && !workspace.contains(&package.id) => {
                errors.push(format!("{key}: path dependency that is not a workspace member"));
            }
            [package] => {
                resolved.insert(key, *package);
            }
            [] => errors.push(format!(
                "{key}: not found in the cargo metadata (generate it with `--all-features` from the same checkout)"
            )),
            _ => errors.push(format!("{key}: ambiguous, matches several metadata packages")),
        }
    }

    if errors.is_empty() {
        Ok(resolved)
    } else {
        Err(anyhow!(
            "cannot resolve packages against the cargo metadata:\n  {}",
            errors.join("\n  ")
        ))
    }
}

fn declared_license(package: &Package) -> DeclaredLicense {
    let Some(declared) = &package.license else {
        return DeclaredLicense::Missing;
    };
    let normalized = spdx::Expression::canonicalize(declared)
        .map(|canonical| canonical.unwrap_or_else(|| declared.clone()))
        .and_then(|canonical| spdx::Expression::parse(&canonical));
    match normalized {
        Ok(expression) => DeclaredLicense::Valid(Box::new(expression)),
        Err(err) => DeclaredLicense::Invalid(err.to_string()),
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use crate::graph::test_util::{graph, path_key};
    use crate::graph::{PackageKey, Role, SourceKind, TargetGraph};
    use cargo_metadata::Metadata;
    use serde_json::{json, Value};

    /// A metadata package for a key, with the given declared license
    pub(crate) fn package(key: &PackageKey, license: Option<&str>) -> Value {
        let id = match key.source {
            SourceKind::Path => format!("path+file:///build/{}#{}", key.name, key.version),
            _ => format!(
                "registry+https://github.com/rust-lang/crates.io-index#{}@{}",
                key.name, key.version
            ),
        };
        let source = match key.source {
            SourceKind::Path => Value::Null,
            _ => json!("registry+https://github.com/rust-lang/crates.io-index"),
        };
        json!({
            "name": key.name,
            "version": key.version.to_string(),
            "id": id,
            "license": license,
            "license_file": null,
            "description": null,
            "source": source,
            "dependencies": [],
            "targets": [],
            "features": {},
            "manifest_path": format!("/build/{}/Cargo.toml", key.name),
            "metadata": null,
            "publish": null,
            "authors": [],
            "categories": [],
            "keywords": [],
            "readme": null,
            "repository": null,
            "homepage": null,
            "documentation": null,
            "edition": "2021",
            "links": null,
            "default_run": null,
            "rust_version": null
        })
    }

    /// Metadata for the given packages; path packages are the workspace members
    pub(crate) fn metadata(packages: &[(PackageKey, Option<&str>)]) -> Metadata {
        let values: Vec<Value> = packages.iter().map(|(k, l)| package(k, *l)).collect();
        let members: Vec<Value> = values
            .iter()
            .filter(|p| p["source"].is_null())
            .map(|p| p["id"].clone())
            .collect();
        serde_json::from_value(json!({
            "packages": values,
            "workspace_members": members,
            "workspace_default_members": members,
            "resolve": null,
            "workspace_root": "/build",
            "target_directory": "/build/target",
            "version": 1,
            "metadata": null
        }))
        .unwrap()
    }

    /// Metadata and the graph of a workspace root `app 1.0.0` that depends directly on each of
    /// the given (package, declared license, role)
    pub(crate) fn app_with(deps: &[(PackageKey, Option<&str>, Role)]) -> (Metadata, TargetGraph) {
        let app = path_key("app", "1.0.0");
        let mut packages: Vec<(PackageKey, Option<&str>)> =
            deps.iter().map(|(k, l, _)| (k.clone(), *l)).collect();
        packages.push((app.clone(), None));
        let members: Vec<(&PackageKey, Role)> = deps.iter().map(|(k, _, r)| (k, *r)).collect();
        let edges: Vec<(&PackageKey, &PackageKey)> =
            deps.iter().map(|(k, _, _)| (&app, k)).collect();
        (metadata(&packages), graph(&app, &members, &edges))
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::metadata;
    use super::*;
    use crate::graph::test_util::{graph, key, path_key};

    fn names<'a>(inventory: &'a Inventory, role: Role) -> Vec<&'a str> {
        inventory
            .components
            .iter()
            .filter(|c| c.role == role)
            .map(|c| c.package.name.as_str())
            .collect()
    }

    #[test]
    fn merges_targets_with_runtime_winning_and_resolves_against_metadata() {
        let app = path_key("app", "1.0.0");
        let (ring, aws, cc, tool) = (
            key("ring", "0.17.14"),
            key("aws-lc-rs", "1.18.1"),
            key("cc", "1.2.60"),
            key("tool", "1.0.0"),
        );
        let meta = metadata(&[
            (app.clone(), None),
            (ring.clone(), Some("Apache-2.0 AND ISC")),
            (aws.clone(), Some("ISC AND (Apache-2.0 OR ISC)")),
            (cc.clone(), Some("MIT/Apache-2.0")),
            (tool.clone(), None),
        ]);
        let graphs = [
            graph(
                &app,
                &[
                    (&ring, Role::Runtime),
                    (&cc, Role::BuildTime),
                    (&tool, Role::BuildTime),
                ],
                &[(&app, &ring), (&ring, &cc)],
            ),
            graph(
                &app,
                &[(&aws, Role::Runtime), (&cc, Role::Runtime)],
                &[(&app, &aws)],
            ),
        ];
        let inventory = build(&graphs, &meta, |name| name == "tool").unwrap();

        assert_eq!(inventory.root.name, "app");
        assert_eq!(
            names(&inventory, Role::Runtime),
            ["aws-lc-rs", "cc", "ring", "tool"]
        );
        assert!(names(&inventory, Role::BuildTime).is_empty());
        assert_eq!(inventory.edges.len(), 3);
        let cc = inventory
            .components
            .iter()
            .find(|c| c.package.name == "cc")
            .unwrap();
        match &cc.license {
            DeclaredLicense::Valid(expr) => assert_eq!(expr.to_string(), "MIT OR Apache-2.0"),
            other => panic!("unexpected {other:?}"),
        }
        let tool = inventory
            .components
            .iter()
            .find(|c| c.package.name == "tool")
            .unwrap();
        assert!(matches!(tool.license, DeclaredLicense::Missing));
        assert!(!tool.first_party);
    }

    #[test]
    fn runtime_wins_whatever_the_target_order() {
        let app = path_key("app", "1.0.0");
        let cc = key("cc", "1.0.0");
        let meta = metadata(&[(app.clone(), None), (cc.clone(), Some("MIT"))]);
        let build_time = || graph(&app, &[(&cc, Role::BuildTime)], &[]);
        let runtime = || graph(&app, &[(&cc, Role::Runtime)], &[]);
        for graphs in [[build_time(), runtime()], [runtime(), build_time()]] {
            let inventory = build(&graphs, &meta, |_| false).unwrap();
            assert_eq!(inventory.components[0].role, Role::Runtime);
        }
    }

    #[test]
    fn rejects_a_root_that_is_not_a_workspace_member() {
        let root = key("x", "1.0.0");
        let meta = metadata(&[(root.clone(), Some("MIT"))]);
        let err = build(&[graph(&root, &[], &[])], &meta, |_| false).unwrap_err();
        assert!(err.to_string().contains("not a workspace member"), "{err}");
    }

    #[test]
    fn rejects_targets_with_different_roots() {
        let (a, b) = (path_key("a", "1.0.0"), path_key("b", "1.0.0"));
        let meta = metadata(&[(a.clone(), None), (b.clone(), None)]);
        let graphs = [graph(&a, &[], &[]), graph(&b, &[], &[])];
        assert!(build(&graphs, &meta, |_| false).is_err());
    }

    #[test]
    fn reports_every_unresolvable_package() {
        let app = path_key("app", "1.0.0");
        let missing = key("missing", "1.0.0");
        let git = PackageKey {
            source: SourceKind::Other("git+https://example.com/x".to_string()),
            ..key("gitdep", "1.0.0")
        };
        let stray_path = path_key("vendored", "1.0.0");
        let mut meta = metadata(&[(app.clone(), None), (stray_path.clone(), None)]);
        meta.workspace_members
            .retain(|id| !id.repr.contains("vendored"));
        let graphs = [graph(
            &app,
            &[
                (&missing, Role::Runtime),
                (&git, Role::Runtime),
                (&stray_path, Role::Runtime),
            ],
            &[],
        )];
        let err = build(&graphs, &meta, |_| false).unwrap_err().to_string();
        assert!(err.contains("missing@1.0.0: not found"), "{err}");
        assert!(err.contains("unsupported source"), "{err}");
        assert!(err.contains("not a workspace member"), "{err}");
    }

    #[test]
    fn flags_invalid_license_expressions() {
        let app = path_key("app", "1.0.0");
        let odd = key("odd", "1.0.0");
        let meta = metadata(&[(app.clone(), None), (odd.clone(), Some("MIT AND"))]);
        let inventory = build(&[graph(&app, &[(&odd, Role::Runtime)], &[])], &meta, |_| {
            false
        })
        .unwrap();
        assert!(matches!(
            inventory.components[0].license,
            DeclaredLicense::Invalid(_)
        ));
    }
}
