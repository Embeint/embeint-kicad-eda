// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::IO` from `libs/kiplatform/include/kiplatform/io.h`.
//!
//! Ported from the header itself: [`CLOUD_SYNC_BUFFER_SIZE`], [`TargetAttrs`] and its
//! default member initializers, [`FileLockState`], the defaulted `FILE_LOCK()` constructor
//! ([`FileLock::new`]) with the POSIX members it initializes, the inline
//! `MAPPED_FILE::Data()` and `MAPPED_FILE::Size()` ([`MappedFile::data`],
//! [`MappedFile::size`]) over the POSIX members, and the `aError = nullptr` default
//! arguments ([`IoPlatform`]'s `*_without_error` methods).
//!
//! The other declarations are the host boundary [`IoPlatform`]: their Linux bodies are in
//! `libs/kiplatform/os/unix/io.cpp` ([`crate::owners::OS_UNIX_UI`]) and
//! `libs/kiplatform/os/common/io.cpp` ([`crate::owners::OS_COMMON_UI`]). The `_WIN32` members
//! (`MAPPED_FILE::m_fileHandle`, `m_mapHandle`, `FILE_LOCK::m_handle`) are not part of the
//! Linux reference and are not modelled; they belong to [`crate::owners::OS_WINDOWS_UI`].

use alloc::vec::Vec;
use kicad_map_helpers::WxString;

/// `static constexpr size_t CLOUD_SYNC_BUFFER_SIZE = 512 * 1024`.
pub const CLOUD_SYNC_BUFFER_SIZE: usize = 512 * 1024;

/// `KIPLATFORM::IO::TARGET_ATTRS`: `std::uint32_t value = 0; bool captured = false;`.
///
/// The default member initializers make `TARGET_ATTRS a;`, `TARGET_ATTRS{}` and every
/// member omitted from a brace list take these values, which [`Default`] gives
/// (`TARGET_ATTRS{ v }` is `TargetAttrs { value: v, ..Default::default() }`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TargetAttrs {
    /// `std::uint32_t value`.
    pub value: u32,
    /// `bool captured`.
    pub captured: bool,
}

/// `KIPLATFORM::IO::FILE_LOCK::STATE` (`enum class`, underlying type `int`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FileLockState {
    /// `NONE`: no file is open.
    None = 0,
    /// `HELD`: we hold the lock.
    Held = 1,
    /// `BUSY`: another process holds the lock.
    Busy = 2,
    /// `UNSUPPORTED`: the file is open but the filesystem cannot answer.
    Unsupported = 3,
}

impl FileLockState {
    /// The enumerator's value, `static_cast<int>( state )`.
    pub const fn value(self) -> i32 {
        self as i32
    }

    /// The enumerator with value `value`, if there is one.
    pub const fn from_value(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Held),
            2 => Some(Self::Busy),
            3 => Some(Self::Unsupported),
            _ => None,
        }
    }
}

/// `KIPLATFORM::IO::FILE_LOCK` with its POSIX members `int m_fd = -1;` and
/// `STATE m_state = STATE::NONE;`.
///
/// Only the defaulted constructor is defined in the header. The member functions, the
/// destructor (which calls `Release()`) and the move operations are host operations in
/// [`IoPlatform`]; this type therefore has no `Drop` and no `Clone` (the C++ copy
/// operations are deleted). A host that opened a descriptor must release it through
/// [`IoPlatform::file_lock_destroy`].
#[derive(Debug, PartialEq, Eq)]
pub struct FileLock {
    fd: i32,
    state: FileLockState,
}

impl FileLock {
    /// `FILE_LOCK() = default`: the default member initializers, `m_fd = -1` and
    /// `m_state = STATE::NONE`.
    pub const fn new() -> Self {
        Self {
            fd: -1,
            state: FileLockState::None,
        }
    }

    /// The private member `m_fd`.
    pub const fn fd(&self) -> i32 {
        self.fd
    }

    /// The private member `m_state`.
    pub const fn state(&self) -> FileLockState {
        self.state
    }

    /// Store `m_fd` and `m_state`, as the implementing member functions do.
    pub fn set_members(&mut self, fd: i32, state: FileLockState) {
        self.fd = fd;
        self.state = state;
    }
}

impl Default for FileLock {
    /// `FILE_LOCK()` is the default constructor.
    fn default() -> Self {
        Self::new()
    }
}

