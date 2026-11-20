// SPDX-License-Identifier: GPL-3.0-or-later
//! Discovery of KiCad's local IPC endpoints without dialing them: a port of
//! `tools/live_schematic_mcp/instance_discovery.py` (see INSTANCE_DISCOVERY.md there).
//!
//! Endpoint names follow `KICAD_API_SERVER::Start` in `common/api/api_server.cpp`:
//! `ipc://<temp>/kicad/api.sock` (or `api-<pid>.sock`).  On Linux/WSL that is a Unix socket
//! file; on Windows NNG serves the same URL as the named pipe `\\.\pipe\<temp>\kicad\api.sock`,
//! which exists only while a server holds it.  A candidate is never proof of a live server.
//! Nothing here locks, removes or creates sockets, pipes or files.
//!
//! The validation rules take a [`Platform`], so both rule sets are testable on any host; the
//! filesystem and pipe inspection always concerns the host.

use std::path::{Path, PathBuf};

use serde_json::{json, Value as Json};

/// `ENDPOINT_KINDS`: entries that can be selected.
const ENDPOINT_KINDS: [&str; 2] = ["socket", "named_pipe"];
const PIPE_ROOT: &str = r"\\.\pipe\";
/// CreateNamedPipe's limit for the whole `\\.\pipe\name`.
const PIPE_NAME_LIMIT: usize = 256;

pub const NO_EDITOR_ACTION: &str =
    "No KiCad editor is listening. Start Embeint KiCad with the API server enabled \
     (Preferences > Plugins > Enable KiCad API) and open a project, as the same \
     user and with the same temporary directory as the MCP, or set KICAD_API_SOCKET.";

/// Which endpoint rules apply: Unix sockets (Linux/WSL, macOS) or Windows named pipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Unix,
    Windows,
}

impl Platform {
    pub fn host() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Unix
        }
    }
}

/// `DiscoveryError(code, message)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryError {
    pub code: String,
    pub message: String,
}

impl DiscoveryError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        DiscoveryError {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DiscoveryError {}

/// One endpoint of an inventory: its URL and what is there (socket, named_pipe, missing, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub socket_url: String,
    pub kind: String,
}

impl Entry {
    fn new(url: impl Into<String>, kind: &str) -> Self {
        Entry {
            socket_url: url.into(),
            kind: kind.to_owned(),
        }
    }

    pub fn is_endpoint(&self) -> bool {
        ENDPOINT_KINDS.contains(&self.kind.as_str())
    }
}

/// The result of [`discover_instances`]: how the endpoint would be chosen and the candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    pub selection: String,
    pub selected_socket: Option<String>,
    pub candidates: Vec<Entry>,
}

