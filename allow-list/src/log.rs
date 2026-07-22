use crate::config::Config;
use crate::licenses::gen_licenses_for;
use anyhow::anyhow;
use cargo_metadata::{Message, Metadata, Package, PackageId};
use semver::Version;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{read_dir, File};
use std::io::{BufReader, Write};
use std::path::Path;

/// Parse the `cargo metadata --format-version 1` JSON emitted by the build.
///
/// This is used only as a typed `PackageId -> Package` dictionary for resolving
/// the crates that actually compiled; it never determines scope, so its
/// (superset) package list is harmless.
fn load_metadata(path: &Path) -> Result<Metadata, anyhow::Error> {
    Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
}

/// Read a single `cargo build --message-format=json` log, resolving each
/// compiled artifact's `PackageId` to its typed name and version via `by_id`.
fn read_log_into(
    path: &Path,
    by_id: &HashMap<&PackageId, &Package>,
    deps: &mut BTreeMap<String, BTreeSet<Version>>,
) -> Result<(), anyhow::Error> {
    let reader = BufReader::new(File::open(path)?);
    for message in Message::parse_stream(reader) {
        if let Message::CompilerArtifact(artifact) = message? {
            // Skip first-party / path dependencies (workspace-local crates).
            // Their PackageId embeds an absolute filesystem path that differs
            // between the build environment (e.g. a `cross` container mounted at
            // /project) and wherever `cargo metadata` ran, so they can't be
            // matched by id across environments. They are our own code, not a
            // third-party license, so they are excluded from the report anyway.
            if artifact.package_id.repr.starts_with("path+") {
                continue;
            }
            // The build log tells us WHICH crates compiled; the metadata gives
            // us their typed name/version. Fail closed if the two are out of sync.
            let package = by_id.get(&artifact.package_id).ok_or_else(|| {
                anyhow!(
                    "compiled crate {:?} is not present in the cargo metadata; \
                     the build log and metadata must come from the same resolution",
                    artifact.package_id
                )
            })?;
            match deps.entry(package.name.clone()) {
                Entry::Vacant(x) => {
                    x.insert(BTreeSet::from([package.version.clone()]));
                }
                Entry::Occupied(mut x) => {
                    x.get_mut().insert(package.version.clone());
                }
            }
        }
    }
    Ok(())
}

/// Drop build-only and vendor-licensed packages, matching the cyclonedx path.
fn filter_deps(
    mut deps: BTreeMap<String, BTreeSet<Version>>,
    config: &Config,
) -> BTreeMap<String, BTreeSet<Version>> {
    deps.retain(|name, _| {
        !config.build_only.contains(name) && !config.vendor.contains_key(name)
    });
    deps
}

/// Generate a license summary from a single cargo build log, using the cargo
/// metadata to resolve crate identities and the configuration for the allow-list.
pub(crate) fn gen_licenses_from_log<W>(
    log_path: &Path,
    metadata_path: &Path,
    config_path: &Path,
    w: W,
) -> Result<(), anyhow::Error>
where
    W: Write,
{
    let config: Config = serde_json::from_reader(BufReader::new(File::open(config_path)?))?;
    let metadata = load_metadata(metadata_path)?;
    let by_id: HashMap<&PackageId, &Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    let mut deps = BTreeMap::new();
    read_log_into(log_path, &by_id, &mut deps)?;
    let deps = filter_deps(deps, &config);
    gen_licenses_for(&deps, &config, w)
}

/// Generate a license summary by consolidating one named build log per immediate
/// subdirectory of `list_dir` (mirrors `gen_licenses_in_dirs`), resolved against
/// a single cargo metadata document.
pub(crate) fn gen_licenses_from_log_dir<W>(
    list_dir: &Path,
    log_file: &str,
    metadata_path: &Path,
    config_path: &Path,
    w: W,
) -> Result<(), anyhow::Error>
where
    W: Write,
{
    let config: Config = serde_json::from_reader(BufReader::new(File::open(config_path)?))?;
    let metadata = load_metadata(metadata_path)?;
    let by_id: HashMap<&PackageId, &Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    let mut deps: BTreeMap<String, BTreeSet<Version>> = BTreeMap::new();
    for item in read_dir(list_dir)? {
        let item = item?;
        if item.file_type()?.is_dir() {
            read_log_into(&item.path().join(log_file), &by_id, &mut deps)?;
        }
    }

    let deps = filter_deps(deps, &config);
    gen_licenses_for(&deps, &config, w)
}
