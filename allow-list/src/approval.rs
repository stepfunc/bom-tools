//! Core: check the inventory against the allow-list configuration
//!
//! The inventory decides what each package is and how it is used; the configuration must
//! approve exactly that. Every violation is collected and reported at once.

use crate::config::{Config, License, Package as ApprovedPackage, VendorPackage};
use crate::graph::Role;
use crate::inventory::{Component, DeclaredLicense, Inventory};
use anyhow::anyhow;
use cargo_metadata::{Package, PackageId};
use semver::Version;
use spdx::LicenseReq;
use std::collections::{BTreeMap, BTreeSet};

/// How a component is approved
#[derive(Debug)]
pub(crate) enum Approval<'c> {
    /// A workspace member: our own code
    FirstParty,
    /// An open-source package whose license was reviewed
    ThirdParty(&'c ApprovedPackage),
    /// A package the vendor licenses to the customer under its commercial license
    Vendor(&'c VendorPackage),
    /// A package approved for use at build time only
    BuildOnly,
}

/// A component together with its approval
#[derive(Debug)]
pub(crate) struct Approved<'m, 'c> {
    pub(crate) component: Component<'m>,
    pub(crate) approval: Approval<'c>,
}

/// An inventory in which every component is approved for the way it is used
#[derive(Debug)]
pub(crate) struct ValidatedInventory<'m, 'c> {
    pub(crate) root: &'m Package,
    pub(crate) components: Vec<Approved<'m, 'c>>,
    pub(crate) edges: BTreeSet<(&'m PackageId, &'m PackageId)>,
}

impl ValidatedInventory<'_, '_> {
    /// The open-source packages that ship in the product, for the license report:
    /// runtime components approved as `third_party`, by name with all their versions
    pub(crate) fn shipped_third_party(&self) -> BTreeMap<String, BTreeSet<Version>> {
        let mut shipped: BTreeMap<String, BTreeSet<Version>> = BTreeMap::new();
        for approved in &self.components {
            if approved.component.role == Role::Runtime
                && matches!(approved.approval, Approval::ThirdParty(_))
            {
                let package = approved.component.package;
                shipped
                    .entry(package.name.clone())
                    .or_default()
                    .insert(package.version.clone());
            }
        }
        shipped
    }
}

/// Approve every component of the inventory, or report every violation
pub(crate) fn validate<'m, 'c>(
    inventory: Inventory<'m>,
    config: &'c Config,
) -> Result<ValidatedInventory<'m, 'c>, anyhow::Error> {
    let mut violations = BTreeSet::new();
    let mut components = Vec::new();
    for component in inventory.components {
        match approve(&component, config) {
            Ok(approval) => components.push(Approved {
                component,
                approval,
            }),
            Err(violation) => {
                violations.insert(format!(
                    "{} {}: {violation}",
                    component.package.name, component.package.version
                ));
            }
        }
    }

    if !violations.is_empty() {
        let violations: Vec<String> = violations.into_iter().collect();
        return Err(anyhow!(
            "the allow-list does not approve the product's dependencies:\n  {}",
            violations.join("\n  ")
        ));
    }
    Ok(ValidatedInventory {
        root: inventory.root,
        components,
        edges: inventory.edges,
    })
}

fn approve<'c>(component: &Component, config: &'c Config) -> Result<Approval<'c>, String> {
    if component.first_party {
        return Ok(Approval::FirstParty);
    }
    let name = component.package.name.as_str();
    let third_party = config.third_party.get(name);
    let vendor = config.vendor.get(name);
    let build_only = config.build_only.contains(name);
    let runtime = component.role == Role::Runtime;

    let approval = match (third_party, vendor, build_only) {
        (Some(approved), None, false) => Approval::ThirdParty(approved),
        (None, Some(vendor), false) => Approval::Vendor(vendor),
        (None, None, true) => Approval::BuildOnly,
        (None, None, false) if runtime => {
            return Err("ships in the product but is not in `third_party` or `vendor`".to_string())
        }
        (None, None, false) => {
            return Err(
                "used at build time but is not in any list (add it to `build_only`)".to_string(),
            )
        }
        _ => {
            return Err(
                "is listed in more than one of `third_party`, `vendor` and `build_only`"
                    .to_string(),
            )
        }
    };

    if let Approval::ThirdParty(approved) = approval {
        check_entry(name, approved)?;
    }
    match approval {
        Approval::BuildOnly if runtime => Err(
            "is approved only as a build tool (`build_only`) but now ships in the product"
                .to_string(),
        ),
        Approval::ThirdParty(approved) if runtime => {
            check_reviewed_license(approved, &component.license)?;
            Ok(approval)
        }
        _ => Ok(approval),
    }
}

