// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::ENV` from `libs/kiplatform/include/kiplatform/environment.h`.
//!
//! The header defines one type, [`ProxyConfig`]; its functions are the host boundary
//! [`EnvPlatform`]. Every Linux body is in `libs/kiplatform/os/unix/environment.cpp`
//! ([`crate::owners::OS_UNIX_UI`]).

use kicad_map_helpers::{WxError, WxString};

/// `KIPLATFORM::ENV::PROXY_CONFIG`: three `wxString` members.
///
/// Both `PROXY_CONFIG cfg;` and `PROXY_CONFIG{}` give three empty strings (the
/// `wxString` default constructor), which is [`Default`]. The implicit copy constructor
/// copies each member in declaration order; [`ProxyConfig::try_clone`] does the same
/// with the fallible `wxString` replacement.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProxyConfig {
    /// `wxString host`.
    pub host: WxString,
    /// `wxString username`.
    pub username: WxString,
    /// `wxString password`.
    pub password: WxString,
}

impl ProxyConfig {
    /// The implicit `PROXY_CONFIG( const PROXY_CONFIG& )`: `host`, `username`, then
    /// `password`. An allocation refusal is the reference's `std::bad_alloc`.
    pub fn try_clone(&self) -> Result<Self, WxError> {
        Ok(Self {
            host: self.host.try_clone()?,
            username: self.username.try_clone()?,
            password: self.password.try_clone()?,
        })
    }
}

/// The `KIPLATFORM::ENV` functions, implemented by the native host.
pub trait EnvPlatform {
    /// Host handle for a `wxWindow*`.
    type Window;

    /// `void Init()`.
    fn init(&mut self);

    /// `bool MoveToTrash( const wxString& aPath, wxString& aError )`.
    fn move_to_trash(&mut self, path: &WxString, error: &mut WxString) -> bool;

    /// `bool IsNetworkPath( const wxString& aPath )`.
    fn is_network_path(&mut self, path: &WxString) -> bool;

    /// `bool IsRemovablePath( const wxString& aPath )`.
    fn is_removable_path(&mut self, path: &WxString) -> bool;

    /// `wxString GetDocumentsPath()`.
    fn get_documents_path(&mut self) -> WxString;

    /// `wxString GetUserConfigPath()`.
    fn get_user_config_path(&mut self) -> WxString;

    /// `wxString GetUserDataPath()`.
    fn get_user_data_path(&mut self) -> WxString;

    /// `wxString GetUserLocalDataPath()`.
    fn get_user_local_data_path(&mut self) -> WxString;

    /// `wxString GetUserCachePath()`.
    fn get_user_cache_path(&mut self) -> WxString;

    /// `bool GetSystemProxyConfig( const wxString& aURL, PROXY_CONFIG& aCfg )`.
    fn get_system_proxy_config(&mut self, url: &WxString, cfg: &mut ProxyConfig) -> bool;

    /// `bool VerifyFileSignature( const wxString& aPath )`.
    fn verify_file_signature(&mut self, path: &WxString) -> bool;

    /// `wxString GetAppUserModelId()`.
    fn get_app_user_model_id(&mut self) -> WxString;

    /// `void SetAppDetailsForWindow( wxWindow* aWindow, const wxString& aRelaunchCommand,
    /// const wxString& aRelaunchDisplayName )`. `None` is a null window.
    fn set_app_details_for_window(
        &mut self,
        window: Option<&Self::Window>,
        relaunch_command: &WxString,
        relaunch_display_name: &WxString,
    );

    /// `wxString GetCommandLineStr()`.
    fn get_command_line_str(&mut self) -> WxString;

    /// `void AddToRecentDocs( const wxString& aPath )`.
    fn add_to_recent_docs(&mut self, path: &WxString);
}
