<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Font header review of e36cd8131d: corrections

The independent Sol 6.1 review of PR #104 at head `e36cd8131d` (tree `ce790867`) requested
changes for three findings. It found the ordinary inline bodies faithful and the
`NameAsToken` lifetime argument correct. This note records how each finding was resolved;
the unit's main record is [font-header-p001-port.md](font-header-p001-port.md).

## P1: the font closure broke the locked reference-image build

`port/oracle/CMakeLists.txt` requires FreeType, HarfBuzz, fontconfig and pkg-config on every
oracle build, but the candidate image installs only its locked packages, offline. So
`qualify.sh --with-oracle` (and anything else building the current oracle in the image) could
not configure.

Resolution: the four development packages were added to `candidate.toml`'s requested set and
the lock re-resolved with `tools/lock_inputs.py` from the same base image digest and the same
snapshot (`20261120T000000Z`), as every earlier lock was. The resolution is additive: 39 new
packages (288 locked `.deb` files, 378 installed), and every candidate-1 package, the libm
record, Rust and all 14 identity files unchanged. A different package set is a different
image, so the candidate is renamed `candidate-2`. `lock_inputs.py --check` re-resolves the
lock byte for byte from the network, with the snapshot InRelease signatures verified by
`gpgv`. `check_candidate.py` passes. Locally, at `75fb1ab0bc` (the same candidate and lock bytes),
`qualify.sh --with-oracle` built the image offline from the hash-checked inputs. `verify_image.py` matched (378 packages, 14 identity files),
the probe passed 12/12, the oracle built in the image and its 16 CTests passed. The hosted
`rust-port-reference-env` candidate probe (oracle and difftest) is the reference run.

Two consumers bind the candidate-1 lock by hash:

- `rust-port.yml`'s dispatch-only `migrate_resistor_sse2` job built its image with the tools at
  `33535308` (candidate-1) and then built the *current* oracle in it, which could no longer
  configure. It now builds the image from the dispatched revision's own lock, so the oracle
  builds. The job was a one-time migration, already applied in `cee4373220`.
  `port/tools/migrate_resistor_reference.py` still requires the candidate-1 lock and the
  pre-migration golden (`8d1cc9e8…`), so on any revision after the migration the job fails
  closed before blessing anything. That was already true on `rust` before this PR, because
  the golden is the migrated `db61789…`. The reviewed tool is deliberately left unchanged:
  rebinding a completed migration to another image would need its own review.
- `rust-kimath-nan.yml`'s manual `object-reference` job binds the derived, unreviewed
  experimental object lock (input lock `c4d1c9c0…`, the oracle's `DT_NEEDED` set). With
  candidate-2, and with the font libraries now linked into the oracle, it fails closed on those
  two checks as well as on its documented, never-passed blockers. Only a new review may rebind
  that lock. Its `frozen-reference` job builds the historical `8ef0c0e` oracle in the candidate-1
  image its locks name, and is unaffected.

## P2: unitbuf stream guards refused defined outcomes

The adapter refused all 114 `unitbuf` controls with a failing sync and a nonempty exception
mask. But `~sentry`'s `setstate( badbit )` throws, and so terminates, only when `badbit` is
in the mask. Otherwise the insertion returns or throws normally (the review's
`insert-random-1871` returns after eight syncs with `bad|fail`).

Resolution: the refusal is removed on both sides. Every insertion now runs in a forked child
whose terminate handler reports the state reached, as `core_wx_stl_compat.cpp` does. A SIGABRT
in the child is a harness failure, never a result. Rust reports the accepted stream model's
`InsertError::Terminate` as outcome `terminate`. The 114 former controls are public cases,
unchanged. New cases cover terminating sentries at every insertion position. Of the 278 cases
with a failing sync and a nonempty mask, 91 terminate, 34 return and 153 throw.

## P2: wx assertion guards hid defined conversion paths

The adapter refused 33 defined requests (21 `NameAsToken`, 12 stream) whose name conversion
reaches `wxFAIL_MSG`.

Resolution: the refusal is removed on both sides.

- The oracle wraps the process's own assertion handler for the operation. It records each
  `wxOnAssert` call's line, function, condition and message in the reply, then calls the wrapped
  handler. Without a `wxApp` that handler is wx 3.2.4's default, which logs and returns. An
  assertion from any site other than `strconv.cpp` fails the request.
- The Rust conversions take a typed callee (`wx_conv::OnAssert`) that receives those arguments.
  The production callee defers as `UNPORTED(include.font.application.ui.p001) wxOnAssert`. The
  shim's `--assert-host=returning` records the call and returns, so the continuation is
  compared too: an empty token, or a null C string that sets `badbit`.
- Stream cases evaluate in order: an earlier insertion that throws or terminates never asserts.

`rust-font-live` compares the corpus twice. In production 117 cases defer: 12 Draw and 105
`wxOnAssert` (21 `NameAsToken`, 84 stream). With the returning host only the 12 Draw cases defer,
and the 105 compare exactly, assertion arguments included. `check_deferrals.py` enforces that
accounting. The condition argument is the stringified `"Assert failure"`, with its quotes,
because `wxFAIL_MSG` passes `#cond`.

The remaining 113 controls are undefined evaluations (2 null `FONT*`, 110 `faceSize` /
`subscriptSize` overflow or out-of-range conversion) and 1 malformed request.

The stale `name_as_token` schema row in `rust/kicad-font-shim/README.md` was corrected as well.
