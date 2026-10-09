# Changelog

Notable changes to the `allow-list` tool, newest first. Version numbers mark
CLI releases (so a consumer can pin a specific build, e.g. `cargo install --tag`);
they are not a library API compatibility promise.

## [0.3.0] - 2026-10-09

Breaking: the cargo-cyclonedx commands are removed, and `gen-licenses-log` /
`gen-licenses-log-dir` take new inputs and arguments and their output changes (see below).
See the README for how to produce the inputs. Consumers that install from `main` without a
tag should pin `--tag 0.2.1` until they migrate.

### Added

- `gen-sbom-log` and `gen-sbom-log-dir`: a CycloneDX 1.5 JSON SBOM from the same validated
  inventory as the license report. Runtime packages have `scope: required`, build-time
  packages `scope: excluded`; licenses are the declared SPDX expressions (commercial license
  for first-party and vendor packages); `--lockfile` adds SHA-256 hashes; dependencies are
  the observed package edges; `--omit-serial-number` with `SOURCE_DATE_EPOCH` gives
  reproducible output.
- Every compiled package is classified as runtime or build-time by `cargo tree` (Cargo's own
  resolver), cross-checked against the build log: packages and (package, features) variants
  must match exactly. Build logs are validated (successful `build-finished`, no test, example
  or bench artifacts, a product artifact for the root).
- `allowed.json` is enforced as an approval list instead of a filter: every package outside
  the workspace must be in exactly one of `third_party`, `vendor`, `build_only`; a
  `build_only` package that becomes linked fails; reviewed licenses of shipped `third_party`
  packages are checked against the declared SPDX expression in both directions.
- `embedded` flag on `third_party` / `vendor` entries, `commercial_license`, and the
  `Apache2` license.

### Changed

- `gen-licenses-log` / `gen-licenses-log-dir` require `--root-package` and the two
  `cargo tree` outputs per target (`--tree`, `--runtime-tree`, and `--log` for `-g`; fixed names
  `build.json`, `tree.txt`, `runtime-tree.txt` in each target directory for the `-dir`
  variant, which no longer takes `--log-file`). `cargo metadata` should be generated with
  `--all-features`.
- Their report now lists only packages linked into the product: build-time packages
  (proc-macros and their dependencies, build-script dependencies) are no longer listed, and
  first-party crates are matched by name and version instead of being skipped.

### Removed

- `gen-licenses` / `gen-licenses-dir`. They read `cargo cyclonedx` SBOMs, which describe the
  dependency graph for cargo-cyclonedx's own feature selection rather than the features the
  product was built with (e.g. dnp3 1.7.0's report listed `ring` and omitted `aws-lc-*` for
  targets built with `tls-aws-lc`).

## [0.2.1] - 2026-07-22

### Fixed

- `gen-licenses-log` / `gen-licenses-log-dir` now skip first-party (path /
  workspace-local) crates. Their `PackageId` embeds an absolute build path that
  differs between environments (for example a `cross` container mounted at
  `/project` versus the host that ran `cargo metadata`), so they could not be
  matched by id across environments. They are not third-party dependencies, so
  they are excluded from the report; the fail-closed check still applies to
  every registry crate.

## [0.2.0] - 2026-07-22

### Added

- `gen-licenses-log` and `gen-licenses-log-dir` subcommands that derive the
  third-party license report from a cargo build log
  (`cargo build --message-format=json`) rather than a CycloneDX SBOM. The build
  log defines scope (the crates that were actually compiled), and each compiled
  crate's identity is resolved to a typed name and version via
  `cargo metadata --format-version 1` output, supplied with `--metadata`.

  This avoids over-listing optional / feature-gated dependencies that appear in
  the resolved dependency graph but are not compiled into a given build. For
  example, a `--no-default-features` build that disables serial support no
  longer reports the serial-only crates (`serialport`, `tokio-serial`, etc.) in
  its license file, which a CycloneDX-derived report incorrectly includes.

  The existing `gen-licenses` and `gen-licenses-dir` (CycloneDX) subcommands are
  unchanged.

## [0.1.0]

- Initial release: `gen-licenses` and `gen-licenses-dir` subcommands that
  produce a human-readable third-party license report from a CycloneDX SBOM,
  validated against a JSON allow-list configuration.
