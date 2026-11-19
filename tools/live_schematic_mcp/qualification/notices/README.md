# Notices for statically linked libraries

`package_mcp.py` copies into the frozen MCP's `licenses/` folder KiCad's licence, the build
interpreter's `LICENSE.txt` and the licence files in the distribution metadata of each
distribution that ships in the package (`build-info.json` lists them under `dependencies`).
Python's `LICENSE.txt` covers the interpreter and the components it names (bzip2, libffi and,
by reference, Tcl/Tk); it does not cover OpenSSL. Native code that has no notice in that
metadata or in Python's licence has its upstream licence text kept here; all of this folder is
copied to `licenses/third-party/`.

| File | Component | Bundled by | Licence |
| --- | --- | --- | --- |
| `nng-LICENSE.txt` | nng 1.11 (`nanomsg/nng`, tag `v1.11`) | `pynng` 0.9.0 (`_nng`) | MIT |
| `mbedtls-LICENSE.txt` | Mbed TLS 3.6.3 (tag `mbedtls-3.6.3`) | `pynng` 0.9.0 (`_nng`) | Apache-2.0 OR GPL-2.0-or-later |
| `openssl-LICENSE.txt` | OpenSSL 3.x (`libssl`/`libcrypto`; Python 3.12.0 for Windows ships 3.0.11) | the Python runtime (`_ssl`, `_hashlib`) | Apache-2.0 |

All are compatible with KiCad's GPL-3.0-or-later. `openssl-LICENSE.txt` is OpenSSL's
`LICENSE.txt` as vcpkg installs it (`share/openssl/copyright`, OpenSSL 3.6.5); the text is the
Apache License 2.0 and identical in every OpenSSL 3 release checked (3.1.1, 3.6.5).

`manifest.json` records which version each text was written for. `package_mcp.py` refuses to
build when the environment's `pynng` differs from it or the interpreter's OpenSSL is not 3.x, and
`check_package.py` checks the package's `build-info.json` against it and that every listed file
was delivered. When the `pynng` requirement brings other versions, update the texts, this table
and `manifest.json` together (`pynng._nng.lib.nng_version()` and the `Mbed TLS` version string in
the extension module give the bundled versions).

## Known limitations

- `pydantic_core` and `rpds-py` are Rust extension modules that statically link crates from
  crates.io (mostly MIT or Apache-2.0). Their wheels ship only the project's own licence, and no
  notices for those crates are generated here. Doing it reliably needs each wheel's exact
  `Cargo.lock` and a crate licence collector (for example `cargo about`) run against the
  matching source release; a list assembled by hand would not be trustworthy. Until a release
  process does that, the package does not carry notices for those crates.
- The same applies to native libraries that other wheels vendor (`numpy.libs`,
  `Shapely.libs`): the package carries whatever licence files those wheels include in their
  metadata, and nothing more.
