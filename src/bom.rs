use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub(crate) enum BinaryType {
    Application,
    Library,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenSource {
    pub(crate) spdx_short: String,
    pub(crate) copyright: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum LicenseType {
    Vendor,
    OpenSource(OpenSource),
}

/// The subject of the BOM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Subject {
    /// The crate name
    crate_name: String,
    /// The name of the vendor
    vendor_name: String,
}

/// A dependency that is linked into the subject binary statically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Dependency {
    /// The crate name
    crate_name: String,
    /// Url for the library
    url: String,
    /// Version of the library
    version: semver::Version,
    /// license type
    license: LicenseType,
    /// Any copyright assertions made by the author(s)
    copyright: Option<Vec<String>>,
}

/// Bill of materials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Bom {
    /// Subject of the BOM
    pub(crate) subject: Subject,
    /// Dependencies of the subject that are statically linked into it
    pub(crate) dependencies: Vec<Dependency>,
}
