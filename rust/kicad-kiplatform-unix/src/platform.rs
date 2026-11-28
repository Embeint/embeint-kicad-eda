// SPDX-License-Identifier: GPL-3.0-or-later
//! [`UnixPlatform`]: the Linux bodies behind the accepted header traits.

use alloc::vec::Vec;
use core::convert::Infallible;

use kicad_kiplatform::app::AppPlatform;
use kicad_kiplatform::environment::{EnvPlatform, ProxyConfig};
use kicad_kiplatform::io::{
    FileLock, FileLockState, IoPlatform, MappedFile, RuntimeError, TargetAttrs,
};
use kicad_kiplatform::owners::OS_COMMON_UI;
use kicad_kiplatform::printing::{PrintResult, PrintingPlatform};
use kicad_kiplatform::secrets::{GetDone, SecretsPlatform, StoreDone};
use kicad_map_helpers::WxString;

use crate::environment::{BuildConfig, REFERENCE_BUILD};
use crate::host::UnixHost;
use crate::printing::{Printing, WxPrintData};
use crate::{app, environment, io, printing, secrets, Ptr, Result, UnixError};

/// The Linux platform over a host: the host, the wxWidgets build `ENV::Init` is compiled
/// against, and `printing.cpp`'s `s_lastSettings`.
pub struct UnixPlatform<H> {
    /// The effects.
    pub host: H,
    /// The reference build configuration.
    pub build: BuildConfig,
    /// `printing.cpp`'s translation-unit state.
    pub printing: Printing,
}

impl<H: UnixHost> UnixPlatform<H> {
    /// A platform with the process state the reference starts with (`s_lastSettings` null),
    /// for the pinned reference build.
    pub fn new(host: H) -> Self {
        Self {
            host,
            build: REFERENCE_BUILD,
            printing: Printing::default(),
        }
    }

    /// `PRINT_RESULT PrintPDF( const std::string& aFile, bool fit_to_page, int aCurrentPage )`,
    /// the overload `printing.cpp` defines but `printing.h` does not declare.
    pub fn print_pdf_fit(
        &mut self,
        file: &[u8],
        fit_to_page: bool,
        current_page: i32,
    ) -> Result<PrintResult> {
        self.printing
            .print_pdf_fit(&mut self.host, file, fit_to_page, current_page)
    }
}

/// A member of `os/common/io.cpp`, owned by [`OS_COMMON_UI`].
fn common_io<T>(upstream: &'static str) -> Result<T> {
    Err(UnixError::Unported {
        upstream,
        unit: OS_COMMON_UI,
    })
}

impl<H: UnixHost> AppPlatform for UnixPlatform<H> {
    type Window = Ptr;
    type Error = UnixError;

    fn init(&mut self) -> Result<bool> {
        app::init(&mut self.host)
    }
    fn enable_dark_mode(&mut self, force: bool) -> Result<()> {
        app::enable_dark_mode(force);
        Ok(())
    }
    fn attach_console(&mut self, try_alloc: bool) -> Result<bool> {
        Ok(app::attach_console(try_alloc))
    }
    fn is_operating_system_unsupported(&mut self) -> Result<bool> {
        Ok(app::is_operating_system_unsupported())
    }
    fn register_application_restart(&mut self, command_line: &WxString) -> Result<bool> {
        Ok(app::register_application_restart(command_line))
    }
    fn unregister_application_restart(&mut self) -> Result<bool> {
        Ok(app::unregister_application_restart())
    }
    fn supports_shutdown_block_reason(&mut self) -> Result<bool> {
        Ok(app::supports_shutdown_block_reason())
    }
    /// An empty body.
    fn set_shutdown_block_reason(
        &mut self,
        _window: Option<&Ptr>,
        _reason: &WxString,
    ) -> Result<()> {
        Ok(())
    }
    /// An empty body.
    fn remove_shutdown_block_reason(&mut self, _window: Option<&Ptr>) -> Result<()> {
        Ok(())
    }
    /// An empty body.
    fn force_timer_messages_to_be_created_if_necessary(&mut self) -> Result<()> {
        Ok(())
    }
    /// An empty body.
    fn add_dynamic_library_search_path(&mut self, _path: &WxString) -> Result<()> {
        Ok(())
    }
    fn set_current_thread_priority(&mut self, raise: bool) -> Result<bool> {
        app::set_current_thread_priority(&mut self.host, raise)
    }
    fn terminate_with_code(&mut self, exit_code: i32) -> Result<Infallible> {
        app::terminate_with_code(&mut self.host, exit_code)
    }
}

impl<H: UnixHost> EnvPlatform for UnixPlatform<H> {
    type Window = Ptr;
    type Error = UnixError;

