//! Reader for `cargo build --message-format json` logs

use super::{pkgid, Variant};
use crate::graph::PackageKey;
use anyhow::anyhow;
use cargo_metadata::{Message, TargetKind};
use std::collections::BTreeSet;
use std::io::BufRead;

/// Kinds of artifact that count as a shippable product
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

/// Just the kind of a cargo JSON message
#[derive(serde::Deserialize)]
struct MessageKind {
    reason: Option<String>,
}

/// What a build log proves
#[derive(Debug)]
pub(crate) struct BuildLog {
    /// The (package, features) variant of every compiled artifact except build scripts
    pub(crate) variants: BTreeSet<Variant>,
    /// Packages with a product artifact (a library or binary that is not only `.rmeta`)
    pub(crate) products: BTreeSet<PackageKey>,
}

/// Read and validate a build log.
///
/// The log must come from a successful product build: every JSON line must be a complete
/// message, it must end with a successful `build-finished` message, and it must contain no
/// test/example/bench artifacts.
pub(crate) fn read<R: BufRead>(log: R) -> Result<BuildLog, anyhow::Error> {
    let mut variants = BTreeSet::new();
    let mut products = BTreeSet::new();
    let mut finished = None;

    for (index, line) in log.lines().enumerate() {
        let line = line?;
        let line = line.trim();
        let line_number = index + 1;
        if finished.is_some() {
            if !line.is_empty() {
                return Err(anyhow!(
                    "build log line {line_number}: content after `build-finished`"
                ));
            }
            continue;
        }
        if !line.starts_with('{') {
            continue;
        }
        // Deserialize the original line (never an intermediate `Value`, which would silently
        // keep the last of duplicate keys), so a truncated, concatenated or ambiguous message
        // is an error. Other message kinds (diagnostics, build script output, future kinds)
        // carry no scope and are skipped.
        let malformed = |err: serde_json::Error| {
            anyhow!("build log line {line_number}: malformed message: {err}")
        };
        let kind: MessageKind = serde_json::from_str(line).map_err(malformed)?;
        if !matches!(
            kind.reason.as_deref(),
            Some("compiler-artifact" | "build-finished")
        ) {
            continue;
        }
        let message: Message = serde_json::from_str(line).map_err(malformed)?;
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
                let product = artifact
                    .target
                    .kind
                    .iter()
                    .any(|k| PRODUCT_KINDS.contains(k))
                    && artifact
                        .filenames
                        .iter()
                        .any(|f| f.extension() != Some("rmeta"));
                if product {
                    products.insert(key.clone());
                }
                variants.insert(Variant {
                    key,
                    features: artifact.features.into_iter().collect(),
                });
            }
            Message::BuildFinished(status) => finished = Some(status.success),
            _ => {}
        }
    }

    match finished {
        None => Err(anyhow!(
            "build log has no `build-finished` message (truncated or failed build?)"
        )),
        Some(false) => Err(anyhow!("build log reports a failed build")),
        Some(true) => Ok(BuildLog { variants, products }),
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
    fn reads_variants_and_products_and_skips_build_scripts() {
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
        let log = read(text.as_bytes()).unwrap();
        let expected = BTreeSet::from([
            variant(key("serde", "1.0.228"), &["std"]),
            variant(key("serde", "1.0.228"), &["derive", "std"]),
            variant(path_key("app", "1.0.0"), &["default"]),
        ]);
        assert_eq!(log.variants, expected);
        assert!(log.products.contains(&path_key("app", "1.0.0")));
    }

    #[test]
    fn requires_successful_final_build_finished() {
        let missing = log(&[root_artifact()]);
        assert!(read(missing.as_bytes()).is_err());
        let failed = log(&[root_artifact(), finished(false)]);
        assert!(read(failed.as_bytes()).is_err());
        let after = log(&[finished(true), root_artifact()]);
        assert!(read(after.as_bytes()).is_err());
        let trailing_text = log(&[
            root_artifact(),
            finished(true),
            "{\"reason\":\"compiler-art".to_string(),
        ]);
        assert!(read(trailing_text.as_bytes()).is_err());
        let two_on_one_line = log(&[
            root_artifact(),
            format!("{}{}", finished(true), finished(false)),
        ]);
        assert!(read(two_on_one_line.as_bytes()).is_err());
        let ambiguous = log(&[
            root_artifact(),
            "{\"reason\":\"build-finished\",\"success\":false,\"success\":true}".to_string(),
        ]);
        assert!(read(ambiguous.as_bytes()).is_err());
        let duplicate_reason = log(&[
            root_artifact(),
            "{\"reason\":\"compiler-message\",\"reason\":\"build-finished\",\"success\":true}"
                .to_string(),
        ]);
        assert!(read(duplicate_reason.as_bytes()).is_err());
        let duplicate_features =
            root_artifact().replacen("\"features\":", "\"features\":[],\"features\":", 1);
        let text = log(&[duplicate_features, finished(true)]);
        assert!(read(text.as_bytes()).is_err());
        let blank_after = log(&[root_artifact(), finished(true), String::new()]);
        assert!(read(blank_after.as_bytes()).is_ok());
    }

    #[test]
    fn rejects_malformed_messages() {
        let text = log(&[
            root_artifact(),
            "{\"reason\":\"compiler-artifact\",\"truncated".to_string(),
            finished(true),
        ]);
        assert!(read(text.as_bytes()).is_err());
        let incomplete = log(&[
            root_artifact(),
            "{\"reason\":\"compiler-artifact\"}".to_string(),
            finished(true),
        ]);
        assert!(read(incomplete.as_bytes()).is_err());
    }

    #[test]
    fn ignores_other_message_kinds() {
        let text = log(&[
            "{\"reason\":\"compiler-message\",\"anything\":1}".to_string(),
            "{\"reason\":\"some-future-message\"}".to_string(),
            root_artifact(),
            finished(true),
        ]);
        assert!(read(text.as_bytes()).is_ok());
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
            assert!(read(text.as_bytes()).is_err());
        }
    }

    #[test]
    fn only_linkable_outputs_are_products() {
        let text = log(&[
            artifact(ROOT, &["lib"], &[], "/t/libapp.rmeta"),
            artifact(SERDE, &["lib"], &[], "/t/libserde.rlib"),
            finished(true),
        ]);
        let log = read(text.as_bytes()).unwrap();
        assert_eq!(log.products, BTreeSet::from([key("serde", "1.0.228")]));
    }
}
