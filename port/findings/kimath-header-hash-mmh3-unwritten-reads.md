<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath header hash: 15 MMH3 cases reclassified as unwritten-read refusals

Unit `libs.kimath.include.application.pure.p001`, PR #51. Supersedes the parity claim
of the baseline blessed at `8c54103a7d9a888419e663abb54082c3370fee0a`
(`port/goldens/evidence/libs.kimath.include.application.pure.p001-baseline.json`,
see [the superseding record](kimath-header-hash-baseline-8c54103-superseded.md)).

## Source trigger

At pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`, `MMH3_HASH()` and
`MMH3_HASH( uint32_t )` call `reset`, which sets only `h1`, `h2` and `len`
(`libs/kimath/include/mmh3_hash.h:63-72`); `blocks` is never initialized.
`addData` writes whole blocks and a zero-padded tail (`:74-109`), but
`add( int32_t )` writes only word slot `( len % 16 ) / 4` (`:148-155`) and
`addDataV1` writes from byte 0 regardless of a pending block while counting its
padding in `len` (`:116-136`). Readers: `hashBlock` reads all 16 bytes through
`getblock64` (`:171-191`), `hashTail` reads bytes `0 .. len & 15` (`:193-224`),
`getblock64( i )` reads bytes `8 i .. 8 i + 7`. A read of a never-written byte is
an indeterminate-value read: undefined behaviour, not a reference result.

The 8c54103 oracle adapter zero-filled `blocks` after every construction
(`port/oracle/shims/kimath_header_hash.cpp:523-525` at that head), so these cases
recorded the zero-fill policy as if it were reference output.

## Entries

Each request below is kept byte for byte, unchanged, in
`rust/kicad-hash-shim/fixtures/mmh3-unwritten-reads.requests.jsonl`, with its
expected refusal in `…replies.jsonl`. `check_guards.py` requires both the oracle
and the Rust shim to refuse it before the read. It is no longer a corpus case
(an error reply can never be a golden). "Step" is the zero-based index of the
first step whose reference read is undefined; the old value at that step came
from the invented zero bytes.

| Case | Refused step | Reference reader | Old blessed value at that step | New classification |
| --- | --- | --- | --- | --- |
| `mmh3-data-v1-after-int-1` | 3 (`digest`) | `digest` -> `hashTail` (`mmh3_hash.h:157-160,193-224`) | `3C33A1FF79CA309701A7EB0AEEC0F740` (golden line sha256 `83351ae82f737d09…`) | refusal `hashTail reads unwritten block bytes 4,5,6,7` |
| `mmh3-data-v1-after-int-4` | 3 (`digest`) | `digest` -> `hashTail` (`mmh3_hash.h:157-160,193-224`) | `306809E4916B88F226E6ECB5D71920AF` (golden line sha256 `3195049cf9a66ddd…`) | refusal `hashTail reads unwritten block bytes 8,9,10,11` |
| `mmh3-random-031` | 1 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `8b8f315cd3801c92…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-036` | 2 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 4108718235` (golden line sha256 `b63d2c45578cf94c…`) | refusal `getblock64 reads unwritten block bytes 4,5,6,7` |
| `mmh3-random-039` | 1 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `638696d2b1c059c7…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-053` | 1 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `8a269cc3e8fa874e…`) | refusal `getblock64 reads unwritten block bytes 0,1,2,3,4,5,6,7` |
| `mmh3-random-063` | 1 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `18941b58f30ab55f…`) | refusal `getblock64 reads unwritten block bytes 0,1,2,3,4,5,6,7` |
| `mmh3-random-068` | 2 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `0c76b1bd6853ec1d…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-114` | 1 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `d5c2aaba394ac208…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-115` | 2 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `ab4fdea47ca28529…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-125` | 3 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `67d3f8decaefdca0…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-132` | 2 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `3c80bfc870180aae…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-137` | 3 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `f16034603d83f690…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |
| `mmh3-random-139` | 3 (`digest`) | `digest` -> `hashTail` (`mmh3_hash.h:157-160,193-224`) | `7DC4A061500FC211C37844E793229BC7` (golden line sha256 `2b17f06963de8f17…`) | refusal `hashTail reads unwritten block bytes 4,5,6,7` |
| `mmh3-random-143` | 1 (`getblock64`) | `getblock64` (`mmh3_hash.h:171-174`) | `block 0` (golden line sha256 `00c52e35bf43b007…`) | refusal `getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15` |

## Validation

- The refused set and each first read (operation and byte list) equal the
  independent Sol 6.1 review's source-derived analysis of head 8c54103a7d
  (`defined-domain-analysis.json`, "no undefined executions").
- The Rust port (`kicad_hash::Mmh3Hash`) now tracks written bytes and refuses
  these reads before changing state. The corrected oracle adapter tracks the
  same write sets from the source, never zero-fills, never executes the refused
  member and never reads an unwritten byte for a snapshot.
- Development smoke only (macOS arm64, Apple Clang, libc++; not Linux
  qualification): an adapter build from the pinned `mmh3_hash.h`,
  `md5_hash.cpp` and `transform.cpp` returns exactly the committed 15 refusals.
- Linux x86-64 oracle qualification of the corrected adapter is pending (see
  the PR); no corrected golden has been blessed.

## The other original MMH3 cases

The remaining 321 original MMH3 cases stay in the corpus with byte-identical
requests. Their replies change only in the `blocks` snapshot field: bytes the
reference never wrote are now reported as `??` instead of the invented `00`.
Every `h1`, `h2`, `len`, digest, `out` and `block` value is unchanged (checked by
replaying all 1315 original requests through the corrected Rust shim against the
old golden). Those golden changes need a new reviewed blessing from a live Linux
oracle, with this finding as the reason; until then the committed golden is
stale and CI's golden check fails for them, as it must.
