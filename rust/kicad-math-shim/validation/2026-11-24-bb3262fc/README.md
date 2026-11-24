<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Initial math golden: native generation at bb3262fc

Tested source: `bb3262fc08f7d03303a22846bfcea2b54577c29a`.
Native Actions run: https://github.com/Embeint/embeint-kicad-eda/actions/runs/45380617555
Artifact: `14915400530`, `kimath-object-reference-45380617555`.
Full ZIP SHA-256: `7b559d1277ba523f10415119c7e49b31bfe5e2d6129868e63d119ee7e0d6a27e`.

The pipeline sealed the exact checkout, built the locked Linux amd64 candidate
image and oracle, and compared its installed identity with a no-cache rebuild.
It ran the two-million-point primitive/matrix probes before capturing the actual
objects, link order and include tree. Inside that same image the full object gate
passed: schema 30, eligibility 39, context 41, scope 5, arithmetic 44; no failures
or not-run rows. Emission reproduced the committed generated Rust byte for byte.

The same eligible oracle then ran against native Rust debug and release shims:
7,460/7,460 exact comparisons per profile, zero failures/deferred/unprocessed.
Each profile's refusal suite passed 260 probes / 520 requests. The unchanged
runner performed fresh live propose, bless and check with those same debug
binaries and live provenance. The approval guard required the exact complete
corpus and previously audited output hash, not merely a case count.

| Identity | SHA-256 |
| --- | --- |
| Corpus | `c82021245968c5f9cdbf23b2c1b681257d4ba68836ad1d884b41bb77dc6ac287` |
| Actual golden / C++ output | `5b3e42d65eb6b6126acbbf5d45cfbdcc27d461c25558179a84aa0b1c5dd4525b` |
| Eligible oracle | `51b6e108774fa7b4b990aa8d7b23ff0ca45be3e6aa76f62d287c2141d4f0a585` |
| Debug Rust shim | `4b5a487ba8824b5be1a87481e3f8be3297f51a46ff75fd7ec808e0d9d7402783` |
| Release Rust shim (native runner record) | `305fa325cfa3e1ad558d186176c0c45a1c40a294be1c2235022a3acd6462f231` |
| Installed image identity | `c37568600ebf319225f8e5cb2b152bdfb90f5a8b11d1d08d4a4bb94ba1b85b8b` |

The committed golden and blessing evidence are copied unchanged from the
downloaded artifact. Report JSON is retained losslessly as deterministic gzip;
provenance, compiler commands, image facts and refusal reports retain their
original bytes. Run `sha256sum -c rust/kicad-math-shim/validation/2026-11-24-bb3262fc/SHA256SUMS`
from the repository root.

The parent verified the complete ZIP, all member CRCs, every one of the 981
captured-file hashes, the same-oracle gate/approval/provenance bindings, debug
executable hash, all case identities/order/statuses, the complete golden and
every evidence addition against the empty official-rust baseline. Successful
reports retain per-case compared statuses and whole-stream hashes, rather than
raw Rust replies. The release executable was not retained. No downloaded Linux
binary was executed on the parent's macOS host.

This is a partial-unit baseline for the documented instantiations and classic
stream boundary under GCC13.3/glibc2.39 baseline SSE2 and the measured pre-LWG4084
libstdc++ behavior. It does not establish every template instantiation, locale,
rounding mode, toolchain or a second host. The context review's A1-A5/callee
assumptions remain explicit. Original failed observations remain historical;
this import changes no case, algorithm, comparison rule or existing evidence.
