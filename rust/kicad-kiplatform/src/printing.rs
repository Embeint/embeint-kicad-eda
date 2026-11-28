// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::PRINTING` from `libs/kiplatform/include/printing.h`: minimum co-port for the
//! Linux bodies in `libs/kiplatform/os/unix/printing.cpp` ([`crate::owners::OS_UNIX_UI`]).
//!
//! The header belongs to `libs.kiplatform.include.application.ui.p001` ([`HEADER_UNIT`]),
//! which is not ported. Only `enum class PRINT_RESULT` and the two declarations with the
//! default argument `aCurrentPage = 0` are taken; `PrintResultToString` (translated
//! messages) and the `PRINTING_TRACE` macro's other uses stay with that unit.

/// The unit that owns `libs/kiplatform/include/printing.h`.
pub const HEADER_UNIT: &str = "libs.kiplatform.include.application.ui.p001";

/// `KIPLATFORM::PRINTING::PRINT_RESULT` (`enum class`, underlying type `int`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum PrintResult {
    /// `OK = 0`.
    Ok = 0,
    /// `CANCELLED`.
    Cancelled = 1,
    /// `FILE_NOT_FOUND`.
    FileNotFound = 2,
    /// `FAILED_TO_LOAD`.
    FailedToLoad = 3,
    /// `FAILED_TO_PRINT`.
    FailedToPrint = 4,
    /// `UNSUPPORTED`.
    Unsupported = 5,
    /// `UNKNOWN_ERROR`.
    UnknownError = 6,
}

impl PrintResult {
    /// `static_cast<int>( result )`.
    pub const fn value(self) -> i32 {
        self as i32
    }
}

/// The `KIPLATFORM::PRINTING` functions, implemented by the platform.
pub trait PrintingPlatform {
    /// Host type for `wxPrintData`.
    type PrintData;
    /// Why a call did not complete.
    type Error;

    /// `PRINT_RESULT PrintPDF( const std::string& aFile, int aCurrentPage )`; `file` is every
    /// byte of the `std::string`.
    fn print_pdf(&mut self, file: &[u8], current_page: i32) -> Result<PrintResult, Self::Error>;

    /// `PrintPDF( aFile )`: the default argument `aCurrentPage = 0`.
    fn print_pdf_without_current_page(&mut self, file: &[u8]) -> Result<PrintResult, Self::Error> {
        self.print_pdf(file, 0)
    }

    /// `void ResetPrintToFilePath( wxPrintData& aData )`.
    fn reset_print_to_file_path(&mut self, data: &mut Self::PrintData) -> Result<(), Self::Error>;
}
