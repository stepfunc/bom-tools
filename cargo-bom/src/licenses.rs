use crate::config::{Config, LicenseInfo};
use std::collections::BTreeMap;
use std::path::Path;

/// Generate a license summary file from a build log and configuration file
pub fn gen_licenses<W>(
    log_path: &Path,
    config_path: &Path,
    mut w: W,
) -> Result<(), Box<dyn std::error::Error>>
where
    W: std::io::Write,
{
    let mut log = crate::log::read_log(log_path)?;

    let config: Config = serde_json::from_reader(std::fs::File::open(config_path)?)?;

    // we don't care about these for license purposes, just the OSS that is linked into the library
    log.remove_build_deps(&config);
    log.remove_vendor_deps(&config);

    // first summarize the licenses
    let mut licenses: BTreeMap<&'static str, LicenseInfo> = BTreeMap::new();
    for (id, _) in log.packages.iter() {
        let pkg = config
            .third_party
            .get(id)
            .ok_or_else(|| format!("3rd party package {} not in the allow list", id))?;
        for license in pkg.licenses.iter() {
            licenses.insert(license.spdx_short(), license.info());
        }
    }

    writeln!(
        w,
        "This binary contains open source dependencies under the following licenses:"
    )?;
    writeln!(w)?;
    for (spdx, info) in licenses.iter() {
        writeln!(w, "  * {}", spdx)?;
        writeln!(w, "      - {}", info.url)?;
    }
    writeln!(w)?;
    writeln!(w, "Copies of these licenses are provided at the end of this document. They may also be obtained from the URLs above.")?;
    writeln!(w)?;

    for (id, usage) in log.packages.iter() {
        let versions: Vec<String> = usage.versions.values().map(|x| x.to_string()).collect();

        let pkg = config
            .third_party
            .get(id)
            .ok_or_else(|| format!("3rd party package {} not in the allow list", id))?;
        writeln!(w, "crate: {}", pkg.id)?;
        writeln!(w, "version(s): {}", versions.join(", "))?;
        writeln!(w, "url: {}", pkg.url())?;

        if pkg.licenses.is_empty() {
            return Err(format!("No license specified for {}", id).into());
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
