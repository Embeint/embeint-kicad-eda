// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kiplatform/os/unix/io.cpp`.

use alloc::vec::Vec;

use kicad_kiplatform::io::{MappedData, MappedFile, RuntimeError, TargetAttrs};
use kicad_kiplatform::owners::OS_COMMON_UI;
use kicad_map_helpers::WxString;

use crate::host::{Conv, UnixHost};
use crate::stdhash::string_hash;
use crate::wx::{as_char_buf, lit, starts_with, FileName};
use crate::{c_str, copy_bytes, reserve, Ptr, Result, UnixError};

/// `POSIX_FADV_SEQUENTIAL`.
pub const POSIX_FADV_SEQUENTIAL: i32 = 2;
/// `FNM_PERIOD`.
pub const FNM_PERIOD: i32 = 1 << 2;
/// `FNM_CASEFOLD`.
pub const FNM_CASEFOLD: i32 = 1 << 4;
/// `PATH_MAX`.
pub const PATH_MAX: usize = 4096;
/// `O_RDONLY`.
pub const O_RDONLY: i32 = 0;
/// `PROT_READ`.
pub const PROT_READ: i32 = 1;
/// `MAP_PRIVATE`.
pub const MAP_PRIVATE: i32 = 2;
/// `MADV_SEQUENTIAL`.
pub const MADV_SEQUENTIAL: i32 = 2;
/// `S_IRWXU | S_IRWXG | S_IRWXO`.
pub const S_IRWXUGO: u32 = 0o777;
/// `S_IWUSR`.
pub const S_IWUSR: u32 = 0o200;
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFREG: u32 = 0o100_000;

/// `FILE* SeqFOpen( const wxString& aPath, const wxString& aMode )`: `wxFopen`, then advise
/// sequential access; a failed advice closes the file and returns null.
pub fn seq_f_open<H: UnixHost + ?Sized>(
    host: &mut H,
    path: &WxString,
    mode: &WxString,
) -> Result<Option<Ptr>> {
    // wxFopen( path, mode ) is wxCRT_Fopen( path.fn_str(), mode.fn_str() ).
    let path = as_char_buf(host, Conv::FileName, path)?;
    let mode = as_char_buf(host, Conv::FileName, mode)?;
    let mut fp = host.fopen(c_str(&path), c_str(&mode))?;
    if let Some(stream) = fp {
        let fd = host.fileno(stream)?;
        if host.posix_fadvise(fd, 0, 0, POSIX_FADV_SEQUENTIAL)? != 0 {
            host.fclose(stream)?;
            fp = None;
        }
    }
    Ok(fp)
}

/// `bool DuplicatePermissions( const wxString& aSrc, const wxString& aDest )`: copy the
/// permission bits of `aSrc` to `aDest`.
pub fn duplicate_permissions<H: UnixHost + ?Sized>(
    host: &mut H,
    src: &WxString,
    dest: &WxString,
) -> Result<bool> {
    let src = as_char_buf(host, Conv::FileName, src)?;
    let (rc, source_stat) = host.stat(c_str(&src))?;
    if rc != 0 {
        return Ok(false);
    }
    let permissions = source_stat.mode & S_IRWXUGO;
    let dest = as_char_buf(host, Conv::FileName, dest)?;
    Ok(host.chmod(c_str(&dest), permissions)? == 0)
}