impl Inventory {
    pub fn to_json(&self) -> Json {
        json!({
            "selection": self.selection,
            "selected_socket": self.selected_socket,
            "candidates": self.candidates.iter()
                .map(|e| json!({"socket_url": e.socket_url, "kind": e.kind}))
                .collect::<Vec<_>>(),
        })
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// `str(PureWindowsPath(text))` for the paths used here: separators normalised to `\`, repeated
/// separators and `.` components removed, no trailing separator except after a drive root.
pub fn pure_windows_path(text: &str) -> String {
    let text = text.replace('/', "\\");
    let (prefix, rest) =
        if text.len() >= 2 && text.as_bytes()[1] == b':' && text.is_char_boundary(2) {
            let (drive, rest) = text.split_at(2);
            if let Some(stripped) = rest.strip_prefix('\\') {
                (format!("{drive}\\"), stripped.to_owned())
            } else {
                (drive.to_owned(), rest.to_owned())
            }
        } else if let Some(stripped) = text.strip_prefix('\\') {
            ("\\".to_owned(), stripped.to_owned())
        } else {
            (String::new(), text)
        };
    let parts: Vec<&str> = rest
        .split('\\')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    let joined = parts.join("\\");
    if joined.is_empty() && prefix.is_empty() {
        ".".to_owned()
    } else {
        prefix + &joined
    }
}

/// `str(PurePosixPath(text))` for an absolute path: repeated separators and `.` components
/// removed, no trailing separator; exactly two leading slashes are kept, as POSIX allows.
pub fn pure_posix_path(text: &str) -> String {
    let root = if text.starts_with("//") && !text.starts_with("///") {
        "//"
    } else {
        "/"
    };
    let parts: Vec<&str> = text
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    format!("{root}{}", parts.join("/"))
}

/// The temporary directory KiCad uses on Windows, spelt as KiCad spells it: the first of TMPDIR,
/// TMP and TEMP that is set, else `GetTempPath()` (TMP, TEMP, USERPROFILE, then the Windows
/// directory).  Python's `tempfile`, which prefers TEMP to TMP, is only a last resort.
pub fn windows_temp_dir(lookup: impl Fn(&str) -> Option<String>) -> String {
    for name in ["TMPDIR", "TMP", "TEMP", "USERPROFILE", "SystemRoot"] {
        if let Some(value) = lookup(name).filter(|v| !v.is_empty()) {
            return pure_windows_path(&value);
        }
    }
    pure_windows_path(&std::env::temp_dir().to_string_lossy())
}

/// Python's `tempfile.gettempdir()` on Unix: TMPDIR, TEMP or TMP if it names a directory, else
/// the first of /tmp, /var/tmp and /usr/tmp that exists.
fn unix_temp_dir() -> PathBuf {
    for name in ["TMPDIR", "TEMP", "TMP"] {
        if let Some(value) = env(name) {
            let path = PathBuf::from(&value);
            if path.is_dir() {
                return std::path::absolute(&path).unwrap_or(path);
            }
        }
    }
    for candidate in ["/tmp", "/var/tmp", "/usr/tmp"] {
        if Path::new(candidate).is_dir() {
            return PathBuf::from(candidate);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// `default_socket_url()`: the endpoint a KiCad on this host takes first.
pub fn default_socket_url() -> String {
    match Platform::host() {
        Platform::Windows => format!(
            "ipc://{}\\kicad\\api.sock",
            windows_temp_dir(env).trim_end_matches('\\')
        ),
        Platform::Unix => format!(
            "ipc://{}",
            unix_temp_dir().join("kicad").join("api.sock").display()
        ),
    }
}

fn invalid_url(platform: Platform) -> DiscoveryError {
    match platform {
        Platform::Windows => DiscoveryError::new(
            "invalid_socket",
            "Use a local ipc://C:\\...\\api.sock named-pipe URL in \
             KICAD_API_SOCKET; network transports are not supported.",
        ),
        Platform::Unix => DiscoveryError::new(
            "invalid_socket",
            "Use an absolute local ipc:///path socket URL in \
             KICAD_API_SOCKET; network transports are not supported.",
        ),
    }
}

/// The pipe NNG serves for `ipc://name` is `\\.\pipe\name`, which Windows normalises.  Only an
/// absolute drive path of plain components is accepted, so no `.` or `..` component, forward
/// slash or device prefix can redirect the pipe name to a UNC (network) pipe or another device.
pub fn windows_pipe_path(name: &str) -> Result<String, DiscoveryError> {
    let error = || invalid_url(Platform::Windows);
    let chars: Vec<char> = name.chars().collect();
    if chars.len() < 4
        || !chars[0].is_ascii_alphabetic()
        || chars[1] != ':'
        || chars[2] != '\\'
        || name.contains('\n')
        || PIPE_ROOT.len() + chars.len() > PIPE_NAME_LIMIT
    {
        return Err(error());
    }
    for part in name[3..].split('\\') {
        let last = part.chars().last();
        if part.is_empty()
            || part == "."
            || part == ".."
            || matches!(last, Some('.' | ' '))
            || part.chars().any(|c| "<>:\"/|?*".contains(c))
        {
            return Err(error());
        }
    }
    Ok(name.to_owned())
}

/// Accept local IPC only, without ever echoing an invalid URL.  Unix: an absolute `ipc:///path`
/// socket.  Windows: `ipc://X:\path`, served as a named pipe on this machine.  TCP, WebSocket,
/// relative and query-bearing URLs (and on Windows UNC or dot-segment paths) are rejected.
pub fn local_socket_path(url: &str, platform: Platform) -> Result<String, DiscoveryError> {
    if !url.starts_with("ipc://") || url.chars().any(|c| (c as u32) < 32 || c == '?' || c == '#') {
        return Err(invalid_url(platform));
    }
    match platform {
        Platform::Windows => windows_pipe_path(&url["ipc://".len()..]),
        Platform::Unix => {
            if !url.starts_with("ipc:///") {
                return Err(invalid_url(platform));
            }
            if url == "ipc:///" {
                return Err(DiscoveryError::new(
                    "invalid_socket",
                    "The IPC URL must name a socket file.",
                ));
            }
            Ok(pure_posix_path(&url["ipc://".len()..]))
        }
    }
}

/// `api.sock` or `api-<digits>.sock` (case-insensitive on Windows).
pub fn is_endpoint_name(name: &str, platform: Platform) -> bool {
    let lowered;
    let name = if platform == Platform::Windows {
        lowered = name.to_lowercase();
        lowered.as_str()
    } else {
        name
    };
    let Some(stem) = name.strip_suffix(".sock") else {
        return false;
    };
    match stem.strip_prefix("api") {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix('-')
            .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())),
        None => false,
    }
}

/// Names of this machine's named pipes.  Enumerating them never connects to one.
pub fn list_pipes() -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(PIPE_ROOT)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

/// `os.path.realpath`, resolving the longest existing prefix (Python's non-strict mode).
fn real_path(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = absolute.clone();
    let mut tail = Vec::new();
    loop {
        if let Ok(resolved) = std::fs::canonicalize(&existing) {
            let mut resolved = strip_verbatim(resolved);
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            return resolved;
        }
        match (
            existing.file_name().map(|n| n.to_os_string()),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                existing = parent.to_path_buf();
            }
            _ => return absolute,
        }
    }
}

/// `\\?\C:\x` to `C:\x`, as Python's realpath returns it.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// Compare directories as Windows does: case-insensitively and with long names, so
/// `C:\Users\LONGNA~1\...` and `C:\Users\LongName\...` match.
pub fn directory_key(path: &str) -> String {
    let resolved = real_path(Path::new(path)).to_string_lossy().into_owned();
    if Platform::host() == Platform::Windows {
        resolved.replace('/', "\\").to_lowercase()
    } else {
        resolved
    }
}

fn parent_of(windows_path: &str) -> &str {
    match windows_path.rfind('\\') {
        Some(2) => &windows_path[..3],
        Some(index) => &windows_path[..index],
        None => windows_path,
    }
}

fn file_name_of(windows_path: &str) -> &str {
    windows_path.rsplit('\\').next().unwrap_or(windows_path)
}

fn pipe_entry(url: &str, pipes: &[String]) -> Result<Entry, DiscoveryError> {
    let path = local_socket_path(url, Platform::Windows)?.to_lowercase();
    let present = pipes.iter().any(|name| name.to_lowercase() == path);
    Ok(Entry::new(
        url,
        if present { "named_pipe" } else { "missing" },
    ))
}

#[cfg(unix)]
fn unix_kind(path: &str) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => "socket",
        Ok(meta) if meta.file_type().is_symlink() => "symlink",
        Ok(_) => "not_socket",
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "missing",
        Err(_) => "unavailable",
    }
}

