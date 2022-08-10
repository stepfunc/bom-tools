use cargo_metadata::Message;
use clap::Parser;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// path to Cargo.toml
    #[clap(short, long, value_parser)]
    build_log: PathBuf,
}

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
            versions,
            source: info.source,
        }
    }
}

#[derive(Debug)]
struct PackageUsage {
    versions: BTreeSet<semver::Version>,
    source: String,
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

fn read_packages(path: &Path) -> Result<BTreeMap<String, PackageUsage>, Box<dyn Error>> {
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
                    existing.versions.insert(info.version);
                }
            }
        }
    }
    Ok(packages)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let packages = read_packages(&args.build_log)?;

    for (id, usage) in packages {
        println!("{} {:?}", id, usage.versions)
    }

    Ok(())
}
