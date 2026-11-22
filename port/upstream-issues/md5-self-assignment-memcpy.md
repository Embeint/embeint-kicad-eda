<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MD5 assignment lacks an overlap guard

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`libs/kimath/src/md5_hash.cpp:48-54`, `MD5_HASH::operator=`.

Trigger: assigning an `MD5_HASH` object to itself. The body unconditionally calls
`memcpy(m_hash, aOther.m_hash, 16)`. In that case both pointers identify the same
sixteen-byte array. `memcpy` requires nonoverlapping source and destination;
compiler optimizations or an apparently successful run cannot establish a
defined reference result for that call.

Confirmation state: a source-level precondition concern, not an observed runtime
failure. No undefined-behavior execution is used as parity evidence. The shim
rejects this input before invoking assignment; all distinct-object copy and
assignment behavior remains covered, including partial/finalized contexts and
independent validity mutation. The Rust implementation copies every field safely.

This fork-local finding does not change the reference implementation or the
port's unit scope and is not an upstream submission.
