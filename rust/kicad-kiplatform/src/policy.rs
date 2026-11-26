// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::POLICY` as built for Unix (`libs/kiplatform/os/unix/policy.cpp`).
//!
//! Unix KiCad has no administrative policy store: both functions return their
//! "not configured" answer for every key without reading it. The Windows (registry)
//! and macOS implementations are separate units and are not ported here.

use kicad_map_helpers::WxString;

/// `KIPLATFORM::POLICY::PBOOL` from `libs/kiplatform/include/kiplatform/policy.h`.
///
/// Minimum co-port: the enum is owned by the header unit
/// [`crate::HEADER_UNIT`]; only its three enumerators, in declaration order, are
/// reproduced here. The header's `GetPolicyEnum<T>` template stays with that unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PBool {
    /// `PBOOL::ENABLED`
    Enabled,
    /// `PBOOL::DISABLED`
    Disabled,
    /// `PBOOL::NOT_CONFIGURED`
    NotConfigured,
}

/// `PBOOL KIPLATFORM::POLICY::GetPolicyBool( const wxString& aKey )` (Unix).
///
/// The key is the accepted `wxString` replacement ([`WxString`], 32-bit signed
/// `wchar_t` units of the pinned wxGTK build). It is accepted and ignored, as in the
/// reference, so every key, including empty, NUL-bearing, surrogate and negative
/// units, gives the same answer.
pub fn get_policy_bool(_key: &WxString) -> PBool {
    PBool::NotConfigured
}

/// `std::optional<std::uint32_t> KIPLATFORM::POLICY::GetPolicyEnumUInt( const wxString& aKey )`
/// (Unix).
///
/// The key is accepted and ignored, as in the reference.
pub fn get_policy_enum_uint(_key: &WxString) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use kicad_map_helpers::{WChar, WxError};

    #[test]
    fn unix_bodies_are_constant_for_any_key() -> Result<(), WxError> {
        let keys: [&[WChar]; 5] = [
            &[],
            &[0],
            &[0xD800, 0x11_0000],
            &[-1; 8],
            &[WChar::MIN, WChar::MAX],
        ];
        for units in keys {
            let key = WxString::from_units(units)?;
            assert_eq!(key.units(), units);
            assert_eq!(get_policy_bool(&key), PBool::NotConfigured);
            assert_eq!(get_policy_enum_uint(&key), None);
        }
        Ok(())
    }
}
