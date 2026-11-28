// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kiplatform/os/unix/app.cpp`.

use kicad_map_helpers::WxString;

use crate::host::UnixHost;
use crate::wx::{equals_c_literal, lit};
use crate::Result;

/// `G_LOG_WRITER_HANDLED`.
pub const G_LOG_WRITER_HANDLED: u32 = 1;
/// `PRIO_PROCESS`.
pub const PRIO_PROCESS: i32 = 0;

/// `static GLogWriterOutput nullLogWriter( log_level, fields, n_fields, user_data )`: eats
/// every GLib log message.
pub fn null_log_writer(_log_level: u32, _n_fields: usize) -> u32 {
    G_LOG_WRITER_HANDLED
}

/// `bool KIPLATFORM::APP::Init()`: unless `KICAD_SHOW_GTK_MESSAGES` is `"1"`, install
/// [`null_log_writer`] as the GLib log writer.
pub fn init<H: UnixHost + ?Sized>(host: &mut H) -> Result<bool> {
    let mut show_messages = WxString::new();
    if let Some(value) = host.wx_get_env(&lit("KICAD_SHOW_GTK_MESSAGES")?, true)? {
        show_messages = value;
    }
    // showMessages.IsEmpty() || showMessages != "1": the literal is converted only when the
    // value is not empty.
    if show_messages.is_empty() || !equals_c_literal(host, &show_messages, "1")? {
        host.g_log_set_writer_func(null_log_writer)?;
    }
    Ok(true)
}

/// `void EnableDarkMode( bool aForce )`: nothing on this platform.
pub fn enable_dark_mode(_force: bool) {}

/// `bool AttachConsole( bool aTryAlloc )`: not implemented on this platform, `true`.
pub fn attach_console(_try_alloc: bool) -> bool {
    true
}

/// `bool IsOperatingSystemUnsupported()`: `false`.
pub fn is_operating_system_unsupported() -> bool {
    false
}

/// `bool RegisterApplicationRestart( const wxString& )`: `true`.
pub fn register_application_restart(_command_line: &WxString) -> bool {
    true
}

/// `bool UnregisterApplicationRestart()`: `true`.
pub fn unregister_application_restart() -> bool {
    true
}

/// `bool SupportsShutdownBlockReason()`: `false`.
pub fn supports_shutdown_block_reason() -> bool {
    false
}

/// `bool SetCurrentThreadPriority( bool aRaise )` (`__linux__`): the calling thread's nice
/// value becomes -5 or 5; true when `setpriority` succeeds.
pub fn set_current_thread_priority<H: UnixHost + ?Sized>(
    host: &mut H,
    raise: bool,
) -> Result<bool> {
    // static_cast<pid_t>( syscall( SYS_gettid ) ): long to int, modulo 2^32.
    let tid = host.syscall_gettid()? as i32;
    // setpriority's id_t parameter is unsigned int.
    let who = u32::from_ne_bytes(tid.to_ne_bytes());
    Ok(host.setpriority(PRIO_PROCESS, who, if raise { -5 } else { 5 })? == 0)
}

/// `[[noreturn]] void TerminateWithCode( int aExitCode )`: `_exit( aExitCode )`.
pub fn terminate_with_code<H: UnixHost + ?Sized>(host: &mut H, exit_code: i32) -> ! {
    host.exit(exit_code)
}
