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

Effect on the port: such sequences are outside the reference's defined domain
and have no reference result. `kicad_hash::Mmh3Hash` records which block bytes
the reference has written and refuses, before changing state, any operation
whose reference counterpart would read an unwritten byte; it never reports a
value for one (`blocks()` gives `None`). The oracle adapter
(`port/oracle/shims/kimath_header_hash.cpp`) calls the unmodified header, tracks
the same source-derived write sets externally, never executes a refused member
and never reads an unwritten byte for a snapshot (shown as `??`). An earlier
revision of this PR zero-filled `blocks` after construction instead; that
baseline is superseded
(`port/findings/kimath-header-hash-mmh3-unwritten-reads.md`), and its 15 affected
requests, including `mmh3-data-v1-after-int-1`, are permanent refusal regressions.

Confirmation state: suspected from source; not demonstrated with a sanitizer
or a native undefined execution. An independent probe on explicitly initialized
block states (zero versus `0x5a`) showed the digest of the reproduction above
depends on those bytes. KiCad's own call sites at the pin use `addData`/`add` on
strings and byte vectors (`common/embedded_files.cpp`, `3d-viewer/3d_cache`),
which always define the bytes they read; `addDataV1` is used for legacy
verification of whole buffers. This is a fork-local record, not permission to
change KiCad's behaviour or to send anything upstream.
