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
  returns bytes (the separator need not be ASCII). It applies the runtime's view
  of the grouping (the caller's bytes up to the first NUL byte, see below), then
  ports libstdc++'s `_M_use_grouping` (first group positive and not `CHAR_MAX`,
  `char` signed) and `std::__add_grouping` (groups from the right, last group
  repeats, a negative or `CHAR_MAX` group ends grouping), then the padding.
  `CxxNumericGrouping::grouping()` still returns the caller's bytes unchanged.
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

## A zero byte: the runtime applies the grouping up to the first NUL

History: on the native WSL host (`libstdc++.so.6.0.35`, `libstdc++6 16-20260322-1ubuntu1`)
the oracle cut the grouping at its first zero byte: `"\1\2\0"` formatted like `"\1\2"`,
and `"\1\0\3"` like `"\1"`. The port then followed the GCC template source and the
standard ([facet.numpunct.virtuals]: a group `<= 0` is unlimited), so exactly three corpus
cases failed: `hash128-locale-one-two-zero`, `hash128-locale-random-4` and
`hash128-locale-random-7`. The GCC 13.3 candidate image (`libstdc++.so.6.0.33`,
`libstdc++6 14.2.0-4ubuntu2~24.04.1`) failed the same three, with identical oracle output.

Cause, from the runtime binaries and the official GCC 14.2.0 runtime source:

1. The adapter's facet derives from `std::__cxx11::numpunct<char>` (the default C++11 string
   ABI). `std::locale( classic, facet )` runs `locale::_Impl::_M_install_facet`, which, for a
   twinned facet, also installs `_M_cow_shim` for the COW-ABI `std::numpunct<char>::id`
   (`src/c++98/locale.cc`, `_S_twinned_facets`; in both runtimes the twinned table pairs the
   two ids, adjacent `R_X86_64_64` relocations).
2. The shim (`src/c++11/cxx11-shim_facets.cc`, `numpunct_shim`, `__numpunct_fill_cache`)
   copies the caller's full `grouping()` into a new NUL-terminated `char*` (`__copy`) and does
   not override `do_grouping()`.
3. The reference's `num_put<char>` is the runtime's `extern template` instantiation;
   `_M_insert_int<unsigned long>` reaches `__use_cache<__numpunct_cache<char>>`, which loads
   the GOT slot bound to the COW `_ZNSt8numpunctIcE2idE` in both runtimes, and
   `__numpunct_cache<char>::_M_cache` uses the same id.
4. The COW `numpunct<char>::do_grouping()` returns `_M_data->_M_grouping`, a `char*`
   (`bits/locale_facets.h:1856-1857` in the image's `libstdc++-13-dev`), so the string is
   rebuilt with `strlen` (`call strlen@plt` in both runtimes, in `do_grouping` and in the
   devirtualised path of `_M_cache`). `_M_use_grouping` is then
   `size && (unsigned char)( g[0] - 1 ) <= 125`.

So the runtime applies the caller's grouping bytes up to the first NUL byte, and all of them
when there is none. This is the runtime's actual interpretation, not a correction of it. The
reviewer's probes agree: the runtime-only probe recovers the oracle result, while a probe that
instantiates the cache template locally (and keeps it local) retains all three bytes; a
diagnostic that exports its own instantiation can interpose the runtime, which
`check_reference_bindings.py` refuses for the adapter and oracle.

The port: `CxxNumericGrouping::runtime_grouping()` returns that prefix, and `to_string_in`
formats with it. The caller's bytes stay unchanged and any length is accepted. With it the
three corpus cases, the reviewer's 15 adversarial zero-group mismatches and minimal repro match
the retained genuine replies, and 192 supplementary locale requests (zero bytes at every
position, after negative and `CHAR_MAX` groups, long groupings, separators 0/'.'/255) match a
Python reimplementation; those have not yet run against a live oracle.

Binding: `rust/kicad-hash-shim/reference-bindings.json` lists the measured runtimes by sha256
(`1fd75fe7...` candidate, `5bb0d213...` WSL host), and `check_reference_bindings.py` refuses an
oracle linked to any other libstdc++; no option accepts an unmeasured runtime. For such a runtime
it also reports, as a diagnostic only, whether its grouping path passes the structural checks
above (`--diagnose-runtime` adds the same report for a measured one).
