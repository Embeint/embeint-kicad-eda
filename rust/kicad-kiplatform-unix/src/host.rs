// SPDX-License-Identifier: GPL-3.0-or-later
//! The typed host boundary: every operating-system, GLib/GIO, GTK, poppler, cairo, libsecret
//! and process-state wxWidgets call the pinned Linux bodies make.
//!
//! Each method is one C call (or one wx service) with its C arguments: byte strings are the
//! bytes a `const char*` argument reads (before its NUL), pointers are [`Ptr`] handles the
//! host handed out (`None` is a null pointer), and an out-parameter is part of the return
//! value. A `GError**` out-parameter is returned as the error pointer the call stored
//! (`None` when it left the error null); its message is read with [`UnixHost::g_error_message`]
//! (the C++ reads `err->message`). The portable bodies make these calls in the reference
//! order with the reference arguments; what the call does is the host's.

use alloc::boxed::Box;
use alloc::vec::Vec;

use kicad_kiplatform::secrets::{GetDone, StoreDone};
use kicad_map_helpers::{WChar, WxString};

use crate::{GStr, Ptr, Result};

/// A process-configured wxWidgets multibyte converter, which a native host takes from its
/// process state and a scripted host models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Conv {
    /// `wxConvLibc` (`*wxConvLibcPtr`): the C library's converter for the current locale.
    /// `wxString( const char* )`, `wxString::operator=( const char* )`,
    /// `wxString::compare( const char* )`, `operator+( wxString, const char* )`,
    /// `wxString::assign( const char*, size_t )`, `mb_str()` and `ToStdString()` use it.
    Libc,
    /// `*wxConvFileName`, which `wxString::fn_str()` uses. The GTK `wxApp` replaces it at
    /// start-up according to `G_FILENAME_ENCODING` and the locale.
    FileName,
}

/// The arguments of one `wxOnAssert( file, line, func, cond, msg )` call (wx 3.2
/// `wxFAIL_MSG`). `__FILE__` is the library build's spelling and is left to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WxAssert {
    /// `__LINE__` in the wxWidgets 3.2.4 source.
    pub line: u32,
    /// `__WXFUNCTION__`.
    pub function: &'static str,
    /// The stringized condition (`"\"Assert failure\""` for `wxFAIL_MSG`).
    pub condition: &'static str,
    /// The message.
    pub message: &'static str,
}

/// The members of `struct stat` the bodies read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatBuf {
    /// `st_mode`.
    pub mode: u32,
    /// `st_mtime` (`time_t`).
    pub mtime: i64,
    /// `st_size` (`off_t`).
    pub size: i64,
}

/// A successful `mmap`: the address and the bytes mapped there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// The returned address.
    pub ptr: Ptr,
    /// The mapped bytes (`length` of them).
    pub bytes: Vec<u8>,
}

/// `SecretSchemaAttributeType`.
pub const SECRET_SCHEMA_ATTRIBUTE_STRING: i32 = 0;
/// `SECRET_SCHEMA_NONE`.
pub const SECRET_SCHEMA_NONE: i32 = 0;

/// `SecretSchema` as the static `schema` of `secrets.cpp` defines it: the name, the flags and
/// the attribute list up to its `{ nullptr, ... }` terminator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecretSchema {
    /// `name`.
    pub name: &'static [u8],
    /// `flags`.
    pub flags: i32,
    /// `attributes`, before the terminator.
    pub attributes: &'static [(&'static [u8], i32)],
    /// The terminator entry's type (its name is null).
    pub terminator_type: i32,
}

/// One `"name", value` pair of a variadic libsecret attribute list (terminated by `NULL`).
pub type Attribute<'a> = (&'a [u8], &'a [u8]);

/// The heap `std::function<void( bool )>` `StoreSecretAsync` passes as `user_data`, which
/// `storeFinished` takes ownership of.
pub struct PendingStore {
    /// `std::function<void( bool )>( std::move( aDone ) )`.
    pub done: StoreDone,
}

/// The heap `std::function<void( bool, const wxString& )>` of `GetSecretAsync`.
pub struct PendingLookup {
    /// `std::function<void( bool, const wxString& )>( std::move( aDone ) )`.
    pub done: GetDone,
}

/// A `GList*` returned by `secret_service_search_sync`: its head and its elements in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    /// The list head (null for an empty list).
    pub head: Option<Ptr>,
    /// `it->data` of each node, in order.
    pub items: Vec<Ptr>,
}

