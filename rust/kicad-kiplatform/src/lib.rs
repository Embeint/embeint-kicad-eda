// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![no_std]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! Portable port of KiCad's `libs/kiplatform` platform layer at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! - [`policy`]: the Unix policy source (`libs/kiplatform/os/unix/policy.cpp`, unit [`UNIT`]).
//! - [`app`], [`environment`], [`io`], [`secrets`], [`touchpad`], [`ui`] and [`webview`]: the
//!   public headers `kiplatform/{app,environment,io,secrets,touchpad,ui,webview}.h` (unit
//!   [`INCLUDE_UNIT`]).
//! - [`printing`]: the minimum co-port of `libs/kiplatform/include/printing.h` (`PRINT_RESULT`
//!   and the `PrintPDF`/`ResetPrintToFilePath` declarations) the Linux printing bodies need. `drivers.h`, `sysinfo.h` and `sysinfo_gtk.h` of the same unit are in
//!   `kicad_kiplatform_sysinfo`, next to the Unix bodies they declare.
//!
//! The headers define a few data types, constants and inline bodies, which are ported here
//! exactly. Every other declaration is a platform function whose body lives in an
//! implementation unit (`os/unix`, `os/common`, `port/wxgtk`, ...). Portable code performs no
//! OS call, file or process I/O and has no GUI, so each header's declarations are a typed
//! host boundary: a trait the native host implements, with wx and OS objects as associated
//! handle types. The header's default arguments are provided trait methods that call the
//! full form with the default value, as the C++ call site does. Each trait has an `Error`
//! type, so an implementation can report a deferred continuation, a refused allocation or a
//! host failure instead of returning a guessed value (`kicad_kiplatform_unix` implements the
//! Linux bodies this way).

extern crate alloc;

pub mod app;
pub mod environment;
pub mod io;
pub mod policy;
pub mod printing;
pub mod secrets;
pub mod touchpad;
pub mod ui;
pub mod webview;

/// The unit that owns `libs/kiplatform/os/unix/policy.cpp`.
pub const UNIT: &str = "libs.kiplatform.os.unix.application.thin.replacement.p001";
/// The unit that owns `libs/kiplatform/include/kiplatform/policy.h` (`PBOOL`).
pub const HEADER_UNIT: &str =
    "libs.kiplatform.include.kiplatform.application.thin.replacement.p001";
/// The unit that owns the public headers
/// `libs/kiplatform/include/kiplatform/{app,drivers,environment,io,secrets,sysinfo,sysinfo_gtk,touchpad,ui,webview}.h`.
pub const INCLUDE_UNIT: &str = "libs.kiplatform.include.kiplatform.application.pure.p001";

/// Units that own the platform bodies behind the header declarations. Each trait method
/// names the one that implements it for the Linux/GTK reference build.
pub mod owners {
    /// `libs/kiplatform/os/unix/{app,environment,io,printing,secrets}.cpp`.
    pub const OS_UNIX_UI: &str = "libs.kiplatform.os.unix.application.ui.p001";
    /// `libs/kiplatform/os/common/io.cpp` (atomic save, `FILE_LOCK`, `readIntoBuffer`).
    pub const OS_COMMON_UI: &str = "libs.kiplatform.os.common.application.ui.p001";
    /// `libs/kiplatform/port/wxgtk/{touchpad,ui,webview}.cpp`.
    pub const PORT_WXGTK_UI: &str = "libs.kiplatform.port.wxgtk.application.ui.p001";
    /// Windows bodies (blocked; not part of the Linux reference).
    pub const OS_WINDOWS_UI: &str = "libs.kiplatform.os.windows.application.ui.p001";
    /// Windows `port/wxmsw` bodies (blocked).
    pub const PORT_WXMSW_UI: &str = "libs.kiplatform.port.wxmsw.application.ui.p001";
    /// macOS `os/apple` bodies (blocked).
    pub const OS_APPLE_UI: &str = "libs.kiplatform.os.apple.application.ui.p001";
    /// macOS `port/wxosx` bodies (blocked).
    pub const PORT_WXOSX_UI: &str = "libs.kiplatform.port.wxosx.application.ui.p001";
    /// `kiplatform/sysinfo_windows.h` and `sysinfo_osx.h` (blocked).
    pub const INCLUDE_UI: &str = "libs.kiplatform.include.kiplatform.application.ui.p001";
}
