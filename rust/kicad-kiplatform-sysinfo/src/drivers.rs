// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::DRIVERS` (libs/kiplatform/os/unix/drivers.cpp).

/// `KIPLATFORM::DRIVERS::Valid3DConnexionDriverVersion()`.
///
/// The pinned Unix body is `return false;` ("The 3DConnexion driver doesn't work for
/// Linux"); it reads no driver state.
#[must_use]
pub fn valid_3dconnexion_driver_version() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn unix_driver_is_never_valid() {
        assert!(!super::valid_3dconnexion_driver_version());
    }
}
