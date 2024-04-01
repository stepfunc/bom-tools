use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A copyright statement associated with a license
#[derive(Serialize, Deserialize, Debug)]
pub enum Copyright {
    /// Copyright statement is present in the license file that consists of one of more lines
    Lines(Vec<String>),
    /// No copyright statement is present in the license file
    NotPresent,
}

impl Copyright {
    fn lines(&self) -> Vec<String> {
        match self {
            Copyright::Lines(x) => x.clone(),
            Copyright::NotPresent => vec!["No copyright statement was provided by the author even though they license may refer to it".to_string()],
        }
    }
}

/// Where information about the crate can be found
#[derive(Serialize, Deserialize, Debug)]
pub enum Source {
    /// This crate came from crates.io
    #[serde(rename = "crates.io")]
    CratesIo,
}

/// Information about a license
pub struct LicenseInfo {
    /// URL of the license
    pub url: &'static str,
    /// Text of the license
    pub text: &'static str,
}

/// License type
#[derive(Serialize, Deserialize, Debug)]
pub enum License {
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
}

/// Information about a dependency
#[derive(Serialize, Deserialize, Debug)]
pub struct Package {
    /// id of the allowed package
    pub id: String,
    /// Where the package came from
    pub source: Source,
    /// license identification
    pub licenses: Vec<License>,
}

impl Package {
    pub fn url(&self) -> String {
        match self.source {
            Source::CratesIo => format!("https://crates.io/crates/{}", self.id),
        }
    }
}

/// Information about a vendor package
#[derive(Serialize, Deserialize, Debug)]
pub struct VendorPackage {
    /// SCM URL where the package is located
    pub url: String,
}

/// Represent a configuration file for a particular project
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    /// packages that are build-only dependencies, are not linked/distributed, and are ignored in the build log
    pub build_only: BTreeSet<String>,
    /// packages that are licensed by the vendor and are distributed under a custom license
    pub vendor: BTreeMap<String, VendorPackage>,
    /// 3rd party packages that are allowed to be build dependencies
    pub third_party: BTreeMap<String, Package>,
}

impl License {
    /// Information about the license
    pub fn info(&self) -> LicenseInfo {
        LicenseInfo {
            url: self.url(),
            text: self.text(),
        }
    }

    /// Optional copyright lines provided by the author(s)
    pub fn copyright(&self) -> Option<Vec<String>> {
        match self {
            License::Unknown => None,
            License::Isc { copyright } => Some(copyright.lines()),
            License::Mit { copyright } => Some(copyright.lines()),
            License::OpenSsl => None,
            License::Bsl1 => None,
            License::Mpl2 => None,
            License::Bsd3 { copyright } => Some(copyright.lines()),
            License::UnicodeDfs2016 => None,
        }
    }

    /// The text of the license itself
    pub fn text(&self) -> &'static str {
        match self {
            License::Isc { .. } => std::include_str!("../licenses/isc.txt"),
            License::Mit { .. } => std::include_str!("../licenses/mit.txt"),
            License::OpenSsl => std::include_str!("../licenses/openssl.txt"),
            License::Bsl1 => std::include_str!("../licenses/bsl.txt"),
            License::Mpl2 => std::include_str!("../licenses/mpl2.txt"),
            License::Bsd3 { .. } => std::include_str!("../licenses/bsd3.txt"),
            License::UnicodeDfs2016 => {
                std::include_str!("../licenses/unicode_dfs_2016.txt")
            }
            License::Unknown => panic!("You must define unknown licenses"),
        }
    }

    /// SPDX short abbreviation for the license
    pub fn spdx_short(&self) -> &'static str {
        match self {
            License::Isc { .. } => "ISC",
            License::Mit { .. } => "MIT",
            License::OpenSsl => "OpenSSL",
            License::Bsl1 => "BSL-1.0",
            License::Mpl2 => "MPL-2.0",
            License::Bsd3 { .. } => "BSD-3-Clause",
            License::UnicodeDfs2016 => "Unicode-DFS-2016",
            License::Unknown => {
                panic!("You must define unknown licenses")
            }
        }
    }

    /// The URL with information about the license
    pub fn url(&self) -> &'static str {
        match self {
            License::Isc { .. } => "https://spdx.org/licenses/ISC.html",
            License::Mit { .. } => "https://spdx.org/licenses/MIT.html",
            License::OpenSsl => "https://spdx.org/licenses/OpenSSL.html",
            License::Bsl1 => "https://spdx.org/licenses/BSL-1.0.html",
            License::Mpl2 => "https://spdx.org/licenses/MPL-2.0.html",
            License::Bsd3 { .. } => "https://spdx.org/licenses/BSD-3-Clause.html",
            License::UnicodeDfs2016 => "https://spdx.org/licenses/Unicode-DFS-2016.html",
            License::Unknown => {
                panic!("You must define unknown licenses")
            }
        }
    }
}
