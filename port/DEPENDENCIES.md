# Foundation dependency licences

This port is GPL-3.0-or-later. The dependencies below use GPL-compatible licences.
This record covers the foundation addition; existing MCP dependencies remain
recorded in [rust/MCP.md](../rust/MCP.md#dependencies).

## MD5 pilot

`rust/kicad-md5` uses RustCrypto `md-5` **0.11.0**, with default features disabled,
from crates.io (`https://github.com/RustCrypto/hashes`). Its public
`md5::block_api::compress` implements the MD5 rounds; the KiCad port preserves
the reference wrapper's buffer, length, digest and validity transitions. Using
the ordinary consuming hash API would lose the reference's repeated-finalize
and post-finalize-update behaviour. No cryptographic algorithm is vendored or
reimplemented by this port.

The resolved portable dependency closure in `rust/Cargo.lock` is:

| Package | Version | Licence |
| --- | --- | --- |
| md-5 | 0.11.0 | MIT OR Apache-2.0 |
| digest | 0.11.3 | MIT OR Apache-2.0 |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |

All are GPL-compatible. Licence identifiers and bundled MIT/Apache texts were
checked in the resolved registry packages. `cfg-if` and `typenum` were already
locked by the workspace; the other five packages are new. There is no native
build script or assembly dependency in the selected closure. The separate native
`kicad-md5-shim` reuses `kicad-difftest` for strict duplicate-key parsing and
the workspace's existing `serde_json`; their dependency licences remain recorded
in the harness README and `rust/MCP.md`. Distribution must retain the applicable
dependency licence texts and notices, as for existing packaged Rust binaries.

## solidcore

Source: `https://github.com/tangency-labs/solidcore.git`, commit
`7a588fa284765a83823293ae20b84da8356ab479`, Apache-2.0 (`LICENSE` and
`workspace.package.license` at that revision). Its exact `solidcraft:` trailer is
`93d5492ef295a1124664d36e967f58a81a76d7d2`. The canonical pin and source provenance
are in [pins.toml](pins.toml); no external sources are vendored here.

Only `crates/solidcore` version 1.0.1 is registered as a Cargo workspace dependency,
with default features disabled. Its package manifest inherits edition 2021,
Rust 1.85 and Apache-2.0 from its own root workspace. It declares no `links` key,
C/C++ build dependency, FFI library, OpenCascade, GPU or desktop dependency. Its
automatically discovered `build.rs` is a Rust-only host generator: it sorts
`src/kernel/fixes/*.md` and writes Rust field definitions into `OUT_DIR`, without
compiling or linking native libraries. Normal dependencies are `serde` (derive)
and `thiserror`; its
`serde_json` development dependency is not a consumer requirement. Other crates in
that external workspace, including diagnostics, STEP and goldens, are not selected
as dependencies of our workspace.

The kernel's normal/build closure at its committed `Cargo.lock` is:

| Package | Version | Licence |
| --- | --- | --- |
| solidcore | 1.0.1 | Apache-2.0 |
| serde, serde_core, serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| thiserror, thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |

Proc macros execute at build time; the portable kernel needs no unavailable
native libraries. The workspace dependency alone does not link it into an
existing binary. A future consuming crate will resolve it under `rust/Cargo.lock`,
which can select different semver-compatible transitive versions; update the
licence inventory and packaging notices when that happens. Verify the current
external closure with:

```text
cargo tree --locked --manifest-path .port-deps/solidcore/Cargo.toml -p solidcore --edges normal,build --format "{p} {l}"
```

When distributing a binary containing solidcore, retain its Apache licence,
copyright and applicable notices alongside the GPL distribution, together with
the dependency licence texts. Existing MCP packaging already has a licence
collection mechanism; registering this unused workspace dependency does not add
solidcore to today's package.

## west development environment

west is a Python development tool, never shipped as a port runtime dependency.
Bootstrap uses an isolated venv, with the complete runtime closure pinned in
[requirements-west.txt](tools/requirements-west.txt).

| Package | Version | Licence |
| --- | --- | --- |
| west | 1.5.0 | Apache-2.0 |
| colorama | 0.4.6 | BSD-3-Clause |
| packaging | 26.0 | Apache-2.0 OR BSD-2-Clause |
| pykwalify | 1.8.0 | MIT |
| PyYAML | 6.0.3 | MIT |
| docopt | 0.6.2 | MIT |
| python-dateutil | 2.9.0.post0 | Apache-2.0 OR BSD-3-Clause |
| ruamel.yaml | 0.19.1 | MIT |
| six | 1.17.0 | MIT |

Licences are taken from the installed distribution metadata and bundled licence
texts. Python (PSF-2.0), pip (MIT) and the platform's venv build tooling are host
prerequisites, not shipped application code. Package versions and source revisions
are reproducible; Python, Git, Rust, pip build backends and platform wheel bytes
are supplied by the host. This is not a hermetic compiler/toolchain image.
