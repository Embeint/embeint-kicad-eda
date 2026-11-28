// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kiplatform/os/unix/printing.cpp`: print a PDF through poppler and the GTK print
//! dialog.
//!
//! The translation unit's `s_lastSettings` (the settings of the last accepted dialog) is
//! process state; here it is [`Printing::last_settings`], owned by the platform object.

use kicad_kiplatform::printing::PrintResult;
use kicad_map_helpers::WxString;

use crate::host::{PrintSignals, Signal, Trace, UnixHost, PRINTING_TRACE};
use crate::wx::{from_utf8, is_spool_name, url_to_file_name, utf8_str, FileName};
use crate::{c_str, Ptr, Result, UnixError};

/// `R_OK`.
pub const R_OK: i32 = 4;
/// `GTK_UNIT_POINTS`.
pub const GTK_UNIT_POINTS: i32 = 1;
/// `GTK_PRINT_OPERATION_ACTION_PRINT_DIALOG`.
pub const GTK_PRINT_OPERATION_ACTION_PRINT_DIALOG: i32 = 0;
/// `GTK_PRINT_OPERATION_RESULT_ERROR`.
pub const GTK_PRINT_OPERATION_RESULT_ERROR: i32 = 0;
/// `GTK_PRINT_OPERATION_RESULT_APPLY`.
pub const GTK_PRINT_OPERATION_RESULT_APPLY: i32 = 1;
/// `GTK_PRINT_OPERATION_RESULT_CANCEL`.
pub const GTK_PRINT_OPERATION_RESULT_CANCEL: i32 = 2;
/// `GTK_PAGE_ORIENTATION_PORTRAIT`.
pub const GTK_PAGE_ORIENTATION_PORTRAIT: i32 = 0;
/// `GTK_PAGE_ORIENTATION_LANDSCAPE`.
pub const GTK_PAGE_ORIENTATION_LANDSCAPE: i32 = 1;
/// `GTK_PRINT_PAGES_ALL`.
pub const GTK_PRINT_PAGES_ALL: i32 = 0;
/// `GTK_PRINT_SETTINGS_OUTPUT_URI`.
pub const OUTPUT_URI: &[u8] = b"output-uri";
/// `GTK_PRINT_SETTINGS_OUTPUT_DIR`.
pub const OUTPUT_DIR: &[u8] = b"output-dir";
/// `GTK_PRINT_SETTINGS_OUTPUT_BASENAME`.
pub const OUTPUT_BASENAME: &[u8] = b"output-basename";
/// `GTK_PRINT_SETTINGS_PAGE_RANGES`.
pub const PAGE_RANGES: &[u8] = b"page-ranges";

/// The native data of a `wxPrintData`, as `ResetPrintToFilePath` distinguishes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintNativeData {
    /// A `wxGtkPrintNativeData` with its `GetPrintConfig()` (null in no state the wx class
    /// can reach, but checked by the reference).
    Gtk {
        /// `GtkPrintSettings*`.
        config: Option<Ptr>,
    },
    /// Any other native data (`dynamic_cast` gives null).
    Other,
}

/// The parts of a `wxPrintData` `ResetPrintToFilePath` reads and writes.
#[derive(Debug, PartialEq, Eq)]
pub struct WxPrintData {
    /// `GetFilename()` / `SetFilename()`.
    pub filename: WxString,
    /// `GetNativeData()`.
    pub native: PrintNativeData,
}

/// The translation unit's printing state.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Printing {
    /// `GtkPrintSettings* s_lastSettings = nullptr`.
    pub last_settings: Option<Ptr>,
}

/// `struct PrintData` (`doc`, `fit_to_page`), whose address the three signal handlers
/// receive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrintData {
    /// `PopplerDocument* doc`.
    pub doc: Option<Ptr>,
    /// `bool fit_to_page`.
    pub fit_to_page: bool,
}

impl PrintData {
    /// `PrintData( PopplerDocument* d )`: `fit_to_page` defaults to true.
    pub fn new(doc: Option<Ptr>) -> Self {
        Self {
            doc,
            fit_to_page: true,
        }
    }
}

/// `std::min( a, b )`: `( b < a ) ? b : a`.
fn std_min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// `a / b`, refusing a zero divisor, for which the reference is undefined.
fn divide(a: f64, b: f64) -> Result<f64> {
    if b == 0.0 {
        return Err(UnixError::Undefined(
            "floating division by zero in draw_page",
        ));
    }
    Ok(a / b)
}

