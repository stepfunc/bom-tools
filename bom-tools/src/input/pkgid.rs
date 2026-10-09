//! Parsing of Cargo package id strings, as found in build logs (and Cargo's SBOM precursor files)

use crate::graph::{is_crates_io, PackageKey, SourceKind};
use anyhow::anyhow;
use semver::Version;

/// Parse a package id in Cargo's package id spec format (Cargo 1.77+), e.g.
///
/// * `registry+https://github.com/rust-lang/crates.io-index#serde@1.0.228`
/// * `path+file:///build/dnp3/ffi/dnp3-ffi#1.7.0` (name omitted: it equals the last path segment)
/// * `path+file:///build/dnp3/ffi/schema#dnp3-schema@1.7.0`
pub(crate) fn parse(id: &str) -> Result<PackageKey, anyhow::Error> {
    let (url, fragment) = id.split_once('#').ok_or_else(|| {
        anyhow!("unsupported package id {id:?}: expected `<source>#<name>@<version>` (Cargo 1.77+)")
    })?;
    let (name, version) = match fragment.split_once('@') {
        Some((name, version)) => (name, version),
        None => (last_path_segment(url), fragment),
    };
    if name.is_empty() {
        return Err(anyhow!("package id {id:?} has no package name"));
    }
    let version = Version::parse(version)
        .map_err(|err| anyhow!("package id {id:?} has an invalid version: {err}"))?;
    let source = if is_crates_io(url) {
        SourceKind::CratesIo
    } else if url.starts_with("path+") {
        SourceKind::Path
    } else {
        SourceKind::Other(url.to_string())
    };
    Ok(PackageKey {
        name: name.to_string(),
        version,
        source,
    })
}

fn last_path_segment(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_util::{key, path_key};

    #[test]
    fn parses_registry_ids() {
        assert_eq!(
            parse("registry+https://github.com/rust-lang/crates.io-index#serde@1.0.228").unwrap(),
            key("serde", "1.0.228")
        );
        assert_eq!(
            parse("sparse+https://index.crates.io/#serde@1.0.228").unwrap(),
            key("serde", "1.0.228")
        );
    }

    #[test]
    fn parses_path_ids_with_and_without_name() {
        assert_eq!(
            parse("path+file:///project/ffi/dnp3-ffi#1.7.0").unwrap(),
            path_key("dnp3-ffi", "1.7.0")
        );
        assert_eq!(
            parse("path+file:///home/user/code/dnp3/ffi/schema#dnp3-schema@1.7.0").unwrap(),
            path_key("dnp3-schema", "1.7.0")
        );
    }

    #[test]
    fn keeps_other_sources() {
        let key = parse("git+https://github.com/example/repo?branch=main#thing@0.1.0").unwrap();
        assert_eq!(key.name, "thing");
        assert_eq!(
            key.source,
            SourceKind::Other("git+https://github.com/example/repo?branch=main".to_string())
        );
        let key = parse("registry+https://my.registry/index#thing@0.1.0").unwrap();
        assert!(matches!(key.source, SourceKind::Other(_)));
    }

    #[test]
    fn rejects_legacy_and_malformed_ids() {
        assert!(
            parse("serde 1.0.228 (registry+https://github.com/rust-lang/crates.io-index)").is_err()
        );
        assert!(parse("path+file:///project/x#not-a-version").is_err());
    }
}