#[cfg(not(unix))]
fn unix_kind(_path: &str) -> &'static str {
    "unavailable"
}

/// The inventory entry for one URL.
pub fn socket_entry(url: &str) -> Result<Entry, DiscoveryError> {
    match Platform::host() {
        Platform::Windows => match list_pipes() {
            Ok(pipes) => pipe_entry(url, &pipes),
            Err(_) => {
                local_socket_path(url, Platform::Windows)?;
                Ok(Entry::new(url, "unavailable"))
            }
        },
        Platform::Unix => {
            let path = local_socket_path(url, Platform::Unix)?;
            Ok(Entry::new(url, unix_kind(&path)))
        }
    }
}

/// KiCad pipes named under `root` (this user's temp `kicad` directory), given the machine's
/// pipe names.  Named pipes are machine-wide, so endpoints under other directories are visible
/// too; they are never candidates.  Only the name is checked, not the pipe's owner.
pub fn windows_candidates(
    root: &str,
    pipes: &[String],
    key: impl Fn(&str) -> String,
) -> Vec<Entry> {
    let wanted = key(root);
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut names: Vec<&String> = Vec::new();
    for name in pipes {
        let Ok(path) = windows_pipe_path(name) else {
            continue;
        };
        if !is_endpoint_name(file_name_of(&path), Platform::Windows) {
            continue;
        }
        let parent = parent_of(&path).to_owned();
        let parent_key = match keys.iter().find(|(p, _)| *p == parent) {
            Some((_, k)) => k.clone(),
            None => {
                let k = key(&parent);
                keys.push((parent, k.clone()));
                k
            }
        };
        if parent_key == wanted {
            names.push(name);
        }
    }
    names.sort_by_key(|n| file_name_of(n).to_lowercase());
    names
        .into_iter()
        .map(|n| Entry::new(format!("ipc://{n}"), "named_pipe"))
        .collect()
}

