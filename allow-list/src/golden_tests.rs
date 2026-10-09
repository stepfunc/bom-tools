//! Golden tests on real build evidence of dnp3 1.7.0 (`tests/fixtures/dnp3`): the `dnp3-ffi`
//! library built for `x86_64-unknown-linux-gnu` with `serial,tls-aws-lc` (`targets/aws`) and
//! with `serial,tls` (`targets/ring`). Paths are sanitized and the metadata is trimmed.
//!
//! The expected roles of the `aws` target were derived independently from Cargo's nightly SBOM
//! precursor file for the same build (`normal`-edge reachability from the root).

use crate::commands::{self, target_dirs, Evidence, SbomOptions};
use crate::config::Config;
use crate::graph::Role;
use crate::input::TargetInput;
use crate::sbom::{self, Checksums};
use cargo_metadata::Metadata;
use cyclonedx_bom::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const RUNTIME: [&str; 88] = [
    "aes",
    "aws-lc-rs",
    "aws-lc-sys",
    "base64",
    "base64ct",
    "bitflags",
    "block-buffer",
    "block-padding",
    "bytes",
    "cbc",
    "cfg-if",
    "chrono",
    "cipher",
    "const-oid",
    "cpufeatures",
    "crypto-common",
    "der",
    "digest",
    "dnp3",
    "dnp3-ffi",
    "futures",
    "futures-channel",
    "futures-core",
    "futures-executor",
    "futures-io",
    "futures-sink",
    "futures-task",
    "futures-util",
    "generic-array",
    "hmac",
    "iana-time-zone",
    "inout",
    "itoa",
    "lazy_static",
    "libc",
    "log",
    "memchr",
    "mio",
    "mio-serial",
    "nix",
    "nu-ansi-term",
    "num-traits",
    "num_cpus",
    "once_cell",
    "pbkdf2",
    "pem",
    "pem-rfc7468",
    "pin-project-lite",
    "pkcs5",
    "pkcs8",
    "rand_core",
    "rustls",
    "rustls-pki-types",
    "rustls-webpki",
    "rx509",
    "salsa20",
    "scopeguard",
    "scrypt",
    "scursor",
    "serde",
    "serde_core",
    "serde_json",
    "serialport",
    "sfio-promise",
    "sfio-rustls-config",
    "sha2",
    "sharded-slab",
    "slab",
    "smallvec",
    "socket2",
    "spki",
    "subtle",
    "thiserror",
    "thread_local",
    "tokio",
    "tokio-rustls",
    "tokio-serial",
    "tracing",
    "tracing-core",
    "tracing-log",
    "tracing-serde",
    "tracing-subscriber",
    "typenum",
    "unescaper",
    "untrusted",
    "xxhash-rust",
    "zeroize",
    "zmij",
];

const BUILD_TIME: [&str; 50] = [
    "addr2line",
    "adler2",
    "aho-corasick",
    "anstream",
    "anstyle",
    "anstyle-parse",
    "anstyle-query",
    "autocfg",
    "backtrace",
    "cc",
    "cfg_aliases",
    "clap",
    "clap_builder",
    "clap_derive",
    "clap_lex",
    "cmake",
    "colorchoice",
    "dnp3-schema",
    "dunce",
    "find-msvc-tools",
    "fs_extra",
    "futures-macro",
    "gimli",
    "heck",
    "is_terminal_polyfill",
    "jobserver",
    "miniz_oxide",
    "object",
    "oo-bindgen",
    "pkg-config",
    "platforms",
    "proc-macro2",
    "quote",
    "regex",
    "regex-automata",
    "regex-syntax",
    "rustc-demangle",
    "semver",
    "serde_derive",
    "sfio-tokio-ffi",
    "sfio-tracing-ffi",
    "shlex",
    "strsim",
    "syn",
    "thiserror-impl",
    "tokio-macros",
    "tracing-attributes",
    "unicode-ident",
    "utf8parse",
    "version_check",
];

/// Build-time dependencies whose code ships, marked `embedded` in the fixture's config
const EMBEDDED: [&str; 3] = ["oo-bindgen", "sfio-tokio-ffi", "sfio-tracing-ffi"];

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dnp3")
        .join(path)
}

fn evidence(targets: Vec<TargetInput>) -> Evidence {
    Evidence {
        targets,
        root_package: "dnp3-ffi".to_string(),
        metadata: fixture("metadata.json"),
        config: fixture("allowed.json"),
    }
}

fn all_targets() -> Evidence {
    evidence(target_dirs(&fixture("targets")).unwrap())
}

fn one_target(name: &str) -> Evidence {
    evidence(vec![TargetInput::from_dir(&fixture("targets").join(name))])
}

fn load<T: serde::de::DeserializeOwned>(path: &str) -> T {
    serde_json::from_slice(&std::fs::read(fixture(path)).unwrap()).unwrap()
}

