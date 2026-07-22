# Changelog

Notable changes to the `allow-list` tool, newest first. Version numbers mark
CLI releases (so a consumer can pin a specific build, e.g. `cargo install --tag`);
they are not a library API compatibility promise.

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