/// `discover_instances()`: a stable inventory without dialing.  An explicit endpoint (argument or
/// `KICAD_API_SOCKET`) is never scanned for.  `directory` is a test seam.
pub fn discover_instances(
    explicit: Option<&str>,
    directory: Option<&str>,
) -> Result<Inventory, DiscoveryError> {
    discover(explicit, directory, true)
}

/// discover_instances, reading KICAD_API_SOCKET only when `read_environment` is set.
fn discover(
    explicit: Option<&str>,
    directory: Option<&str>,
    read_environment: bool,
) -> Result<Inventory, DiscoveryError> {
    let explicit = explicit
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            if read_environment {
                env("KICAD_API_SOCKET")
            } else {
                None
            }
        });
    if let Some(url) = explicit {
        let entry = socket_entry(&url)?;
        return Ok(Inventory {
            selection: "explicit".into(),
            selected_socket: Some(url),
            candidates: vec![entry],
        });
    }
    let (default, candidates) = match Platform::host() {
        Platform::Windows => {
            let root = match directory {
                Some(d) => pure_windows_path(d),
                None => parent_of(&local_socket_path(
                    &default_socket_url(),
                    Platform::Windows,
                )?)
                .to_owned(),
            };
            let default = format!("ipc://{}\\api.sock", root.trim_end_matches('\\'));
            let pipes = list_pipes().map_err(|_| {
                DiscoveryError::new(
                    "discovery_unavailable",
                    "Cannot list this machine's named pipes; \
                     set KICAD_API_SOCKET to the intended instance.",
                )
            })?;
            (default, windows_candidates(&root, &pipes, directory_key))
        }
        Platform::Unix => {
            let root = match directory {
                Some(d) => PathBuf::from(d),
                None => {
                    let path = local_socket_path(&default_socket_url(), Platform::Unix)?;
                    Path::new(&path)
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_default()
                }
            };
            let default = format!("ipc://{}", root.join("api.sock").display());
            if std::fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(DiscoveryError::new(
                    "discovery_unavailable",
                    "The KiCad socket directory is a symlink; \
                     select a trusted local socket with KICAD_API_SOCKET.",
                ));
            }
            let mut names = Vec::new();
            match std::fs::read_dir(&root) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry.map_err(|_| unreadable_directory())?;
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if is_endpoint_name(&name, Platform::Unix) {
                            names.push(name);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(unreadable_directory()),
            }
            names.sort();
            let mut candidates = Vec::new();
            for name in names {
                candidates.push(socket_entry(&format!(
                    "ipc://{}",
                    root.join(name).display()
                ))?);
            }
            (default, candidates)
        }
    };
    let sockets: Vec<&Entry> = candidates.iter().filter(|e| e.is_endpoint()).collect();
    let (selection, selected) = if candidates.iter().any(|e| e.kind == "unavailable") {
        ("unavailable", None)
    } else if sockets.len() > 1 {
        ("ambiguous", None)
    } else if let Some(only) = sockets.first() {
        ("single_candidate", Some(only.socket_url.clone()))
    } else if !candidates.is_empty() {
        ("unavailable", None)
    } else {
        ("default_missing", Some(default))
    };
    Ok(Inventory {
        selection: selection.into(),
        selected_socket: selected,
        candidates,
    })
}

