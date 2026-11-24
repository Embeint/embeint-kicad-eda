// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kimath/include/md5_hash.h`: the inline accessors this unit owns.
//!
//! `bool IsValid() const { return m_valid; }` and
//! `void SetValid( bool aValid ) { m_valid = aValid; }` are the existing
//! [`Md5Hash::is_valid`] and [`Md5Hash::set_valid`] in `kicad-md5` (unit
//! `libs.kimath.src.md5_hash.application.pure.p001`, green), which already keeps
//! validity as an independent field. They are reused, not duplicated, so the
//! validity flag the out-of-line `Init`, `Finalize`, copy and assignment
//! operations set is the same one these accessors read and write.

pub use kicad_md5::Md5Hash;
