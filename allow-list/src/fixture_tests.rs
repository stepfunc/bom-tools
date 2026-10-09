//! End-to-end tests that run the installed Cargo on the fixture workspace in
//! `tests/fixtures/workspace`, then read the evidence with the real reader and core.
//!
//! These pin down the contract with Cargo itself (tree grammar, classification by `cargo tree`,
//! artifact features) and fail when a new Cargo release changes it.

use crate::config::Config;
use crate::graph::{PackageKey, Role, TargetGraph};
use crate::input::TargetInput;
use crate::{approval, inventory};
use cargo_metadata::Metadata;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The fixture's build evidence, produced once per test run with the README recipe
struct Evidence {
    dir: PathBuf,
    metadata: Metadata,
}

fn evidence() -> &'static Evidence {
    static EVIDENCE: OnceLock<Evidence> = OnceLock::new();
    EVIDENCE.get_or_init(|| {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workspace");
        let dir = std::env::temp_dir().join(format!("allow-list-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let host = host_triple();
        let args = ["-p", "app", "--target", &host, "--locked", "--offline"];
        let tree_format = [
            "--prefix", "depth", "--color", "never", "--format", "{p}|{f}",
        ];

        let target_dir = dir.join("target");
        let target_dir = target_dir.to_str().unwrap();
        cargo(
            &workspace,
            &dir.join("build.json"),
            &[
                &[
                    "build",
                    "--message-format",
                    "json",
                    "--target-dir",
                    target_dir,
                ][..],
                &args[..],
            ]
            .concat(),
        );
        cargo(
            &workspace,
            &dir.join("tree.txt"),
            &[
                &["tree", "-e", "normal,build"][..],
                &args[..],
                &tree_format[..],
            ]
            .concat(),
        );
        cargo(
            &workspace,
            &dir.join("runtime-tree.txt"),
            &[
                &["tree", "-e", "normal,no-proc-macro"][..],
                &args[..],
                &tree_format[..],
            ]
            .concat(),
        );
        cargo(
            &workspace,
            &dir.join("metadata.json"),
            &[
                "metadata",
                "--format-version",
                "1",
                "--all-features",
                "--locked",
                "--offline",
            ],
        );
        let metadata =
            serde_json::from_slice(&std::fs::read(dir.join("metadata.json")).unwrap()).unwrap();
        Evidence { dir, metadata }
    })
}

fn host_triple() -> String {
    let output = Command::new(cargo_bin()).arg("-vV").output().unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .unwrap()
}

fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Run cargo in `dir`, writing its stdout to `out`
fn cargo(dir: &Path, out: &Path, args: &[&str]) {
    let output = Command::new(cargo_bin())
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(out, output.stdout).unwrap();
}

fn input(log: &Path) -> TargetInput {
    let dir = &evidence().dir;
    TargetInput::LogAndTree {
        log: log.to_path_buf(),
        tree: dir.join("tree.txt"),
        runtime_tree: dir.join("runtime-tree.txt"),
    }
}

fn read() -> TargetGraph {
    input(&evidence().dir.join("build.json"))
        .read("app")
        .unwrap()
}

fn roles(graph: &TargetGraph) -> BTreeMap<&str, Role> {
    graph
        .packages()
        .iter()
        .map(|(key, role)| (key.name.as_str(), *role))
        .collect()
}

fn edges(graph: &TargetGraph) -> BTreeSet<(&str, &str)> {
    graph
        .edges()
        .iter()
        .map(|(from, to)| (from.name.as_str(), to.name.as_str()))
        .collect()
}

#[test]
fn cargo_classifies_runtime_and_build_time_packages() {
    let graph = read();
    let expected = BTreeMap::from([
        ("app", Role::Runtime),
        // renamed optional dependency with a custom lib name, activated through `dep:`
        ("alias-target", Role::Runtime),
        // a runtime dependency also used (with the same features) by a build dependency
        ("common", Role::Runtime),
        // a runtime dependency also used (with different features) beneath a proc-macro
        ("shared", Role::Runtime),
        ("unixonly", Role::Runtime),
        ("pm", Role::BuildTime),
        ("pmonly", Role::BuildTime),
        ("tool", Role::BuildTime),
    ]);
    let expected: BTreeMap<&str, Role> = expected
        .into_iter()
        .filter(|(name, _)| cfg!(unix) || *name != "unixonly")
        .collect();
    // `optdep` (disabled feature) and `winonly`/`unixonly` (other platform) never compile
    assert_eq!(roles(&graph), expected);
}

#[test]
fn cargo_tree_edges_include_every_expansion() {
    let graph = read();
    let mut expected = BTreeSet::from([
        ("app", "alias-target"),
        ("app", "common"),
        ("app", "pm"),
        ("app", "shared"),
        ("app", "tool"),
        ("pm", "pmonly"),
        ("pm", "shared"),
        ("tool", "common"),
    ]);
    if cfg!(unix) {
        expected.insert(("app", "unixonly"));
    }
    assert_eq!(edges(&graph), expected);
}

/// Copy the build log without the first artifact line of `package` that `select` accepts
fn log_without(package: &str, select: impl Fn(&serde_json::Value) -> bool) -> PathBuf {
    let dir = &evidence().dir;
    let text = std::fs::read_to_string(dir.join("build.json")).unwrap();
    let mut removed = false;
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| {
            let message: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
            let is_target = message["reason"] == "compiler-artifact"
                && message["package_id"]
                    .as_str()
                    .unwrap_or("")
                    .contains(&format!("/{package}#"))
                && message["target"]["kind"][0] != "custom-build"
                && select(&message);
            let drop = is_target && !removed;
            removed |= drop;
            !drop
        })
        .collect();
    assert!(removed, "no artifact of {package} to remove");
    let path = dir.join(format!("build-without-{package}.json"));
    std::fs::write(&path, lines.join("\n")).unwrap();
    path
}

#[test]
fn a_dropped_artifact_with_distinct_features_is_detected() {
    let log = log_without("shared", |m| m["features"][0] == "host-feat");
    let err = input(&log).read("app").unwrap_err();
    assert!(
        format!("{err:#}").contains("shared@0.1.0 (path) [host-feat]"),
        "{err:#}"
    );
}

#[test]
fn a_dropped_artifact_with_identical_features_is_not_detected() {
    // Documented limit: `common` compiles twice (host and target) with identical features, so
    // the variant sets cannot tell that one of the artifacts is missing.
    let log = log_without("common", |_| true);
    assert!(input(&log).read("app").is_ok());
}

#[test]
fn the_core_needs_no_approval_for_workspace_members() {
    let graph = read();
    let inventory = inventory::build(&[graph], &evidence().metadata, |_| false).unwrap();
    assert!(inventory.components.iter().all(|c| c.first_party));
    let config: Config =
        serde_json::from_str(r#"{"build_only": [], "vendor": {}, "third_party": {}}"#).unwrap();
    let validated = approval::validate(inventory, &config).unwrap();
    assert!(validated.shipped_third_party().is_empty());
}

#[test]
fn keys_resolve_to_workspace_members() {
    let graph = read();
    let keys: Vec<&PackageKey> = graph.packages().keys().collect();
    assert!(keys
        .iter()
        .all(|k| k.source == crate::graph::SourceKind::Path));
}