fn unreadable_directory() -> DiscoveryError {
    DiscoveryError::new(
        "discovery_unavailable",
        "Cannot inspect the KiCad socket directory; \
         check permissions or set KICAD_API_SOCKET.",
    )
}

/// Whether a listener could receive a request, checked without sending NNG or application bytes:
/// "connectable", "not_listening" (proved: refused, vanished or no pipe) or "unknown".
pub async fn socket_reachability(url: &str) -> Result<&'static str, DiscoveryError> {
    let path = local_socket_path(url, Platform::host())?;
    if Platform::host() == Platform::Windows {
        return Ok(match list_pipes() {
            Ok(pipes) => {
                if pipe_entry(url, &pipes)?.kind == "named_pipe" {
                    "connectable"
                } else {
                    "not_listening"
                }
            }
            Err(_) => "unknown",
        });
    }
    Ok(unix_reachability(&path).await)
}

#[cfg(unix)]
async fn unix_reachability(path: &str) -> &'static str {
    use std::io::ErrorKind;
    match tokio::time::timeout(
        std::time::Duration::from_millis(100),
        tokio::net::UnixStream::connect(path),
    )
    .await
    {
        Ok(Ok(_)) => "connectable",
        Ok(Err(e)) if matches!(e.kind(), ErrorKind::ConnectionRefused | ErrorKind::NotFound) => {
            "not_listening"
        }
        _ => "unknown",
    }
}

#[cfg(not(unix))]
async fn unix_reachability(_path: &str) -> &'static str {
    "unknown"
}

/// The command that explains discovery problems, as the Python MCP names it: the bundled server's
/// `--diagnose` when this executable sits in an MCP package beside it, else the checkout's script.
pub fn diagnose_command() -> &'static str {
    let bundled = std::env::current_exe().ok().and_then(|exe| {
        let name = if cfg!(windows) {
            "embeint-kicad-mcp.exe"
        } else {
            "embeint-kicad-mcp"
        };
        exe.parent().map(|folder| folder.join(name).is_file())
    });
    if bundled == Some(true) {
        "embeint-kicad-mcp --diagnose"
    } else {
        "diagnose.py"
    }
}

/// `select_instance()`: (url, selection) for the instance to use.  selection is "explicit",
/// "single_candidate" or "default_missing"; the last means no instance was found and the url is
/// only the default endpoint a KiCad would take.  With `require_listener` a missing default is
/// the error "no_editor" instead.
pub async fn select_instance(
    explicit: Option<&str>,
    directory: Option<&str>,
    check_liveness: bool,
    require_listener: bool,
) -> Result<(String, String), DiscoveryError> {
    let mut inventory = discover_instances(explicit, directory)?;
    if require_listener && inventory.selection == "default_missing" {
        return Err(DiscoveryError::new(
            "no_editor",
            format!(
                "{NO_EDITOR_ACTION} Nothing was sent. Run {} for details.",
                diagnose_command()
            ),
        ));
    }
    if check_liveness && inventory.selection != "explicit" && inventory.selection != "unavailable" {
        let sockets: Vec<String> = inventory
            .candidates
            .iter()
            .filter(|e| e.is_endpoint())
            .map(|e| e.socket_url.clone())
            .collect();
        let mut usable = Vec::new();
        for url in &sockets {
            if socket_reachability(url).await? != "not_listening" {
                usable.push(url.clone());
            }
        }
        if !sockets.is_empty() {
            inventory.selected_socket = if usable.len() == 1 {
                Some(usable[0].clone())
            } else {
                None
            };
            inventory.selection = match usable.len() {
                1 => "single_candidate",
                0 => "unavailable",
                _ => "ambiguous",
            }
            .into();
        }
    }
    match inventory.selected_socket {
        Some(url) => Ok((url, inventory.selection)),
        None => Err(DiscoveryError::new(
            &inventory.selection,
            format!(
                "KiCad socket selection is {}. Run diagnose.py (optionally --probe) and set \
                 KICAD_API_SOCKET to the intended instance. Stale sockets are never removed by MCP.",
                inventory.selection
            ),
        )),
    }
}

