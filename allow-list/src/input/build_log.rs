//! Reader for `cargo build --message-format json` logs

use super::{pkgid, Variant};
use anyhow::anyhow;
use cargo_metadata::{Message, TargetKind};
use std::collections::BTreeSet;
use std::io::BufRead;

/// Kinds of a root artifact that count as a shippable product
const PRODUCT_KINDS: [TargetKind; 6] = [
    TargetKind::Lib,
    TargetKind::RLib,
    TargetKind::CDyLib,
    TargetKind::StaticLib,
    TargetKind::DyLib,
    TargetKind::Bin,
];

/// Kinds that never belong to a product build
const NON_PRODUCT_KINDS: [TargetKind; 3] =
    [TargetKind::Example, TargetKind::Test, TargetKind::Bench];

/// Read and validate a build log, returning the (package, features) variant of every compiled
/// artifact except build scripts.
///
/// The log must come from a successful product build of `root`: it must end with a successful
/// `build-finished` message, contain no test/example/bench artifacts, and contain a non-`.rmeta`
/// product artifact for `root` (which rejects `cargo check`).
pub(crate) fn read<R: BufRead>(log: R, root: &str) -> Result<BTreeSet<Variant>, anyhow::Error> {
    let mut variants = BTreeSet::new();
    let mut finished = None;
    let mut root_product = false;

    for message in Message::parse_stream(log) {
        let message = message?;
        if finished.is_some() {
            if !matches!(message, Message::TextLine(_)) {
                return Err(anyhow!("build log has messages after `build-finished`"));
            }
            continue;
        }
        match message {
            Message::CompilerArtifact(artifact) => {
                if artifact.profile.test
                    || artifact
                        .target
                        .kind
                        .iter()
                        .any(|k| NON_PRODUCT_KINDS.contains(k))
                {
                    return Err(anyhow!(
                        "build log contains a test, example or bench artifact ({} target {:?}); \
                         it must come from a product build",
                        artifact.package_id,
                        artifact.target.name
                    ));
                }
                if artifact.target.kind.contains(&TargetKind::CustomBuild) {
                    continue;
                }
                let key = pkgid::parse(&artifact.package_id.repr)?;
                if key.name == root
                    && artifact
                        .target
                        .kind
                        .iter()
                        .any(|k| PRODUCT_KINDS.contains(k))
                    && artifact
                        .filenames
                        .iter()
                        .any(|f| f.extension() != Some("rmeta"))
                {
                    root_product = true;
                }
                variants.insert(Variant {
                    key,
                    features: artifact.features.into_iter().collect(),
                });
            }
            Message::BuildFinished(status) => finished = Some(status.success),
            Message::TextLine(line) if line.trim_start().starts_with('{') => {
                return Err(anyhow!("build log contains a malformed message: {line}"));
            }
            _ => {}
        }
    }

    match finished {
        None => Err(anyhow!(
            "build log has no `build-finished` message (truncated or failed build?)"
        )),
        Some(false) => Err(anyhow!("build log reports a failed build")),
        Some(true) if !root_product => Err(anyhow!(
            "build log has no product artifact for root package `{root}` (wrong -p, or `cargo check`?)"
        )),
        Some(true) => Ok(variants),
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use serde_json::{json, Value};

    /// A `compiler-artifact` message line
    pub(crate) fn artifact(id: &str, kind: &[&str], features: &[&str], file: &str) -> String {
        artifact_with(id, kind, features, file, false)
    }

    pub(crate) fn artifact_with(
        id: &str,
        kind: &[&str],
        features: &[&str],
        file: &str,
        test: bool,
    ) -> String {
        let message: Value = json!({
            "reason": "compiler-artifact",
            "package_id": id,
            "manifest_path": "/build/Cargo.toml",
            "target": {
                "kind": kind,
                "crate_types": kind,
                "name": "target",
                "src_path": "/build/src/lib.rs",
                "edition": "2021",
                "doc": true,
                "doctest": true,
                "test": true
            },
            "profile": {
                "opt_level": "3",
                "debuginfo": 0,
                "debug_assertions": false,
                "overflow_checks": false,
                "test": test
            },
            "features": features,
            "filenames": [file],
            "executable": null,
            "fresh": false
        });
        message.to_string()
    }

    pub(crate) fn finished(success: bool) -> String {
        json!({"reason": "build-finished", "success": success}).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;
    use crate::graph::test_util::{key, path_key};

    const ROOT: &str = "path+file:///build/app#1.0.0";
    const SERDE: &str = "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.228";

    fn log(lines: &[String]) -> String {
        lines.join("\n")
    }

    fn root_artifact() -> String {
        artifact(ROOT, &["rlib", "cdylib"], &["default"], "/t/libapp.so")
    }

    #[test]
    fn reads_variants_including_fresh_and_skips_build_scripts() {
        let text = log(&[
            "   Compiling serde v1.0.228".to_string(),
            artifact(SERDE, &["custom-build"], &["std"], "/t/build-script-build"),
            artifact(SERDE, &["lib"], &["std"], "/t/libserde.rlib"),
            artifact(SERDE, &["lib"], &["derive", "std"], "/t/libserde-host.rlib"),
            root_artifact(),
            finished(true),
        ]);
        let variant = |key, features: &[&str]| Variant {
            key,
            features: features.iter().map(|f| f.to_string()).collect(),
        };
        let expected = BTreeSet::from([
            variant(key("serde", "1.0.228"), &["std"]),
            variant(key("serde", "1.0.228"), &["derive", "std"]),
            variant(path_key("app", "1.0.0"), &["default"]),
        ]);
        assert_eq!(read(text.as_bytes(), "app").unwrap(), expected);
    }

    #[test]
    fn requires_successful_final_build_finished() {
        let missing = log(&[root_artifact()]);
        assert!(read(missing.as_bytes(), "app").is_err());
        let failed = log(&[root_artifact(), finished(false)]);
        assert!(read(failed.as_bytes(), "app").is_err());
        let after = log(&[finished(true), root_artifact()]);
        assert!(read(after.as_bytes(), "app").is_err());
    }

    #[test]
    fn rejects_malformed_messages() {
        let text = log(&[
            root_artifact(),
            "{\"reason\":\"compiler-artifact\",\"truncated".to_string(),
            finished(true),
        ]);
        assert!(read(text.as_bytes(), "app").is_err());
    }

    #[test]
    fn rejects_test_example_and_bench_artifacts() {
        for bad in [
            artifact_with(SERDE, &["lib"], &[], "/t/libserde.rlib", true),
            artifact(ROOT, &["example"], &[], "/t/example"),
            artifact(ROOT, &["test"], &[], "/t/test"),
            artifact(ROOT, &["bench"], &[], "/t/bench"),
        ] {
            let text = log(&[root_artifact(), bad, finished(true)]);
            assert!(read(text.as_bytes(), "app").is_err());
        }
    }

    #[test]
    fn requires_a_product_artifact_for_the_root() {
        let check = log(&[
            artifact(ROOT, &["lib"], &[], "/t/libapp.rmeta"),
            finished(true),
        ]);
        assert!(read(check.as_bytes(), "app").is_err());
        let wrong_root = log(&[root_artifact(), finished(true)]);
        assert!(read(wrong_root.as_bytes(), "other").is_err());
        let plain_lib = log(&[
            artifact(ROOT, &["lib"], &[], "/t/libapp.rlib"),
            finished(true),
        ]);
        assert!(read(plain_lib.as_bytes(), "app").is_ok());
    }
}
