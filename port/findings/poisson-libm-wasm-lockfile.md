<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Poisson dependency and standalone libm wasm lockfile

PR #87 head `bed62f0c5b3f8b1d5b90336323d89bf27762a1e6` failed the
glibc-libm job in run 45738462726, job 139637322523. The import-free wasm
build stopped because `--locked` rejected its independent Cargo.lock.
Those wasm comparisons did not run; the report correctly marked them missing.

Poisson introduced the local kicad-boost-random dependency of kicad-math.
The workspace lockfile was updated, but the standalone qualification project
also resolves kicad-math as a development dependency and needs that dependency
in `port/glibc-libm/wasm/Cargo.lock`. Cargo regenerated the latter offline;
only the local Boost package and math dependency edge were added. No
comparison requirement or numerical implementation changed.

The workflow now resolves the standalone graph with `cargo metadata --locked`
before generating qualification records. A disposable stale-lock harness
reproduces the rejection; the regenerated lock passes the same check.
Locked wasm32-unknown-unknown builds pass in debug and release.

The topic also imports accepted core header revision `7a984a76e8`. Shared
workspace and oracle registry conflicts retain both core and Poisson
registrations. The parent verified the lock package union and exact Git blobs
for every imported core file and existing Poisson-owned file outside those
shared registrations.

Fresh final-head hosted CI must run every required comparison, including
import-free wasm. Local build success does not replace that qualification.
