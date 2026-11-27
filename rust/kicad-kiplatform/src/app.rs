// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::APP` from `libs/kiplatform/include/kiplatform/app.h`.
//!
//! The header only declares functions; it has no type, constant, inline body or default
//! argument. Its declarations are the host boundary [`AppPlatform`]. Every Linux body is in
//! `libs/kiplatform/os/unix/app.cpp` ([`crate::owners::OS_UNIX_UI`]); the Windows and macOS
//! bodies belong to the blocked platform units.

use kicad_map_helpers::WxString;

/// The `KIPLATFORM::APP` functions, implemented by the native host.
///
/// A `wxWindow*` argument is an optional host handle: `None` is a null pointer.
pub trait AppPlatform {
    /// Host handle for a `wxWindow*`.
    type Window;

    /// `bool Init()`.
    fn init(&mut self) -> bool;

    /// `void EnableDarkMode( bool aForce )`.
    fn enable_dark_mode(&mut self, force: bool);

    /// `bool AttachConsole( bool aTryAlloc )`.
    fn attach_console(&mut self, try_alloc: bool) -> bool;

    /// `bool IsOperatingSystemUnsupported()`.
    fn is_operating_system_unsupported(&mut self) -> bool;

    /// `bool RegisterApplicationRestart( const wxString& aCommandLine )`.
    fn register_application_restart(&mut self, command_line: &WxString) -> bool;

    /// `bool UnregisterApplicationRestart()`.
    fn unregister_application_restart(&mut self) -> bool;

    /// `bool SupportsShutdownBlockReason()`.
    fn supports_shutdown_block_reason(&mut self) -> bool;

    /// `void SetShutdownBlockReason( wxWindow* aWindow, const wxString& aReason )`.
    fn set_shutdown_block_reason(&mut self, window: Option<&Self::Window>, reason: &WxString);

    /// `void RemoveShutdownBlockReason( wxWindow* aWindow )`.
    fn remove_shutdown_block_reason(&mut self, window: Option<&Self::Window>);

    /// `void ForceTimerMessagesToBeCreatedIfNecessary()`.
    fn force_timer_messages_to_be_created_if_necessary(&mut self);

    /// `void AddDynamicLibrarySearchPath( const wxString& aPath )`.
    fn add_dynamic_library_search_path(&mut self, path: &WxString);

    /// `bool SetCurrentThreadPriority( bool aRaise )`.
    fn set_current_thread_priority(&mut self, raise: bool) -> bool;

    /// `[[noreturn]] void TerminateWithCode( int aExitCode )`: the process ends, so the
    /// method never returns.
    fn terminate_with_code(&mut self, exit_code: i32) -> !;
}