    fn init(&mut self) -> Result<()> {
        let build = self.build;
        environment::init(&mut self.host, &build)
    }
    fn move_to_trash(&mut self, path: &WxString, error: &mut WxString) -> Result<bool> {
        environment::move_to_trash(&mut self.host, path, error)
    }
    fn is_network_path(&mut self, path: &WxString) -> Result<bool> {
        Ok(environment::is_network_path(path))
    }
    fn is_removable_path(&mut self, path: &WxString) -> Result<bool> {
        environment::is_removable_path(&mut self.host, path)
    }
    fn get_documents_path(&mut self) -> Result<WxString> {
        environment::get_documents_path(&mut self.host)
    }
    fn get_user_config_path(&mut self) -> Result<WxString> {
        environment::get_user_config_path(&mut self.host)
    }
    fn get_user_data_path(&mut self) -> Result<WxString> {
        environment::get_user_data_path(&mut self.host)
    }
    fn get_user_local_data_path(&mut self) -> Result<WxString> {
        environment::get_user_local_data_path(&mut self.host)
    }
    fn get_user_cache_path(&mut self) -> Result<WxString> {
        environment::get_user_cache_path(&mut self.host)
    }
    fn get_system_proxy_config(&mut self, url: &WxString, cfg: &mut ProxyConfig) -> Result<bool> {
        environment::get_system_proxy_config(&mut self.host, url, cfg)
    }
    fn verify_file_signature(&mut self, path: &WxString) -> Result<bool> {
        Ok(environment::verify_file_signature(path))
    }
    fn get_app_user_model_id(&mut self) -> Result<WxString> {
        Ok(environment::get_app_user_model_id())
    }
    /// An empty body.
    fn set_app_details_for_window(
        &mut self,
        _window: Option<&Ptr>,
        _relaunch_command: &WxString,
        _relaunch_display_name: &WxString,
    ) -> Result<()> {
        Ok(())
    }
    fn get_command_line_str(&mut self) -> Result<WxString> {
        Ok(environment::get_command_line_str())
    }
    /// An empty body.
    fn add_to_recent_docs(&mut self, _path: &WxString) -> Result<()> {
        Ok(())
    }
}

impl<H: UnixHost> IoPlatform for UnixPlatform<H> {
    type File = Ptr;
    /// `wxFileName&`; the only Unix use (`LongPathAdjustment`) does nothing with it.
    type FileName = WxString;
    type Error = UnixError;