impl<H: UnixHost + ?Sized> PrintSignals<H> for PrintData {
    /// `void begin_print_callback( operation, context, user_data )`.
    fn begin_print(&mut self, host: &mut H, operation: Ptr, _context: Ptr) -> Result<()> {
        let Some(doc) = self.doc else {
            return host.gtk_print_operation_cancel(operation);
        };
        let num_pages = host.poppler_document_get_n_pages(doc)?;
        if num_pages <= 0 {
            return host.gtk_print_operation_cancel(operation);
        }
        host.gtk_print_operation_set_n_pages(operation, num_pages)
    }

    /// `void draw_page( operation, context, page_nr, user_data )`.
    fn draw_page(
        &mut self,
        host: &mut H,
        _operation: Ptr,
        context: Ptr,
        page_nr: i32,
    ) -> Result<()> {
        let Some(doc) = self.doc else {
            return Ok(());
        };
        let Some(page) = host.poppler_document_get_page(doc, page_nr)? else {
            return Ok(());
        };
        // cleanup_page: g_object_unref( page ) when the scope ends.
        let drawn = draw_page_body(host, *self, context, page);
        let released = host.g_object_unref(page);
        drawn.and(released)
    }

    /// `void request_page_setup_callback( operation, context, page_nr, setup, user_data )`.
    fn request_page_setup(
        &mut self,
        host: &mut H,
        _operation: Ptr,
        _context: Ptr,
        page_nr: i32,
        setup: Ptr,
    ) -> Result<()> {
        let Some(doc) = self.doc else {
            return Ok(());
        };
        let Some(page) = host.poppler_document_get_page(doc, page_nr)? else {
            return Ok(());
        };
        let (page_width, page_height) = host.poppler_page_get_size(page)?;
        let orientation = if page_width > page_height {
            GTK_PAGE_ORIENTATION_LANDSCAPE
        } else {
            GTK_PAGE_ORIENTATION_PORTRAIT
        };
        host.gtk_page_setup_set_orientation(setup, orientation)?;
        host.g_object_unref(page)
    }
}

/// `draw_page` after the page is fetched, up to the end of the scope that restores the cairo
/// state.
fn draw_page_body<H: UnixHost + ?Sized>(
    host: &mut H,
    data: PrintData,
    context: Ptr,
    page: Ptr,
) -> Result<()> {
    let Some(cr) = host.gtk_print_context_get_cairo_context(context)? else {
        return Ok(());
    };
    let (page_width, page_height) = host.poppler_page_get_size(page)?;
    let print_width = host.gtk_print_context_get_width(context)?;
    let print_height = host.gtk_print_context_get_height(context)?;
    host.cairo_save(cr)?;
    // cleanup_cairo: cairo_restore( cr ) when the scope ends, before cleanup_page.
    let painted = (|| -> Result<()> {
        if data.fit_to_page {
            let scale_x = divide(print_width, page_width)?;
            let scale_y = divide(print_height, page_height)?;
            let scale = std_min(scale_x, scale_y);
            let scaled_width = page_width * scale;
            let scaled_height = page_height * scale;
            let offset_x = (print_width - scaled_width) / 2.0;
            let offset_y = (print_height - scaled_height) / 2.0;
            host.cairo_translate(cr, offset_x, offset_y)?;
            host.cairo_scale(cr, scale, scale)?;
        }
        host.cairo_set_source_rgb(cr, 1.0, 1.0, 1.0)?;
        host.cairo_paint(cr)?;
        host.poppler_page_render(page, cr)
    })();
    // A refused (undefined) evaluation stops at once; otherwise the scope ends normally.
    if let Err(UnixError::Undefined(why)) = painted {
        return Err(UnixError::Undefined(why));
    }
    let restored = host.cairo_restore(cr);
    painted.and(restored)
}

/// `GtkWindow* activeToplevel()`: the GTK toplevel of wx's active top-level window, or null.
fn active_toplevel<H: UnixHost + ?Sized>(host: &mut H) -> Result<Option<Ptr>> {
    let window = host.wx_get_active_window()?;
    let Some(active) = host.wx_get_top_level_parent(window)? else {
        return Ok(None);
    };
    // !active->GetHandle(), then gtk_widget_get_toplevel( active->GetHandle() ): two calls.
    if host.wx_window_get_handle(active)?.is_none() {
        return Ok(None);
    }
    let Some(handle) = host.wx_window_get_handle(active)? else {
        return Err(UnixError::Host(
            "wxWindow::GetHandle changed between two calls",
        ));
    };
    let toplevel = host.gtk_widget_get_toplevel(handle)?;
    Ok(if host.gtk_widget_is_toplevel(toplevel)? {
        toplevel
    } else {
        None
    })
}