/// `bool MakeWriteable( const wxString& aFilePath )`: add the user write bit.
pub fn make_writeable<H: UnixHost + ?Sized>(host: &mut H, file_path: &WxString) -> Result<bool> {
    let path = as_char_buf(host, Conv::FileName, file_path)?;
    let (rc, file_stat) = host.stat(c_str(&path))?;
    if rc == 0 {
        let new_permissions = file_stat.mode | S_IWUSR;
        let path = as_char_buf(host, Conv::FileName, file_path)?;
        if host.chmod(c_str(&path), new_permissions)? == 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `TARGET_ATTRS CaptureTargetAttributes( const wxString& )`: `TARGET_ATTRS{}`.
pub fn capture_target_attributes(_path: &WxString) -> TargetAttrs {
    TargetAttrs::default()
}

/// `bool ApplyTargetAttributes( const wxString&, const TARGET_ATTRS& )`: `true`.
pub fn apply_target_attributes(_path: &WxString, _attrs: &TargetAttrs) -> bool {
    true
}

/// `bool IsFileHidden( const wxString& aFileName )`: the name part starts with a dot.
pub fn is_file_hidden(file_name: &WxString) -> Result<bool> {
    let fn_ = FileName::new(file_name.units())?;
    Ok(starts_with(fn_.name().units(), lit(".")?.units())?.is_some())
}

/// `dir_path + '/' + name`.
fn join(dir: &[u8], name: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    reserve(
        &mut out,
        dir.len().saturating_add(name.len()).saturating_add(1),
    )?;
    out.extend_from_slice(dir);
    out.push(b'/');
    out.extend_from_slice(name);
    Ok(out)
}

/// `long long TimestampDir( const wxString& aDirPath, const wxString& aFilespec )`: a
/// signature of the matching entries (modification times in milliseconds plus sizes of the
/// regular files, symlinks followed one level; the name hash of an entry `lstat` fails on).
///
/// A signed overflow in `st_mtime * 1000` or in the running sum is undefined in the reference
/// and refused.
pub fn timestamp_dir<H: UnixHost + ?Sized>(
    host: &mut H,
    dir_path: &WxString,
    filespec: &WxString,
) -> Result<i64> {
    let overflow = || UnixError::Undefined("signed overflow in TimestampDir");
    let mut timestamp: i64 = 0;
    // std::string pattern( aFilespec.fn_str() ), std::string dir_path( aDirPath.fn_str() ):
    // from a const char*, so each ends at its first NUL.
    let pattern = copy_bytes(c_str(&as_char_buf(host, Conv::FileName, filespec)?))?;
    let dir_path = copy_bytes(c_str(&as_char_buf(host, Conv::FileName, dir_path)?))?;

    let Some(dir) = host.opendir(&dir_path)? else {
        return Ok(timestamp);
    };
    while let Some(name) = host.readdir(dir)? {
        let name = c_str(&name);
        if host.fnmatch(&pattern, name, FNM_CASEFOLD | FNM_PERIOD)? != 0 {
            continue;
        }
        let entry_path = join(&dir_path, name)?;
        let (rc, mut entry_stat) = host.lstat(&entry_path)?;
        if rc == 0 {
            if entry_stat.mode & S_IFMT == S_IFLNK {
                let (path_len, buffer) = host.readlink(&entry_path, PATH_MAX)?;
                if path_len > 0 {
                    let len = usize::try_from(path_len).map_err(|_| UnixError::Host("readlink"))?;
                    if len > PATH_MAX || len > buffer.len() {
                        return Err(UnixError::Host("readlink wrote more than PATH_MAX bytes"));
                    }
                    // buffer[pathLen] = '\0'; dir_path + '/' + buffer reads up to the first NUL.
                    let target = c_str(buffer.get(..len).unwrap_or(&[]));
                    let linked_path = join(&dir_path, target)?;
                    let (linked_rc, linked_stat) = host.lstat(&linked_path)?;
                    if linked_rc == 0 {
                        entry_stat = linked_stat;
                    }
                }
            }
            if entry_stat.mode & S_IFMT == S_IFREG {
                let millis = entry_stat.mtime.checked_mul(1000).ok_or_else(overflow)?;
                timestamp = timestamp.checked_add(millis).ok_or_else(overflow)?;
                timestamp = timestamp
                    .checked_add(entry_stat.size)
                    .ok_or_else(overflow)?;
            }
        } else {
            // (signed) std::hash<std::string>{}( ... ): size_t to int, modulo 2^32.
            let hash = string_hash(name) as i32;
            timestamp = timestamp
                .checked_add(i64::from(hash))
                .ok_or_else(overflow)?;
        }
    }
    host.closedir(dir)?;
    Ok(timestamp)
}

/// `bool FlushToDisk( FILE* aFp )`: `fflush`, then `fsync` of its descriptor.
pub fn flush_to_disk<H: UnixHost + ?Sized>(host: &mut H, fp: Option<Ptr>) -> Result<bool> {
    let Some(fp) = fp else {
        return Ok(false);
    };
    if host.fflush(fp)? != 0 {
        return Ok(false);
    }
    let fd = host.fileno(fp)?;
    if fd < 0 {
        return Ok(false);
    }
    Ok(host.fsync(fd)? == 0)
}

/// `std::runtime_error( std::string( prefix ) + aFileName.ToStdString() )`: `ToStdString()`
/// converts with `wxConvLibc`; `what()` returns the message as a C string, so a reader sees it
/// up to its first NUL.
fn runtime_error<H: UnixHost + ?Sized>(
    host: &mut H,
    prefix: &[u8],
    file_name: &WxString,
) -> Result<RuntimeError> {
    let name = as_char_buf(host, Conv::Libc, file_name)?;
    let mut what = Vec::new();
    reserve(&mut what, prefix.len().saturating_add(name.len()))?;
    what.extend_from_slice(prefix);
    what.extend_from_slice(c_str(&name));
    Ok(RuntimeError { what })
}

/// `explicit MAPPED_FILE( const wxString& aFileName )`: map the file read-only, or leave it
/// empty for a zero-length file. A thrown `std::runtime_error` is the inner `Err`.
///
/// When `mmap` fails the constructor calls `readIntoBuffer` (`os/common/io.cpp`), which is
/// deferred to its owner.
pub fn mapped_file_open<H: UnixHost + ?Sized>(
    host: &mut H,
    file_name: &WxString,
) -> Result<core::result::Result<MappedFile, RuntimeError>> {
    let name = as_char_buf(host, Conv::FileName, file_name)?;
    let fd = host.open(c_str(&name), O_RDONLY)?;
    if fd < 0 {
        return Ok(Err(runtime_error(host, b"Cannot open file: ", file_name)?));
    }
    let (rc, st) = host.fstat(fd)?;
    if rc != 0 {
        host.close(fd)?;
        return Ok(Err(runtime_error(host, b"Cannot stat file: ", file_name)?));
    }
    // m_size = static_cast<size_t>( st.st_size ): modulo 2^64.
    let size = st.size as u64 as usize;
    if size == 0 {
        host.close(fd)?;
        return Ok(Ok(MappedFile::from_members(
            MappedData::Null,
            size,
            false,
            Vec::new(),
        )));
    }
    let mapping = host.mmap(size, PROT_READ, MAP_PRIVATE, fd, 0)?;
    host.close(fd)?;
    let Some(mapping) = mapping else {
        // UNPORTED(libs.kiplatform.os.common.application.ui.p001):
        // KIPLATFORM::IO::MAPPED_FILE::readIntoBuffer
        return Err(UnixError::Unported {
            upstream: "KIPLATFORM::IO::MAPPED_FILE::readIntoBuffer",
            unit: OS_COMMON_UI,
        });
    };
    host.madvise(mapping.ptr, size, MADV_SEQUENTIAL)?;
    Ok(Ok(MappedFile::from_members(
        MappedData::Mapping {
            address: mapping.ptr.0,
            bytes: mapping.bytes,
        },
        size,
        true,
        Vec::new(),
    )))
}

/// `~MAPPED_FILE()`: unmap a mapping.
pub fn mapped_file_close<H: UnixHost + ?Sized>(host: &mut H, file: MappedFile) -> Result<()> {
    // if( m_isMapped && m_data ): m_data is non-null only for a mapping or a fallback buffer.
    if file.is_mapped() {
        if let MappedData::Mapping { address, .. } = file.data_member() {
            host.munmap(Ptr(*address), file.size())?;
        }
    }
    Ok(())
}
