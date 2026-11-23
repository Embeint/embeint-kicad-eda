<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath header hash: HASH_128::ToString depends on the global C++ locale

Unit `libs.kimath.include.application.pure.p001`, PR #51. Reference:
`libs/kimath/include/hash_128.h:44-51` at pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

`ToString` writes both words to a fresh `std::stringstream`, which is imbued with
the global C++ locale at construction. libstdc++'s `num_put::_M_insert_int` inserts
that locale's `numpunct<char>::thousands_sep()` according to `grouping()` for every
base, hexadecimal included. It does so before the `setw( 16 )`/`setfill( '0' )`
padding, which pads on the left and never truncates. The independent review
reproduced, against the pinned header, grouping `"\3"` with `'_'`:
`Value64 = { 0x123456789abcdef0, 0x1234 }` gives
`1_234_567_89A_BCD_EF0000000000001_234` (macOS arm64, Apple Clang 17, libc++).

## Port contract (no portable mutable global)

- `Hash128::to_hex_string()` is the classic-locale result, unchanged.
- `Hash128::to_string_in( &CxxNumericGrouping )` takes the caller's
  `thousands_sep` byte and `grouping` bytes explicitly, as immutable state, and
  returns bytes (the separator need not be ASCII). It ports libstdc++'s
  `_M_use_grouping` (first group positive and not `CHAR_MAX`, `char` signed) and
  `std::__add_grouping` (groups from the right, last group repeats, a non-positive
  or `CHAR_MAX` group ends grouping), then the padding.
- A grouping of any length is accepted, as `numpunct` accepts it. An earlier
  revision of this PR refused more than 64 bytes with an invented `GroupingTooLong`
  error, which the reference does not have; it is removed. Only the shared
  4096-byte transport bound of every hex field remains, on both programs.
- Other facets (for example a custom `ctype<char>` that widens digits) are
  outside the contract and must be classic.

## Cases

The `hash128-locale-*` corpus cases (32) install a real `numpunct<char>` facet as
the global locale around the reference `ToString` call in the oracle adapter, then
restore the previous locale. Four of them have groupings of 65, 100, 216 and 4096
bytes. Every `hash128-*` case also checks the classic `ToString`. `check_guards.py`
covers malformed locale input and the 4097-byte transport refusal.

## Open: a zero group depends on the libstdc++ runtime

On the native WSL qualification host the oracle links the distribution
`libstdc++.so.6.0.35` (`libstdc++6 16-20260322-1ubuntu1`, `__GLIBCXX__` 20260321).
`num_put<char>` is an `extern template`, so this runtime's compiled code formats
the value. It cuts the grouping at its first zero byte: `"\1\2\0"` formats like
`"\1\2"`, and `"\1\0\3"` like `"\1"`. The installed GCC 15 template source
(`/usr/include/c++/15/bits/locale_facets.tcc:82-98,1292-1327`) and the C++ standard
([facet.numpunct.virtuals]: a group `<= 0` is unlimited) end grouping at the zero
instead, and the port follows them. A standalone probe built against the same
runtime shows this (`grouping-probe.cpp` and its output in the PR's validation
directory).

Exactly three corpus cases have a zero after a positive group and digits left to
group: `hash128-locale-one-two-zero`, `hash128-locale-random-4` and
`hash128-locale-random-7`. They fail on this host and are kept unchanged. No
libstdc++ 13 runtime (hosted CI, Ubuntu 24.04) was available here, so whether it
matches the GCC 15 template or this runtime is not verified.

The reference result for such groupings is a property of the C++ runtime build,
not of the KiCad source. The next step is a parent decision: name the qualified
libstdc++ runtime, or treat zero groups after a positive group as outside the
compared domain. Nothing is normalized or dropped meanwhile.

KiCad itself is not known to install a global C++ locale with grouping; this
records the reference's actual dependency rather than assuming the classic
locale.