/// (name, role) of every component of a validated inventory, plus the root
fn roles(evidence: &Evidence, config: &Config) -> BTreeMap<String, Role> {
    let metadata: Metadata = load("metadata.json");
    let validated = commands::validate(evidence, &metadata, config).unwrap();
    let mut roles: BTreeMap<String, Role> = validated
        .components
        .iter()
        .map(|a| (a.component.package.name.clone(), a.component.role))
        .collect();
    roles.insert(validated.root.name.clone(), Role::Runtime);
    roles
}

#[test]
fn reader_classifies_the_aws_build_like_cargos_sbom_precursor() {
    let graph = TargetInput::from_dir(&fixture("targets/aws"))
        .read("dnp3-ffi")
        .unwrap();
    let names = |role| -> BTreeSet<&str> {
        graph
            .packages()
            .iter()
            .filter(|(_, r)| **r == role)
            .map(|(k, _)| k.name.as_str())
            .collect()
    };
    assert_eq!(names(Role::Runtime), BTreeSet::from(RUNTIME));
    assert_eq!(names(Role::BuildTime), BTreeSet::from(BUILD_TIME));
}

#[test]
fn embedded_build_dependencies_ship_and_their_dependencies_do_not() {
    let config: Config = load("allowed.json");
    let roles = roles(&one_target("aws"), &config);
    for name in EMBEDDED {
        assert_eq!(roles[name], Role::Runtime, "{name}");
    }
    // oo-bindgen's own dependencies stay build-time
    assert_eq!(roles["regex"], Role::BuildTime);
    assert_eq!(roles["heck"], Role::BuildTime);
}

#[test]
fn each_target_contains_only_its_tls_provider_and_the_merge_contains_both() {
    let config: Config = load("allowed.json");
    let aws = roles(&one_target("aws"), &config);
    let ring = roles(&one_target("ring"), &config);
    let merged = roles(&all_targets(), &config);
    assert!(aws.contains_key("aws-lc-sys") && !aws.contains_key("ring"));
    assert!(ring.contains_key("ring") && !ring.contains_key("aws-lc-sys"));
    assert_eq!(merged["ring"], Role::Runtime);
    assert_eq!(merged["aws-lc-sys"], Role::Runtime);
}

#[test]
fn unapproved_or_misreviewed_dependencies_are_all_reported() {
    let mut config: Config = load("allowed.json");
    config.third_party.remove("aws-lc-sys");
    config.build_only.remove("jobserver");
    config.build_only.insert("tokio".to_string());
    config.third_party.remove("tokio");
    config
        .third_party
        .get_mut("ring")
        .unwrap()
        .licenses
        .remove(0);
    let metadata: Metadata = load("metadata.json");
    let err = commands::validate(&all_targets(), &metadata, &config)
        .unwrap_err()
        .to_string();
    for expected in [
        "aws-lc-sys 0.45.0: ships in the product but is not in `third_party` or `vendor`",
        "jobserver 0.1.34: used at build time but is not in any list",
        "tokio 1.52.1: is approved only as a build tool",
        "ring 0.17.14: reviewed licenses (ISC) do not satisfy",
    ] {
        assert!(err.contains(expected), "missing {expected:?} in:\n{err}");
    }
}

/// The (crate, version) pairs listed by the license report of `evidence`
fn reported(evidence: &Evidence) -> BTreeSet<(String, String)> {
    let mut out = Vec::new();
    commands::gen_licenses(evidence, &mut out).unwrap();
    let report = String::from_utf8(out).unwrap();
    let mut reported = BTreeSet::new();
    let mut lines = report.lines();
    while let Some(line) = lines.next() {
        if let Some(name) = line.strip_prefix("crate: ") {
            let versions = lines.next().unwrap().strip_prefix("version(s): ").unwrap();
            for version in versions.split(", ") {
                reported.insert((name.to_string(), version.to_string()));
            }
        }
    }
    reported
}

#[test]
fn license_report_lists_exactly_the_shipped_third_party_crates() {
    // independent expectation: the precursor-derived runtime set, minus first-party and vendor
    // packages, plus the embedded third-party build dependency
    let mut expected: BTreeSet<&str> = BTreeSet::from(RUNTIME);
    for not_third_party in ["dnp3", "dnp3-ffi", "sfio-promise"] {
        expected.remove(not_third_party);
    }
    expected.insert("oo-bindgen");
    let reported = reported(&one_target("aws"));
    let names: BTreeSet<&str> = reported.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, expected);
}

#[test]
fn license_report_and_sbom_agree_on_every_shipped_version() {
    assert_eq!(
        reported(&all_targets()),
        sbom_required_third_party(&sbom_document())
    );
}

