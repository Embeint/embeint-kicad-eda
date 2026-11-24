<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation evidence

Implementation base: `eb4cd4724ff6f5fa2119c86248c1a4beb4847451` on official
`origin/rust`; native source pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

Tested implementation revision:
`012e8c9ac61bfd7ab5a61b2301c395494324ab87`. The final evidence commit adds only
logs and documentation on top of that implementation. Official `origin/rust`
was fetched again after these reruns and still equals the accepted base.

Local validation on Darwin 25.5.0 arm64, rustc/cargo 1.96.1, capped at `-j2`:
20 tests across kicad-math and the shim passed in both debug and release.
`cargo fmt --all --check`, scoped clippy `--all-targets -- -D warnings`, locked
wasm32 `cargo check -p kicad-math`, `validate_units.py`, deterministic generator
check and `git diff --check` passed. Debug and release binaries each passed 411
refusals, 7 explicit host deferrals and 418 successful continuation calls. All
10,656 arithmetic reply lines were byte-identical between Rust debug/release:
SHA-256 `db9fe1117b870099b21d457e108643017a91a819cf5eec665fcd3bf637b688de`.
Commands, exits, binary/corpus hashes and results are committed under
[validation/2026-11-24-012e8c9ac6](validation/2026-11-24-012e8c9ac6/summary.json).
The C++ adapter additionally passed a Clang C++20 syntax-only check with
`-Wall -Wextra -Werror`, using a temporary empty config.h. This checks the adapter
against the pinned-equivalent util.h, not a real wx build or any native results.
No local C++ result is claimed: this workstation has no wx-config; the audited
oracle intentionally builds on Linux. No Windows/WSL process or full KiCad build
was started.

Hosted Linux live arithmetic comparison and native controls are pending on this
PR's head. Registration uses the existing `rust-port.yml`/partial-unit path.
Expected fixture sizes (not claimed runtime passes): 10,656 real arithmetic
results, 411 refusal controls, 7 explicit logging transport deferrals and 418
successful post-control continuations. The script also supports debug/release
comparison when those binaries are built; its availability is not execution evidence.

The unit stays partial even if every arithmetic comparison succeeds. wx logging,
MSVC/fallback profiles and downstream vector/header wiring remain unqualified.
There is no golden. See AUDIT.md for the per-symbol evidence/gap table.
