use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::log::BuildLog;

/// Type of binary
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum BinaryType {
    /// Binary is an application
    Application,
    /// Binary is a library
    Library,
}

/// Information about an open source dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSource {
    /// SPDX short abbreviation for the dependency
    pub spdx_short: String,
    /// Optional copyright lines provided by the author(s)
    pub copyrights: Option<Vec<String>>,
}

/// Type of license
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LicenseType {
    /// Customer-specific license governed by a custom license agreement
    Vendor,
    /// One or more open source licenses
    OpenSource(Vec<OpenSource>),
}

/// The subject of the BOM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    /// The crate name
    pub crate_name: String,
    /// url of the subject crate
    pub url: String,
    /// Version of the library
    pub version: semver::Version,
}

/// A dependency that is linked into the subject binary statically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// The crate name
    pub crate_name: String,
    /// Url for the library
    pub url: String,
    /// Versions of the library
    pub versions: Vec<semver::Version>,
    /// license type
    pub license: LicenseType,
}

/// Bill of materials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bom {
    /// Time of creation of the BOM
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Subject of the BOM
    pub subject: Subject,
    /// Dependencies of the subject that are statically linked into it
    pub dependencies: Vec<Dependency>,
}

/// Create a BOM from:
///
/// * subject_config - configuration for the subject
/// * log - build log output by cargo
/// * config - configuration for the package
///
pub fn create_bom(
    subject_name: String,
    mut log: BuildLog,
    mut config: Config,
) -> Result<Bom, Box<dyn std::error::Error>> {
    // we do not care about build-only dependencies in the BOM
    log.remove_build_deps(&config);

    // the subject must be one of the vendor crates
    let subject_pkg = match config.vendor.remove(&subject_name) {
        None => {
            return Err(
                format!("subject {} is not in the vendor package list", subject_name).into(),
            )
        }
        Some(pkg) => pkg,
    };

    let subject_usage = match log.packages.remove(&subject_name) {
        None => return Err(format!("Subject crate {} not in build log", subject_name).into()),
        Some(usage) => usage,
    };

    let subject_version = {
        let mut versions = subject_usage.versions.values();
        match versions.next() {
            Some(x) => x.clone(),
            None => {
                return Err(format!(
                    "Subject crate {} does not include a version in build log",
                    subject_name
                )
                .into())
            }
        }
    };

    let subject = Subject {
        crate_name: subject_name.clone(),
        version: subject_version,
        url: subject_pkg.url,
    };

    let mut dependencies = Vec::new();
    for (id, usage) in log.packages {
        // check if this is vendor dependency
        let dep = match config.vendor.get(&id) {
            Some(pkg) => Dependency {
                crate_name: id.clone(),
                url: pkg.url.to_string(),
                versions: usage.versions.values().cloned().collect(),
                license: LicenseType::Vendor,
            },
            None => {
                let pkg = match config.third_party.get(&id) {
                    Some(x) => x,
                    None => {
                        return Err(
                            format!("3rd party package not found in allow-list: {}", id).into()
                        )
                    }
                };

                let licenses: Vec<OpenSource> = pkg
                    .licenses
                    .iter()
                    .map(|lic| OpenSource {
                        spdx_short: lic.spdx_short().to_string(),
                        copyrights: lic.copyright(),
                    })
                    .collect();

                Dependency {
                    crate_name: id.clone(),
                    url: pkg.url(),
                    versions: usage.versions.values().cloned().collect(),
                    license: LicenseType::OpenSource(licenses),
                }
            }
        };

        dependencies.push(dep);
    }

    let bom = Bom {
        timestamp: chrono::Utc::now(),
        subject,
        dependencies,
    };

    Ok(bom)
}
