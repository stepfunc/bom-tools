use crate::log::BuildLog;
use crate::Config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub(crate) enum BinaryType {
    Application,
    Library,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenSource {
    pub(crate) spdx_short: String,
    pub(crate) copyrights: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum LicenseType {
    Vendor,
    OpenSource(Vec<OpenSource>),
}

/// The subject of the BOM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Subject {
    /// The crate name
    crate_name: String,
    /// The name of the vendor
    vendor_name: String,
    /// Version of the library
    version: semver::Version,
}

/// A dependency that is linked into the subject binary statically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Dependency {
    /// The crate name
    crate_name: String,
    /// Url for the library
    url: String,
    /// Versions of the library
    versions: Vec<semver::Version>,
    /// license type
    license: LicenseType,
}

/// Bill of materials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Bom {
    /// Time of creation of the BOM
    pub(crate) timestamp: chrono::DateTime<chrono::Utc>,
    /// Subject of the BOM
    pub(crate) subject: Subject,
    /// Dependencies of the subject that are statically linked into it
    pub(crate) dependencies: Vec<Dependency>,
}

pub(crate) struct SubjectConfig {
    pub(crate) crate_name: String,
    pub(crate) vendor_name: String,
}

pub(crate) fn create_bom(
    subject_config: SubjectConfig,
    mut log: BuildLog,
    config: Config,
) -> Result<Bom, Box<dyn std::error::Error>> {
    // we do not care about build-only dependencies in the BOM
    log.remove_build_deps(&config);

    let subject_usage = match log.packages.remove(&subject_config.crate_name) {
        None => {
            return Err(format!(
                "Subject crate {} not in build log",
                subject_config.crate_name
            )
            .into())
        }
        Some(usage) => usage,
    };

    let subject_version = {
        let mut versions = subject_usage.versions.values();
        match versions.next() {
            Some(x) => x.clone(),
            None => {
                return Err(format!(
                    "Subject crate {} does not include a version in build log",
                    subject_config.crate_name
                )
                .into())
            }
        }
    };

    let subject = Subject {
        crate_name: subject_config.crate_name,
        vendor_name: subject_config.vendor_name,
        version: subject_version,
    };

    let mut dependencies = Vec::new();
    for (id, usage) in log.packages {
        // check if this is vendor dependency
        let dep = if config.vendor.contains(&id) {
            Dependency {
                crate_name: id.clone(),
                url: "TODO".to_string(),
                versions: usage.versions.values().cloned().collect(),
                license: LicenseType::Vendor,
            }
        } else {
            let pkg = match config.third_party.get(&id) {
                Some(x) => x,
                None => {
                    return Err(format!("3rd party package not found in allow-list: {}", id).into())
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
