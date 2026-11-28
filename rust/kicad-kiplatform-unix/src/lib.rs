// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![no_std]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! Portable port of KiCad's Linux platform bodies `libs/kiplatform/os/unix/{app,environment,
//! io,printing,secrets}.cpp` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7 (unit [`UNIT`]).
//!
//! The bodies are thin layers over the operating system, GLib/GIO, GTK, poppler, cairo,
//! libsecret and a few wxWidgets services. None of those effects happens here: every call the
//! C++ makes to them is a method of the typed host boundary [`host::UnixHost`], with the same
//! arguments and in the same order, and this crate ports what the C++ does around those
//! calls: argument construction, string conversion and formatting, error handling, fallbacks,
//! loops, arithmetic and the order of calls and releases.
//!
//! The wxWidgets 3.2.4 library code the bodies reach is ported where it is deterministic
//! computation ([`wx`]: the `wxString` operations and conversion wrappers, the strict UTF-8
//! converter, the Unix `wxFileName` path split, the `wxURI` authority parser and
//! `wxURI::Unescape`, `wxFileName::URLToFileName` and the fixed spool-name regular
//! expression). What depends on process state is a host call: the environment, the locale's
//! case mapping and multibyte converter, the file-name converter `wxConvFileName`, logging and
//! the assertion handler. [`stdhash`] ports libstdc++'s `std::hash<std::string>`.
//!
//! [`platform::UnixPlatform`] implements the accepted header traits of `kicad_kiplatform`
//! (`AppPlatform`, `EnvPlatform`, `IoPlatform`, `SecretsPlatform`, `PrintingPlatform`) with
//! these bodies. The out-of-line `os/common/io.cpp` members of `IoPlatform` belong to
//! [`kicad_kiplatform::owners::OS_COMMON_UI`] and are deferred.

extern crate alloc;

pub mod app;
pub mod environment;
pub mod host;
pub mod io;
pub mod platform;
pub mod printing;
pub mod secrets;
pub mod stdhash;
pub mod wx;

use alloc::vec::Vec;
use kicad_map_helpers::WxError;

/// The port unit implemented by this crate.
pub const UNIT: &str = kicad_kiplatform::owners::OS_UNIX_UI;

/// A non-null pointer the host handed out (`GFile*`, `FILE*`, `GtkPrintOperation*`, ...).
/// Portable code only passes it back; `Option<Ptr>` is a pointer that may be null.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ptr(pub u64);

/// A NUL-terminated C string the host owns or allocated (`const gchar*`, `char*`): its
/// address and its bytes up to, not including, the first NUL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GStr {
    /// The pointer, as passed to `g_free` or `free`.
    pub ptr: Ptr,
    /// The bytes before the terminating NUL.
    pub bytes: Vec<u8>,
}

/// Why a body did not complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnixError {
    /// A call into code no accepted unit implements (`// UNPORTED(<unit>): <symbol>`).
    Unported {
        /// The C++ symbol or continuation.
        upstream: &'static str,
        /// The unit that owns it.
        unit: &'static str,
    },
    /// A refused `wxString` allocation (the reference would throw `std::bad_alloc`).
    Wx(WxError),
    /// Any other refused allocation (`std::bad_alloc`).
    Alloc,
    /// `std::bad_function_call`, thrown by invoking an empty `std::function`.
    BadFunctionCall,
    /// The reference evaluation is undefined (signed overflow, a pointer formed outside its
    /// array); the port refuses it instead of choosing a result.
    Undefined(&'static str),
    /// The host could not answer a call (a scripted host without a reply). A native host
    /// never returns it.
    Host(&'static str),
}

impl From<WxError> for UnixError {
    fn from(error: WxError) -> Self {
        Self::Wx(error)
    }
}

/// Shorthand for this crate's results.
pub type Result<T> = core::result::Result<T, UnixError>;

/// Reserve `count` more elements of `vec`, reporting a refusal instead of aborting.
pub(crate) fn reserve<T>(vec: &mut Vec<T>, count: usize) -> Result<()> {
    vec.try_reserve(count).map_err(|_| UnixError::Alloc)
}

/// Copy `bytes` into a new vector, reporting a refused allocation.
pub(crate) fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut copy = Vec::new();
    reserve(&mut copy, bytes.len())?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

/// The bytes a `const char*` reads: everything before the first NUL.
pub fn c_str(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|&b| b == 0) {
        Some(end) => bytes.get(..end).unwrap_or(bytes),
        None => bytes,
    }
}
