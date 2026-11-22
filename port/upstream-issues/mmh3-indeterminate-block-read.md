<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MMH3_HASH can read block bytes that were never written

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`libs/kimath/include/mmh3_hash.h:63-155,193-225` (`MMH3_HASH` constructors,
`reset`, `addDataV1`, `add( int32_t )`, `hashTail`).

Trigger: the constructors call `reset`, which sets `h1`, `h2` and `len` but never
initializes the `blocks` union; nothing else does before the first write. Most
sequences write every byte that `hashBlock`/`hashTail` later read (`addData`
zero-fills a tail block). Two writers break that invariant: `add( int32_t )`
writes only the word slot `( len % 16 ) / 4`, and `addDataV1` writes from byte 0
regardless of a pending partial block while counting padding in `len`.

Reproduction (source analysis): `MMH3_HASH h; h.add( int32_t( 1 ) );
h.addDataV1( p, 1 ); h.digest();`. After `add`, `len` is 4 and bytes 0..3 are
written. `addDataV1` copies one byte to offset 0, zeroes three padding bytes at
1..3 and sets `len` to 8. `hashTail` then reads bytes 0..7 (`len & 15 == 8`);
bytes 4..7 hold indeterminate values, which is undefined behaviour.

Effect on the port: the comparison needs a defined reference. The oracle adapter
(`port/oracle/shims/kimath_header_hash.cpp`) zero-fills `blocks` through the
explicit-instantiation private member access immediately after every
construction, and `kicad_hash::Mmh3Hash` starts with zero block bytes. Every
other step calls the unmodified header. Corpus cases such as
`mmh3-data-v1-after-int-1` therefore compare the defined zero-initialized state;
they do not claim what an unprimed KiCad object would produce.

Confirmation state: suspected from source; not demonstrated with a sanitizer
or a differing run. KiCad's own call sites at the pin use `addData`/`add` on
strings and byte vectors (`common/embedded_files.cpp`, `3d-viewer/3d_cache`),
which always define the bytes they read; `addDataV1` is used for legacy
verification of whole buffers. This is a fork-local record, not permission to
change KiCad's behaviour or to send anything upstream.
