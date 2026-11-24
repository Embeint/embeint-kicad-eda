<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Reference environment (candidate)

**Status: candidate-1, unqualified.** This directory prepares one reproducible
Linux x86-64 environment in which the unchanged pinned KiCad C++ oracle and the
Rust port can be compared bit for bit and byte for byte. It is a recipe, a lock
and a validation entrypoint for independent review. It is **not** the accepted
oracle environment: the pins, goldens, unit statuses, evidence records and the
hosted phase zero gate (`rust-port.yml`) are unchanged. Candidate images were
built natively at `1b4aa6a8` (never pushed). That qualification **failed** (two
probe-tooling defects, since fixed but not re-qualified, and 13 numeric resistor
mismatches that remain open); see [Validation performed](#validation-performed).

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
`c4d1c9c0aca71985f07bd36ff759301daa88a43e4a17335d31e4446bbd6101d2` at this
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
including the oracle and native Rust (which calls the same libm), inherit it.
Setting the variable is not taken as proof; execution is checked. The primitive
probe (`probe/trace_call.h`) first calls each checked function once untraced. It
then calls it once in a `ptrace`d child, single-stepping from the public entry
until the call returns, and records:

- every instruction address;
- the return address on the stack at the entry, and the address reached after the
  return;
- each GOT slot read live at a RIP-relative jump;
- the address its own compiled reference binds to.

The run keeps copies of the locked libm and `libc6-dbg` debug file (build ID
`f772fc0c42a75e38f138f8b66f1ebc8aad060525`) as `libm/libm.so.6` and
`libm/libm.debug`. Each must have the lock's SHA-256, be a regular file and be at
most 16 MiB. Every static fact is re-derived from those bytes: default-version
public definitions, each `.plt.sec` stub with its decoded slot and GOT relocation,
IFUNC resolvers and function ranges. The facts recorded in `environment.json` must
equal the derived ones exactly, as canonical JSON, so changed bytes, slots,
symbols, versions or types are all rejected; recorded facts are never trusted on
their own. `dispatch_proof.py` then requires, per function:

- the compiled reference and `dlsym` agree, inside the locked libm;
- the entry is the start of the **default-version** public definition. It is either
  an IFUNC whose resolved target is the entry (direct: `sin`, `cos`, `sincos`, `tan`,
  `atan` and the float functions), or a FUNC wrapper (`atan2`, `exp`, `log`, `pow`).
  The wrapper must reach its target through a `.plt.sec` stub of the locked libm's
  exact form (`endbr64; jmp *rel32(%rip); nopw`). The stub's slot must have exactly
  one `R_X86_64_IRELATIVE` relocation with symbol 0 and an IFUNC resolver as its
  addend, and the live slot read at the jump must hold the next traced address;
- the target is the function's locked `__<f>_sse2`/`__ieee754_<f>_sse2` and the SSE2
  variant of the resolver that relocation names;
- every step decodes in libm's executable code, and every successor is that
  instruction's fall-through, its direct target, the return address of a traced
  call, or (for the stub's jump only) the slot value. The decoder
  (`probe/x86_decode.py`) is fail-closed: it accepts an explicit subset of legacy,
  0F, 0F38 and 0F3A encodings and refuses VEX/EVEX (AVX, FMA), x87, string, system
  and other indirect transfers. Inside the image it is cross-checked against
  objdump's length and kind for every instruction it accepts in the locked libm;
- the trace ends with the entry's own `ret` at call depth 0, and the address after
  it equals the return address read at the entry, outside libm. Single-address,
  entry-only, truncated, mid-instruction, skipped-instruction and
  forged-completion traces fail;
- every address lies in a known function or stub, and the only multiarch
  implementation executed is the target.

In the locked libm, `__sin_sse2` is at `0x2f8e0`, `__cos_sse2` at `0x301f0` and
`__sincos_sse2` at `0x31e10`. `exp@@GLIBC_2.29` (`0x3a700`) calls stub `0x10400`,
which jumps through slot `0xe7fa0` (IRELATIVE, resolver `0x280d0`
`__ieee754_exp`) to `__ieee754_exp_sse2` at `0x27ea0`.

Failures include:

- an executed `_fma`, `_fma4`, `_avx`, `_avx2`, `_avx512` or `_evex` implementation,
  or any other variant;
- a compat-version entry;
- an unrecognised stub, or a slot whose relocation is a JUMP_SLOT;
- a wrapper that returns without a hop, an incomplete trace, a wrong object, or a
  missing or reordered record.

The trace needs `ptrace` of the probe's own child: Docker's default seccomp profile
permits it, and a refusal fails the check as `PTRACE_TRACEME refused`. The
tracer is bounded in steps, recorded addresses and wall-clock time (20 s per
call, 10 s per wait, 5 s to reap). Each wait polls `waitpid` with a monotonic
deadline while `SIGCHLD` is blocked, without a process-wide alarm. The child dies
with the tracer (`PR_SET_PDEATHSIG` before tracing, `PTRACE_O_EXITKILL` after).
Any child not yet reaped is killed and reaped on success, timeout or error, and
the probe fails if any child could not be reaped. `run_probe.py` bounds every
command at 1800 s.

Scope: one ordinary input per function. A valid trace proves the path that input
took, through the locked code; other wrapper branches and special-input paths are
not certified. The hashes bind the evidence to the locked files; they do not
authenticate a run. `compare_observations.py` compares the traces in
`primitive-contract.json` byte for byte. The return addresses in them belong to
the probe binary built by each run, so they compare equal across hosts only while
the same image builds the same probe. The image
also refuses `glibc-hwcaps` library directories, `/etc/ld.so.preload` and `LD_*`
overrides.

Both the lock generator and the runtime collector name libm's implementations
with one classifier (`probe/libm_elf.py`). An implementation is a defined FUNC
named `__<stem>_<isa>`, with a non-empty stem. IFUNC dispatchers such as the bare
`__fma` (public `fma`'s resolver), bare names, undefined symbols and non-function
symbols are not implementations; they are recorded with their reason, not dropped.

The checked list is a **subset**, not an audited closure of all calls made by
pinned kimath. In particular, pinned geometry can reach `asin`/`acos`; their
variants appear in the lock but are not probed here. Code-derived examples at
pin `4cf93977`: `eda_angle.h:217/219` (Arccos/Arcsin),
`arc_chord_params.cpp:107` (asin), and
`convert_basic_shapes_to_polygon.cpp:275` (acos). Those and other unlisted
paths remain uncertified. Neither these checks nor matching sample bits certify
all-kimath or native/wasm portability. Original NaN and SetRotation failures
remain failures.

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
   identity files from the `.deb` contents and names libm's implementations
   (shared classifier, see [CPU dispatch contract](#cpu-dispatch-contract)).
2. `tools/fetch_inputs.py` downloads every locked object into a build context and
   checks each SHA-256.
3. The hashed lock binds `inputs.sha256`, a complete inventory of payloads and
   controls (including checksum lists, metadata, keyring and installer list). The
   bare-base verifier rejects extras, omissions, duplicates and symlinks and
   checks every byte before sourcing any control file. The Dockerfile installs
   Rust components only from the verified installer list, without Rust archive
   or installer globs. Its `.deb` glob expands the exact verified package inventory;
   extra files are rejected before installation.
   Signature-stubbed fixtures test inventory coverage only, not cryptography.
   `qualify.sh` builds with `docker build --network=none`. Inside the pinned base,
   `image/verify_chain.sh` checks the archive keyring hash, verifies each InRelease
   with the base's own `gpgv`, takes index hashes only from the verified text,
   decompresses the checked index, and requires every staged `.deb` to appear with
   the same filename, SHA-256, package, version and architecture; extra files fail.
   It verifies the Rust manifest signature and that every locked tarball hash is
   named by it.
4. The image has no package index and the build uses `--network=none`:
   `apt-get install` consumes the verified local files without network access.
   The `--no-download` apt option is deliberately absent: apt 2.8.4 fails on
   these local-file acquisition paths when it is set. `image/verify_image.py` then
   checks the package set, `dpkg --verify`, identity files, compiler facts, loader
   state, the dispatch variable and Rust, writing `/opt/refenv/image-identity.json`.
   The Rust identity includes all files and in-prefix file symlinks under
   `/usr/local/bin`, `lib`, `libexec` and `etc`: compiler drivers, linker helpers,
   LLVM/driver libraries, native and wasm `.rlib`/`.rmeta` files and installation
   manifests. Stable paths, modes, link targets and hashes are compared on every
   run and rebuild. Escaping/directory symlinks and missing/extra inputs fail.
   Documentation/completions and the non-input append-only `rustlib/install.log`
   are excluded. Every probe, oracle and difftest run re-checks the image against
   that record.

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
  x87 control word, rounding mode, locale, the traced call of each checked libm
  function (entry, compiled binding, every executed address), `sin`/`cos`/`sincos` and the compiled cos+sin pair for 37
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
  versions, package list, loaded object hashes, image identity, libm's multiarch
  table with excluded names and the static dispatch facts), `libm/libm.so.6` and
  `libm/libm.debug` (the locked files the proof is re-derived from), and `summary.json`
  (contract checks with the per-function dispatch chain, comparisons, output hashes).

The exit status is 1 when the environment contract fails, or 4 when the native
C++/Rust bit comparisons differ. Contract-versus-host-default and paired-versus-
separate-call comparisons are diagnostic; all differences are listed with counts
and examples. Nothing updates expected results or accepts differences.
Summary schema 2 includes the exact lock/candidate files and hashes every
artifact, including built probes. Before comparison the reader rejects duplicate
keys, floating/nonfinite JSON numbers, malformed schemas, missing/extra files,
wrong types/counts/identities and any binary sweep not exactly `24 * count` bytes.
Every sweep input word must also match the fixed-seed sequence in its recorded
position, even when all runs contain the same altered inputs and fresh hashes.
It recomputes comparisons and key environment checks, including the exact types
of nested counts (a boolean is not an integer count); summary flags alone are
insufficient. It re-runs the dispatch proof from the recorded traces and facts.
Historical schema-1 observations are not upgraded or accepted; the native `1b4`
probe output (primitive and environment schema 1) is retained as historical
evidence and is not valid input for the current reader.
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

The independent review of `e15b233` found installer-membership, artifact-validation,
Rust-closure and source-binding blockers despite its 25 passing tests. The
follow-up fixes and new adversarial fixtures are implementation work; historical
results above are not relabelled as evidence for the fixed head. Any cancelled
native qualification attempt is historical/unqualified and must be restarted
at the independently reviewed fixed head.

The re-review of `a3ad6f53` found directory-link containment and fixed-seed input
validation gaps, plus boolean comparison counts passing integer equality. The
follow-up checks reject symlinked source ancestors before reading tracked files,
enumerate unrecorded directory links without following them, reconstruct every
sweep input independently, and validate nested comparison types. Recorded source
links retain their exact identity; their actual target directories are still
enumerated. The 46 tooling tests include real-entrypoint directory-link probes,
altered/reordered inputs on either or both sides with refreshed hashes, and
boolean replacements of every nested comparison count. These are validation
regressions, not native amd64 qualification.

The re-review of `b580e380` caught the normal Linux venv directory alias created
by `bootstrap_west.py`. The guard permits only `.port-venv/linux/lib64 -> lib`,
with a real sibling `lib` directory resolving to that exact location inside the
snapshot. Other unrecorded directory links, escaped or missing targets, and a
symlinked `lib` directory still fail. The 48-test suite includes CLI checks
before and after CPython's actual venv directory-creation method, plus those
negative cases. On macOS the test changes only that method's platform selector;
this does not establish native Linux or full west/bootstrap qualification.

Native WSL preflight at `b684f9b0` passed the 48 tests and real signed-input
verification in an extracted base rootfs, without a Docker build or package
installation. Image, dispatch, oracle and difftest qualification remained blocked
by the missing engine/checkout. It also found that Git's archive conversion
could produce non-blob source bytes under Windows line-ending configuration.
The exporter now reads the raw tree and streams blobs directly, verifies each
Git object ID, and independently verifies every resulting archive member.
The 51-test suite covers CRLF checkouts, tracked `eol=crlf`, `export-subst` and
`export-ignore` attributes, plus local replacement objects. No Git configuration
is changed; these tests do not constitute native Windows or image qualification.

The real Windows producer at `ac1f2a54` verified every raw source entry (bytes,
mode, link and membership), but its subsequent checkout check rejected Git for
Windows' real symlink representation of the known QA alias. It did not reach
the reference archive or produce a bundle digest. The checkout matcher now
accepts that alias's native drive-root/NT-prefix representation as well as an
exact text stub. This applies only to that known link in a Windows checkout;
the archive and Linux snapshot retain `/work/qa/resources` exactly, and the
actual alias must still be unavailable. The 53 tests cover matching host
representations and changed-target/drive/scope rejection. Native Windows
revalidation at `33f5d286` subsequently passed, including the full raw-blob
source/reference export and independent checks of all 24,598 source and 915
reference entries. The digest-bound WSL consumer passed the 53 tests and repeated
source/context checks. No Git settings workaround was used.

The first native amd64 image build at `33f5d286` failed in apt 2.8.4 with
`Internal Error, Pathname to install is not absolute`. A network-disabled
diagnostic using the same pinned base and verified inputs reproduced the failure
with `--no-download`; removing only that flag installed the expected 339-package
set successfully. This was a diagnostic container, not a candidate image.
The Dockerfile now omits that flag while retaining signature/inventory checks,
network-disabled builds and installed-image verification. A fresh image build
and independent review of this follow-up remain pending. Engine version/info is
now retained in the run directory before the first pull/build; complete
image-bound host provenance still requires a successful image.

The next native build at `b9592bea` installed all locked Debian and Rust inputs,
then failed the final verifier because cleanup removed the apt-owned directories
`/var/lib/apt/lists/partial` and `/var/log/apt`. The base also omits the former.
Cleanup now recreates both directories after removing disposable contents; the
package verifier gains no exception. Host provenance records the producer's
platform from the source proof and says "clean at recorded source export",
without implying that a Windows-produced bundle came from a Linux checkout.
Focused tests exercise cleanup in an isolated filesystem and host-record CLI
behavior with Windows/Linux/macOS producer proofs. They do not run dpkg or prove
a native image pass. Independent review at `1b4aa6a8` found no functional blocker;
native qualification remains pending.

At the time of `0cd423cf` (README only), the native `b9592bea` build had run
`verify_image.py`, which failed; no successful image existed yet.

### Native amd64 qualification at `1b4aa6a8` (historical, failed)

Run `20261123T071520Z` used Rancher Desktop Moby 29.5.3 on a Ryzen 9 5950X under
WSL2. It ran `qualify.sh --revision 1b4aa6a8… --source-bundle … --with-oracle
--rebuild-check --sweep-count 2000000`, with the Windows-produced bundle
`source.json` SHA-256 `3c552fe0…97cf`, pin `4cf93977` and lock `c4d1c9c0…01d2`.
These results belong to `1b4aa6a8`. `0cd423cf` changed only this README and is not
relabelled.

- Image `sha256:8b70857a…3597` (tagged locally, never pushed) passed the strict
  final `verify_image.py`: 339 packages, 14 identity files. The no-cache rebuild
  `sha256:7281a5eb…309f` produced a byte-identical identity record. The image IDs
  differ.
- Oracle in the image passed: 76 strict compilations (`-O3 -fno-fast-math
  -ffp-contract=off`), 8/8 CTests, binary `19a3cd84…f5ef`.
- Probe at 2,000,000 angles: the C++ and Rust libm values, sweep and NaN arithmetic
  are identical. Contract checks: **10 passed, 2 failed**. Both failures are
  probe-tooling defects:
  1. The dispatch check took the `dlsym` wrappers of `atan2`/`exp`/`log`/`pow` for
     implementations.
  2. The collector counted the bare IFUNC dispatcher `__fma` as an 80th
     implementation.
- Manual run of the `qualify.sh` difftest payload. The official `--with-difftest`
  preflight did **not run** (no native Linux Git clone).
  - Rtree p003: 1525/1525 compared.
  - MD5: 5973/5973 compared.
  - **Resistors** (`pcb_calculator` p001): **13 failed** of 1050 (1037 compared,
    0 deferred, 0 unprocessed). In every failure Rust equals the golden and the
    native SSE2 oracle differs. Cases: series-0..8, series--1,
    series--2147483648, series-2147483647 and format-boundaries-3. In the series
    cases, decade exponent 126 gives `1e126` as `0x5a17a2ecc414a040` rather than
    `…a03f`. In format-boundaries-3 the outputs differ near `e116`.
  - IEC 60664 was not reached (fail-fast). A supplementary per-unit run compared
    14190/14190 with the golden matched; it is not part of `qualify.sh`.
- Secondary measurements, same image and 1b4 runner, in both debug and release:
  - PR51 `a5ceb989` hash unit: 41 failures (38 NaN, 3 locale) in each build.
  - PR50 `8ef0c0e8` math unit: of 7460 cases, 2273 failures in debug and 2687 in
    release, all mixed-NaN.
  - Both revisions repeat the 13 resistor failures.
  - No case was deferred or left unprocessed.

An independent read-only diagnosis read the live GOT slots and found the
contract process selecting the locked `__ieee754_*_sse2` behind each wrapper, and
host default selecting `_fma`. It found that bare `__fma` is a resolver and that
the generator predicate reproduces the 79-entry lock exactly. Changing only
`GLIBC_TUNABLES` on the same oracle binary turns all 13 resistor replies into
golden matches: runtime `pow(10, 126)` is `…a040` under SSE2 and `…a03f` under FMA.
This is causal evidence, not a decision. The CPU contract, the portable algorithm,
the cases, the goldens and the tolerances are unchanged. The historical golden
(GCC 15.2/glibc 2.43) has no recorded dispatch proof. The resistor, PR51 and PR50
failures remain open, and reference adoption remains unresolved.

### Probe-tooling fixes at `7e36f33f` (historical)

`7e36f33f` (implementation `3bfafe0b`, tree `b00104d3`) replaced the
dlsym-offset check with a traced proof and shared the multiarch classifier
between the lock generator and the collector. The lock was unchanged: the shared
classifier yields exactly its 79 entries. On Linux, 75 tests passed with the 3
runtime tests skipped. In the retained 1b4 image (new tree `1e3510db`), all 75
tests passed and the probe passed 12/12.

An independent review of exact `7e36f33f` then reproduced two P2 defects and a
bounds objection. Its native diagnostic also passed 75/75 tests in the image with
0 skips, and 12/12 under Docker's default security.

1. The proof trusted recorded stub, slot, relocation and version facts. Artifacts
   with every stub's bytes replaced by `cc`, slot `0xdeadbeef`, relocation symbol
   17, or null versions, integer defaults and a boolean schema still passed the
   full comparator.
2. Range membership is not a trace. Direct traces cut to their first address,
   wrappers cut on arrival at the target, and an inserted `entry+1` all passed.
3. The tracer's waits had no wall-clock deadline. A blocking callee hung it
   until an external watchdog killed it.

### Review fixes after `7e36f33f`

The three are fixed as described in
[CPU dispatch contract](#cpu-dispatch-contract):

- facts are re-derived from locked ELF copies kept with the run, and recorded
  facts must equal them;
- a fail-closed decoder validates every control-flow edge, the call stack and the
  return edge, together with live GOT slot reads;
- the tracer is bounded in wall-clock time, with child cleanup.

The lock, CPU contract, algorithms, cases and goldens are unchanged. Tested code
tree `09bd6fce00cd2d61c70c493e69240e918ad3fda6`:

- **WSL Linux**, Python 3.14.4, canonical LF export: **86 tests, 82 passed, 4
  skipped** (the in-image runtime tests). This includes 6 real-process lifecycle
  tests: success, 17 repeated calls, blocking before the entry, blocking in the
  callee, exit, abort, exit before the entry, the step bound, and the tracee dying
  with its killed tracer. Each proves bounded exit, a reaped child and no child
  left. `check_candidate.py` passes.
- **Development diagnostic in the retained 1b4 image** `sha256:8b70857a…3597`,
  not a qualification, under default Docker security (no privileges, capabilities
  or seccomp changes): **86 tests, 86 passed, 0 skipped**. These include objdump
  agreement for every decoder-accepted instruction of the locked libm. They also
  include the full comparator rejecting mutated copies of a real run: bad stubs
  and slots, bad versions and types, a changed libm copy, entry-only traces,
  truncated wrappers, `entry+1`, a forged return and a forged slot.
- `run_probe.py` at 200,000 angles with the matrix probe passed **12/12**, and
  `artifacts.validate` passed. Primitive probe `a921cb8c…bfe`, Rust probe
  `7d8e157d…ded`, matrix probe `21d76a8e…0d7`.
- All 17 contract traces end in their entry's return. The host-default run is
  reported as executing `_fma` code, which the decoder refuses as VEX.
- The reviewer's unmodified artifact counterexample script, run against these
  fresh artifacts, now exits 1 with a specific reason in all four cases.
- The probe and the lifecycle harness compile in the image with `-Wall -Wextra
  -Wpedantic` and no warnings.
- **Windows** Python 3.12.0, canonical export: 86 tests, with 1 failure and 55
  error entries, the same test names and counts as the review's `7e36f33f` Windows
  log. They come from POSIX-shell, Linux-path and Git-fixture tests that predate
  this change. All new tests pass on Windows, and the 10 skips are the runtime and
  lifecycle tests. The full Windows suite is not claimed to pass.

**Not completed:** fresh independent review of these fixes; an image built from
the fixed head, its probe and the official `--with-difftest` route. A passing
probe would not resolve the resistor (13/1050), PR51 (41) or PR50 (2273 debug,
2687 release of 7460) failures, which remain open.

The `fead6cdc` review found two remaining validation/lifecycle gaps. Completion
markers name the recorded probe object; artifact validation binds that object name
to the compiler's recorded `-o` output. The primitive observation is schema 3;
older observations remain historical, not upgraded. Cleanup rejects inherited
SIGCHLD auto-reaping dispositions before forking, and treats ECHILD as loss of
PID ownership, never as permission to signal that PID. The helper no longer
flushes parent stdio before forking: all child exits use `_exit`, so inherited
buffers need not be flushed and a full pipe cannot block that startup step.
The decoder rejects unlisted three-byte encodings and C6/C7 groups, including
XBEGIN/XABORT. Native review at `a867cebf` confirmed those fixes, then identified
coordinated library-alias and ELF-header counterexamples. The caller now cannot
also be a recorded shared-object name or resolved alias. Its retained file must
be a supported x86-64 executable/PIE, with bounded, file-backed executable
`PT_LOAD` mappings covering both its entry point and completion offset. Mapping
checks use program headers, not section flags; clearing a section annotation
does not remove the loader's execute permission. Probe offsets are relative to
the first mapped page, matching [glibc 2.39 `dladdr`](https://github.com/bminor/glibc/blob/glibc-2.39/elf/dl-addr.c)
and its [segment mapper](https://github.com/bminor/glibc/blob/glibc-2.39/elf/dl-map-segments.h).
LOCK and invalid MOVMSK forms are also outside the decoder's supported subset.
The `50cacbba` native review confirmed those fixes but found equivalent-path
aliases and oversized program-header tables. Caller and shared-object identities
now compare guaranteed-equivalent Linux spellings, folding repeated separators
and `.` without consulting the reviewer's filesystem. `..` is rejected because
symlink traversal prevents safe lexical folding. The probe's own recorded path
must also be canonical, as emitted by the fixed compiler command. Program headers
are capped at
65,536 bytes, following [Linux `load_elf_phdrs`](https://github.com/torvalds/linux/blob/v6.18/fs/binfmt_elf.c):
1,170 ELF64 entries fit, 1,171 do not. These follow-ups still need independent
review and native validation.

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
revision=$(git rev-parse HEAD) # must be the independently reviewed fixed head
bash port/reference-env/qualify.sh --revision "$revision" --work ~/refenv-work --rebuild-check \
    --with-difftest ~/refenv-src --sweep-count 2000000
```

Before building, `qualify.sh --revision <full SHA>` requires a clean native Git
checkout at that exact head. It exports canonical Git bytes, independently
compares the reference archive/manifest against the pin's actual Git blobs,
checks the original stage revision/clean flag and preserves that provenance.
Both source and reference archives are made from unfiltered Git blobs, not
`git archive`'s working-tree conversions. `core.autocrlf`, EOL attributes,
export substitution/omission and local object replacements cannot change or
omit committed inputs. A supplied stage containing converted bytes is rejected;
regenerate the source bundle rather than regenerate the input lock.
A supplied `--with-difftest REPO` checkout must also be clean and at that head
before any build; execution uses the verified snapshot rather than that mutable
checkout. Snapshot checks run before/after oracle build, dependency bootstrap,
Cargo build and difftest, and before every image build. Extra source files,
actual-byte drift or mixed stage metadata fail. Tracked sources cannot acquire
untracked bytecode caches; generated dependencies/build output have explicit
separate permitted directories. The policy enumerates every corpus in this
exact source revision, including accepted partial units; it does not limit the
run to three pilot corpora.

For a Windows checkout, prepare the bundle with **Windows Python and Windows
Git**, before entering WSL (replace the placeholders with the intended full SHA
and a new output directory):

```powershell
python port/reference-env/tools/source_binding.py export --repo . --revision <HEAD-SHA> --out C:/Temp/refenv-source
# Record the printed source.json SHA-256. Optional --stage <prepared-stage>
# validates and copies an existing Windows-prepared stage without rewriting it.
```

In WSL, consume that digest-bound bundle without running Git on the Windows
checkout:

```bash
# Python tolerates the Windows checkout's CRLF; Bash needs canonical LF bytes.
python3 /mnt/c/path/to/checkout/port/reference-env/tools/source_binding.py materialize \
    --bundle /mnt/c/Temp/refenv-source --revision <HEAD-SHA> \
    --source-sha256 <EXPORTED-SHA256> --out ~/refenv-bootstrap
bash ~/refenv-bootstrap/source/port/reference-env/qualify.sh \
    --revision <HEAD-SHA> --source-bundle /mnt/c/Temp/refenv-source \
    --source-sha256 <EXPORTED-SHA256> --work ~/refenv-work \
    --with-oracle --rebuild-check
```

For `--with-difftest`, also supply a clean **Linux clone** at the same head.
The bundle digest is an integrity handoff from the trusted native Git export,
not a signature or independent qualification. The exporter records native Git
version/platform; WSL verifies the digest and snapshots without invoking Git in
the Windows checkout. Both modes build the preserved source/stage snapshots. The pre-existing absolute
QA-only alias `qa/tests/resources -> /work/qa/resources` is preserved without
following it and must remain unavailable; external QA resources are not certified
or used by this qualification route. Other absolute/escaping source links fail.

`qualify.sh` takes `~/kicad-port.build-lock`, caps the oracle at `ORACLE_JOBS`
(default 8) and writes everything to `~/refenv-work/runs/<UTC time>/`:
`host-provenance.json`, `image-inspect.json`, `image-identity.json`,
`rebuild-check.txt`, `oracle/` (provenance, binary hash, compile database, source
manifest), `probe/` and `difftest/`. To compare with a second host or the hosted
dispatch job, run `python3 port/reference-env/probe/compare_observations.py
RUN_A/probe RUN_B/probe`. The historical `1b4` run matched the R-tree, MD5 and
supplementary IEC corpora but retained 13 resistor mismatches. The controlled
SSE2/FMA `pow(10,126)` difference is recorded above; it is not a golden-update
or comparison exemption.

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
