// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::SECRETS` from `libs/kiplatform/include/kiplatform/secrets.h`.
//!
//! The header declares functions only; its one default argument, `aDone = nullptr` of
//! `StoreSecretAsync`, is [`SecretsPlatform::store_secret_async_without_done`]. The Linux
//! bodies are in `libs/kiplatform/os/unix/secrets.cpp` ([`crate::owners::OS_UNIX_UI`]).

use alloc::boxed::Box;
use kicad_map_helpers::WxString;

/// `std::function<void( bool )>` as `StoreSecretAsync` takes it: `None` is an empty
/// function (`nullptr`).
pub type StoreDone = Option<Box<dyn FnMut(bool)>>;

/// `std::function<void( bool, const wxString& )>` as `GetSecretAsync` takes it: `None` is an
/// empty function.
pub type GetDone = Option<Box<dyn FnMut(bool, &WxString)>>;

/// The `KIPLATFORM::SECRETS` functions, implemented by the native host.
pub trait SecretsPlatform {
    /// Why a call did not complete.
    type Error;
    /// `bool StoreSecret( const wxString& aService, const wxString& aKey,
    /// const wxString& aSecret )`.
    fn store_secret(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &WxString,
    ) -> Result<bool, Self::Error>;

    /// `void StoreSecretAsync( const wxString& aService, const wxString& aKey,
    /// const wxString& aSecret, std::function<void( bool )> aDone )`.
    fn store_secret_async(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &WxString,
        done: StoreDone,
    ) -> Result<(), Self::Error>;

    /// `StoreSecretAsync( aService, aKey, aSecret )`: the default argument
    /// `aDone = nullptr`, an empty `std::function`.
    fn store_secret_async_without_done(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &WxString,
    ) -> Result<(), Self::Error> {
        self.store_secret_async(service, key, secret, None)
    }

    /// `bool GetSecret( const wxString& aService, const wxString& aKey, wxString& aSecret )`.
    fn get_secret(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &mut WxString,
    ) -> Result<bool, Self::Error>;

    /// `void GetSecretAsync( const wxString& aService, const wxString& aKey,
    /// std::function<void( bool, const wxString& )> aDone )`.
    fn get_secret_async(
        &mut self,
        service: &WxString,
        key: &WxString,
        done: GetDone,
    ) -> Result<(), Self::Error>;

    /// `bool GetSecretNoPrompt( const wxString& aService, const wxString& aKey,
    /// wxString& aSecret )`.
    fn get_secret_no_prompt(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &mut WxString,
    ) -> Result<bool, Self::Error>;

    /// `bool DeleteSecret( const wxString& aService, const wxString& aKey )`.
    fn delete_secret(&mut self, service: &WxString, key: &WxString) -> Result<bool, Self::Error>;
}
