<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Reference environment (candidate)

**Status: candidate-1, unqualified.** This directory prepares one reproducible
Linux x86-64 environment in which the unchanged pinned KiCad C++ oracle and the
Rust port can be compared bit for bit and byte for byte. It is a recipe, a lock
and a validation entrypoint for independent review. It is **not** the accepted
oracle environment: the pins, goldens, unit statuses, evidence records and the
hosted phase zero gate (`rust-port.yml`) are unchanged. No image has been built
or pushed from this recipe yet; see [Validation performed](#validation-performed).

## Why

Exact comparison needs one reference binary, toolchain, runtime and CPU dispatch.
Measurements reported in [PR #50](https://github.com/Embeint/embeint-kicad-eda/pull/50) showed:

- 1,824 of 3,500 valid mixed-NaN matrix cases differed between GCC 15.2/glibc 2.43
  (WSL Ubuntu 26.04) and other toolchains: the surviving NaN payload is the
  compiler's operand order;
- glibc's CPU dispatch changed `sin` for 1,397 and `cos` for 1,196 of 2,000,275
  angles by one ulp, and native/wasm rotations differed;
- libstdc++ from GCC 15 implements LWG 4084, so `std::uppercase|std::fixed` prints
  `INF`/`NAN` where earlier runtimes print `inf`/`nan`.

The reported hosted `ubuntu-24.04` and WSL Ubuntu 26.04 (GCC 15) environments
already disagree. Rather than relax NaN payloads, add tolerances,
normalise outputs or accept per-host results, the candidate fixes every one of
these inputs and makes the libm dispatch explicit and checkable.

## Candidate selection

All values are resolved by `tools/lock_inputs.py` from primary sources and
recorded in [inputs.lock.json](inputs.lock.json) (SHA-256
`7b29048e8b46f6fe809f76d6b9f4f5bbcb94e5cb5307b13bdab2e48c02a12105` at this
revision); [candidate.toml](candidate.toml) is the human-readable candidate selection, awaiting independent review.

| Input | Pin | Source |
| --- | --- | --- |
| Base image | `docker.io/library/ubuntu:noble-20261110.1`, index `sha256:882e1e56…1560`, linux/amd64 manifest `sha256:9a04f13b3f80ef056361e30868593b1cfd2279fe19e4081cb93a90bf56819d6a`, config `sha256:2b147973…f0e4`, single layer `sha256:8e8df940…9050` (Ubuntu 24.04.5) | Docker Official Image, built from `git.launchpad.net/cloud-images/+oci/ubuntu-base` revision `3160ed726dafb8da2af14f4a2b3420c9e9886fb6` |
| Packages | snapshot `20261120T000000Z` of `noble`, `noble-updates`, `noble-security` (main, universe); 249 locked `.deb` files on the base's 92 packages: 247 new packages and upgrades of `libc6` and `libsystemd0` (339 installed in total) | `snapshot.ubuntu.com`; InRelease signed by the Ubuntu Archive Automatic Signing Key (2018), `F6ECB3762474EDA9D21B7022871920D1991BC93C` |
| C/C++ compiler | GCC `13.3.0-6ubuntu2~24.04.1` (`gcc`/`g++` 4:13.2.0-7ubuntu1 select it), binutils `2.42-4ubuntu2.10` | Ubuntu |
| C library, libm | glibc `2.39-0ubuntu8.12` (upgraded from the base's 8.9 because `libc6-dev` requires the exact runtime version), `libc6-dbg` of the same build | Ubuntu |
| C++ runtime | `libstdc++6 14.2.0-4ubuntu2~24.04.1`, `libstdc++.so.6.0.33` (GCC 14 family), headers `libstdc++-13-dev` | Ubuntu (the base image already ships this runtime) |
| Build tools | CMake `3.28.3-1build7`, Ninja `1.11.1-2`, Python `3.12.3-0ubuntu2.1`, Git `1:2.43.0-1ubuntu7.4`, rsync `3.2.7-1ubuntu1.5`, wxWidgets `3.2.4+dfsg-4build1`, Boost `1.83.0.1ubuntu2` | Ubuntu |
| Rust | 1.99.1 (`channel-rust-1.99.1.toml` dated 2026-10-15, SHA-256 `fb152be8…f463`): `rustc`, `cargo`, `rust-std` for `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown` | `static.rust-lang.org`; manifest signed by the Rust release key `108F66205EAEB0AAA8DD5E1C85AB96E6FA1BE5FE` |

The LWG 4084 change is in the compiled runtime (`__num_base::_S_format_float` in
`src/c++98/locale_facets.cc`), not the headers. It is present at the
`releases/gcc-15.1.0` tag and absent from `releases/gcc-13.3.0` and from the head
of `releases/gcc-14` (gcc.gnu.org git, checked 2026-11-23). Ubuntu's patch set was
not audited; the probe checks the runtime behaviour itself.

The base image tag moves; the manifest digest is the pin. A newer snapshot or base
image is a new candidate with a new lock, not an update of this one.

### Identity files

`image/verify_image.py` requires these exact bytes in the image (hashes taken from
the locked `.deb` files):

| File | SHA-256 |
| --- | --- |
| `/usr/bin/x86_64-linux-gnu-gcc-13` | `1b99826121ae6682a634e5efe09bd3e3df58ce58e0b28f849114ab5b89139c26` |
| `/usr/bin/x86_64-linux-gnu-g++-13` | `1353e9bdd29a7295c7226bf6c63abccce056d8cac31f112e5cdbecc3f28c2769` |
| `/usr/libexec/gcc/x86_64-linux-gnu/13/cc1` | `5d1679131184e2de4435b426eb264bf13472fe026db8e5c6bc97445814e8e2f4` |
| `/usr/libexec/gcc/x86_64-linux-gnu/13/cc1plus` | `840b332fb62ec6f694ac77d91fe69ef7f80b0d69512ed89374af0ee7a506255d` |
| `/usr/libexec/gcc/x86_64-linux-gnu/13/collect2` | `4d1f341ae5b763b513258ee2812422a45e063c30a2f1924a0cf63d3699f3a158` |
| `/usr/bin/x86_64-linux-gnu-as` | `21aff249b692b5c31a44007491f922dcb49f41323e362c57d2ada3f52eddb7f0` |
| `/usr/bin/x86_64-linux-gnu-ld.bfd` | `e9ceb054c12207970f2726dfc07e9a66b411602748628baf27399f02a9bbb31b` |
| `/usr/lib/x86_64-linux-gnu/libm.so.6` | `5dcc4d25466e29ca390ed6fabb371c69b65dff6fa4e91c34561057a020d6994a` |
| `/usr/lib/x86_64-linux-gnu/libc.so.6` | `755a371e86cc9f564ab54f66140747700fb0a103f698f659f2060f707897e09a` |
| `/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2` | `5fc0940888e9d2e1a00e66d6b1a559c36364ee867683652e1074fa576952cc3f` |
| `/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.33` | `1fd75fe70354a416d75aef22bcae68c47bd25d20e2d0568c30b1a9838cf62f11` |
| `/usr/lib/x86_64-linux-gnu/libgcc_s.so.1` | `d93224d2b0dab4247598be683adca02f5cf00586f99c187579cd7e92058fb7cb` |
| `/usr/lib/gcc/x86_64-linux-gnu/13/libstdc++.a` | `1f5a271fb1917b827ddcc3f65be5bf9bb4ab86be8de6acb6566aa638e92f602c` |
| `/usr/lib/gcc/x86_64-linux-gnu/13/libgcc.a` | `d9376c55f97863a4e5fbb6c6beca2591b319bf786f3a7ca56da8964c5662171f` |

At image build time, other package files are checked as root with `dpkg --verify`
against package metadata (MD5, excluding paths omitted by the base image). Before
each run, the installed package list and compiler/runtime identity file SHA-256s
must equal the lock. Runtime checks do not reread root-only package files.

## CPU dispatch contract

glibc 2.39 selects x86-64 libm implementations at load time (IFUNC). For `sin` and
`cos` the selector (`sysdeps/x86_64/fpu/multiarch/ifunc-avx-fma4.h`) picks `fma`
when FMA and AVX2 are usable, then `fma4`, then `avx`, then `sse2`; `sincos`
(`ifunc-fma4.h`) picks `fma`, `fma4`, then `sse2`. Other functions (`exp`, `log`,
`pow`, `atan`, `tan`, float variants) have similar selectors. The result therefore
depends on the CPU that runs the binary, not on the binary.

The image sets

```text
GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F
```

(names accepted by `sysdeps/x86/cpu-tunables.c` at glibc 2.39), which selects the
baseline SSE2 implementations for the checked libm functions on x86-64 hosts. Programs in the container,
including the oracle and native Rust (which calls the same libm), inherit it. It
is verified, not assumed: the probe resolves each function with `dlsym`, takes its
offset in `libm.so.6`, and names it from the symbol table of the matching
`libc6-dbg` debug file (build ID `f772fc0c42a75e38f138f8b66f1ebc8aad060525`). In the
locked libm, `__sin_sse2` is at `0x2f8e0`, `__cos_sse2` at `0x301f0` and
`__sincos_sse2` at `0x31e10`; the `fma`, `fma4` and `avx` variants are elsewhere.
Each checked function must resolve to its own named SSE2 implementation in the
locked libm; a resolved `_fma`, `_fma4`, `_avx`, `_avx2`, `_avx512` or `_evex`
implementation fails the contract. A missing record, unknown offset, wrong function or wrong object also fails. The
image also refuses `glibc-hwcaps` library directories,
`/etc/ld.so.preload` and `LD_*` overrides.

Code generation is not changed: no `-march` is added to the oracle. The image
checks that GCC's defaults are `-march=x86-64` and that the predefined macros at
the oracle flags include `__SSE2__` and exclude `__AVX__`, `__AVX2__`, `__FMA__`,
`__FMA4__`, `__AVX512F__` and `__FAST_MATH__`, with `__FLT_EVAL_METHOD__ == 0`.
The oracle keeps `-fno-fast-math -ffp-contract=off` and its audit. The probe also
requires the default MXCSR (`0x1f80`: no flush-to-zero or denormals-are-zero) and
round-to-nearest.

GCC 13 at the oracle flags combines a `cos` and `sin` of the same argument into one
`sincos` call (seen in the cross-compiled probe assembly). Code shaped like
`MATRIX3x3::SetRotation` may therefore call `sincos`, whose selector differs from
`sin`/`cos`; the probe records both paths.

## Supply chain and hermeticity

1. `tools/lock_inputs.py` resolves the base by digest, reads its dpkg status,
   fetches the snapshot indices (checked against InRelease) and computes one closed
   package set: highest version per package across the suites, existing base
   packages kept when they satisfy a dependency, exact-version dependencies
   upgrading base packages (cascading), conflicts rejected, and an error for any
   virtual package with several providers and no recorded choice. It hashes the
   identity files from the `.deb` contents and names libm's implementations.
2. `tools/fetch_inputs.py` downloads every locked object into a build context and
   checks each SHA-256.
3. `qualify.sh` builds with `docker build --network=none`. Inside the pinned base,
   `image/verify_chain.sh` checks the archive keyring hash, verifies each InRelease
   with the base's own `gpgv`, takes index hashes only from the verified text,
   decompresses the checked index, and requires every staged `.deb` to appear with
   the same filename, SHA-256, package, version and architecture; extra files fail.
   It verifies the Rust manifest signature and that every locked tarball hash is
   named by it.
4. The image has no package index: `apt-get install --no-download` of the locked
   local files cannot fetch or choose anything else. `image/verify_image.py` then
   checks the package set, `dpkg --verify`, identity files, compiler facts, loader
   state, the dispatch variable and Rust, writing `/opt/refenv/image-identity.json`.
   Every probe, oracle and difftest run re-checks the image against that record.

### Remaining non-hermetic and entropy inputs

- **Image bytes are not reproducible.** Layer tarballs carry timestamps, and dpkg
  maintainer scripts write caches (`ld.so.cache`, alternatives, Python bytecode,
  `ca-certificates` bundles). The image ID will differ between builds. Identity is
  defined by the locked inputs plus the identity record; `--rebuild-check`
  rebuilds without cache and requires an identical record.
- **Random identifiers.** The dependency closure installs `systemd` and `dbus`
  (`libwxgtk3.2` -> GTK 3 -> `dconf-service` -> `default-dbus-session-bus`, whose
  only provider is `dbus-user-session` -> `libpam-systemd`). Their maintainer scripts may
  write a random `/etc/machine-id` and D-Bus machine ID at build time. No probe or
  oracle output reads them; they are outside the identity record.
- **Builder.** Docker Engine/BuildKit version and storage driver are recorded, not
  pinned. The Dockerfile uses no frontend image (`# syntax`), so the engine's
  built-in frontend is used; `RUN --mount` needs BuildKit.
- **Host kernel and CPU.** The kernel, CPU model, microcode and CPUID features vary
  by host and are recorded. The tunables remove the dispatch dependence observed so
  far; other CPU-dependent behaviour would show as differences between hosts in
  `compare_observations.py`.
- **Network services.** Docker Hub, `snapshot.ubuntu.com` and `static.rust-lang.org`
  must remain available (content is pinned by digest; availability is not). The
  optional difftest step also downloads west's PyPI packages (pinned versions in
  `port/tools/requirements-west.txt`) and crates (`Cargo.lock` checksums).
- **Signatures on the lock host.** The lock tool verifies InRelease with `gpgv` only
  if the host has it; the image build always verifies InRelease and the Rust
  manifest. The Rust key itself is trusted on its fingerprint, fetched over HTTPS.
- **Paths and parallelism.** The oracle is built at the fixed container path
  `/work/kicad-port` so the audited provenance and embedded binary are host
  independent; whether the binary hash is reproducible across hosts and job counts
  is not yet measured.
- **Ubuntu patches.** Ubuntu's GCC and glibc patch sets apply; behaviour is
  measured, not derived from upstream sources alone.

## Files

| Path | Purpose |
| --- | --- |
| `candidate.toml` | Reviewed selection: base, snapshot, packages, toolchain and dispatch contract, Rust, identity files |
| `inputs.lock.json` | Generated lock (`tools/lock_inputs.py`; `--check` re-resolves and compares) |
| `Dockerfile` | Pinned base, verified offline install, dispatch variable, identity record |
| `image/verify_chain.sh` | Signature/hash chain check in the bare base image |
| `image/verify_image.py` | Exact image identity check |
| `probe/` | C++ primitive and matrix probes, Rust probe, orchestration, provenance collector, comparison |
| `qualify.sh` | Native x86-64 build, probe, optional oracle build and difftest |
| `tools/` | Lock, fetch, offline consistency check (`check_candidate.py`), host provenance |
| `tests/` | Offline tooling tests |

## The probe

`probe/run_probe.py` runs inside the image and writes, without accepting any
difference:

- `primitive-contract.json`: compiler and runtime facts, loaded objects, MXCSR,
  x87 control word, rounding mode, locale, the resolved libm implementation of each
  checked function, `sin`/`cos`/`sincos` and the compiled cos+sin pair for 37
  representative inputs (zeros, subnormals, boundaries, common and
  reduction-heavy angles, huge values, infinities, quiet and signalling NaNs with
  payloads), primitive mixed-NaN `+ - * /` for 49 operand pairs, and `ostream` and
  `printf` formatting of special values under 11 stream states (including the
  LWG 4084 discriminator);
- `primitive-contract.sweep.bin`: a fixed-seed sweep (default 200,000 angles; at
  most 2,000,000) of `x, sin(x), cos(x)` bits over the principal range, large
  arguments, tenth-degree angles and random magnitudes;
- the same primitive probe with `GLIBC_TUNABLES` removed (`primitive-host-default`),
  showing this host's own dispatch; informational, never the reference;
- `rust-contract.*`: native Rust `f64::sin`, `cos`, `sin_cos`, the same sweep and
  NaN arithmetic, built with the pinned rustc;
- with an oracle build, `matrix-contract.json`: 96 fixed-seed mixed-NaN cases
  through the unchanged pinned `MATRIX3x3<double>` (determinant, inverse, matrix,
  vector and scalar products) and its `operator<<`, compiled with the exact flags of
  an oracle shim from the audited compile database;
- `environment.json` (host, loader diagnostics with and without the contract, tool
  versions, package list, loaded object hashes, image identity) and `summary.json`
  (contract checks, comparisons, output hashes).

The exit status is 1 when the environment contract fails, or 4 when the native
C++/Rust bit comparisons differ. Contract-versus-host-default and paired-versus-
separate-call comparisons are diagnostic; all differences are listed with counts
and examples. Nothing updates expected results or accepts differences.
`compare_observations.py A B` compares two runs, for example two hosts: a missing
or failed contract/comparison summary, any difference in a reference observation,
or a different image identity exits 1.

Limits: the matrix probe characterises this compiler on the pinned header in the
probe's own translation unit. Inlining in a unit's shim can choose a different
operand order, so a unit must still measure through its own oracle shim. The probe
does not run wasm (no wasm runtime is in the image). Choosing a reference
environment does not resolve how portable Rust/wasm will reproduce its NaN
payloads or transcendental bits. Those mismatches remain failures; a portable
implementation strategy and acceptance of this candidate need separate review.
This tooling makes neither decision.

## Validation performed

During initial preparation on macOS arm64, without any amd64 emulation
(prior run evidence recovered from this task; these checks do not qualify amd64):

- `tools/lock_inputs.py` resolved the lock from Docker Hub, the Ubuntu snapshot and
  the Rust dist server (InRelease signatures not checked there: no `gpgv`).
- `tools/fetch_inputs.py` staged all 249 packages and 4 Rust components (381 MB in total).
- `image/verify_chain.sh` ran in the arm64 variant of the same base tag (same
  `gpgv` 2.4.4, keyring and mawk) against the staged amd64 inputs: both signatures
  good, all 249 packages verified. A changed list entry, a changed index file and a
  changed Rust hash each failed.
- `primitive_probe.cpp` compiled with Ubuntu's `x86_64-linux-gnu-g++-13` 13.3.0
  cross compiler at the strict flags with `-Wall -Wextra` and no warnings (the
  assembly had no FMA instructions and the cos+sin pair became one `sincos` call);
  `matrix_probe.cpp` compiled and ran natively on arm64 against the pinned headers
  with a stub `config.h` (syntax and format only); `rust_probe.rs` compiled and ran
  with a local rustc.
- `tools/check_candidate.py` and the then-current 23 offline tests passed; shell syntax checked.

During completion on the official `rust` base `9261e1705d`, the lock was
regenerated from the official network sources with source identities for all
packages. Offline consistency, all 25 tooling tests, Python compilation, shell
syntax and Rust probe formatting passed. All locked inputs were re-staged and
the signature/hash chain rechecked successfully in the native arm64 base. The
Rust probe also compiled with local rustc 1.96.1 on macOS arm64 and exported
1,000 sweep records (not pinned Rust 1.99.1 comparison evidence). A native arm64 base container
with a representative Python/certificates/rsync/D-Bus/GTK/CMake package set
produced no `dpkg --verify` diagnostics outside the documented doc exclusions;
this tests the verification policy, not the amd64 candidate install.

**Not performed:** no amd64 image was built, so `verify_image.py`, the probes on
x86-64, the oracle build inside the image, `--rebuild-check` and the difftest have
never run. A recipe is not a passed reproducible build.

## Native WSL qualification handoff

On the shared WSL Ubuntu host with a native x86-64 Docker engine (not the Rancher
arm64 VM), in a Linux clone of the PR head (Git for a Windows worktree must run
from Windows; a separate Linux clone avoids that):

```bash
git clone --branch t3/rust-reference-environment https://github.com/Embeint/embeint-kicad-eda.git ~/refenv-src
cd ~/refenv-src && git rev-parse HEAD     # record the commit under test
python3 port/reference-env/tools/check_candidate.py
python3 -m unittest discover -s port/reference-env/tests
# Build twice (no cache), probe, build and audit the oracle inside the image,
# then run the hosted-CI-equivalent difftest in the image:
bash port/reference-env/qualify.sh --work ~/refenv-work --rebuild-check \
    --with-difftest ~/refenv-src --sweep-count 2000000
```

`qualify.sh` takes `~/kicad-port.build-lock`, caps the oracle at `ORACLE_JOBS`
(default 8) and writes everything to `~/refenv-work/runs/<UTC time>/`:
`host-provenance.json`, `image-inspect.json`, `image-identity.json`,
`rebuild-check.txt`, `oracle/` (provenance, binary hash, compile database, source
manifest), `probe/` and `difftest/`. To compare with a second host or the hosted
dispatch job, run `python3 port/reference-env/probe/compare_observations.py
RUN_A/probe RUN_B/probe`. Expected and still unknown: whether the existing goldens
match in this environment (the phase zero pilots are integer or text based, so a
difference would itself be a finding to record, not to bless).

Publication is a separate, authorised step: tag the locally built image as
`registry.taileef599.ts.net/embeint-kicad/rust-port-reference:noble-20261110.1-gcc13.3-candidate-1`,
push it, and record the pushed digest; consumers must then pin
`…/rust-port-reference@sha256:<digest>`. No tool here pushes.

## Licences

Primary implementation references: [glibc 2.39 sin/cos selector](https://github.com/bminor/glibc/blob/glibc-2.39/sysdeps/x86_64/fpu/multiarch/ifunc-avx-fma4.h),
[glibc 2.39 sincos selector](https://github.com/bminor/glibc/blob/glibc-2.39/sysdeps/x86_64/fpu/multiarch/ifunc-fma4.h),
[GCC 15.1 runtime formatting](https://github.com/gcc-mirror/gcc/blob/releases/gcc-15.1.0/libstdc%2B%2B-v3/src/c%2B%2B98/locale_facets.cc),
and [Ubuntu snapshot documentation](https://snapshot.ubuntu.com/). The lock
contains the exact official object URLs and hashes for the selected inputs.

The tooling is GPL-3.0-or-later. The image contains unmodified Ubuntu binary
packages under their own licences, notably GCC and binutils (GPL-3.0-or-later; the
GCC runtime libraries with the GCC Runtime Library Exception), glibc
(LGPL-2.1-or-later), wxWidgets (wxWindows Library Licence), Boost (BSL-1.0), CMake
(BSD-3-Clause), Ninja (Apache-2.0), Python (PSF-2.0), Git (GPL-2.0-only) and rsync
(GPL-3.0-or-later), plus the Rust toolchain (MIT OR Apache-2.0). All are compatible
with building and running the GPL-3.0-or-later port and oracle. Each package keeps
`/usr/share/doc/<package>/copyright` (the base image excludes other docs). Any
distribution of the image, including to the internal registry for wider use, must
offer the corresponding sources: the lock records each package's source package and
version, available from the same snapshot (`deb-src`).
