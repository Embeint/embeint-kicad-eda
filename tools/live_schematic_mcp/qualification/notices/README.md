# Notices for statically linked libraries

`package_mcp.py` copies each distribution's licence files from the build environment into the
frozen MCP's `licenses/` folder, and Python's own `LICENSE.txt` (which also covers OpenSSL, libffi
and the Microsoft Visual C++ runtime the interpreter ships with). Some native code is linked into
an extension module without a notice in that distribution's metadata; its upstream licence texts
are kept here and copied to `licenses/third-party/`.

| File | Component | Bundled by | Licence |
| --- | --- | --- | --- |
| `nng-LICENSE.txt` | nng 1.11 (`nanomsg/nng`, tag `v1.11`) | `pynng` 0.9.0 (`_nng`) | MIT |
| `mbedtls-LICENSE.txt` | Mbed TLS 3.6.3 (tag `mbedtls-3.6.3`) | `pynng` 0.9.0 (`_nng`) | Apache-2.0 OR GPL-2.0-or-later |

Both are compatible with KiCad's GPL-3.0-or-later. Update this table and the texts when the
`pynng` requirement changes the bundled versions (`pynng._nng.lib.nng_version()` and the
`Mbed TLS` version string in the extension module).
