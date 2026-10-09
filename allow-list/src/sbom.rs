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
use std::collections::{BTreeMap, HashMap};
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
        Ok(Self(
            lockfile
                .package
                .into_iter()
                .filter(|p| p.source.as_deref().is_some_and(is_crates_io))
                .filter_map(|p| Some(((p.name, p.version), p.checksum?)))
                .collect(),
        ))
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
    let mut refs: HashMap<&PackageId, String> = HashMap::new();
    for package in packages {
        let purl = purl(package)?.to_string();
        if refs.values().any(|r| *r == purl) {
            return Err(anyhow!("two packages share the reference {purl}"));
        }
        refs.insert(&package.id, purl);
    }

    let mut root_component = component(root)?;
    root_component.licenses = Some(commercial());

    let mut components = Vec::new();
    for approved in &validated.components {
        let package = approved.component.package;
        let mut component = component(package)?;
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

fn purl(package: &Package) -> Result<Purl, anyhow::Error> {
    Ok(Purl::new(
        "cargo",
        &package.name,
        &package.version.to_string(),
    )?)
}

/// A library component referenced by its purl
fn component(package: &Package) -> Result<Component, anyhow::Error> {
    let purl = purl(package)?;
    let mut component = Component::new(
        Classification::Library,
        &package.name,
        &package.version.to_string(),
        Some(purl.to_string()),
    );
    component.purl = Some(purl);
    Ok(component)
}
