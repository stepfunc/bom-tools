//! Renderer: a validated inventory as a CycloneDX 1.5 JSON SBOM

use crate::approval::{Approval, ValidatedInventory};
use crate::config::CommercialLicense;
use crate::graph::{is_crates_io, Role};
use crate::inventory::DeclaredLicense;
use anyhow::anyhow;
use cargo_metadata::{Package, PackageId};
use cyclonedx_bom::models::component::{Classification, Scope};
use cyclonedx_bom::models::dependency::{Dependencies, Dependency};
use cyclonedx_bom::models::external_reference::{
    ExternalReference, ExternalReferenceType, ExternalReferences,
};
use cyclonedx_bom::models::hash::{Hash, HashAlgorithm, HashValue, Hashes};
use cyclonedx_bom::models::license::{License, LicenseChoice, Licenses};
use cyclonedx_bom::models::tool::Tools;
use cyclonedx_bom::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;

/// SHA-256 checksums of crates.io packages, from a `Cargo.lock`
pub(crate) struct Checksums(HashMap<(String, String), String>);

impl Checksums {
    /// Parse the checksums of every crates.io package in a `Cargo.lock`
    pub(crate) fn from_lockfile(text: &str) -> Result<Self, anyhow::Error> {
        #[derive(Deserialize)]
        struct Lockfile {
            #[serde(default)]
            package: Vec<LockedPackage>,
        }
        #[derive(Deserialize)]
        struct LockedPackage {
            name: String,
            version: String,
            source: Option<String>,
            checksum: Option<String>,
        }
        let lockfile: Lockfile = toml::from_str(text)?;
        let mut checksums = HashMap::new();
        for package in lockfile.package {
            if !package.source.as_deref().is_some_and(is_crates_io) {
                continue;
            }
            let Some(checksum) = package.checksum else {
                continue;
            };
            if checksum.len() != 64 || !checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(anyhow!(
                    "invalid SHA-256 checksum for {} {} in the lockfile: {checksum:?}",
                    package.name,
                    package.version
                ));
            }
            checksums.insert((package.name, package.version), checksum);
        }
        Ok(Self(checksums))
    }

    fn get(&self, package: &Package) -> Option<&String> {
        self.0
            .get(&(package.name.clone(), package.version.to_string()))
    }
}

/// How to produce the document
pub(crate) struct Options<'a> {
    /// Add SHA-256 hashes of crates.io packages from this lockfile
    pub(crate) checksums: Option<&'a Checksums>,
    /// Omit the random serial number, for reproducible output
    pub(crate) omit_serial_number: bool,
    /// The document's creation time
    pub(crate) timestamp: DateTime,
}

