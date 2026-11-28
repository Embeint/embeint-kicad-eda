// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kiplatform/os/unix/environment.cpp`.

use kicad_kiplatform::environment::ProxyConfig;
use kicad_map_helpers::WxString;

use crate::host::{Conv, UnixHost};
use crate::wx::{
    as_char_buf, cmp_no_case, concat, contains, find_first, from_c_str, from_c_str_utf8, left, lit,
    mid, units, utf8_str, Uri,
};
use crate::{c_str, Result, UnixError, UNIT};

/// The wxWidgets build configuration `ENV::Init` is compiled against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildConfig {
    /// `wxCHECK_VERSION( 3, 3, 2 )`.
    pub wx_at_least_3_3_2: bool,
    /// `wxHAS_EGL` (consulted only from wx 3.3.2).
    pub wx_has_egl: bool,
    /// `wxUSE_GLCANVAS_EGL` (consulted before wx 3.3.2).
    pub wx_use_glcanvas_egl: bool,
}

/// The pinned reference build: wxWidgets 3.2.4 (`3.2.4+dfsg-4build1`), whose `setup.h`
/// defines `wxUSE_GLCANVAS_EGL` as 0. The oracle reports these macros (operation
/// `build_config`) so a different reference build shows as a mismatch.
pub const REFERENCE_BUILD: BuildConfig = BuildConfig {
    wx_at_least_3_3_2: false,
    wx_has_egl: false,
    wx_use_glcanvas_egl: false,
};

/// `void KIPLATFORM::ENV::Init()`: work around Unity's menu proxy, select the GL backend and
/// disable XInput 2 smooth scrolling off Wayland, through environment variables.
pub fn init<H: UnixHost + ?Sized>(host: &mut H, build: &BuildConfig) -> Result<()> {
    // wxString wm; wxGetEnv( ..., &wm ) && wm.CmpNoCase( wxT( "Unity" ) ) == 0
    if let Some(wm) = host.wx_get_env(&lit("XDG_CURRENT_DESKTOP")?, true)? {
        if cmp_no_case(host, wm.units(), lit("Unity")?.units())? == 0 {
            host.wx_set_env(&lit("UBUNTU_MENUPROXY")?, &lit("0")?)?;
        }
    }

    let mut force_x11 = false;
    if build.wx_at_least_3_3_2 {
        if build.wx_has_egl {
            if host.wx_get_env(&lit("WAYLAND_DISPLAY")?, false)?.is_none() {
                // UNPORTED(libs.kiplatform.os.unix.application.ui.p001):
                // wxSystemOptions::SetOption, compiled only against wx >= 3.3.2.
                return Err(UnixError::Unported {
                    upstream: "wxSystemOptions::SetOption",
                    unit: UNIT,
                });
            }
        } else {
            force_x11 = true;
        }
    } else if !build.wx_use_glcanvas_egl {
        force_x11 = true;
    }

    if force_x11 {
        host.wx_set_env(&lit("GDK_BACKEND")?, &lit("x11")?)?;
    }

    if host.wx_get_env(&lit("WAYLAND_DISPLAY")?, false)?.is_none() {
        host.wx_set_env(&lit("GDK_CORE_DEVICE_EVENTS")?, &lit("1")?)?;
    }
    Ok(())
}

/// `bool MoveToTrash( const wxString& aPath, wxString& aError )`.
pub fn move_to_trash<H: UnixHost + ?Sized>(
    host: &mut H,
    path: &WxString,
    error: &mut WxString,
) -> Result<bool> {
    let name = as_char_buf(host, Conv::FileName, path)?;
    let file = host.g_file_new_for_path(c_str(&name))?;
    let (ret_val, err) = host.g_file_trash(file)?;
    if !ret_val {
        if let Some(err) = err {
            // aError = err->message: operator=( const char* ) with wxConvLibc.
            let message = host.g_error_message(err)?;
            *error = from_c_str(host, Conv::Libc, Some(&message))?;
        }
    }
    host.g_clear_error(err)?;
    host.g_object_unref(file)?;
    Ok(ret_val)
}

/// `bool IsNetworkPath( const wxString& )`: a placeholder, `false`.
pub fn is_network_path(_path: &WxString) -> bool {
    false
}

/// `bool IsRemovablePath( const wxString& aPath )`: whether the mount enclosing the resolved
/// path has a removable drive.
pub fn is_removable_path<H: UnixHost + ?Sized>(host: &mut H, path: &WxString) -> Result<bool> {
    let name = as_char_buf(host, Conv::FileName, path)?;
    let Some(resolved) = host.realpath(c_str(&name))? else {
        return Ok(false);
    };
    let file = host.g_file_new_for_path(&resolved.bytes)?;
    host.free(Some(resolved.ptr))?;
    let mount = host.g_file_find_enclosing_mount(file)?;
    host.g_object_unref(file)?;
    let Some(mount) = mount else {
        return Ok(false);
    };
    let drive = host.g_mount_get_drive(mount)?;
    let removable = match drive {
        Some(drive) => host.g_drive_is_removable(drive)?,
        None => false,
    };
    if let Some(drive) = drive {
        host.g_object_unref(drive)?;
    }
    host.g_object_unref(mount)?;
    Ok(removable)
}

