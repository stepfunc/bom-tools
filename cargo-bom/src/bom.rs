use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::log::BuildLog;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum BinaryType {
    Application,
    Library,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSource {
    pub spdx_short: String,
    pub copyrights: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LicenseType {
    Vendor,
    OpenSource(Vec<OpenSource>),
}

/// The subject of the BOM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    /// The crate name
    crate_name: String,
    /// url of the subject crate
    url: String,
    /// Version of the library
    version: semver::Version,
}

/// A dependency that is linked into the subject binary statically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
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
pub struct Bom {
    /// Time of creation of the BOM
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Subject of the BOM
    pub subject: Subject,
    /// Dependencies of the subject that are statically linked into it
    pub dependencies: Vec<Dependency>,
}

pub struct SubjectConfig {
    pub crate_name: String,
    pub url: String,
}

pub fn create_bom(
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
        version: subject_version,
        url: subject_config.url,
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