/// Write the SBOM of a validated inventory
pub(crate) fn write<W: Write>(
    validated: &ValidatedInventory,
    commercial_license: Option<&CommercialLicense>,
    options: &Options,
    mut w: W,
) -> Result<(), anyhow::Error> {
    let commercial_license = commercial_license.ok_or_else(|| {
        anyhow!("the configuration needs a `commercial_license` (name and url) to describe first-party and vendor packages")
    })?;
    let commercial = || {
        let mut license = License::named_license(&commercial_license.name);
        license.url = Some(Uri::new(&commercial_license.url));
        Licenses(vec![LicenseChoice::License(license)])
    };

    // every package is referenced by its purl, which must therefore be unique
    let root = validated.root;
    let packages =
        std::iter::once(root).chain(validated.components.iter().map(|a| a.component.package));
    let mut purls: HashMap<&PackageId, Purl> = HashMap::new();
    let mut refs: HashMap<&PackageId, String> = HashMap::new();
    let mut unique = HashSet::new();
    for package in packages {
        let purl = Purl::new("cargo", &package.name, &package.version.to_string())?;
        let bom_ref = purl.to_string();
        if !unique.insert(bom_ref.clone()) {
            return Err(anyhow!("two packages share the reference {bom_ref}"));
        }
        purls.insert(&package.id, purl);
        refs.insert(&package.id, bom_ref);
    }
    // a library component referenced by its purl
    let component = |package: &Package| {
        let mut component = Component::new(
            Classification::Library,
            &package.name,
            &package.version.to_string(),
            Some(refs[&package.id].clone()),
        );
        component.purl = Some(purls[&package.id].clone());
        component
    };

    let mut root_component = component(root);
    root_component.licenses = Some(commercial());

    let mut components = Vec::new();
    for approved in &validated.components {
        let package = approved.component.package;
        let mut component = component(package);
        component.scope = Some(match approved.component.role {
            Role::Runtime => Scope::Required,
            Role::BuildTime => Scope::Excluded,
        });
        component.licenses = match &approved.approval {
            Approval::FirstParty | Approval::Vendor(_) => Some(commercial()),
            Approval::ThirdParty(_) | Approval::BuildOnly => match &approved.component.license {
                DeclaredLicense::Valid(expression) => {
                    Some(Licenses(vec![LicenseChoice::Expression(
                        SpdxExpression::new(AsRef::<str>::as_ref(&**expression)),
                    )]))
                }
                DeclaredLicense::Missing | DeclaredLicense::Invalid(_) => None,
            },
        };
        if let Approval::Vendor(vendor) = &approved.approval {
            component.external_references = Some(ExternalReferences(vec![ExternalReference::new(
                ExternalReferenceType::Vcs,
                Uri::new(&vendor.url),
            )]));
        }
        // only crates.io packages reach here besides first-party ones (see `inventory`)
        if let (Some(checksums), false) = (options.checksums, approved.component.first_party) {
            let checksum = checksums.get(package).ok_or_else(|| {
                anyhow!(
                    "the lockfile has no checksum for {} {}",
                    package.name,
                    package.version
                )
            })?;
            component.hashes = Some(Hashes(vec![Hash {
                alg: HashAlgorithm::SHA_256,
                content: HashValue(checksum.clone()),
            }]));
        }
        components.push(component);
    }

    let mut dependencies: BTreeMap<&str, Vec<&str>> =
        refs.values().map(|r| (r.as_str(), Vec::new())).collect();
    for (from, to) in &validated.edges {
        if let Some(list) = dependencies.get_mut(refs[from].as_str()) {
            list.push(refs[to].as_str());
        }
    }

    let tool = Component::new(
        Classification::Application,
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        None,
    );
    let bom = Bom {
        spec_version: SpecVersion::V1_5,
        serial_number: (!options.omit_serial_number).then(UrnUuid::generate),
        metadata: Some(Metadata {
            timestamp: Some(options.timestamp.clone()),
            tools: Some(Tools::Object {
                services: None,
                components: Some(Components(vec![tool])),
            }),
            component: Some(root_component),
            ..Metadata::default()
        }),
        components: Some(Components(components)),
        dependencies: Some(Dependencies(
            dependencies
                .into_iter()
                .map(|(from, mut to)| {
                    to.sort_unstable();
                    Dependency {
                        dependency_ref: from.to_string(),
                        dependencies: to.into_iter().map(str::to_string).collect(),
                    }
                })
                .collect(),
        )),
        ..Bom::default()
    };

    let validation = bom.validate_version(SpecVersion::V1_5);
    if !validation.passed() {
        return Err(anyhow!(
            "generated SBOM is not valid CycloneDX 1.5: {validation:?}"
        ));
    }
    let mut json = Vec::new();
    bom.output_as_json_v1_5(&mut json)?;
    // cyclonedx-bom writes `"serialNumber": null` when omitted, which the JSON schema rejects
    let mut json: serde_json::Value = serde_json::from_slice(&json)?;
    if let Some(document) = json.as_object_mut() {
        if document
            .get("serialNumber")
            .is_some_and(serde_json::Value::is_null)
        {
            document.remove("serialNumber");
        }
    }
    serde_json::to_writer_pretty(&mut w, &json)?;
    writeln!(w)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval;
    use crate::config::Config;
    use crate::graph::test_util::{key, path_key};
    use crate::graph::{PackageKey, TargetGraph};
    use crate::inventory::{self, test_util::metadata};
    use std::collections::{BTreeMap, BTreeSet};

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn config() -> Config {
        serde_json::from_value(serde_json::json!({
            "build_only": ["cc"],
            "vendor": {"sfio": {"url": "https://example.com/sfio"}},
            "third_party": {"serde": {"id": "serde", "source": "crates.io", "licenses": [{"MIT": {"copyright": "NotPresent"}}]}},
            "commercial_license": {"name": "Commercial", "url": "https://example.com/license"}
        }))
        .unwrap()
    }

    fn lockfile(entries: &[(&str, &str, &str)]) -> String {
        entries
            .iter()
            .map(|(name, version, checksum)| {
                format!("[[package]]\nname = \"{name}\"\nversion = \"{version}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{checksum}\"\n")
            })
            .collect()
    }

    /// Render the SBOM of `app` with the given dependencies, all directly under the root
    fn render(
        deps: &[(PackageKey, Option<&str>, Role)],
        checksums: Option<&Checksums>,
    ) -> Result<serde_json::Value, anyhow::Error> {
        let app = path_key("app", "1.0.0");
        let mut packages: Vec<(PackageKey, Option<&str>)> =
            deps.iter().map(|(k, l, _)| (k.clone(), *l)).collect();
        packages.push((app.clone(), None));
        let meta = metadata(&packages);
        let mut members: BTreeMap<PackageKey, Role> =
            deps.iter().map(|(k, _, r)| (k.clone(), *r)).collect();
        members.insert(app.clone(), Role::Runtime);
        let edges: BTreeSet<_> = deps
            .iter()
            .map(|(k, _, _)| (app.clone(), k.clone()))
            .collect();
        let graph = TargetGraph::new(app, members, edges)?;
        let inventory = inventory::build(&[graph], &meta, |_| false)?;
        let config = config();
        let validated = approval::validate(inventory, &config)?;
        let options = Options {
            checksums,
            omit_serial_number: true,
            timestamp: DateTime::try_from("2025-01-01T00:00:00Z".to_string()).unwrap(),
        };
        let mut out = Vec::new();
        write(
            &validated,
            config.commercial_license.as_ref(),
            &options,
            &mut out,
        )?;
        Ok(serde_json::from_slice(&out)?)
    }

    fn component<'a>(
        bom: &'a serde_json::Value,
        name: &str,
        version: &str,
    ) -> &'a serde_json::Value {
        bom["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name && c["version"] == version)
            .unwrap()
    }

    #[test]
    fn encodes_scope_license_and_references_per_approval() {
        let bom = render(
            &[
                (key("serde", "1.0.0"), Some("MIT/Apache-2.0"), Role::Runtime),
                (key("cc", "1.0.0"), None, Role::BuildTime),
                (key("sfio", "1.0.0"), None, Role::Runtime),
            ],
            None,
        )
        .unwrap();
        let serde = component(&bom, "serde", "1.0.0");
        assert_eq!(serde["scope"], "required");
        assert_eq!(serde["bom-ref"], "pkg:cargo/serde@1.0.0");
        assert_eq!(serde["licenses"][0]["expression"], "MIT OR Apache-2.0");
        let cc = component(&bom, "cc", "1.0.0");
        assert_eq!(cc["scope"], "excluded");
        assert!(cc.get("licenses").is_none());
        let sfio = component(&bom, "sfio", "1.0.0");
        assert_eq!(sfio["licenses"][0]["license"]["name"], "Commercial");
        assert_eq!(
            sfio["licenses"][0]["license"]["url"],
            "https://example.com/license"
        );
        assert_eq!(
            sfio["externalReferences"][0]["url"],
            "https://example.com/sfio"
        );
        assert_eq!(
            bom["metadata"]["component"]["licenses"][0]["license"]["name"],
            "Commercial"
        );
        let root_deps = bom["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["ref"] == "pkg:cargo/app@1.0.0")
            .unwrap();
        assert_eq!(root_deps["dependsOn"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn hashes_each_version_from_its_own_lockfile_entry() {
        let checksums = Checksums::from_lockfile(&lockfile(&[
            ("serde", "1.0.0", HASH_A),
            ("serde", "2.0.0", HASH_B),
        ]))
        .unwrap();
        let bom = render(
            &[
                (key("serde", "1.0.0"), Some("MIT"), Role::Runtime),
                (key("serde", "2.0.0"), Some("MIT"), Role::Runtime),
            ],
            Some(&checksums),
        )
        .unwrap();
        assert_eq!(
            component(&bom, "serde", "1.0.0")["hashes"][0]["content"],
            HASH_A
        );
        assert_eq!(
            component(&bom, "serde", "2.0.0")["hashes"][0]["content"],
            HASH_B
        );
    }

    #[test]
    fn rejects_invalid_checksums() {
        for bad in [
            "0000",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        ] {
            assert!(Checksums::from_lockfile(&lockfile(&[("serde", "1.0.0", bad)])).is_err());
        }
        let trailing = format!("{HASH_A}NOTHEX");
        assert!(Checksums::from_lockfile(&lockfile(&[("serde", "1.0.0", &trailing)])).is_err());
    }

    #[test]
    fn rejects_colliding_references() {
        // a workspace member and a crates.io package with the same name and version
        let err = render(
            &[
                (path_key("serde", "1.0.0"), None, Role::Runtime),
                (key("serde", "1.0.0"), Some("MIT"), Role::Runtime),
            ],
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("share the reference"), "{err}");
    }
}
