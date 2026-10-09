//! Renderer: the human-readable report of the open-source licenses that ship in the product

use crate::approval::ValidatedInventory;
use crate::config::LicenseInfo;
use std::collections::BTreeMap;
use std::io::Write;

/// Write the license report of a validated inventory
pub(crate) fn write<W: Write>(validated: &ValidatedInventory, mut w: W) -> std::io::Result<()> {
    let shipped = validated.shipped_third_party();

    // every license that applies, once, with its text appended at the end
    let mut licenses: BTreeMap<&'static str, LicenseInfo> = BTreeMap::new();
    for (entry, _) in shipped.values() {
        for info in entry.licenses.iter().filter_map(|l| l.info()) {
            licenses.insert(info.spdx, info);
        }
    }

    writeln!(
        w,
        "This distribution contains open source dependencies under the following licenses:"
    )?;
    writeln!(w)?;
    for (spdx, info) in &licenses {
        writeln!(w, "  * {spdx}")?;
        writeln!(w, "      - {}", info.url())?;
    }
    writeln!(w)?;
    writeln!(w, "Copies of these licenses are provided at the end of this document. They may also be obtained from the URLs above.")?;
    writeln!(w)?;

    for (entry, versions) in shipped.values() {
        let versions: Vec<String> = versions.iter().map(ToString::to_string).collect();
        let spdx: Vec<&str> = entry
            .licenses
            .iter()
            .filter_map(|l| l.info())
            .map(|info| info.spdx)
            .collect();
        writeln!(w, "crate: {}", entry.id)?;
        writeln!(w, "version(s): {}", versions.join(", "))?;
        writeln!(w, "url: {}", entry.url())?;
        writeln!(w, "license(s): {}", spdx.join(" AND "))?;
        for line in entry
            .licenses
            .iter()
            .filter_map(|l| l.copyright())
            .flatten()
        {
            writeln!(w, "{line}")?;
        }
        writeln!(w)?;
    }

    for info in licenses.values() {
        writeln!(w, "{}", info.text)?;
        writeln!(w)?;
    }

    Ok(())
}