/// A `third_party` entry must be well formed, however the package is used
fn check_entry(name: &str, approved: &ApprovedPackage) -> Result<(), String> {
    if approved.id != name {
        return Err(format!(
            "`third_party` entry is keyed `{name}` but has id `{}`",
            approved.id
        ));
    }
    if approved.licenses.is_empty() {
        return Err("`third_party` entry has no licenses".to_string());
    }
    if approved
        .licenses
        .iter()
        .any(|l| matches!(l, License::Unknown))
    {
        return Err("`third_party` entry has an `Unknown` license".to_string());
    }
    Ok(())
}

/// The reviewed licenses must still be part of the declared expression (no stale extras) and
/// must satisfy it (no newly mandatory licenses)
fn check_reviewed_license(
    approved: &ApprovedPackage,
    declared: &DeclaredLicense,
) -> Result<(), String> {
    let expression = match declared {
        DeclaredLicense::Valid(expression) => expression,
        DeclaredLicense::Missing => {
            return Err("declares no SPDX `license`, so the review cannot be checked".to_string())
        }
        DeclaredLicense::Invalid(reason) => {
            return Err(format!("declares an invalid SPDX `license`: {reason}"))
        }
    };

    let reviewed: Vec<LicenseReq> = approved
        .licenses
        .iter()
        .map(|l| {
            spdx::license_id(l.spdx_short())
                .map(LicenseReq::from)
                .ok_or_else(|| format!("reviewed license {} is not an SPDX id", l.spdx_short()))
        })
        .collect::<Result<_, _>>()?;
    let reviewed_names = || {
        reviewed
            .iter()
            .map(LicenseReq::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };

    if let Some(stale) = reviewed
        .iter()
        .find(|r| !expression.requirements().any(|e| &e.req == *r))
    {
        return Err(format!(
            "reviewed license {stale} is not part of the declared license `{expression}`"
        ));
    }
    if !expression.evaluate(|req| reviewed.contains(req)) {
        return Err(format!(
            "reviewed licenses ({}) do not satisfy the declared license `{expression}`",
            reviewed_names()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_util::{key, path_key};
    use crate::graph::{PackageKey, TargetGraph};
    use crate::inventory::test_util::metadata;
    use cargo_metadata::Metadata;

    fn config(json: serde_json::Value) -> Config {
        serde_json::from_value(json).unwrap()
    }

    fn mit(id: &str) -> serde_json::Value {
        serde_json::json!({"id": id, "source": "crates.io", "licenses": [{"MIT": {"copyright": "NotPresent"}}]})
    }

    /// Inventory of an `app` with the given (name, declared license, role) dependencies
    fn check(deps: &[(&str, Option<&str>, Role)], config: &Config) -> Result<(), String> {
        let app = path_key("app", "1.0.0");
        let keys: Vec<(PackageKey, Option<&str>, Role)> = deps
            .iter()
            .map(|(name, license, role)| (key(name, "1.0.0"), *license, *role))
            .collect();
        let mut packages: Vec<(PackageKey, Option<&str>)> =
            keys.iter().map(|(k, l, _)| (k.clone(), *l)).collect();
        packages.push((app.clone(), None));
        let meta: Metadata = metadata(&packages);
        let mut members: BTreeMap<PackageKey, Role> =
            keys.iter().map(|(k, _, r)| (k.clone(), *r)).collect();
        members.insert(app.clone(), Role::Runtime);
        let graph = TargetGraph::new(app, members, BTreeSet::new()).unwrap();
        let inventory = crate::inventory::build(&[graph], &meta, |_| false).unwrap();
        validate(inventory, config)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn empty() -> serde_json::Value {
        serde_json::json!({"build_only": [], "vendor": {}, "third_party": {}})
    }

    #[test]
    fn approves_each_list_for_its_use() {
        let mut json = empty();
        json["third_party"]["serde"] = mit("serde");
        json["third_party"]["regex"] = mit("regex");
        json["vendor"]["sfio-promise"] = serde_json::json!({"url": "https://example.com"});
        json["build_only"] = serde_json::json!(["cc"]);
        let config = config(json);
        check(
            &[
                ("serde", Some("MIT OR Apache-2.0"), Role::Runtime),
                ("regex", Some("MIT OR Apache-2.0"), Role::BuildTime),
                ("sfio-promise", None, Role::Runtime),
                ("cc", Some("MIT OR Apache-2.0"), Role::BuildTime),
            ],
            &config,
        )
        .unwrap();
    }

    #[test]
    fn reports_every_violation_at_once() {
        let mut json = empty();
        json["build_only"] = serde_json::json!(["cc", "both"]);
        json["third_party"]["both"] = mit("both");
        let err = check(
            &[
                ("aws-lc-rs", Some("ISC"), Role::Runtime),
                ("jobserver", Some("MIT"), Role::BuildTime),
                ("cc", Some("MIT"), Role::Runtime),
                ("both", Some("MIT"), Role::BuildTime),
            ],
            &config(json),
        )
        .unwrap_err();
        assert!(
            err.contains("aws-lc-rs 1.0.0: ships in the product"),
            "{err}"
        );
        assert!(err.contains("jobserver 1.0.0: used at build time"), "{err}");
        assert!(
            err.contains("cc 1.0.0: is approved only as a build tool"),
            "{err}"
        );
        assert!(
            err.contains("both 1.0.0: is listed in more than one"),
            "{err}"
        );
    }

    #[test]
    fn checks_reviewed_licenses_against_the_declared_expression() {
        let reviewed = |licenses: serde_json::Value| {
            let mut json = empty();
            json["third_party"]["x"] =
                serde_json::json!({"id": "x", "source": "crates.io", "licenses": licenses});
            config(json)
        };
        let mit = serde_json::json!([{"MIT": {"copyright": "NotPresent"}}]);
        let mit_isc = serde_json::json!([{"MIT": {"copyright": "NotPresent"}}, {"ISC": {"copyright": "NotPresent"}}]);
        let apache_isc = serde_json::json!(["Apache2", {"ISC": {"copyright": "NotPresent"}}]);
        let runtime = |declared: &str, config: &Config| {
            check(&[("x", Some(declared), Role::Runtime)], config)
        };

        assert!(runtime("MIT OR Apache-2.0", &reviewed(mit.clone())).is_ok());
        assert!(runtime("MIT/Apache-2.0", &reviewed(mit.clone())).is_ok());
        assert!(runtime("Apache-2.0 AND ISC", &reviewed(apache_isc)).is_ok());
        // newly mandatory license
        let err = runtime("MIT AND Apache-2.0", &reviewed(mit.clone())).unwrap_err();
        assert!(err.contains("do not satisfy"), "{err}");
        // stale extra reviewed license
        let err = runtime("MIT", &reviewed(mit_isc)).unwrap_err();
        assert!(err.contains("ISC is not part of"), "{err}");
        // exceptions are part of a requirement
        assert!(runtime("MIT WITH LLVM-exception", &reviewed(mit.clone())).is_err());
        // no usable declaration
        let err = check(&[("x", None, Role::Runtime)], &reviewed(mit.clone())).unwrap_err();
        assert!(err.contains("declares no SPDX"), "{err}");
        // build-time packages are not checked
        assert!(check(
            &[("x", Some("GPL-3.0-only"), Role::BuildTime)],
            &reviewed(mit)
        )
        .is_ok());
    }

    #[test]
    fn checks_third_party_entry_sanity() {
        let mut json = empty();
        json["third_party"]["x"] = serde_json::json!({"id": "y", "source": "crates.io", "licenses": [{"MIT": {"copyright": "NotPresent"}}]});
        assert!(check(&[("x", Some("MIT"), Role::Runtime)], &config(json))
            .unwrap_err()
            .contains("keyed"));
        let mut json = empty();
        json["third_party"]["x"] =
            serde_json::json!({"id": "x", "source": "crates.io", "licenses": []});
        assert!(check(&[("x", Some("MIT"), Role::Runtime)], &config(json))
            .unwrap_err()
            .contains("no licenses"));
        let mut json = empty();
        json["third_party"]["x"] =
            serde_json::json!({"id": "x", "source": "crates.io", "licenses": ["Unknown"]});
        assert!(
            check(&[("x", Some("MIT"), Role::Runtime)], &config(json.clone()))
                .unwrap_err()
                .contains("Unknown")
        );
        // entries are checked however the package is used
        assert!(check(&[("x", Some("MIT"), Role::BuildTime)], &config(json))
            .unwrap_err()
            .contains("Unknown"));
    }

    #[test]
    fn license_report_lists_only_shipped_third_party_packages() {
        let app = path_key("app", "1.0.0");
        let (serde, regex) = (key("serde", "1.0.0"), key("regex", "1.0.0"));
        let meta = metadata(&[
            (app.clone(), None),
            (serde.clone(), Some("MIT")),
            (regex.clone(), Some("MIT")),
        ]);
        let members = BTreeMap::from([
            (app.clone(), Role::Runtime),
            (serde, Role::Runtime),
            (regex, Role::BuildTime),
        ]);
        let graph = TargetGraph::new(app, members, BTreeSet::new()).unwrap();
        let inventory = crate::inventory::build(&[graph], &meta, |_| false).unwrap();
        let mut json = empty();
        json["third_party"]["serde"] = mit("serde");
        json["third_party"]["regex"] = mit("regex");
        let config = config(json);
        let validated = validate(inventory, &config).unwrap();
        let shipped: Vec<String> = validated.shipped_third_party().into_keys().collect();
        assert_eq!(shipped, ["serde"]);
    }
}
