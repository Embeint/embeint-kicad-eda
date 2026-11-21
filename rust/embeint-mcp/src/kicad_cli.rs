// SPDX-License-Identifier: GPL-3.0-or-later
//! A port of `tools/live_schematic_mcp/kicad_cli.py`: run kicad-cli (a separate KiCad process)
//! with the environment its KiCad was launched with, limited to read-only commands.
//!
//! The allow-list, the refused options and outputs and their messages are Python's exactly: they
//! are the security boundary of `live_kicad_cli`.  kicad-cli runs under `tokio::process` and is
//! killed when it times out or the call is dropped, so no process outlives the tool call.  No
//! KiCad calls here: the caller supplies what KiCad reported.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use embeint_ipc::client::Error;
use regex::Regex;
use serde_json::{json, Map, Value as Json};
use tokio::io::AsyncReadExt;

use crate::footprint_gen::{py_str, py_str_repr};
use crate::model_handoff::{py_path, to_wsl};

/// The read-only commands live_kicad_cli may run and their allowed export kinds (None: the
/// command takes no kind), in Python's `COMMANDS` order.
const SCH_EXPORTS: &[&str] = &[
    "bom",
    "dxf",
    "hpgl",
    "netlist",
    "pdf",
    "png",
    "ps",
    "python-bom",
    "svg",
];
const PCB_EXPORTS: &[&str] = &[
    "3dpdf", "bom", "brep", "drill", "dxf", "gencad", "gerbers", "glb", "hpgl", "idf", "ipc2581",
    "ipcd356", "odb", "pdf", "ply", "png", "pos", "ps", "specctra", "stackup", "stats", "step",
    "stl", "stpz", "svg", "u3d", "vrml", "xao",
];
const COMMANDS: &[(&[&str], Option<&[&str]>)] = &[
    (&["sch", "erc"], None),
    (&["sch", "export"], Some(SCH_EXPORTS)),
    (&["pcb", "drc"], None),
    (&["pcb", "export"], Some(PCB_EXPORTS)),
    (&["sym", "export"], Some(&["svg"])),
    (&["fp", "export"], Some(&["svg"])),
    (&["version"], None),
];
/// Options that make an allowed command write into the project
const WRITE_OPTIONS: &[&str] = &["--save-board"];
/// Options whose value is a file or folder kicad-cli writes
const OUTPUT_OPTIONS: &[&str] = &["-o", "--output", "--models-dir", "--report-path"];
/// Outputs that would overwrite a design, project or library file
const PROTECTED_SUFFIXES: &[&str] = &[
    ".kicad_sch",
    ".kicad_pcb",
    ".kicad_pro",
    ".kicad_prl",
    ".kicad_sym",
    ".kicad_mod",
    ".kicad_dru",
];
const PROTECTED_NAMES: &[&str] = &["sym-lib-table", "fp-lib-table", "design-block-lib-table"];
pub const MAX_TIMEOUT_S: f64 = 600.0;
const OUTPUT_LIMIT: usize = 20000;

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a valid pattern"))
}

/// `WINDOWS_PATH`: `C:\...` or `C:/...`.
fn is_windows_path(text: &str) -> bool {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"^[A-Za-z]:[\\/]").is_match(text)
}

/// The MCP's environment for programs it starts (`child_environment`).  Windows variable names
/// are upper-cased, as Python's `os.environ` gives them there.  This native server is not a
/// PyInstaller bundle, so no private library path needs restoring.
pub fn child_environment() -> BTreeMap<String, String> {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .map(|(key, value)| {
            if cfg!(windows) {
                (key.to_uppercase(), value)
            } else {
                (key, value)
            }
        })
        .collect()
}

/// `Path(path).resolve()` as text: the absolute path with links resolved (without the `\\?\`
/// prefix Rust's canonical paths carry on Windows), or the path as it is if it cannot be resolved.
fn resolve(path: &str) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(resolved) => {
            let text = resolved.to_string_lossy().into_owned();
            if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
                PathBuf::from(format!(r"\\{rest}"))
            } else if let Some(rest) = text.strip_prefix(r"\\?\") {
                PathBuf::from(rest)
            } else {
                resolved
            }
        }
        Err(_) => std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path)),
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `setdefault(key, value)`.
fn set_default(env: &mut BTreeMap<String, String>, key: &str, value: String) {
    env.entry(key.to_owned()).or_insert(value);
}

