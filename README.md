# bom-tools

`bom-tools` produces the third-party license report (`third-party-licenses.txt`) and a
CycloneDX 1.5 SBOM for a Rust product, from the evidence of the build that produced it, and
checks every dependency against a reviewed allow list (`allowed.json`).

```sh
cargo install --git https://github.com/stepfunc/bom-tools.git --tag 0.3.0
```

## How it works

The scope of the report and the SBOM is what the build actually compiled, never what the
dependency graph could contain. For each target the product is built for:

1. the `cargo build --message-format json` log proves which packages compiled, with which
   features;
2. `cargo tree` (Cargo's own resolver) tells which of them are linked into the product
   (**runtime**) and which only run while building it (**build time**: build scripts,
   proc-macros and their dependencies);
3. the two are cross-checked: if they disagree on the compiled packages or their enabled
   features (for example a tree made with different flags), the run fails.

Targets are merged (a package that is runtime on any target is runtime), resolved against
`cargo metadata` for licenses and sources, and approved against `allowed.json`. The license
report and the SBOM are two views of that one validated inventory: the report lists exactly
the runtime `third_party` packages, which are exactly the SBOM's `required` third-party
components.

## Producing the build evidence

Run these in the **same job and environment as the build** (same host, toolchain, checkout,
lockfile and Cargo configuration), with **one shared argument string**, and keep the files
only if all three succeed:

```sh
ARGS="-p dnp3-ffi --target $TARGET --no-default-features --features serial,tls --locked"
TREE_FMT="--prefix depth --color never --format {p}|{f}"
mkdir -p evidence/$TARGET
cargo build --release $ARGS --message-format json > evidence/$TARGET/build.json
cargo tree $ARGS -e normal,build         $TREE_FMT > evidence/$TARGET/tree.txt
cargo tree $ARGS -e normal,no-proc-macro $TREE_FMT > evidence/$TARGET/runtime-tree.txt
```

Always pass `--target`, even for a native build: without it `cargo tree` does not separate
host and target features, so a dependency that builds with different features for a build
script and for the product (e.g. `tracing-core` in dnp3) makes the cross-check fail.

Use `cross build` instead of `cargo build` for cross builds; the trees can run on the runner
when its host triple equals the container's. The trees must come from the build's host:
build dependencies are resolved for the host, so a tree produced on a Linux packaging runner
does not describe a build that ran on Windows.

A complete example of the recipe in CI, including Windows, macOS and `cross`, is dnp3's
[`build-evidence.sh`](https://github.com/stepfunc/dnp3/blob/main/.github/scripts/build-evidence.sh).

Once, anywhere, from the same checkout:

```sh
cargo metadata --format-version 1 --all-features --locked > metadata.json
```

This is only a dictionary of package details (license, source, workspace membership); it
never decides scope or roles.

## Commands

Both commands read a directory with one subdirectory per target (holding `build.json`,
`tree.txt` and `runtime-tree.txt`); a single target is a directory with one subdirectory.

```sh
bom-tools licenses -e evidence --root-package dnp3-ffi -m metadata.json -c allowed.json \
    > third-party-licenses.txt
bom-tools sbom -e evidence --root-package dnp3-ffi -m metadata.json -c allowed.json \
    --lockfile Cargo.lock > dnp3-ffi-1.7.0.cdx.json
```

`sbom` also takes:

- `--lockfile <Cargo.lock>` (required): the build's lockfile, for each crates.io package's
  SHA-256 (the `.crate` archive hash Cargo verified when downloading it);
- `--omit-serial-number`: with `SOURCE_DATE_EPOCH` (used as the timestamp), output is
  byte-for-byte reproducible.

### Migrating from 0.2

0.3.0 renamed the executable from `allow-list` to `bom-tools`, replaced `gen-licenses-log*`
with `licenses`, added `sbom`, and removed `gen-licenses` / `gen-licenses-dir`, which read
`cargo cyclonedx` SBOMs: those describe cargo-cyclonedx's own feature selection, not the
product's build. Until you migrate, pin
`cargo install --git https://github.com/stepfunc/bom-tools.git --tag 0.2.1`.

## `allowed.json`

```jsonc
{
  // approved to run at build time only; license not reviewed
  "build_only": ["cc", "syn"],
  // licensed to the customer by the vendor under its commercial license
  "vendor": {
    "sfio-tokio-ffi": { "url": "https://github.com/stepfunc/tokio-ffi", "embedded": true }
  },
  // open-source, license reviewed
  "third_party": {
    "ring": { "id": "ring", "source": "crates.io", "licenses": ["Apache2", { "ISC": { "copyright": "NotPresent" } }] }
  },
  // names the license of first-party and vendor packages in the SBOM
  "commercial_license": { "name": "Step Function I/O License Agreement", "url": "https://..." }
}
```

Rules, checked on every run (all violations are reported together). Workspace members are
our own code and need no entry; the root package must be one.

- every compiled package outside the workspace is in **exactly one** list;
- a runtime package outside the workspace must be in `third_party` or `vendor`; a `build_only`
  package that becomes linked into the product is an error;
- a build-time package may be in any list;
- only crates.io and workspace packages are supported;
- for runtime `third_party` packages, every reviewed license must be part of the package's
  declared SPDX `license`, and the reviewed licenses must satisfy it (reviewed `MIT` satisfies
  `MIT OR Apache-2.0` but not `MIT AND Apache-2.0`).

`commercial_license` is required by the `sbom` command. `embedded: true` marks a build-time dependency whose own code ships anyway (for example a build
script that copies source it provides into the product); it is treated as runtime, its
dependencies are not. License names: `MIT`, `ISC`, `BSD3` (with `copyright`), `Apache2`,
`OpenSSL`, `BSLv1`, `MPLv2`, `UnicodeDFS2016`.

## Guarantees and limits

The cross-checks prove that the full `cargo tree` has exactly the packages, and exactly the
(package, enabled features) variants, of the build log, and that every variant in the runtime
tree is also in the full tree. They do not prove that the files came from one invocation, nor
anything about edges. That catches different feature flags, wider builds and dropped artifacts whose
features differ from the package's other variants.

Not detected, so guaranteed only by producing the files as above:

- a truncated tree file;
- a dropped artifact whose features equal another variant of the same package (e.g. the host
  and target builds of a package with no features);
- differences that leave the variant sets unchanged (Cargo does not guarantee `cargo tree`
  matches a build exactly).

Supported builds: `cargo build` of a library or binary (`cargo check`, tests, examples and
benches are rejected), crates.io and workspace packages. Not supported: build-script overrides,
`-Zbuild-std`, and non-Rust components inside `-sys` crates beyond the crate's declared license.

When Cargo's SBOM precursor files (`-Z sbom`) are stable, `bom-tools` will read them as its
build evidence instead of the build log and `cargo tree` outputs
([#8](https://github.com/stepfunc/bom-tools/issues/8)). The approval rules, the license report
and the SBOM stay the same; only the input reader changes.
