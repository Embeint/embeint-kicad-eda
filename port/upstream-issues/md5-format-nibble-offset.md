<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MD5 Format maps nibbles 10..15 to B..G

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`libs/kimath/src/md5_hash.cpp:97-123` (`MD5_HASH::Format`).

Trigger: any stored digest containing a nibble from 10 through 15. The reference
first adds `'0'`, then adds `'A'-'9'` when the result exceeds `'9'`; nibble 10
therefore becomes `':' + 8`, or `'B'`. The normal hexadecimal letters A..F
become B..G. This applies to compact and spaced/default output.

Reproduction: construct a real `MD5_HASH`, call `Hash` with the public bytes
`97,98,99` (`abc`), call `Finalize`, then `Format(true)` and `Format()`.
The standard digest bytes are `900150983cd24fb0d6963f7d28e17f72`; source analysis
predicts compact `900150983DE24GC0E6963G7E28F17G72` and spaced
`90 01 50 98 3D E2 4G C0 E6 96 3G 7E 28 F1 7G 72 ` (with a final space).

Confirmation state: confirmed by the real pinned own-library oracle at tested
overlay `a295db8bcadb8de7ae084178c2e1705fcf99d762`, case `rfc1321-2`. Its actual
compact/default results exactly equal the strings above; the Rust shim agrees.
The full reply is retained in the per-unit baseline and immutable evidence;
`rust/kicad-md5-shim/VALIDATION.md` records the local run. The portable wrapper
preserves this result exactly. This is a fork-local reference finding, not permission to
change KiCad's behavior, normalize difftest output, or send anything upstream.