/// `layout_environment`: what the launchers set up, from where kicad-cli is installed (when KiCad
/// cannot say).  Variables already set win.
fn layout_environment(
    cli_path: &str,
    mut env: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let resolved = resolve(cli_path);
    let install = resolved
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let root = install.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut libs = vec![text(&install.join("lib"))];
    let deps = root
        .join("deps")
        .join("usr")
        .join("lib")
        .join("x86_64-linux-gnu");
    if deps.is_dir() {
        libs.insert(0, text(&deps));
    }
    if !cfg!(windows) {
        let current: Vec<String> = env
            .get("LD_LIBRARY_PATH")
            .map(String::as_str)
            .unwrap_or("")
            .split(':')
            .filter(|p| !p.is_empty() && !libs.iter().any(|l| l == p))
            .map(str::to_owned)
            .collect();
        let joined: Vec<String> = libs.iter().cloned().chain(current).collect();
        env.insert("LD_LIBRARY_PATH".into(), joined.join(":"));
    }
    if install.join("libexec").join("environment.sh").is_file() {
        let config = match env.get("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            Some(config) => config.clone(),
            None => {
                let home = env.get("HOME").cloned().unwrap_or_else(home_dir);
                py_path(&format!("{home}/.config"))
            }
        };
        set_default(
            &mut env,
            "KICAD_CONFIG_HOME",
            py_path(&format!("{config}/embeint-kicad")),
        );
    } else if root.join("config").is_dir() {
        set_default(&mut env, "KICAD_CONFIG_HOME", text(&root.join("config")));
    }
    let stock = py_path(
        env.get("KICAD_LIBRARY_ROOT")
            .map(String::as_str)
            .unwrap_or("/mnt/c/Program Files/KiCad/10.0/share/kicad"),
    );
    for (kind, name) in [("symbols", "SYMBOL"), ("footprints", "FOOTPRINT")] {
        let folder = root.join("share").join(kind);
        if folder.is_dir() {
            for version in [10, 11] {
                set_default(
                    &mut env,
                    &format!("KICAD{version}_{name}_DIR"),
                    text(&folder),
                );
            }
        }
    }
    if root.join("share").join("symbols").is_dir() && Path::new(&stock).is_dir() {
        for (kind, name) in [("3dmodels", "3DMODEL"), ("template", "TEMPLATE")] {
            for version in [10, 11] {
                set_default(
                    &mut env,
                    &format!("KICAD{version}_{name}_DIR"),
                    text(&Path::new(&stock).join(kind)),
                );
            }
        }
    }
    env
}

/// `Path.home()`: USERPROFILE on Windows, HOME elsewhere.
fn home_dir() -> String {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var(key).unwrap_or_default()
}

/// `cli_environment`: (environment, description) for running kicad-cli.  KiCad's report wins; None
/// (KiCad could not say) or an empty report rebuilds the environment from the install layout.
pub fn cli_environment(
    cli_path: &str,
    kicad_variables: Option<&BTreeMap<String, String>>,
) -> (BTreeMap<String, String>, Map<String, Json>) {
    let mut env = child_environment();
    if let Some(variables) = kicad_variables.filter(|v| !v.is_empty()) {
        env.extend(variables.iter().map(|(k, v)| (k.clone(), v.clone())));
        let mut description = Map::new();
        description.insert("source".into(), json!("kicad"));
        description.insert(
            "variables".into(),
            json!(variables.keys().collect::<Vec<_>>()),
        );
        return (env, description);
    }
    let before = env.clone();
    let env = layout_environment(cli_path, env);
    let changed: Vec<&String> = env
        .iter()
        .filter(|(key, value)| before.get(*key) != Some(*value))
        .map(|(key, _)| key)
        .collect();
    let mut description = Map::new();
    description.insert("source".into(), json!("install layout"));
    description.insert("variables".into(), json!(changed));
    if kicad_variables.is_some() {
        description.insert(
            "note".into(),
            json!("KiCad reported an empty launch environment"),
        );
    }
    (env, description)
}