/// `python instance_discovery.py`: the offline inventory of the default location as JSON, with
/// KICAD_API_SOCKET ignored, and the exit status (2 if the endpoints cannot be listed).
pub fn inventory_report() -> (Json, i32) {
    match discover(None, None, false) {
        Ok(inventory) => (inventory.to_json(), 0),
        Err(error) => (
            json!({"selection": error.code, "selected_socket": null, "candidates": [], "error": error.message}),
            2,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_urls_are_drive_paths_of_plain_components() {
        let ok = |u: &str| local_socket_path(u, Platform::Windows);
        assert_eq!(
            ok(r"ipc://C:\Temp\kicad\api.sock").unwrap(),
            r"C:\Temp\kicad\api.sock"
        );
        for bad in [
            r"ipc://\\server\pipe\x",
            r"ipc://\\.\pipe\x",
            r"ipc://C:/Temp/api.sock",
            r"ipc://C:\Temp\..\api.sock",
            r"ipc://C:\Temp\.\api.sock",
            r"ipc://C:\Temp \api.sock",
            r"ipc://C:\Temp.\api.sock",
            r"ipc://C:\Te<mp\api.sock",
            r"ipc://C:\Temp\\api.sock",
            r"ipc://Temp\api.sock",
            "tcp://127.0.0.1:1",
            r"ipc://C:\Temp\api.sock?x",
        ] {
            assert_eq!(ok(bad).unwrap_err().code, "invalid_socket", "{bad}");
        }
        let long = format!(r"ipc://C:\{}", "a".repeat(PIPE_NAME_LIMIT));
        assert!(ok(&long).is_err());
    }

    #[test]
    fn unix_urls_are_absolute() {
        let ok = |u: &str| local_socket_path(u, Platform::Unix);
        assert_eq!(
            ok("ipc:///tmp/kicad/api.sock").unwrap(),
            "/tmp/kicad/api.sock"
        );
        assert_eq!(
            ok("ipc:///").unwrap_err().message,
            "The IPC URL must name a socket file."
        );
        assert!(ok("ipc://relative.sock").is_err());
        assert!(ok("ws://x").is_err());
        assert!(ok("ipc:///tmp/a\u{1}b").is_err());
    }

    #[test]
    fn endpoint_names() {
        assert!(is_endpoint_name("api.sock", Platform::Unix));
        assert!(is_endpoint_name("api-123.sock", Platform::Unix));
        assert!(!is_endpoint_name("API.sock", Platform::Unix));
        assert!(is_endpoint_name("API-7.SOCK", Platform::Windows));
        assert!(!is_endpoint_name("api-.sock", Platform::Unix));
        assert!(!is_endpoint_name("api-1a.sock", Platform::Unix));
        assert!(!is_endpoint_name("xapi.sock", Platform::Unix));
    }

    #[test]
    fn windows_candidates_keep_only_this_directory() {
        let pipes: Vec<String> = [
            r"C:\Temp\kicad\api-12.sock",
            r"C:\Temp\kicad\API.sock",
            r"C:\Other\kicad\api.sock",
            r"C_\repo\.git\fsmonitor--daemon.ipc",
            r"C:\Temp\kicad\notes.sock",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let found = windows_candidates(r"C:\Temp\kicad", &pipes, |p| p.to_lowercase());
        let urls: Vec<_> = found.iter().map(|e| e.socket_url.as_str()).collect();
        // Sorted by lower-cased file name, as Python does: "-" sorts before "."
        assert_eq!(
            urls,
            [
                r"ipc://C:\Temp\kicad\api-12.sock",
                r"ipc://C:\Temp\kicad\API.sock"
            ]
        );
    }

    #[test]
    fn windows_temp_dir_prefers_tmpdir_then_tmp() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            windows_temp_dir(env(&[("TEMP", r"C:\B"), ("TMP", r"C:\A\")])),
            r"C:\A"
        );
        assert_eq!(
            windows_temp_dir(env(&[("TMPDIR", "C:/X/./y//")])),
            r"C:\X\y"
        );
    }
}