    fn seq_f_open(&mut self, path: &WxString, mode: &WxString) -> Result<Option<Ptr>> {
        io::seq_f_open(&mut self.host, path, mode)
    }
    fn duplicate_permissions(&mut self, src: &WxString, dest: &WxString) -> Result<bool> {
        io::duplicate_permissions(&mut self.host, src, dest)
    }
    fn make_writeable(&mut self, file_path: &WxString) -> Result<bool> {
        io::make_writeable(&mut self.host, file_path)
    }
    fn is_file_hidden(&mut self, file_name: &WxString) -> Result<bool> {
        io::is_file_hidden(file_name)
    }
    /// A no-op on this platform.
    fn long_path_adjustment(&mut self, _filename: &mut WxString) -> Result<()> {
        Ok(())
    }
    fn timestamp_dir(&mut self, dir_path: &WxString, filespec: &WxString) -> Result<i64> {
        io::timestamp_dir(&mut self.host, dir_path, filespec)
    }
    fn flush_to_disk(&mut self, fp: Option<&mut Ptr>) -> Result<bool> {
        io::flush_to_disk(&mut self.host, fp.map(|fp| *fp))
    }
    fn flush_directory(&mut self, _dir_path: &WxString) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FlushDirectory
        common_io("KIPLATFORM::IO::FlushDirectory")
    }
    fn atomic_rename(
        &mut self,
        _src: &WxString,
        _dst: &WxString,
        _error: Option<&mut WxString>,
    ) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::AtomicRename
        common_io("KIPLATFORM::IO::AtomicRename")
    }
    fn make_sibling_temp_path(&mut self, _target_path: &WxString) -> Result<WxString> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::MakeSiblingTempPath
        common_io("KIPLATFORM::IO::MakeSiblingTempPath")
    }
    fn open_unique_sibling_temp_file(
        &mut self,
        _target_path: &WxString,
        _mode: &WxString,
        _temp_path_out: Option<&mut WxString>,
        _error: Option<&mut WxString>,
    ) -> Result<Option<Ptr>> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::OpenUniqueSiblingTempFile
        common_io("KIPLATFORM::IO::OpenUniqueSiblingTempFile")
    }
    fn resolve_symlink_target(&mut self, _path: &WxString) -> Result<WxString> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::ResolveSymlinkTarget
        common_io("KIPLATFORM::IO::ResolveSymlinkTarget")
    }
    fn capture_target_attributes(&mut self, path: &WxString) -> Result<TargetAttrs> {
        Ok(io::capture_target_attributes(path))
    }
    fn apply_target_attributes(&mut self, path: &WxString, attrs: &TargetAttrs) -> Result<bool> {
        Ok(io::apply_target_attributes(path, attrs))
    }
    fn commit_temp_file(
        &mut self,
        _temp_path: &WxString,
        _target_path: &WxString,
        _error: Option<&mut WxString>,
    ) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::CommitTempFile
        common_io("KIPLATFORM::IO::CommitTempFile")
    }
    fn atomic_write_file(
        &mut self,
        _target_path: &WxString,
        _data: &[u8],
        _error: Option<&mut WxString>,
    ) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::AtomicWriteFile
        common_io("KIPLATFORM::IO::AtomicWriteFile")
    }
    fn mapped_file_open(
        &mut self,
        file_name: &WxString,
    ) -> Result<core::result::Result<MappedFile, RuntimeError>> {
        io::mapped_file_open(&mut self.host, file_name)
    }
    fn mapped_file_close(&mut self, file: MappedFile) -> Result<()> {
        io::mapped_file_close(&mut self.host, file)
    }
    fn file_lock_acquire(
        &mut self,
        _lock: &mut FileLock,
        _path: &WxString,
        _created: &mut bool,
    ) -> Result<FileLockState> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::Acquire
        common_io("KIPLATFORM::IO::FILE_LOCK::Acquire")
    }
    fn file_lock_open_for_inspect(
        &mut self,
        _lock: &mut FileLock,
        _path: &WxString,
        _held_by_another: &mut bool,
    ) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::OpenForInspect
        common_io("KIPLATFORM::IO::FILE_LOCK::OpenForInspect")
    }
    fn file_lock_is_open(&self, _lock: &FileLock) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::IsOpen
        common_io("KIPLATFORM::IO::FILE_LOCK::IsOpen")
    }
    fn file_lock_read_all(&self, _lock: &FileLock, _contents: &mut Vec<u8>) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::ReadAll
        common_io("KIPLATFORM::IO::FILE_LOCK::ReadAll")
    }
    fn file_lock_rewrite(&mut self, _lock: &mut FileLock, _contents: &[u8]) -> Result<bool> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::Rewrite
        common_io("KIPLATFORM::IO::FILE_LOCK::Rewrite")
    }
    fn file_lock_release(&mut self, _lock: &mut FileLock) -> Result<()> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::Release
        common_io("KIPLATFORM::IO::FILE_LOCK::Release")
    }
    fn file_lock_destroy(&mut self, _lock: FileLock) -> Result<()> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::~FILE_LOCK
        common_io("KIPLATFORM::IO::FILE_LOCK::~FILE_LOCK")
    }
    fn file_lock_move_construct(&mut self, _other: &mut FileLock) -> Result<FileLock> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::FILE_LOCK(FILE_LOCK&&)
        common_io("KIPLATFORM::IO::FILE_LOCK::FILE_LOCK(FILE_LOCK&&)")
    }
    fn file_lock_move_assign(&mut self, _lock: &mut FileLock, _other: &mut FileLock) -> Result<()> {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001): KIPLATFORM::IO::FILE_LOCK::operator=(FILE_LOCK&&)
        common_io("KIPLATFORM::IO::FILE_LOCK::operator=(FILE_LOCK&&)")
    }
}

impl<H: UnixHost> SecretsPlatform for UnixPlatform<H> {
    type Error = UnixError;

    fn store_secret(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &WxString,
    ) -> Result<bool> {
        secrets::store_secret(&mut self.host, service, key, secret)
    }
    fn store_secret_async(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &WxString,
        done: StoreDone,
    ) -> Result<()> {
        secrets::store_secret_async(&mut self.host, service, key, secret, done)
    }
    fn get_secret(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &mut WxString,
    ) -> Result<bool> {
        secrets::get_secret(&mut self.host, service, key, secret)
    }
    fn get_secret_async(
        &mut self,
        service: &WxString,
        key: &WxString,
        done: GetDone,
    ) -> Result<()> {
        secrets::get_secret_async(&mut self.host, service, key, done)
    }
    fn get_secret_no_prompt(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &mut WxString,
    ) -> Result<bool> {
        secrets::get_secret_no_prompt(&mut self.host, service, key, secret)
    }
    fn delete_secret(&mut self, service: &WxString, key: &WxString) -> Result<bool> {
        secrets::delete_secret(&mut self.host, service, key)
    }
}

impl<H: UnixHost> PrintingPlatform for UnixPlatform<H> {
    type PrintData = WxPrintData;
    type Error = UnixError;

    fn print_pdf(&mut self, file: &[u8], current_page: i32) -> Result<PrintResult> {
        self.printing.print_pdf(&mut self.host, file, current_page)
    }
    fn reset_print_to_file_path(&mut self, data: &mut WxPrintData) -> Result<()> {
        printing::reset_print_to_file_path(&mut self.host, data)
    }
}