/// What `MAPPED_FILE::m_data` points at.
#[derive(Debug, PartialEq, Eq)]
pub enum MappedData {
    /// `nullptr`, the default member initializer (and the value for an empty file).
    Null,
    /// The host's read-only mapping of the file (`mmap`), whose bytes the host supplies.
    Mapping(Vec<u8>),
    /// `m_fallbackBuffer.data()`.
    FallbackBuffer,
}

/// `KIPLATFORM::IO::MAPPED_FILE` with its POSIX members: `const uint8_t* m_data = nullptr;`,
/// `size_t m_size = 0;`, `bool m_isMapped = false;` and `std::vector<uint8_t>
/// m_fallbackBuffer;`.
///
/// The constructor (open, `fstat`, `mmap`, or `readIntoBuffer`) and destructor (`munmap`)
/// are host operations ([`IoPlatform::mapped_file_open`], [`IoPlatform::mapped_file_close`]);
/// they build the members with [`MappedFile::from_members`]. Copying is deleted in C++, so
/// this type has no `Clone`.
#[derive(Debug, PartialEq, Eq)]
pub struct MappedFile {
    data: MappedData,
    size: usize,
    is_mapped: bool,
    fallback_buffer: Vec<u8>,
}

impl MappedFile {
    /// The members as the constructor leaves them.
    pub fn from_members(
        data: MappedData,
        size: usize,
        is_mapped: bool,
        fallback_buffer: Vec<u8>,
    ) -> Self {
        Self {
            data,
            size,
            is_mapped,
            fallback_buffer,
        }
    }

    /// `const uint8_t* Data() const { return m_data; }`.
    ///
    /// `None` is a null pointer; otherwise the bytes `m_data` points at, to the end of the
    /// buffer that holds them. `m_fallbackBuffer.data()` is null while the vector has never
    /// allocated, which with libstdc++ is the case for an empty fallback buffer (it is
    /// resized once, from empty), so an empty fallback buffer gives `None`.
    pub fn data(&self) -> Option<&[u8]> {
        match &self.data {
            MappedData::Null => None,
            MappedData::Mapping(bytes) => Some(bytes),
            MappedData::FallbackBuffer if self.fallback_buffer.is_empty() => None,
            MappedData::FallbackBuffer => Some(&self.fallback_buffer),
        }
    }

    /// `size_t Size() const { return m_size; }`.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// What `m_data` points at.
    pub const fn data_member(&self) -> &MappedData {
        &self.data
    }

    /// The private member `m_isMapped`.
    pub const fn is_mapped(&self) -> bool {
        self.is_mapped
    }

    /// The private member `m_fallbackBuffer`.
    pub fn fallback_buffer(&self) -> &[u8] {
        &self.fallback_buffer
    }
}

/// A `std::runtime_error` thrown by a host operation; `what` is its message bytes.
#[derive(Debug, PartialEq, Eq)]
pub struct RuntimeError {
    /// `what()`, without the terminating NUL.
    pub what: Vec<u8>,
}

/// The `KIPLATFORM::IO` functions and the out-of-line `MAPPED_FILE` and `FILE_LOCK` members,
/// implemented by the native host.
///
/// Pointer parameters are `Option`s (`None` is a null pointer); `FILE*` and `wxFileName`
/// are host handles. The `*_without_error` methods are the declarations' default argument
/// `wxString* aError = nullptr`.
pub trait IoPlatform {
    /// Host handle for a C `FILE`.
    type File;
    /// Host handle for a `wxFileName`.
    type FileName;

    /// `FILE* SeqFOpen( const wxString& aPath, const wxString& mode )`
    /// (`os/unix/io.cpp`).
    fn seq_f_open(&mut self, path: &WxString, mode: &WxString) -> Option<Self::File>;

    /// `bool DuplicatePermissions( const wxString& aSrc, const wxString& aDest )`
    /// (`os/unix/io.cpp`).
    fn duplicate_permissions(&mut self, src: &WxString, dest: &WxString) -> bool;

    /// `bool MakeWriteable( const wxString& aFilePath )` (`os/unix/io.cpp`).
    fn make_writeable(&mut self, file_path: &WxString) -> bool;