impl Printing {
    /// `void rememberSettings( GtkPrintSettings* aSettings )`: keep a copy of the accepted
    /// settings without the output file, page selection and page ranges, keeping the output
    /// folder unless the file was a GTK spool file.
    fn remember_settings<H: UnixHost + ?Sized>(
        &mut self,
        host: &mut H,
        settings: Ptr,
    ) -> Result<()> {
        let settings = host.gtk_print_settings_copy(settings)?;
        if let Some(uri) = host.gtk_print_settings_get(settings, OUTPUT_URI)? {
            let file = host.g_file_new_for_uri(c_str(&uri))?;
            let dir = host.g_file_get_parent(file)?;
            let name = host.g_file_get_basename(file)?;
            let dir_path = match dir {
                Some(dir) => host.g_file_get_path(dir)?,
                None => None,
            };
            if let (Some(dir_path), Some(name)) = (&dir_path, &name) {
                let name = from_utf8(Some(&name.bytes))?;
                let name = FileName::new(name.units())?;
                if !is_spool_name(host, name.name())? {
                    host.gtk_print_settings_set(settings, OUTPUT_DIR, &dir_path.bytes)?;
                }
            }
            host.g_free(dir_path.map(|s| s.ptr))?;
            host.g_free(name.map(|s| s.ptr))?;
            if let Some(dir) = dir {
                host.g_object_unref(dir)?;
            }
            host.g_object_unref(file)?;
            host.gtk_print_settings_unset(settings, OUTPUT_URI)?;
        }
        host.gtk_print_settings_set_print_pages(settings, GTK_PRINT_PAGES_ALL)?;
        host.gtk_print_settings_unset(settings, PAGE_RANGES)?;
        if let Some(last) = self.last_settings {
            host.g_object_unref(last)?;
        }
        self.last_settings = Some(settings);
        Ok(())
    }

    /// `PRINT_RESULT PrintPDF( const std::string& aFile, bool fit_to_page, int aCurrentPage )`.
    pub fn print_pdf_fit<H: UnixHost + ?Sized>(
        &mut self,
        host: &mut H,
        file: &[u8],
        fit_to_page: bool,
        current_page: i32,
    ) -> Result<PrintResult> {
        if host.access(c_str(file), R_OK)? != 0 {
            return Ok(PrintResult::FileNotFound);
        }
        let Some(uri) = host.g_filename_to_uri(c_str(file))? else {
            return Ok(PrintResult::FileNotFound);
        };
        let (doc, error) = host.poppler_document_new_from_file(&uri.bytes)?;
        host.g_free(Some(uri.ptr))?;
        if let Some(error) = error {
            let message = host.g_error_message(error)?;
            host.wx_log_trace(
                PRINTING_TRACE,
                Trace::PopplerLoad {
                    file,
                    message: &message,
                },
            )?;
            host.g_error_free(error)?;
            return Ok(PrintResult::FailedToLoad);
        }
        let Some(doc) = doc else {
            return Ok(PrintResult::FailedToLoad);
        };
        // cleanup_doc: g_object_unref( doc ) when the function returns.
        let result = self.print_document(host, file, fit_to_page, current_page, doc);
        let released = host.g_object_unref(doc);
        match (result, released) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
    }

    /// `PrintPDF` from the page count check to the end, with `doc` loaded.
    fn print_document<H: UnixHost + ?Sized>(
        &mut self,
        host: &mut H,
        file: &[u8],
        fit_to_page: bool,
        current_page: i32,
        doc: Ptr,
    ) -> Result<PrintResult> {
        let num_pages = host.poppler_document_get_n_pages(doc)?;
        if num_pages <= 0 {
            return Ok(PrintResult::FailedToLoad);
        }
        let mut print_data = PrintData::new(Some(doc));
        print_data.fit_to_page = fit_to_page;
        let Some(op) = host.gtk_print_operation_new()? else {
            return Ok(PrintResult::FailedToPrint);
        };
        // cleanup_op: g_object_unref( op ) when the function returns, before cleanup_doc.
        let result = self.run_operation(host, file, current_page, num_pages, &mut print_data, op);
        let released = host.g_object_unref(op);
        match (result, released) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
    }