/// The `GtkPrintOperation` signals `PrintPDF` connects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Signal {
    /// `"begin-print"` with `begin_print_callback`.
    BeginPrint,
    /// `"draw-page"` with `draw_page`.
    DrawPage,
    /// `"request-page-setup"` with `request_page_setup_callback`.
    RequestPageSetup,
}

impl Signal {
    /// The detailed signal name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::BeginPrint => "begin-print",
            Self::DrawPage => "draw-page",
            Self::RequestPageSetup => "request-page-setup",
        }
    }
}

/// The signal handlers `PrintPDF` connects, which the host's `gtk_print_operation_run` emits
/// while the dialog and the print run. Each receives the host so it can make its own calls.
pub trait PrintSignals<H: ?Sized> {
    /// `begin_print_callback( operation, context, &print_data )`.
    fn begin_print(&mut self, host: &mut H, operation: Ptr, context: Ptr) -> Result<()>;
    /// `draw_page( operation, context, page_nr, &print_data )`.
    fn draw_page(&mut self, host: &mut H, operation: Ptr, context: Ptr, page_nr: i32)
        -> Result<()>;
    /// `request_page_setup_callback( operation, context, page_nr, setup, &print_data )`.
    fn request_page_setup(
        &mut self,
        host: &mut H,
        operation: Ptr,
        context: Ptr,
        page_nr: i32,
        setup: Ptr,
    ) -> Result<()>;
}

/// The `wxLogTrace( PRINTING_TRACE, ... )` messages of `printing.cpp`, with their raw
/// arguments. wx formats a message (converting the narrow arguments with `wxConvLibc`) and
/// delivers it only when the trace mask is enabled, so formatting is the host's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trace<'a> {
    /// `"Poppler could not load %s: %s"` with `aFile` (a `std::string`) and `error->message`.
    PopplerLoad {
        /// `aFile`, all of its bytes.
        file: &'a [u8],
        /// `error->message`.
        message: &'a [u8],
    },
    /// `"GTK print failed: %s"` with `error->message`.
    GtkPrint {
        /// `error->message`.
        message: &'a [u8],
    },
}

/// The `char**` list `g_proxy_resolver_lookup` returns: its pointer (for `g_strfreev`) and its
/// entries before the terminating `NULL`.
pub type ProxyList = (Ptr, Vec<Vec<u8>>);

/// `wxLogTrace`'s mask in `printing.cpp`: `PRINTING_TRACE`.
pub const PRINTING_TRACE: &str = "KICAD_PRINT";

/// Every effect of the pinned Linux bodies. A native host makes the real calls; the shim's
/// scripted host and the C++ oracle's fakes answer them from a case script.
pub trait UnixHost {
    // ---- wxWidgets process services ------------------------------------------------------

    /// `wxGetEnv( var, value )`: `None` when the variable is unset; otherwise its value
    /// (`*value = getenv( ... )` converted with `wxConvLibc`), which the caller ignores when
    /// it passed a null `value` (`want_value` false).
    fn wx_get_env(&mut self, var: &WxString, want_value: bool) -> Result<Option<WxString>>;

    /// `wxSetEnv( variable, value )`.
    fn wx_set_env(&mut self, variable: &WxString, value: &WxString) -> Result<bool>;

    /// The units a process-configured converter produces for `bytes` (`ToWChar` of a
    /// `const char*` range), or `None` for `wxCONV_FAILED`.
    fn mb_to_wc(&mut self, conv: Conv, bytes: &[u8]) -> Result<Option<Vec<WChar>>>;

    /// The bytes a process-configured converter produces for exactly `units`
    /// (`FromWChar` with an explicit length, NUL units included), or `None` for
    /// `wxCONV_FAILED`.
    fn wc_to_mb(&mut self, conv: Conv, units: &[WChar]) -> Result<Option<Vec<u8>>>;

    /// `towlower( c )` in the current locale, as `wcscasecmp` applies it.
    fn towlower(&mut self, c: WChar) -> Result<WChar>;

    /// `wxOnAssert( __FILE__, __LINE__, __WXFUNCTION__, cond, msg )` of a failed `wxFAIL_MSG`.
    /// The narrow file, function and condition reach the handler as `wxString`s converted with
    /// `wxConvLibc`. `Ok` when the handler returned (the caller continues as the reference
    /// does); an error stops the evaluation there.
    fn on_assert(&mut self, assert: &WxAssert) -> Result<()>;