/// `Path.with_suffix(".exe")`.
fn with_exe_suffix(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = match name.rfind('.') {
        Some(index) if index > 0 && index < name.len() - 1 => &name[..index],
        _ => &name,
    };
    path.with_file_name(format!("{stem}.exe"))
}

/// `default_cli`: kicad-cli without asking KiCad: $KICAD_CLI, the installed package beside this
/// server, or the development build.  None if there is none.
///
/// The packaged locations are always tried: this server is a native binary installed where the
/// frozen Python server is (Python tries them only when frozen).
pub fn default_cli() -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(cli) = std::env::var("KICAD_CLI").ok().filter(|v| !v.is_empty()) {
        candidates.push(py_path(&cli));
    }
    if let Ok(exe) = std::env::current_exe() {
        let exe = resolve(&text(&exe));
        let parents: Vec<&Path> = exe.ancestors().skip(1).collect();
        // Windows bundle: install\mcp\embeint-mcp.exe -> install\bin\kicad-cli.exe; Linux package:
        // /opt/embeint-kicad/libexec/mcp/embeint-mcp -> /opt/embeint-kicad/bin/kicad-cli
        for index in [1, 2] {
            if let Some(parent) = parents.get(index) {
                candidates.push(text(&parent.join("bin").join("kicad-cli")));
            }
        }
    }
    let live_root = std::env::var("KICAD_LIVE_ROOT")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| home_dir());
            format!("{home}/kicad-live")
        });
    candidates.push(py_path(&format!("{live_root}/install/bin/kicad-cli")));
    for candidate in candidates {
        let candidate = PathBuf::from(candidate);
        for path in [candidate.clone(), with_exe_suffix(&candidate)] {
            if path.is_file() {
                return Some(text(&path));
            }
        }
    }
    None
}

