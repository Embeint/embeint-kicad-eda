<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath header hash: HASH_128::ToString depends on the global C++ locale

Unit `libs.kimath.include.application.pure.p001`, PR #51. Reference:
`libs/kimath/include/hash_128.h:44-51` at pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

`ToString` writes both words to a fresh `std::stringstream`, which is imbued with
the global C++ locale at construction. On the qualified platform (libstdc++),
`num_put::_M_insert_int` inserts that locale's `numpunct<char>::thousands_sep()`
according to `grouping()` for every base, hexadecimal included, before the
`setw( 16 )`/`setfill( '0' )` padding, which pads on the left and never
truncates. The independent review reproduced, against the pinned header, grouping
`"\3"` with `'_'`: `Value64 = { 0x123456789abcdef0, 0x1234 }` gives
`1_234_567_89A_BCD_EF0000000000001_234` (macOS arm64, Apple Clang 17, libc++).

Port contract (no portable mutable global):

- `Hash128::to_hex_string()` is the classic-locale result, unchanged.
- `Hash128::to_string_in( &CxxNumericGrouping )` takes the caller's
  `thousands_sep` byte and `grouping` bytes explicitly and returns bytes (the
  separator need not be ASCII). It ports libstdc++'s `_M_use_grouping` (first
  group positive and not `CHAR_MAX`, `char` signed) and `std::__add_grouping`
  (groups from the right, last group repeats, a non-positive or `CHAR_MAX` group
  ends grouping), then the padding.
- A grouping longer than 64 bytes is refused with the typed
  `GroupingTooLong`; it is never truncated or clamped.
- Other facets (for example a custom `ctype<char>` that widens digits) are
  outside the contract and must be classic.

Tests: `hash128-locale-*` corpus cases (28) install a real `numpunct<char>`
facet as the global locale around the reference `ToString` call in the oracle
adapter and restore the previous locale afterwards; `check_guards.py` covers
malformed locale input and the 65-byte refusal. Development smoke only (macOS,
libc++): the adapter and the Rust port agree on all 28 cases. Linux libstdc++
qualification is pending with the rest of the corrected adapter.

KiCad itself is not known to install a global C++ locale with grouping; this
records the reference's actual dependency rather than assuming the classic
locale.