/// The SBOM of all targets, with a fixed timestamp and no serial number
fn sbom_bytes() -> Vec<u8> {
    let config: Config = load("allowed.json");
    let metadata: Metadata = load("metadata.json");
    let validated = commands::validate(&all_targets(), &metadata, &config).unwrap();
    let checksums =
        Checksums::from_lockfile(&std::fs::read_to_string(fixture("Cargo.lock")).unwrap()).unwrap();
    let options = sbom::Options {
        checksums: Some(&checksums),
        omit_serial_number: true,
        timestamp: DateTime::try_from("2025-10-09T08:53:20Z".to_string()).unwrap(),
    };
    let mut out = Vec::new();
    sbom::write(
        &validated,
        config.commercial_license.as_ref(),
        &options,
        &mut out,
    )
    .unwrap();
    out
}

fn sbom_document() -> serde_json::Value {
    serde_json::from_slice(&sbom_bytes()).unwrap()
}

/// Names of the `required` components whose license is an SPDX expression (third-party code)
fn sbom_required_third_party(bom: &serde_json::Value) -> BTreeSet<(String, String)> {
    let config: Config = load("allowed.json");
    bom["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["scope"] == "required")
        .filter(|c| config.third_party.contains_key(c["name"].as_str().unwrap()))
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_string(),
                c["version"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn sbom_scopes_licenses_hashes_and_edges() {
    // the final bytes (after post-processing) must still parse and validate as CycloneDX 1.5
    let bytes = sbom_bytes();
    let parsed = Bom::parse_from_json_v1_5(bytes.as_slice()).unwrap();
    let validation = parsed.validate_version(SpecVersion::V1_5);
    assert!(validation.passed(), "{validation:?}");

    let bom: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let components = bom["components"].as_array().unwrap();
    let component = |name: &str| components.iter().find(|c| c["name"] == name).unwrap();

    assert_eq!(bom["metadata"]["component"]["name"], "dnp3-ffi");
    assert!(bom.get("serialNumber").is_none());
    assert_eq!(component("ring")["scope"], "required");
    assert_eq!(component("aws-lc-sys")["scope"], "required");
    assert_eq!(component("serde_derive")["scope"], "excluded");
    assert_eq!(component("sfio-tokio-ffi")["scope"], "required");
    assert_eq!(
        component("ring")["licenses"][0]["expression"],
        "Apache-2.0 AND ISC"
    );
    // legacy `/` syntax is normalized
    assert_eq!(
        component("unescaper")["licenses"][0]["expression"],
        "GPL-3.0 OR MIT"
    );
    assert_eq!(
        component("dnp3")["licenses"][0]["license"]["name"],
        "Step Function I/O License Agreement"
    );
    assert_eq!(
        component("sfio-promise")["externalReferences"][0]["type"],
        "vcs"
    );
    assert!(component("dnp3").get("hashes").is_none());

    // every crates.io component carries the checksum of its own (name, version) lock entry
    let lock: toml::Value =
        toml::from_str(&std::fs::read_to_string(fixture("Cargo.lock")).unwrap()).unwrap();
    let locked: BTreeMap<(&str, &str), &str> = lock["package"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| {
            Some((
                (p["name"].as_str()?, p["version"].as_str()?),
                p.get("checksum")?.as_str()?,
            ))
        })
        .collect();
    let mut hashed = 0;
    for c in components {
        let key = (c["name"].as_str().unwrap(), c["version"].as_str().unwrap());
        match locked.get(&key) {
            Some(checksum) => {
                assert_eq!(c["hashes"][0]["alg"], "SHA-256");
                assert_eq!(c["hashes"][0]["content"], *checksum, "{key:?}");
                hashed += 1;
            }
            None => assert!(c.get("hashes").is_none(), "{key:?}"),
        }
    }
    assert!(hashed > 100);

    let depends_on = |from: &str, to: &str| {
        let from = format!("pkg:cargo/{from}@");
        let to = format!("pkg:cargo/{to}@");
        bom["dependencies"].as_array().unwrap().iter().any(|d| {
            d["ref"].as_str().unwrap().starts_with(&from)
                && d["dependsOn"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t.as_str().unwrap().starts_with(&to))
        })
    };
    assert!(depends_on("dnp3-ffi", "dnp3"));
    assert!(depends_on("dnp3", "tokio"));
    assert!(depends_on("aws-lc-rs", "aws-lc-sys"));
    assert!(depends_on("dnp3-ffi", "oo-bindgen"));
    assert!(!depends_on("dnp3", "dnp3"));
}

#[test]
fn sbom_is_reproducible() {
    assert_eq!(sbom_bytes(), sbom_bytes());
}

#[test]
fn sbom_command_uses_the_given_lockfile() {
    // a lockfile without crates.io checksums (the path-only fixture workspace's) must be
    // rejected, which proves the command passes `--lockfile` through to the renderer
    let options = SbomOptions {
        lockfile: Some(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workspace/Cargo.lock"),
        ),
        omit_serial_number: false,
    };
    let err = commands::gen_sbom(&all_targets(), &options, &mut Vec::new()).unwrap_err();
    assert!(err.to_string().contains("no checksum"), "{err}");
}
