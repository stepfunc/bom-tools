use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Formatter;
use std::path::Path;
use std::str::FromStr;

use cargo_metadata::Message;

use crate::config::Config;

#[derive(Debug)]
struct PackageInfo {
    id: String,
    version: semver::Version,
    source: String,
}

impl From<PackageInfo> for PackageUsage {
    fn from(info: PackageInfo) -> Self {
        let mut versions = BTreeSet::new();
        versions.insert(info.version);
        PackageUsage {
            versions: Versions { inner: versions },
            source: info.source,
        }
    }
}

#[derive(Debug)]
pub struct Versions {
    inner: BTreeSet<semver::Version>,
}

impl Versions {
    pub fn contains(&self, version: &semver::Version) -> bool {
        self.inner.contains(version)
    }

    pub fn values(&self) -> impl Iterator<Item = &semver::Version> {
        self.inner.iter()
    }
}

impl std::fmt::Display for Versions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        let strings: Vec<String> = self.inner.iter().map(|x| x.to_string()).collect();
        write!(f, "{}", strings.join(", "))?;
        write!(f, "]")
    }
}

#[derive(Debug)]
pub struct PackageUsage {
    pub versions: Versions,
    pub source: String,
}

fn error<S: AsRef<str>>(text: S) -> Box<dyn std::error::Error> {
    text.as_ref().into()
}

impl FromStr for PackageInfo {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split_ascii_whitespace();

        let id = split.next().ok_or_else(|| error("missing id"))?;
        let version = split.next().ok_or_else(|| error("missing version"))?;
        let source = split.next().ok_or_else(|| error("missing source"))?;

        Ok(Self {
            id: id.to_string(),
            version: semver::Version::parse(version)?,
            source: source.to_string(),
        })
    }
}

pub struct BuildLog {
    pub packages: BTreeMap<String, PackageUsage>,
}

impl BuildLog {
    pub fn remove_vendor_deps(&mut self, config: &Config) {
        self.packages
            .retain(|id, _| !config.vendor.contains_key(id))
    }
    pub fn remove_build_deps(&mut self, config: &Config) {
        self.packages
            .retain(|id, _| !config.build_only.contains(id));
    }
}

pub fn read_log(path: &Path) -> Result<BuildLog, Box<dyn Error>> {
    let file = std::fs::File::open(path)?;

    let reader = std::io::BufReader::new(file);
    let mut packages: BTreeMap<_, PackageUsage> = BTreeMap::new();
    for item in Message::parse_stream(reader) {
        if let Message::CompilerArtifact(art) = item? {
            let info = PackageInfo::from_str(&art.package_id.repr)?;
            match packages.get_mut(&info.id) {
                None => {
                    packages.insert(info.id.clone(), info.into());
                }
                Some(existing) => {
                    if existing.source != info.source {
                        return Err(error(format!(
                            "package {} has different sources, {} and {}",
                            info.id, info.source, existing.source
                        )));
                    }
                    existing.versions.inner.insert(info.version);
                }
            }
        }
    }
    Ok(BuildLog { packages })
}
