<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# parseVersionString never terminates on a byte it cannot parse

- **Location:** `libs/core/version_compare.cpp:23-44` (`parseVersionString`), KiCad
  `4cf939778df04d2b747bb2c4714fc5014a7565c7`; reached through `compareVersionStrings`.
- **Trigger:** any start position where `std::from_chars<int>` matches nothing and
  the byte is not `.`: a letter, space, `+`, `,`, `_`, a non-ASCII byte, or a `-` not
  followed by a digit. Examples: `"1.a"`, `"1-"`, `"+1"`, `"10.0.1-rc1"`, `"1.2.3 "`.
- **Effect:** `from_chars` returns `ptr == first`, the iterator does not advance and
  the `.` check fails, so the loop appends `0` to the vector forever until allocation
  fails (`std::bad_alloc` or the process is killed). The only caller,
  `KIPLATFORM::DRIVERS` on Windows (`libs/kiplatform/os/windows/drivers.cpp:44`),
  passes a registry string, so a non-numeric driver version would hang it.
- **Reproduction:** the `refuse-version-*` controls in
  `port/oracle/fixtures/core-p001-controls.jsonl`; the Rust port returns
  `VersionError::NoProgress { offset }` at the first non-advancing position.
  `rust/kicad-core-shim/tests/corpus.rs` proves the adapter's domain predicate
  (every `-` followed by a digit, every other byte a digit or `.`) equals the Rust
  progress check on all 5,461 strings of length <= 6 over `0-.a`.
- **Evidence:** source reading of the loop and the `from_chars` contract. The
  non-terminating inputs are refused before the reference is called; they are never
  executed to obtain an alleged native result.
- **Confirmation:** source-confirmed. Not a comparison exemption: every terminating
  input, including odd separators, leading zeros and out-of-range numbers, is
  compared exactly.