    /// `bool IsFileHidden( const wxString& aFileName )` (`os/unix/io.cpp`).
    fn is_file_hidden(&mut self, file_name: &WxString) -> bool;

    /// `void LongPathAdjustment( wxFileName& aFilename )` (`os/unix/io.cpp`).
    fn long_path_adjustment(&mut self, filename: &mut Self::FileName);

    /// `long long TimestampDir( const wxString& aDirPath, const wxString& aFilespec )`
    /// (`os/unix/io.cpp`).
    fn timestamp_dir(&mut self, dir_path: &WxString, filespec: &WxString) -> i64;

    /// `bool FlushToDisk( FILE* aFp )` (`os/unix/io.cpp`).
    fn flush_to_disk(&mut self, fp: Option<&mut Self::File>) -> bool;

    /// `bool FlushDirectory( const wxString& aDirPath )` (`os/common/io.cpp`).
    fn flush_directory(&mut self, dir_path: &WxString) -> bool;

    /// `bool AtomicRename( const wxString& aSrc, const wxString& aDst, wxString* aError )`
    /// (`os/common/io.cpp`).
    fn atomic_rename(
        &mut self,
        src: &WxString,
        dst: &WxString,
        error: Option<&mut WxString>,
    ) -> bool;

    /// `AtomicRename( aSrc, aDst )`: the default argument `aError = nullptr`.
    fn atomic_rename_without_error(&mut self, src: &WxString, dst: &WxString) -> bool {
        self.atomic_rename(src, dst, None)
    }

    /// `wxString MakeSiblingTempPath( const wxString& aTargetPath )` (`os/common/io.cpp`).
    fn make_sibling_temp_path(&mut self, target_path: &WxString) -> WxString;

    /// `FILE* OpenUniqueSiblingTempFile( const wxString& aTargetPath, const wxString& aMode,
    /// wxString* aTempPathOut, wxString* aError )` (`os/common/io.cpp`).
    fn open_unique_sibling_temp_file(
        &mut self,
        target_path: &WxString,
        mode: &WxString,
        temp_path_out: Option<&mut WxString>,
        error: Option<&mut WxString>,
    ) -> Option<Self::File>;

    /// `OpenUniqueSiblingTempFile( aTargetPath, aMode, aTempPathOut )`: the default
    /// argument `aError = nullptr`.
    fn open_unique_sibling_temp_file_without_error(
        &mut self,
        target_path: &WxString,
        mode: &WxString,
        temp_path_out: Option<&mut WxString>,
    ) -> Option<Self::File> {
        self.open_unique_sibling_temp_file(target_path, mode, temp_path_out, None)
    }

    /// `wxString ResolveSymlinkTarget( const wxString& aPath )` (`os/common/io.cpp`).
    fn resolve_symlink_target(&mut self, path: &WxString) -> WxString;

    /// `TARGET_ATTRS CaptureTargetAttributes( const wxString& aPath )` (`os/unix/io.cpp`).
    fn capture_target_attributes(&mut self, path: &WxString) -> TargetAttrs;

    /// `bool ApplyTargetAttributes( const wxString& aPath, const TARGET_ATTRS& aAttrs )`
    /// (`os/unix/io.cpp`).
    fn apply_target_attributes(&mut self, path: &WxString, attrs: &TargetAttrs) -> bool;

    /// `bool CommitTempFile( const wxString& aTempPath, const wxString& aTargetPath,
    /// wxString* aError )` (`os/common/io.cpp`).
    fn commit_temp_file(
        &mut self,
        temp_path: &WxString,
        target_path: &WxString,
        error: Option<&mut WxString>,
    ) -> bool;

    /// `CommitTempFile( aTempPath, aTargetPath )`: the default argument `aError = nullptr`.
    fn commit_temp_file_without_error(
        &mut self,
        temp_path: &WxString,
        target_path: &WxString,
    ) -> bool {
        self.commit_temp_file(temp_path, target_path, None)
    }

    /// `bool AtomicWriteFile( const wxString& aTargetPath, const void* aData, size_t aSize,
    /// wxString* aError )` (`os/common/io.cpp`); `data` is the `aSize` bytes at `aData`.
    fn atomic_write_file(
        &mut self,
        target_path: &WxString,
        data: &[u8],
        error: Option<&mut WxString>,
    ) -> bool;