/// The allowed commands, for messages: "sch erc, sch export <kind>, ...".
fn allowed_commands() -> String {
    COMMANDS
        .iter()
        .map(|(verb, kinds)| {
            format!(
                "{}{}",
                verb.join(" "),
                if kinds.is_some() { " <kind>" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn repr(text: &str) -> String {
    py_str_repr(text)
}

/// Refuses an output that would overwrite a design, project or library file, or land in a .pretty
/// footprint library (every file there is read as a footprint).
fn check_output(option: &str, value: &str) -> Result<(), Error> {
    let parts: Vec<&str> = value.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    let name = parts.last().map(|p| p.to_lowercase()).unwrap_or_default();
    if PROTECTED_SUFFIXES.iter().any(|s| name.ends_with(s))
        || PROTECTED_NAMES.contains(&name.as_str())
    {
        return Err(Error::Value(format!(
            "{option} {} would overwrite a design, project or library file; write kicad-cli outputs elsewhere",
            repr(value)
        )));
    }
    if parts.iter().any(|p| p.to_lowercase().ends_with(".pretty")) {
        return Err(Error::Value(format!(
            "{option} {} is inside a footprint library (.pretty folder); write kicad-cli outputs elsewhere",
            repr(value)
        )));
    }
    Ok(())
}

/// Refuses options that write into the project and checks every output target.  Combined short
/// options ("-no out") are refused because they can hide an output.
fn check_options(options: &[String]) -> Result<(), Error> {
    static NUMBER: OnceLock<Regex> = OnceLock::new();
    for (index, arg) in options.iter().enumerate() {
        let long_option = arg.starts_with("--");
        let (name, assign, value) = match (long_option, arg.split_once('=')) {
            (true, Some((name, value))) => (name, true, value),
            _ => (arg.as_str(), false, ""),
        };
        if WRITE_OPTIONS.contains(&name) {
            return Err(Error::Value(format!(
                "{name} writes into the project; live_kicad_cli only runs read-only commands"
            )));
        }
        // len(arg) counts characters; re.match(r"-[\d.]") lets negative numbers through
        if !long_option
            && arg.chars().count() > 2
            && arg.starts_with('-')
            && !regex(&NUMBER, r"^-[\d.]").is_match(arg)
        {
            return Err(Error::Value(format!(
                "combined short options are not accepted ({}); give each option separately",
                repr(arg)
            )));
        }
        if OUTPUT_OPTIONS.contains(&name) {
            if assign {
                check_output(name, value)?;
            } else if let Some(next) = options.get(index + 1) {
                check_output(name, next)?;
            }
        }
    }
    Ok(())
}

/// On Linux/WSL, `C:\...` (alone or as `--option=C:\...`) becomes `/mnt/c/...`.
fn translate_path(arg: &str) -> String {
    if is_windows_path(arg) {
        return to_wsl(arg);
    }
    if let (true, Some((name, value))) = (arg.starts_with("--"), arg.split_once('=')) {
        if is_windows_path(value) {
            return format!("{name}={}", to_wsl(value));
        }
    }
    arg.to_owned()
}

/// `cli_arguments`: validates kicad-cli arguments against the read-only allow-list and the
/// output rules; on Linux/WSL, Windows paths become /mnt/c/... .
pub fn cli_arguments(args: &[String]) -> Result<Vec<String>, Error> {
    if args.is_empty() {
        return Err(Error::Value(
            "args must be a non-empty list of strings, e.g. [\"sch\", \"erc\", \"design.kicad_sch\"]"
                .into(),
        ));
    }
    if args.iter().any(|a| a.contains('\0')) {
        return Err(Error::Value(
            "arguments must not contain NUL characters".into(),
        ));
    }
    let verb_len = if args[0] == "version" {
        1
    } else {
        2.min(args.len())
    };
    let verb = &args[..verb_len];
    let Some((_, kinds)) = COMMANDS.iter().find(|(allowed, _)| {
        allowed.len() == verb.len() && allowed.iter().zip(verb).all(|(a, b)| a == b)
    }) else {
        return Err(Error::Value(format!(
            "{} is not allowed; live_kicad_cli runs only read-only kicad-cli commands: {}",
            repr(&verb.join(" ")),
            allowed_commands()
        )));
    };
    let mut options = &args[verb_len..];
    if let Some(kinds) = kinds {
        if options
            .first()
            .is_none_or(|kind| !kinds.contains(&kind.as_str()))
        {
            return Err(Error::Value(format!(
                "{} needs one of these kinds next: {}",
                verb.join(" "),
                kinds.join(", ")
            )));
        }
        options = &options[1..];
    }
    check_options(options)?;
    if cfg!(windows) {
        Ok(args.to_vec())
    } else {
        Ok(args.iter().map(|a| translate_path(a)).collect())
    }
}

/// Python's text of an OSError from starting a process: `[WinError 267] The directory name is
/// invalid` on Windows, `[Errno 2] No such file or directory: '/x'` elsewhere.
pub fn os_error_text(error: &std::io::Error, filename: Option<&str>) -> String {
    let Some(code) = error.raw_os_error() else {
        return error.to_string();
    };
    let message = error.to_string();
    let message = message
        .strip_suffix(&format!(" (os error {code})"))
        .unwrap_or(&message);
    if cfg!(windows) {
        // Python strips trailing dots and whitespace from FormatMessage's text
        let message = message.trim_end_matches(|c: char| c == '.' || c.is_whitespace());
        match filename {
            Some(name) => format!("[WinError {code}] {message}: {}", repr(name)),
            None => format!("[WinError {code}] {message}"),
        }
    } else {
        match filename {
            Some(name) => format!("[Errno {code}] {message}: {}", repr(name)),
            None => format!("[Errno {code}] {message}"),
        }
    }
}

/// Process output as text, as `decode()` gives it: invalid bytes replaced, CRLF as LF.  (Python
/// decodes with the locale's preferred encoding; this is UTF-8, which is the same for the ASCII
/// kicad-cli prints and the encoding of Linux locales.)
fn decode(data: &[u8]) -> String {
    String::from_utf8_lossy(data).replace("\r\n", "\n")
}

/// The last 20000 characters, and whether it was cut.
fn tail(text: &str) -> (String, bool) {
    let count = text.chars().count();
    if count > OUTPUT_LIMIT {
        (text.chars().skip(count - OUTPUT_LIMIT).collect(), true)
    } else {
        (text.to_owned(), false)
    }
}

/// What a finished process gave: exit code (negative signal number when killed by a signal, as
/// Python reports it), stdout and stderr bytes.
pub struct Finished {
    pub returncode: i64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn exit_code(status: std::process::ExitStatus) -> i64 {
    if let Some(code) = status.code() {
        return i64::from(code);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return -i64::from(signal);
        }
    }
    -1
}

/// Runs one process with stdin from the null device and both outputs captured, killed when it
/// does not finish within `timeout` (Err(None)) or when this future is dropped.  A start failure
/// is Err(Some(text)) with Python's OSError text.
pub async fn run_process(
    command: &[String],
    env: &BTreeMap<String, String>,
    cwd: Option<&str>,
    timeout: Duration,
) -> Result<Finished, Option<String>> {
    let mut process = tokio::process::Command::new(&command[0]);
    process
        .args(&command[1..])
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    let mut child = process.spawn().map_err(|error| {
        // Python names the folder when changing into it failed, else the program (POSIX only)
        let filename = if cfg!(windows) {
            None
        } else if cwd.is_some_and(|c| !Path::new(c).is_dir()) {
            cwd
        } else {
            Some(command[0].as_str())
        };
        Some(os_error_text(&error, filename))
    })?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let finished = tokio::time::timeout(timeout, async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let read_out = async {
            if let Some(pipe) = stdout.as_mut() {
                let _ = pipe.read_to_end(&mut out).await;
            }
        };
        let read_err = async {
            if let Some(pipe) = stderr.as_mut() {
                let _ = pipe.read_to_end(&mut err).await;
            }
        };
        let ((), (), status) = tokio::join!(read_out, read_err, child.wait());
        (out, err, status)
    })
    .await;
    match finished {
        Ok((out, err, Ok(status))) => Ok(Finished {
            returncode: exit_code(status),
            stdout: out,
            stderr: err,
        }),
        Ok((_, _, Err(error))) => Err(Some(os_error_text(&error, None))),
        Err(_) => {
            let _ = child.kill().await;
            Err(None)
        }
    }
}

/// `run_async`: runs kicad-cli once after validating everything; returns its exit code and the
/// last 20000 characters of each output.  `timeout_s` is the argument as given (an int default
/// prints as `300`, a float as `300.0`, as Python formats it).
pub async fn run(
    cli_path: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    cwd: Option<&str>,
    timeout_s: &Json,
) -> Result<Map<String, Json>, Error> {
    let seconds = timeout_s.as_f64().unwrap_or(f64::NAN);
    if !(seconds > 0.0 && seconds <= MAX_TIMEOUT_S) {
        return Err(Error::Value(format!(
            "timeout_s must be between 0 and {} seconds",
            MAX_TIMEOUT_S as i64
        )));
    }
    let mut command = vec![cli_path.to_owned()];
    command.extend(cli_arguments(args)?);
    let cwd = cwd.map(|c| {
        if cfg!(windows) {
            c.to_owned()
        } else {
            to_wsl(c)
        }
    });
    let finished = run_process(
        &command,
        env,
        cwd.as_deref(),
        Duration::from_secs_f64(seconds),
    )
    .await
    .map_err(|error| match error {
        Some(text) => Error::Runtime(text),
        None => Error::Runtime(format!(
            "kicad-cli did not finish within {} s and was stopped",
            py_str(timeout_s)
        )),
    })?;
    let (stdout, stdout_cut) = tail(&decode(&finished.stdout));
    let (stderr, stderr_cut) = tail(&decode(&finished.stderr));
    let mut result = Map::new();
    result.insert("command".into(), json!(command));
    result.insert("returncode".into(), json!(finished.returncode));
    result.insert("stdout".into(), json!(stdout));
    result.insert("stderr".into(), json!(stderr));
    if stdout_cut {
        result.insert("stdout_truncated".into(), json!(true));
    }
    if stderr_cut {
        result.insert("stderr_truncated".into(), json!(true));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn refused(items: &[&str]) -> String {
        cli_arguments(&args(items)).unwrap_err().to_string()
    }

    /// Texts checked against kicad_cli.cli_arguments in the MCP interpreter.
    #[test]
    fn the_allow_list_and_refusals_are_pythons() {
        assert!(cli_arguments(&args(&["version"])).is_ok());
        assert!(
            cli_arguments(&args(&["sch", "erc", "-o", "/tmp/erc.json", "d.kicad_sch"])).is_ok()
        );
        assert!(cli_arguments(&args(&[
            "pcb",
            "export",
            "gerbers",
            "--output=/tmp/g",
            "b.kicad_pcb"
        ]))
        .is_ok());
        assert!(cli_arguments(&args(&["pcb", "drc", "--severity-error", "-1"])).is_ok());
        assert_eq!(
            refused(&["jobset", "run", "x"]),
            "'jobset run' is not allowed; live_kicad_cli runs only read-only kicad-cli commands: \
             sch erc, sch export <kind>, pcb drc, pcb export <kind>, sym export <kind>, fp export <kind>, version"
        );
        assert!(refused(&["sch"]).starts_with("'sch' is not allowed"));
        assert_eq!(
            refused(&["sym", "export", "png"]),
            "sym export needs one of these kinds next: svg"
        );
        assert_eq!(
            refused(&["pcb", "drc", "--save-board", "b.kicad_pcb"]),
            "--save-board writes into the project; live_kicad_cli only runs read-only commands"
        );
        assert_eq!(
            refused(&["pcb", "drc", "--save-board=yes"]),
            "--save-board writes into the project; live_kicad_cli only runs read-only commands"
        );
        assert_eq!(
            refused(&["sch", "erc", "-no", "x"]),
            "combined short options are not accepted ('-no'); give each option separately"
        );
        assert_eq!(
            refused(&["sch", "erc", "-o", "C:\\p\\d.KICAD_SCH"]),
            "-o 'C:\\\\p\\\\d.KICAD_SCH' would overwrite a design, project or library file; write kicad-cli outputs elsewhere"
        );
        assert_eq!(
            refused(&["fp", "export", "svg", "--output=lib.pretty/x.svg"]),
            "--output 'lib.pretty/x.svg' is inside a footprint library (.pretty folder); write kicad-cli outputs elsewhere"
        );
        assert!(
            refused(&["sch", "erc", "--report-path", "fp-lib-table"]).contains("would overwrite")
        );
        assert_eq!(refused(&[]), "args must be a non-empty list of strings, e.g. [\"sch\", \"erc\", \"design.kicad_sch\"]");
        assert_eq!(
            refused(&["version", "a\0b"]),
            "arguments must not contain NUL characters"
        );
    }

    #[test]
    fn windows_paths_are_translated_for_wsl() {
        assert_eq!(translate_path(r"C:\x\y.kicad_sch"), "/mnt/c/x/y.kicad_sch");
        assert_eq!(translate_path(r"--output=D:\out"), "--output=/mnt/d/out");
        assert_eq!(translate_path(r"-o=D:\out"), "-o=D:\\out");
        assert_eq!(translate_path("relative"), "relative");
    }

    #[test]
    fn output_tails() {
        let long = "x".repeat(OUTPUT_LIMIT + 5);
        assert_eq!(tail(&long).0.len(), OUTPUT_LIMIT);
        assert!(tail(&long).1);
        assert_eq!(decode(b"a\r\nb\rc"), "a\nb\rc");
        assert_eq!(
            with_exe_suffix(Path::new("a/kicad-cli")),
            Path::new("a/kicad-cli.exe")
        );
        assert_eq!(
            with_exe_suffix(Path::new("a/kicad-cli.cmd")),
            Path::new("a/kicad-cli.exe")
        );
    }
}
