// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::APP` from `libs/kiplatform/include/kiplatform/app.h`.
//!
//! The header only declares functions; it has no type, constant, inline body or default
//! argument. Its declarations are the host boundary [`AppPlatform`]. Every Linux body is in
//! `libs/kiplatform/os/unix/app.cpp` ([`crate::owners::OS_UNIX_UI`]); the Windows and macOS
//! bodies belong to the blocked platform units.

use core::convert::Infallible;

use kicad_map_helpers::WxString;

/// The `KIPLATFORM::APP` functions, implemented by the platform.
///
/// A `wxWindow*` argument is an optional host handle: `None` is a null pointer. Each call
/// can fail with [`AppPlatform::Error`] (a deferred continuation, a refused allocation or a
/// host failure) instead of returning a guessed result.
pub trait AppPlatform {
    /// Host handle for a `wxWindow*`.
    type Window;
    /// Why a call did not complete.
    type Error;

    /// `bool Init()`.
    fn init(&mut self) -> Result<bool, Self::Error>;

    /// `void EnableDarkMode( bool aForce )`.
    fn enable_dark_mode(&mut self, force: bool) -> Result<(), Self::Error>;

    /// `bool AttachConsole( bool aTryAlloc )`.
    fn attach_console(&mut self, try_alloc: bool) -> Result<bool, Self::Error>;

    /// `bool IsOperatingSystemUnsupported()`.
    fn is_operating_system_unsupported(&mut self) -> Result<bool, Self::Error>;

    /// `bool RegisterApplicationRestart( const wxString& aCommandLine )`.
    fn register_application_restart(
        &mut self,
        command_line: &WxString,
    ) -> Result<bool, Self::Error>;

    /// `bool UnregisterApplicationRestart()`.
    fn unregister_application_restart(&mut self) -> Result<bool, Self::Error>;

    /// `bool SupportsShutdownBlockReason()`.
    fn supports_shutdown_block_reason(&mut self) -> Result<bool, Self::Error>;

    /// `void SetShutdownBlockReason( wxWindow* aWindow, const wxString& aReason )`.
    fn set_shutdown_block_reason(
        &mut self,
        window: Option<&Self::Window>,
        reason: &WxString,
    ) -> Result<(), Self::Error>;

    /// `void RemoveShutdownBlockReason( wxWindow* aWindow )`.
    fn remove_shutdown_block_reason(
        &mut self,
        window: Option<&Self::Window>,
    ) -> Result<(), Self::Error>;

    /// `void ForceTimerMessagesToBeCreatedIfNecessary()`.
    fn force_timer_messages_to_be_created_if_necessary(&mut self) -> Result<(), Self::Error>;

    /// `void AddDynamicLibrarySearchPath( const wxString& aPath )`.
    fn add_dynamic_library_search_path(&mut self, path: &WxString) -> Result<(), Self::Error>;

    /// `bool SetCurrentThreadPriority( bool aRaise )`.
    fn set_current_thread_priority(&mut self, raise: bool) -> Result<bool, Self::Error>;

    /// `[[noreturn]] void TerminateWithCode( int aExitCode )`: the process ends, so the call
    /// returns only with an error (`Infallible` cannot be constructed).
    fn terminate_with_code(&mut self, exit_code: i32) -> Result<Infallible, Self::Error>;
}