    /// `wxLogError( _( msgid ), arg )` (from `wxRegEx::Matches`).
    fn wx_log_error(&mut self, msgid: &str, arg: &str) -> Result<()>;

    /// `wxLogTrace( mask, ... )`.
    fn wx_log_trace(&mut self, mask: &str, trace: Trace<'_>) -> Result<()>;

    /// `wxGetActiveWindow()`.
    fn wx_get_active_window(&mut self) -> Result<Option<Ptr>>;

    /// `wxGetTopLevelParent( win )`.
    fn wx_get_top_level_parent(&mut self, win: Option<Ptr>) -> Result<Option<Ptr>>;

    /// `win->GetHandle()` (the window's `GtkWidget*`).
    fn wx_window_get_handle(&mut self, win: Ptr) -> Result<Option<Ptr>>;

    // ---- GLib -----------------------------------------------------------------------------

    /// `g_log_set_writer_func( func, user_data, user_data_free )`. `writer` is the function
    /// the C++ passes (`nullLogWriter`); the user data and its destroy notify are null.
    fn g_log_set_writer_func(&mut self, writer: fn(u32, usize) -> u32) -> Result<()>;

    /// `err->message` of a `GError*` the host returned.
    fn g_error_message(&mut self, error: Ptr) -> Result<Vec<u8>>;

    /// `g_error_free( error )`.
    fn g_error_free(&mut self, error: Ptr) -> Result<()>;

    /// `g_clear_error( &err )` with `err` holding `error`.
    fn g_clear_error(&mut self, error: Option<Ptr>) -> Result<()>;

    /// `g_object_unref( object )`.
    fn g_object_unref(&mut self, object: Ptr) -> Result<()>;

    /// `g_free( mem )`.
    fn g_free(&mut self, mem: Option<Ptr>) -> Result<()>;

    /// `g_strfreev( list )`.
    fn g_strfreev(&mut self, list: Ptr) -> Result<()>;

    /// `g_get_user_data_dir()`.
    fn g_get_user_data_dir(&mut self) -> Result<Option<Vec<u8>>>;

    /// `g_get_user_config_dir()`.
    fn g_get_user_config_dir(&mut self) -> Result<Option<Vec<u8>>>;

    /// `g_get_user_cache_dir()`.
    fn g_get_user_cache_dir(&mut self) -> Result<Option<Vec<u8>>>;

    /// `g_get_home_dir()`.
    fn g_get_home_dir(&mut self) -> Result<Option<Vec<u8>>>;

    /// `g_filename_to_uri( filename, NULL, NULL )`.
    fn g_filename_to_uri(&mut self, filename: &[u8]) -> Result<Option<GStr>>;

    /// `g_signal_connect( instance, signal, handler, &print_data )`
    /// (`g_signal_connect_data( ..., NULL, 0 )`); the handler is the one [`Signal`] names.
    fn g_signal_connect(&mut self, instance: Ptr, signal: Signal) -> Result<u64>;

    // ---- GIO ------------------------------------------------------------------------------

    /// `g_file_new_for_path( path )`.
    fn g_file_new_for_path(&mut self, path: &[u8]) -> Result<Ptr>;

    /// `g_file_new_for_uri( uri )`.
    fn g_file_new_for_uri(&mut self, uri: &[u8]) -> Result<Ptr>;

    /// `g_file_trash( file, NULL, &err )`: the result and the error stored.
    fn g_file_trash(&mut self, file: Ptr) -> Result<(bool, Option<Ptr>)>;

    /// `g_file_find_enclosing_mount( file, NULL, NULL )`.
    fn g_file_find_enclosing_mount(&mut self, file: Ptr) -> Result<Option<Ptr>>;

    /// `g_mount_get_drive( mount )`.
    fn g_mount_get_drive(&mut self, mount: Ptr) -> Result<Option<Ptr>>;

    /// `g_drive_is_removable( drive )`.
    fn g_drive_is_removable(&mut self, drive: Ptr) -> Result<bool>;

    /// `g_file_get_parent( file )`.
    fn g_file_get_parent(&mut self, file: Ptr) -> Result<Option<Ptr>>;

    /// `g_file_get_basename( file )`.
    fn g_file_get_basename(&mut self, file: Ptr) -> Result<Option<GStr>>;

    /// `g_file_get_path( file )`.
    fn g_file_get_path(&mut self, file: Ptr) -> Result<Option<GStr>>;

    /// `g_proxy_resolver_get_default()`.
    fn g_proxy_resolver_get_default(&mut self) -> Result<Option<Ptr>>;

    /// `g_proxy_resolver_lookup( resolver, uri, NULL, &error )`: the `NULL`-terminated list
    /// (its pointer and entries) and the error stored.
    fn g_proxy_resolver_lookup(
        &mut self,
        resolver: Ptr,
        uri: &[u8],
    ) -> Result<(Option<ProxyList>, Option<Ptr>)>;

    // ---- C library ------------------------------------------------------------------------

    /// `syscall( SYS_gettid )`.
    fn syscall_gettid(&mut self) -> Result<i64>;

    /// `setpriority( which, who, prio )`.
    fn setpriority(&mut self, which: i32, who: u32, prio: i32) -> Result<i32>;

    /// `_exit( status )`: the process ends.
    fn exit(&mut self, status: i32) -> !;

    /// `realpath( path, NULL )`: the allocated resolved path (freed with `free`), or `None`.
    fn realpath(&mut self, path: &[u8]) -> Result<Option<GStr>>;

    /// `free( ptr )`.
    fn free(&mut self, ptr: Option<Ptr>) -> Result<()>;

    /// `fopen( path, mode )` (`fopen64` with `_FILE_OFFSET_BITS=64`).
    fn fopen(&mut self, path: &[u8], mode: &[u8]) -> Result<Option<Ptr>>;

    /// `fileno( stream )`.
    fn fileno(&mut self, stream: Ptr) -> Result<i32>;

    /// `posix_fadvise( fd, offset, len, advice )`.
    fn posix_fadvise(&mut self, fd: i32, offset: i64, len: i64, advice: i32) -> Result<i32>;

    /// `fclose( stream )`.
    fn fclose(&mut self, stream: Ptr) -> Result<i32>;

    /// `fflush( stream )`.
    fn fflush(&mut self, stream: Ptr) -> Result<i32>;

    /// `fsync( fd )`.
    fn fsync(&mut self, fd: i32) -> Result<i32>;

    /// `stat( path, &buf )`: the result and the buffer (read only when the result is 0).
    fn stat(&mut self, path: &[u8]) -> Result<(i32, StatBuf)>;

    /// `lstat( path, &buf )`.
    fn lstat(&mut self, path: &[u8]) -> Result<(i32, StatBuf)>;

    /// `chmod( path, mode )`.
    fn chmod(&mut self, path: &[u8], mode: u32) -> Result<i32>;

    /// `opendir( name )`.
    fn opendir(&mut self, name: &[u8]) -> Result<Option<Ptr>>;

    /// `readdir( dir )`: the next entry's `d_name`, or `None` at the end.
    fn readdir(&mut self, dir: Ptr) -> Result<Option<Vec<u8>>>;

    /// `closedir( dir )`.
    fn closedir(&mut self, dir: Ptr) -> Result<i32>;

    /// `readlink( path, buf, bufsiz )`: the result and the bytes it wrote.
    fn readlink(&mut self, path: &[u8], bufsiz: usize) -> Result<(isize, Vec<u8>)>;

    /// `fnmatch( pattern, string, flags )`.
    fn fnmatch(&mut self, pattern: &[u8], string: &[u8], flags: i32) -> Result<i32>;

    /// `open( path, flags )` (`open64`).
    fn open(&mut self, path: &[u8], flags: i32) -> Result<i32>;

    /// `fstat( fd, &buf )`.
    fn fstat(&mut self, fd: i32) -> Result<(i32, StatBuf)>;

    /// `close( fd )`.
    fn close(&mut self, fd: i32) -> Result<i32>;

    /// `mmap( NULL, length, prot, flags, fd, offset )`: `None` is `MAP_FAILED`.
    fn mmap(
        &mut self,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> Result<Option<Mapping>>;

    /// `madvise( addr, length, advice )`.
    fn madvise(&mut self, addr: Ptr, length: usize, advice: i32) -> Result<i32>;

    /// `munmap( addr, length )`.
    fn munmap(&mut self, addr: Ptr, length: usize) -> Result<i32>;

    /// `access( path, mode )`.
    fn access(&mut self, path: &[u8], mode: i32) -> Result<i32>;

    // ---- libsecret ------------------------------------------------------------------------

    /// `secret_password_store_sync( schema, collection, label, password, NULL, &error,
    /// attributes..., NULL )`: the result and the error stored.
    fn secret_password_store_sync(
        &mut self,
        schema: &SecretSchema,
        collection: &[u8],
        label: &[u8],
        password: &[u8],
        attributes: &[Attribute<'_>],
    ) -> Result<(bool, Option<Ptr>)>;

    /// `secret_password_store( schema, collection, label, password, NULL, storeFinished,
    /// user_data, attributes..., NULL )`. The host keeps `pending` and later completes the
    /// call with [`crate::secrets::store_finished`].
    fn secret_password_store(
        &mut self,
        schema: &SecretSchema,
        collection: &[u8],
        label: &[u8],
        password: &[u8],
        attributes: &[Attribute<'_>],
        pending: Box<PendingStore>,
    ) -> Result<()>;

    /// `secret_password_store_finish( result, NULL )`.
    fn secret_password_store_finish(&mut self, result: Ptr) -> Result<bool>;

    /// `secret_password_lookup_sync( schema, NULL, &error, attributes..., NULL )`.
    fn secret_password_lookup_sync(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
    ) -> Result<(Option<GStr>, Option<Ptr>)>;

    /// `secret_password_lookup( schema, NULL, lookupFinished, user_data, attributes...,
    /// NULL )`; completed later with [`crate::secrets::lookup_finished`].
    fn secret_password_lookup(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
        pending: Box<PendingLookup>,
    ) -> Result<()>;

    /// `secret_password_lookup_finish( result, NULL )`.
    fn secret_password_lookup_finish(&mut self, result: Ptr) -> Result<Option<GStr>>;

    /// `secret_attributes_build( schema, attributes..., NULL )`.
    fn secret_attributes_build(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
    ) -> Result<Ptr>;

    /// `secret_service_search_sync( NULL, schema, attributes, flags, NULL, &error )`.
    fn secret_service_search_sync(
        &mut self,
        schema: &SecretSchema,
        attributes: Ptr,
        flags: i32,
    ) -> Result<(List, Option<Ptr>)>;

    /// `g_hash_table_unref( table )`.
    fn g_hash_table_unref(&mut self, table: Ptr) -> Result<()>;

    /// `secret_item_get_locked( item )`.
    fn secret_item_get_locked(&mut self, item: Ptr) -> Result<bool>;

    /// `secret_item_get_secret( item )`.
    fn secret_item_get_secret(&mut self, item: Ptr) -> Result<Option<Ptr>>;

    /// `secret_value_get_text( value )`.
    fn secret_value_get_text(&mut self, value: Ptr) -> Result<Option<Vec<u8>>>;

    /// `secret_value_unref( value )`.
    fn secret_value_unref(&mut self, value: Ptr) -> Result<()>;

    /// `g_list_free_full( list, g_object_unref )`: the host frees the nodes and calls
    /// `g_object_unref` on each element in order.
    fn g_list_free_full_unref(&mut self, list: &List) -> Result<()>;

    /// `secret_password_clear_sync( schema, NULL, &error, attributes..., NULL )`.
    fn secret_password_clear_sync(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
    ) -> Result<(bool, Option<Ptr>)>;

    // ---- poppler, GTK printing and cairo --------------------------------------------------

    /// `poppler_document_new_from_file( uri, NULL, &error )`.
    fn poppler_document_new_from_file(&mut self, uri: &[u8]) -> Result<(Option<Ptr>, Option<Ptr>)>;

    /// `poppler_document_get_n_pages( document )`.
    fn poppler_document_get_n_pages(&mut self, document: Ptr) -> Result<i32>;

    /// `poppler_document_get_page( document, index )`.
    fn poppler_document_get_page(&mut self, document: Ptr, index: i32) -> Result<Option<Ptr>>;

    /// `poppler_page_get_size( page, &width, &height )`.
    fn poppler_page_get_size(&mut self, page: Ptr) -> Result<(f64, f64)>;

    /// `poppler_page_render( page, cairo )`.
    fn poppler_page_render(&mut self, page: Ptr, cairo: Ptr) -> Result<()>;

    /// `gtk_print_operation_new()`.
    fn gtk_print_operation_new(&mut self) -> Result<Option<Ptr>>;

    /// `gtk_print_operation_set_use_full_page( op, full_page )`.
    fn gtk_print_operation_set_use_full_page(&mut self, op: Ptr, full_page: bool) -> Result<()>;

    /// `gtk_print_operation_set_unit( op, unit )`.
    fn gtk_print_operation_set_unit(&mut self, op: Ptr, unit: i32) -> Result<()>;

    /// `gtk_print_operation_set_print_settings( op, settings )`.
    fn gtk_print_operation_set_print_settings(&mut self, op: Ptr, settings: Ptr) -> Result<()>;

    /// `gtk_print_operation_set_job_name( op, job_name )`.
    fn gtk_print_operation_set_job_name(&mut self, op: Ptr, job_name: &[u8]) -> Result<()>;

    /// `gtk_print_operation_set_current_page( op, current_page )`.
    fn gtk_print_operation_set_current_page(&mut self, op: Ptr, current_page: i32) -> Result<()>;

    /// `gtk_print_operation_run( op, action, parent, &error )`, during which the host emits
    /// the connected signals through `signals`: the result and the error stored.
    fn gtk_print_operation_run(
        &mut self,
        op: Ptr,
        action: i32,
        parent: Option<Ptr>,
        signals: &mut dyn PrintSignals<Self>,
    ) -> Result<(i32, Option<Ptr>)>;

    /// `gtk_print_operation_get_print_settings( op )`.
    fn gtk_print_operation_get_print_settings(&mut self, op: Ptr) -> Result<Option<Ptr>>;

    /// `gtk_print_operation_cancel( op )`.
    fn gtk_print_operation_cancel(&mut self, op: Ptr) -> Result<()>;

    /// `gtk_print_operation_set_n_pages( op, n_pages )`.
    fn gtk_print_operation_set_n_pages(&mut self, op: Ptr, n_pages: i32) -> Result<()>;

    /// `gtk_print_settings_new()`.
    fn gtk_print_settings_new(&mut self) -> Result<Ptr>;

    /// `gtk_print_settings_copy( other )`.
    fn gtk_print_settings_copy(&mut self, other: Ptr) -> Result<Ptr>;

    /// `gtk_print_settings_get( settings, key )`.
    fn gtk_print_settings_get(&mut self, settings: Ptr, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// `gtk_print_settings_set( settings, key, value )`.
    fn gtk_print_settings_set(&mut self, settings: Ptr, key: &[u8], value: &[u8]) -> Result<()>;

    /// `gtk_print_settings_unset( settings, key )`.
    fn gtk_print_settings_unset(&mut self, settings: Ptr, key: &[u8]) -> Result<()>;

    /// `gtk_print_settings_set_print_pages( settings, pages )`.
    fn gtk_print_settings_set_print_pages(&mut self, settings: Ptr, pages: i32) -> Result<()>;

    /// `gtk_print_context_get_cairo_context( context )`.
    fn gtk_print_context_get_cairo_context(&mut self, context: Ptr) -> Result<Option<Ptr>>;

    /// `gtk_print_context_get_width( context )`.
    fn gtk_print_context_get_width(&mut self, context: Ptr) -> Result<f64>;

    /// `gtk_print_context_get_height( context )`.
    fn gtk_print_context_get_height(&mut self, context: Ptr) -> Result<f64>;

    /// `gtk_page_setup_set_orientation( setup, orientation )`.
    fn gtk_page_setup_set_orientation(&mut self, setup: Ptr, orientation: i32) -> Result<()>;

    /// `gtk_widget_get_toplevel( widget )`.
    fn gtk_widget_get_toplevel(&mut self, widget: Ptr) -> Result<Option<Ptr>>;

    /// `gtk_widget_is_toplevel( widget )`.
    fn gtk_widget_is_toplevel(&mut self, widget: Option<Ptr>) -> Result<bool>;

    /// `cairo_save( cr )`.
    fn cairo_save(&mut self, cr: Ptr) -> Result<()>;

    /// `cairo_restore( cr )`.
    fn cairo_restore(&mut self, cr: Ptr) -> Result<()>;

    /// `cairo_translate( cr, tx, ty )`.
    fn cairo_translate(&mut self, cr: Ptr, tx: f64, ty: f64) -> Result<()>;

    /// `cairo_scale( cr, sx, sy )`.
    fn cairo_scale(&mut self, cr: Ptr, sx: f64, sy: f64) -> Result<()>;

    /// `cairo_set_source_rgb( cr, red, green, blue )`.
    fn cairo_set_source_rgb(&mut self, cr: Ptr, red: f64, green: f64, blue: f64) -> Result<()>;

    /// `cairo_paint( cr )`.
    fn cairo_paint(&mut self, cr: Ptr) -> Result<()>;
}