/// `wxString GetDocumentsPath()`: the XDG user data directory, or `~/.local/share` made
/// absolute when GLib returns an empty one.
pub fn get_documents_path<H: UnixHost + ?Sized>(host: &mut H) -> Result<WxString> {
    let data_dir = host.g_get_user_data_dir()?;
    let docs_path = from_c_str(host, Conv::Libc, data_dir.as_deref())?;
    if docs_path.is_empty() {
        // wxFileName fallback; fallback.AssignDir( g_get_home_dir() ) converts its argument.
        let home = host.g_get_home_dir()?;
        let _home = from_c_str(host, Conv::Libc, home.as_deref())?;
        // UNPORTED(libs.kiplatform.os.unix.application.ui.p001): wxFileName::AssignDir,
        // AppendDir, MakeAbsolute (wxFileName::Normalize with the process working directory
        // and home directory) and GetFullPath.
        return Err(UnixError::Unported {
            upstream: "wxFileName::MakeAbsolute",
            unit: UNIT,
        });
    }
    Ok(docs_path)
}

/// `wxString GetUserConfigPath()`: `g_get_user_config_dir()`.
pub fn get_user_config_path<H: UnixHost + ?Sized>(host: &mut H) -> Result<WxString> {
    let dir = host.g_get_user_config_dir()?;
    from_c_str(host, Conv::Libc, dir.as_deref())
}

/// `wxString GetUserDataPath()`: `g_get_user_data_dir()`.
pub fn get_user_data_path<H: UnixHost + ?Sized>(host: &mut H) -> Result<WxString> {
    let dir = host.g_get_user_data_dir()?;
    from_c_str(host, Conv::Libc, dir.as_deref())
}

/// `wxString GetUserLocalDataPath()`: also `g_get_user_data_dir()`.
pub fn get_user_local_data_path<H: UnixHost + ?Sized>(host: &mut H) -> Result<WxString> {
    get_user_data_path(host)
}

/// `wxString GetUserCachePath()`: `g_get_user_cache_dir()`.
pub fn get_user_cache_path<H: UnixHost + ?Sized>(host: &mut H) -> Result<WxString> {
    let dir = host.g_get_user_cache_dir()?;
    from_c_str(host, Conv::Libc, dir.as_deref())
}

/// `bool GetSystemProxyConfig( const wxString& aURL, PROXY_CONFIG& aCfg )`: the first
/// non-direct proxy GIO's default resolver returns for the URL, as `scheme://host:port` with
/// the user and password from its user information.
pub fn get_system_proxy_config<H: UnixHost + ?Sized>(
    host: &mut H,
    url: &WxString,
    cfg: &mut ProxyConfig,
) -> Result<bool> {
    let mut success = false;
    let Some(resolver) = host.g_proxy_resolver_get_default()? else {
        return Ok(false);
    };
    let url_utf8 = utf8_str(host, url)?;
    let (proxy_list, error) = host.g_proxy_resolver_lookup(resolver, c_str(&url_utf8))?;
    if let Some(error) = error {
        host.g_error_free(error)?;
        return Ok(false);
    }
    let Some((list, entries)) = proxy_list else {
        return Ok(false);
    };

    let direct = lit("direct://")?;
    let colon = lit(":")?;
    for entry in &entries {
        let proxy_uri_str = from_c_str_utf8(entry)?;
        if proxy_uri_str.units() == direct.units() {
            continue;
        }
        let proxy_uri = Uri::new(host, &proxy_uri_str)?;

        cfg.host = WxString::new();
        cfg.username = WxString::new();
        cfg.password = WxString::new();

        let scheme = &proxy_uri.scheme;
        if !scheme.is_empty() {
            cfg.host = concat(&[scheme, lit("://")?.units()])?;
        }
        let server = &proxy_uri.server;
        let is_ipv6 = contains(server, colon.units());
        if is_ipv6 && proxy_uri.has_port() {
            let bracketed = concat(&[lit("[")?.units(), server, lit("]")?.units()])?;
            cfg.host = concat(&[cfg.host.units(), bracketed.units()])?;
        } else {
            cfg.host = concat(&[cfg.host.units(), server])?;
        }
        if proxy_uri.has_port() {
            let port = concat(&[colon.units(), &proxy_uri.port])?;
            cfg.host = concat(&[cfg.host.units(), port.units()])?;
        }

        let user_info = &proxy_uri.userinfo;
        if !user_info.is_empty() {
            match find_first(user_info, colon.units()) {
                Some(colon_pos) => {
                    cfg.username = left(user_info, colon_pos)?;
                    cfg.password = mid(user_info, colon_pos + 1, None)?;
                }
                None => cfg.username = units(user_info)?,
            }
        }
        success = true;
        break;
    }
    host.g_strfreev(list)?;
    Ok(success)
}

/// `bool VerifyFileSignature( const wxString& )`: `true`.
pub fn verify_file_signature(_path: &WxString) -> bool {
    true
}

/// `wxString GetAppUserModelId()`: `wxEmptyString`.
pub fn get_app_user_model_id() -> WxString {
    WxString::new()
}

/// `wxString GetCommandLineStr()`: `wxEmptyString`.
pub fn get_command_line_str() -> WxString {
    WxString::new()
}
