use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A copyright statement associated with a license
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Copyright {
    /// Copyright statement is present in the license file that consists of one of more lines
    Lines(Vec<String>),
    /// No copyright statement is present in the license file
    NotPresent,
}

impl Copyright {
    fn lines(&self) -> Vec<String> {
        match self {
            Self::Lines(x) => x.clone(),
            Self::NotPresent => vec!["No copyright statement was provided by the author even though they license may refer to it".to_string()],
        }
    }
}

/// Where information about the crate can be found
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Source {
    /// This crate came from crates.io
    #[serde(rename = "crates.io")]
    CratesIo,
}

/// What the tool knows about a supported license
pub(crate) struct LicenseInfo {
    /// SPDX identifier
    pub(crate) spdx: &'static str,
    /// Full text, appended to the license report
    pub(crate) text: &'static str,
}

impl LicenseInfo {
    /// The license's page on spdx.org
    pub(crate) fn url(&self) -> String {
        format!("https://spdx.org/licenses/{}.html", self.spdx)
    }
}

/// License type
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum License {
    Unknown,
    #[serde(rename = "ISC")]
    Isc {
        copyright: Copyright,
    },
    #[serde(rename = "MIT")]
    Mit {
        copyright: Copyright,
    },
    /// Openssl / SSLeay license - <https://www.openssl.org/source/license-openssl-ssleay.txt>
    #[serde(rename = "OpenSSL")]
    OpenSsl,
    /// Boost software license v1 - <https://www.boost.org/users/license.html>
    #[serde(rename = "BSLv1")]
    Bsl1,
    /// MPL Version 2.0 - <https://www.mozilla.org/en-US/MPL/2.0/>
    #[serde(rename = "MPLv2")]
    Mpl2,
    /// 3-clause BSD  - <https://opensource.org/licenses/BSD-3-Clause>
    #[serde(rename = "BSD3")]
    Bsd3 {
        copyright: Copyright,
    },
    /// Unicode License Agreement - Data Files and Software (2016)
    #[serde(rename = "UnicodeDFS2016")]
    UnicodeDfs2016,
    /// Apache License 2.0 - <https://www.apache.org/licenses/LICENSE-2.0>
    #[serde(rename = "Apache2")]
    Apache2,
}

/// An allow-list entry for an open-source package whose license was reviewed
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ThirdPartyEntry {
    /// id of the allowed package
    pub(crate) id: String,
    /// Where the package came from
    pub(crate) source: Source,
    /// license identification
    pub(crate) licenses: Vec<License>,
    /// set when a build-time dependency copies its own code into the product (e.g. a build
    /// script writes source it provides), so it ships even though it is not linked
    #[serde(default)]
    pub(crate) embedded: bool,
}

impl ThirdPartyEntry {
    pub(crate) fn url(&self) -> String {
        match self.source {
            Source::CratesIo => format!("https://crates.io/crates/{}", self.id),
        }
    }
}

/// An allow-list entry for a package the vendor licenses to the customer under its commercial
/// license
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct VendorEntry {
    /// SCM URL where the package is located
    pub(crate) url: String,
    /// see [`ThirdPartyEntry::embedded`]
    #[serde(default)]
    pub(crate) embedded: bool,
}

/// The commercial license under which the vendor licenses its own code to the customer
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CommercialLicense {
    /// Human readable name of the license
    pub(crate) name: String,
    /// URL of the license text
    pub(crate) url: String,
}

/// Represent a configuration file for a particular project
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    /// packages approved for use at build time only (not linked or distributed); license not reviewed
    pub(crate) build_only: BTreeSet<String>,
    /// packages licensed to the customer by the vendor under its commercial license
    pub(crate) vendor: BTreeMap<String, VendorEntry>,
    /// open-source packages whose license was reviewed; approved to ship and to build
    pub(crate) third_party: BTreeMap<String, ThirdPartyEntry>,
    /// license of first-party and vendor packages, required to generate an SBOM
    #[serde(default)]
    pub(crate) commercial_license: Option<CommercialLicense>,
}

impl Config {
    /// Whether the package is marked as shipping its code although it is a build-time dependency
    pub(crate) fn is_embedded(&self, name: &str) -> bool {
        self.third_party.get(name).is_some_and(|p| p.embedded)
            || self.vendor.get(name).is_some_and(|p| p.embedded)
    }
}

impl License {
    /// The license's SPDX id and text, or `None` for `Unknown` (which approval rejects)
    pub(crate) fn info(&self) -> Option<LicenseInfo> {
        let (spdx, text) = match self {
            License::Unknown => return None,
            License::Isc { .. } => ("ISC", include_str!("../licenses/isc.txt")),
            License::Mit { .. } => ("MIT", include_str!("../licenses/mit.txt")),
            License::OpenSsl => ("OpenSSL", include_str!("../licenses/openssl.txt")),
            License::Bsl1 => ("BSL-1.0", include_str!("../licenses/bsl.txt")),
            License::Mpl2 => ("MPL-2.0", include_str!("../licenses/mpl2.txt")),
            License::Bsd3 { .. } => ("BSD-3-Clause", include_str!("../licenses/bsd3.txt")),
            License::UnicodeDfs2016 => (
                "Unicode-DFS-2016",
                include_str!("../licenses/unicode_dfs_2016.txt"),
            ),
            License::Apache2 => ("Apache-2.0", include_str!("../licenses/apache2.txt")),
        };
        Some(LicenseInfo { spdx, text })
    }

    /// Copyright lines recorded in the review, for licenses that require them
    pub(crate) fn copyright(&self) -> Option<Vec<String>> {
        match self {
            License::Isc { copyright }
            | License::Mit { copyright }
            | License::Bsd3 { copyright } => Some(copyright.lines()),
            License::Unknown
            | License::OpenSsl
            | License::Bsl1
            | License::Mpl2
            | License::UnicodeDfs2016
            | License::Apache2 => None,
        }
    }
}