    /// `PrintPDF` with the operation created.
    fn run_operation<H: UnixHost + ?Sized>(
        &mut self,
        host: &mut H,
        file: &[u8],
        current_page: i32,
        num_pages: i32,
        print_data: &mut PrintData,
        op: Ptr,
    ) -> Result<PrintResult> {
        host.gtk_print_operation_set_use_full_page(op, false)?;
        host.gtk_print_operation_set_unit(op, GTK_UNIT_POINTS)?;

        // wxFileName( wxString::FromUTF8( aFile ) ).GetName().ToStdString( wxConvUTF8 ):
        // FromUTF8( const std::string& ) converts all of aFile (explicit length).
        let name = from_utf8_std_string(file)?;
        let name = FileName::new(name.units())?;
        let job_name = utf8_str(host, name.name())?;
        let settings = match self.last_settings {
            Some(last) => host.gtk_print_settings_copy(last)?,
            None => host.gtk_print_settings_new()?,
        };
        host.gtk_print_settings_set(settings, OUTPUT_BASENAME, c_str(&job_name))?;
        host.gtk_print_operation_set_print_settings(op, settings)?;
        host.g_object_unref(settings)?;
        host.gtk_print_operation_set_job_name(op, c_str(&job_name))?;

        if current_page > 0 && current_page <= num_pages {
            host.gtk_print_operation_set_current_page(op, current_page - 1)?;
        }

        host.g_signal_connect(op, Signal::BeginPrint)?;
        host.g_signal_connect(op, Signal::DrawPage)?;
        host.g_signal_connect(op, Signal::RequestPageSetup)?;

        let parent = active_toplevel(host)?;
        let (result, error) = host.gtk_print_operation_run(
            op,
            GTK_PRINT_OPERATION_ACTION_PRINT_DIALOG,
            parent,
            print_data,
        )?;

        if let Some(error) = error {
            let message = host.g_error_message(error)?;
            host.wx_log_trace(PRINTING_TRACE, Trace::GtkPrint { message: &message })?;
            host.g_error_free(error)?;
            return Ok(PrintResult::FailedToPrint);
        }
        Ok(match result {
            GTK_PRINT_OPERATION_RESULT_APPLY => {
                let Some(settings) = host.gtk_print_operation_get_print_settings(op)? else {
                    return Err(UnixError::Host(
                        "gtk_print_operation_get_print_settings returned null after set",
                    ));
                };
                self.remember_settings(host, settings)?;
                PrintResult::Ok
            }
            GTK_PRINT_OPERATION_RESULT_CANCEL => PrintResult::Cancelled,
            _ => PrintResult::FailedToPrint,
        })
    }

    /// `PRINT_RESULT PrintPDF( const std::string& aFile, int aCurrentPage )`: fit to page.
    pub fn print_pdf<H: UnixHost + ?Sized>(
        &mut self,
        host: &mut H,
        file: &[u8],
        current_page: i32,
    ) -> Result<PrintResult> {
        self.print_pdf_fit(host, file, true, current_page)
    }
}

/// `wxString::FromUTF8( const std::string& )`: `FromUTF8( utf8.c_str(), utf8.length() )`, the
/// strict decoding of every byte, NUL bytes included (each a NUL unit). A multibyte sequence
/// cannot contain a NUL byte, so decoding each NUL-separated segment and joining them with NUL
/// units is the same conversion; any failed segment fails the whole string (empty result).
pub fn from_utf8_std_string(bytes: &[u8]) -> Result<WxString> {
    if bytes.is_empty() {
        return Ok(WxString::new());
    }
    let mut out = alloc::vec::Vec::new();
    crate::reserve(&mut out, bytes.len())?;
    for (index, segment) in bytes.split(|&b| b == 0).enumerate() {
        if index > 0 {
            out.push(0);
        }
        let decoded = from_utf8(Some(segment))?;
        if !segment.is_empty() && decoded.is_empty() {
            return Ok(WxString::new());
        }
        out.extend_from_slice(decoded.units());
    }
    crate::wx::units(&out)
}

/// `void ResetPrintToFilePath( wxPrintData& aData )`: clear a GTK spool file chosen as the
/// print-to-file destination, in the wx file name and in the GTK output URI.
pub fn reset_print_to_file_path<H: UnixHost + ?Sized>(
    host: &mut H,
    data: &mut WxPrintData,
) -> Result<()> {
    let name = FileName::new(data.filename.units())?;
    let full_name = name.full_name()?;
    if is_spool_name(host, &full_name)? {
        data.filename = WxString::new();
    }
    let PrintNativeData::Gtk {
        config: Some(config),
    } = data.native
    else {
        return Ok(());
    };
    let Some(uri) = host.gtk_print_settings_get(config, OUTPUT_URI)? else {
        return Ok(());
    };
    let uri = from_utf8(Some(&uri))?;
    let destination = url_to_file_name(host, &uri)?;
    let full_name = destination.full_name()?;
    if is_spool_name(host, &full_name)? {
        host.gtk_print_settings_unset(config, OUTPUT_URI)?;
    }
    Ok(())
}