    /// `AtomicWriteFile( aTargetPath, aData, aSize )`: the default argument
    /// `aError = nullptr`.
    fn atomic_write_file_without_error(&mut self, target_path: &WxString, data: &[u8]) -> bool {
        self.atomic_write_file(target_path, data, None)
    }

    /// `explicit MAPPED_FILE( const wxString& aFileName )` (`os/unix/io.cpp`, with
    /// `readIntoBuffer` in `os/common/io.cpp`); a thrown `std::runtime_error` is `Err`.
    fn mapped_file_open(&mut self, file_name: &WxString) -> Result<MappedFile, RuntimeError>;

    /// `~MAPPED_FILE()` (`os/unix/io.cpp`).
    fn mapped_file_close(&mut self, file: MappedFile);

    /// `STATE FILE_LOCK::Acquire( const wxString& aPath, bool& aCreated )`
    /// (`os/common/io.cpp`).
    fn file_lock_acquire(
        &mut self,
        lock: &mut FileLock,
        path: &WxString,
        created: &mut bool,
    ) -> FileLockState;

    /// `bool FILE_LOCK::OpenForInspect( const wxString& aPath, bool& aHeldByAnother )`
    /// (`os/common/io.cpp`).
    fn file_lock_open_for_inspect(
        &mut self,
        lock: &mut FileLock,
        path: &WxString,
        held_by_another: &mut bool,
    ) -> bool;

    /// `bool FILE_LOCK::IsOpen() const` (`os/common/io.cpp`).
    fn file_lock_is_open(&self, lock: &FileLock) -> bool;

    /// `bool FILE_LOCK::ReadAll( std::string& aContents ) const` (`os/common/io.cpp`).
    fn file_lock_read_all(&self, lock: &FileLock, contents: &mut Vec<u8>) -> bool;

    /// `bool FILE_LOCK::Rewrite( const std::string& aContents )` (`os/common/io.cpp`).
    fn file_lock_rewrite(&mut self, lock: &mut FileLock, contents: &[u8]) -> bool;

    /// `void FILE_LOCK::Release()` (`os/common/io.cpp`).
    fn file_lock_release(&mut self, lock: &mut FileLock);

    /// `~FILE_LOCK()` (`os/common/io.cpp`).
    fn file_lock_destroy(&mut self, lock: FileLock);

    /// `FILE_LOCK( FILE_LOCK&& aOther ) noexcept` (`os/common/io.cpp`).
    fn file_lock_move_construct(&mut self, other: &mut FileLock) -> FileLock;

    /// `FILE_LOCK& operator=( FILE_LOCK&& aOther ) noexcept` (`os/common/io.cpp`). The
    /// reference's self-assignment branch cannot be reached through two distinct `&mut`.
    fn file_lock_move_assign(&mut self, lock: &mut FileLock, other: &mut FileLock);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn header_values() {
        assert_eq!(CLOUD_SYNC_BUFFER_SIZE, 524_288);
        assert_eq!(
            TargetAttrs::default(),
            TargetAttrs {
                value: 0,
                captured: false
            }
        );
        let lock = FileLock::new();
        assert_eq!((lock.fd(), lock.state()), (-1, FileLockState::None));
        for (value, state) in [
            (0, FileLockState::None),
            (1, FileLockState::Held),
            (2, FileLockState::Busy),
            (3, FileLockState::Unsupported),
        ] {
            assert_eq!(state.value(), value);
            assert_eq!(FileLockState::from_value(value), Some(state));
        }
        assert_eq!(FileLockState::from_value(4), None);
    }

    #[test]
    fn mapped_file_accessors() {
        let empty = MappedFile::from_members(MappedData::Null, 0, false, Vec::new());
        assert_eq!((empty.data(), empty.size()), (None, 0));
        let mapped = MappedFile::from_members(MappedData::Mapping(vec![1, 2]), 2, true, Vec::new());
        assert_eq!((mapped.data(), mapped.size()), (Some(&[1u8, 2][..]), 2));
        let fallback = MappedFile::from_members(MappedData::FallbackBuffer, 1, false, vec![9]);
        assert_eq!((fallback.data(), fallback.size()), (Some(&[9u8][..]), 1));
        let none = MappedFile::from_members(MappedData::FallbackBuffer, 0, false, Vec::new());
        assert_eq!(none.data(), None);
    }
}
