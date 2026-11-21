<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Oracle validation

Tested in WSL Ubuntu 26.04.1, x86_64, GCC 15.2.0, wxWidgets 3.2.9,
Boost 1.90, Python 3.14.4, CMake 4.2 and Ninja. The reference export contains
822 unchanged files from `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

The dedicated trees are `/home/karlvonrichter/kicad-port/oracle-src` and
`/home/karlvonrichter/kicad-port/oracle-build`. No build ran in `~/kicad-live`.
The initial real build acquired `~/kicad-port.build-lock` and used `-j8`.
The build compiled 52 C++ translation units in the initial scaffold (including
all upstream kimath/core/Clipper2 sources and the driver/example) and linked
the executable to `reference/kimath/libkimath.a`, `reference/core/libcore.a`
and `reference/clipper2/libclipper2.a`. No KiCad application target was configured.
`nm -C kicad-oracle` showed `T int rescale<int>(int, int, int)`, proving the
example calls the linked specialization rather than a shim implementation.

The source and flag audit passed on all initial 52 compilations. Its exact
commands show `-fno-fast-math -ffp-contract=off` on every pinned library and
overlay source, with the same options at the executable link.

Initial `ctest --output-on-failure` passed both original tests. The expanded
process test then passed against that real WSL executable:

```text
Oracle protocol: 263 actual kimath calls, 21 refusals, streaming OK
```

This includes signed rounding ties, bounded large inputs, fixed-seed generated
integer inputs checked against independent rational arithmetic, malformed JSON,
duplicate keys, missing fields, floats/bools/oversized integers, zero denominator,
unknown units, EOF without a newline and replies flushed before EOF.

The four Python guard tests passed in WSL. They check unsafe flags even when
later overridden, a whole audit refusing `-Ofast` before invoking a build,
modified/extra reference files, the 1..8 job limit, refusal of the shared live
tree and preservation of another worker's busy build lock. The shell script
also passes `bash -n`; Python files compile and the staged diff has no whitespace
errors.

The final C++ dispatch refactor and separate regression executable were then
built serially after a dry run confirmed **only eight overlay compilations and
two links**, with no reference-library work. This lightweight check left the
extractor's heavy-build lock untouched. It took **21.28 seconds**, peak RSS
**351196 KB**. The complete compile database has **57 translation units**, all
audited for the pin and fixed flags. The original successful reference build's
Ninja log reached `libkimath.a` after **17.83 seconds** at `-j8`; this measures
compilation through archive creation, not apt installation/configuration or a
hosted cold-build wall time.

Final results:

```text
oracle-protocol      Passed
oracle-build-guards  Passed
oracle-dispatch      Passed
100% tests passed, 0 tests failed out of 3
Oracle protocol: 263 actual kimath calls, 21 refusals, streaming OK
```

The dispatch test verifies the exact deferred envelope, ordinary execution
failures, non-finite/invalid-string results and duplicate registration. No
synthetic callback enters the production oracle. All heavy reference work
finished before the extractor acquired the shared lock; no new heavy build was
started while it held that lock.

The final committed-provenance check caught and fixed a Ninja dependency issue:
the audit's generated header is now an explicit byproduct, so metadata changes
recompile the embedding source in the same build. The protocol test compares
the binary's `--provenance` result to the audit sidecar to reject stale embedded
metadata. Warm builds must pass that check too.

The first Clang 21 configure exposed an incomplete system GCC 16 toolchain:
Clang could not find `libstdc++`. The installed GCC 15 toolchain built successfully.
No system package or existing KiCad installation was changed.

Ubuntu 24.04 hosted execution and pilot Rust/oracle parity remain the CI/pilot
workers' responsibility. The documented dependency closure builds locally and
requires no full KiCad build. A 5-minute hosted build budget is an initial estimate,
not a measured hosted result. No pilot units or green statuses are claimed.
Distribution wxWidgets/runtime/libm flags are outside the source build guarantee.
