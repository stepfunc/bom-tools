use crate::config::{Config, LicenseInfo};
use cyclonedx_bom::prelude::Bom;
use semver::Version;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Generate a license summary file from a build log and configuration file
pub(crate) fn gen_licenses<W>(
    bom_path: &Path,
    config_path: &Path,
    w: W,
) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    let bom = Bom::parse_from_json_v1_4(std::fs::File::open(bom_path)?)?;
    let config: Config = serde_json::from_reader(std::fs::File::open(config_path)?)?;

    let components = extract_deps(bom, &config)?;

    gen_licenses_for(&components, &config, w)?;

    Ok(())
}

/// Generate a license summary file from a build log and configuration file
pub(crate) fn gen_licenses_in_dirs<W>(
    list_dir: &Path,
    bom_file: &str,
    config_path: &Path,
    w: W,
) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    let config: Config = serde_json::from_reader(std::fs::File::open(config_path)?)?;
    let mut components: BTreeMap<String, BTreeSet<Version>> = BTreeMap::new();

    for item in std::fs::read_dir(list_dir)? {
        let item = item?;
        if item.file_type()?.is_dir() {
            let bom = Bom::parse_from_json_v1_4(std::fs::File::open(item.path().join(bom_file))?)?;
            for (name, versions) in extract_deps(bom, &config)? {
                match components.entry(name.clone()) {
                    Entry::Vacant(x) => {
                        x.insert(versions.into());
                    }
                    Entry::Occupied(mut occ) => {
                        for version in versions {
                            occ.get_mut().insert(version);
                        }
                    }
                }
            }
        }
    }

    gen_licenses_for(&components, &config, w)?;

    Ok(())
}

/// Generate a license summary file from a build log and configuration file
pub(crate) fn gen_licenses_for<W>(
    components: &BTreeMap<String, BTreeSet<Version>>,
    config: &Config,
    mut w: W,
) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    // first summarize the licenses
    let mut licenses: BTreeMap<&'static str, LicenseInfo> = BTreeMap::new();
    for (name, _) in components.iter() {
        let pkg = config.third_party.get(name).ok_or_else(|| {
            anyhow::Error::msg(format!("3rd party package {name} not in the allow list"))
        })?;
        for license in pkg.licenses.iter() {
            licenses.insert(license.spdx_short(), license.info());
        }
    }

    writeln!(
        w,
        "This distribution contains open source dependencies under the following licenses:"
    )?;
    writeln!(w)?;
    for (spdx, info) in licenses.iter() {
        writeln!(w, "  * {}", spdx)?;
        writeln!(w, "      - {}", info.url)?;
    }
    writeln!(w)?;
    writeln!(w, "Copies of these licenses are provided at the end of this document. They may also be obtained from the URLs above.")?;
    writeln!(w)?;

    for (name, versions) in components.iter() {
        let versions: Vec<String> = versions.iter().map(|x| x.to_string()).collect();

        let pkg = config.third_party.get(name).ok_or_else(|| {
            anyhow::Error::msg(format!("3rd party package {name} not in the allow list"))
        })?;
        writeln!(w, "crate: {}", pkg.id)?;
        writeln!(w, "version(s): {}", versions.join(", "))?;
        writeln!(w, "url: {}", pkg.url())?;

        if pkg.licenses.is_empty() {
            return Err(anyhow::Error::msg(format!(
                "No license specified for {name}",
            )));
        }

        let licenses: Vec<String> = pkg
            .licenses
            .iter()
            .map(|x| x.spdx_short().to_string())
            .collect();
        writeln!(w, "license(s): {}", licenses.join(" AND "))?;

        // write out copyright statements
        for lic in pkg.licenses.iter() {
            if let Some(lines) = lic.copyright() {
                for line in lines {
                    writeln!(w, "{}", line)?;
                }
            }
        }

        writeln!(w)?;
    }

    for info in licenses.values() {
        writeln!(w, "{}", info.text)?;
        writeln!(w)?;
    }

    Ok(())
}

fn extract_deps(
    bom: Bom,
    config: &Config,
) -> Result<BTreeMap<String, BTreeSet<Version>>, anyhow::Error> {
    let mut deps: BTreeMap<String, BTreeSet<Version>> = BTreeMap::new();

    let components = &bom
        .components
        .ok_or_else(|| anyhow::Error::msg("required field 'components' is 'None'"))?
        .0;

    'deps: for component in components.iter() {
        let version = component.version.as_ref().ok_or_else(|| {
            anyhow::Error::msg(format!("Missing version in component {}", component.name))
        })?;
        let version = semver::Version::parse(version)?;
        if config.build_only.contains(component.name.as_ref()) {
            continue 'deps;
        }

        if config.vendor.contains_key(component.name.as_ref()) {
            continue 'deps;
        }

        match deps.entry(component.name.to_string()) {
            Entry::Vacant(x) => {
                x.insert(BTreeSet::from([version]));
            }
            Entry::Occupied(mut x) => {
                x.get_mut().insert(version);
            }
        }
    }

    Ok(deps)
}
