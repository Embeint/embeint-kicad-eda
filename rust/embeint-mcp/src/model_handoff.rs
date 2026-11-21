// SPDX-License-Identifier: GPL-3.0-or-later
//! A port of `tools/live_schematic_mcp/model_handoff.py`: hand a footprint's 3D model to
//! SolidCraft and bring the STEP back, and check a saved footprint's model in KiCad.
//!
//! The plan is a SolidCraft MCP `batch` that builds the package from datasheet numbers in KiCad's
//! 3D model conventions, plus the `export` call that writes the STEP (see the Python module for
//! the conventions).  Embedded files are decoded as KiCad stores them: base64 of a zstd stream
//! (`base64` and `ruzstd`).  The checks run kicad-cli on a scratch board in a temporary folder.
//!
//! Text files are read and written as Python's text mode does: universal newlines when reading,
//! and `\n` written as `\r\n` on Windows.  Python uses the locale's encoding for them; this uses
//! UTF-8, KiCad's encoding (the same bytes for ASCII content, and on Linux).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine as _;
use embeint_ipc::client::Error;
use regex::{NoExpand, Regex};
use serde_json::{json, Map, Value as Json};

use crate::footprint_gen::{self, py_str, py_str_repr, r, truthy, Num};
use crate::kicad_cli::{os_error_text, run_process, Finished};

const BODY_COLOR: &str = "#1f1f1f";
const LEAD_COLOR: &str = "#c9ccd1";
/// subprocess.run(..., timeout=300) for the render and STEP export
const CHECK_TIMEOUT_S: u64 = 300;

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a valid pattern"))
}

/// `C:\Users\x\m.step` -> `/mnt/c/Users/x/m.step` (other paths unchanged).
pub fn to_wsl(path: &str) -> String {
    static CELL: OnceLock<Regex> = OnceLock::new();
    match regex(&CELL, r"^([A-Za-z]):[\\/](.*)$").captures(path) {
        Some(c) => format!("/mnt/{}/{}", c[1].to_lowercase(), c[2].replace('\\', "/")),
        None => path.to_owned(),
    }
}

/// `/mnt/c/Users/x/m.step` -> `C:\Users\x\m.step` (other paths unchanged).
pub fn to_windows(path: &str) -> String {
    static CELL: OnceLock<Regex> = OnceLock::new();
    match regex(&CELL, r"^/mnt/([a-z])/(.*)$").captures(path) {
        Some(c) => format!("{}:\\{}", c[1].to_uppercase(), c[2].replace('/', "\\")),
        None => path.to_owned(),
    }
}

/// The path as the KiCad this MCP drives reads it: natively on Windows a WSL `/mnt/x` path is
/// mapped back to `X:\`, inside WSL a Windows path is mapped to `/mnt/x`.
pub fn kicad_path(path: &str) -> String {
    if cfg!(windows) {
        to_windows(path)
    } else {
        to_wsl(path)
    }
}

/// `str(Path(text))`: the path in Python's normal form (Windows: backslashes, repeated
/// separators and `.` parts removed, no trailing separator; POSIX likewise with `/`, keeping
/// exactly two leading slashes).  The empty path is `.`.
pub fn py_path(text: &str) -> String {
    if cfg!(windows) {
        let text = text.replace('/', "\\");
        let bytes = text.as_bytes();
        let (prefix, rest) =
            if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
                (text[..2].to_owned(), &text[2..])
            } else if text.starts_with(r"\\") && !text.starts_with(r"\\\") {
                (r"\".to_owned(), &text[1..])
            } else {
                (String::new(), text.as_str())
            };
        let root = rest.starts_with('\\');
        let parts: Vec<&str> = rest
            .split('\\')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        let joined = format!(
            "{prefix}{}{}",
            if root { "\\" } else { "" },
            parts.join("\\")
        );
        if joined.is_empty() {
            ".".into()
        } else {
            joined
        }
    } else {
        let lead = if text.starts_with("//") && !text.starts_with("///") {
            "//"
        } else if text.starts_with('/') {
            "/"
        } else {
            ""
        };
        let parts: Vec<&str> = text
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        let joined = format!("{lead}{}", parts.join("/"));
        if joined.is_empty() {
            ".".into()
        } else {
            joined
        }
    }
}

/// `str(Path(base) / part)`.
pub fn py_join(base: &str, part: &str) -> String {
    let base = py_path(base);
    if base == "." {
        py_path(part)
    } else {
        py_path(&format!("{base}/{part}"))
    }
}

/// `Path(path).suffix`: from the last dot of the final component, not a leading one, and not a
/// trailing one.
pub fn py_suffix(path: &str) -> String {
    let separators: &[char] = if cfg!(windows) { &['\\', '/'] } else { &['/'] };
    let trimmed = path.trim_end_matches(separators);
    let name = trimmed.rsplit(separators).next().unwrap_or("");
    let name = if cfg!(windows) {
        // a drive ("C:name") is not part of the name
        name.split_once(':').map_or(name, |(_, rest)| rest)
    } else {
        name
    };
    match name.rfind('.') {
        Some(index) if index > 0 && index + 1 < name.len() => name[index..].to_owned(),
        _ => String::new(),
    }
}

/// Python's text-mode read: universal newlines.
pub fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `Path.read_text()`: the file's text with universal newlines; a failure has Python's OSError
/// text (`[Errno 2] No such file or directory: '...'`).
pub fn read_text(path: &str) -> Result<String, Error> {
    let bytes = std::fs::read(path).map_err(|error| Error::Runtime(file_error(&error, path)))?;
    Ok(universal_newlines(&String::from_utf8_lossy(&bytes)))
}

/// Python's text of an OSError for a file operation on `path` (a `Path`, so its name is in
/// Python's normal form).
pub fn file_error(error: &std::io::Error, path: &str) -> String {
    let path = py_path(path);
    let path = path.as_str();
    match error.kind() {
        // Python reports these with the errno text, on every host
        std::io::ErrorKind::NotFound => {
            format!("[Errno 2] No such file or directory: {}", py_str_repr(path))
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("[Errno 13] Permission denied: {}", py_str_repr(path))
        }
        _ => os_error_text(error, Some(path)),
    }
}

/// `Path.write_text(text)`: `\n` is written as the platform's line separator.
fn write_text(path: &Path, text: &str) -> Result<(), Error> {
    let data = if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    };
    std::fs::write(path, data)
        .map_err(|error| Error::Runtime(file_error(&error, &path.to_string_lossy())))
}

// ---- Checking a footprint's 3D model in KiCad -----------------------------------------------

/// `_balanced`: the parenthesised expression starting at `start`, honouring quoted strings and
/// `|...|` data.
fn balanced(text: &str, start: usize) -> Result<&str, Error> {
    let (mut depth, mut quoted, mut barred, mut escaped) = (0i64, false, false, false);
    for (offset, ch) in text[start..].char_indices() {
        if barred {
            barred = ch != '|';
        } else if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else {
                quoted = ch != '"';
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == '|' {
            barred = true;
        } else if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Ok(&text[start..start + offset + 1]);
            }
        }
    }
    Err(Error::Value("unbalanced expression".into()))
}

/// Python's `float(text)` for the coordinate texts footprint_board finds (`-?[\d.]+`).
fn py_float(text: &str) -> Result<f64, Error> {
    let ascii = text
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '-');
    text.parse::<f64>().ok().filter(|_| ascii).ok_or_else(|| {
        Error::Value(format!(
            "could not convert string to float: {}",
            py_str_repr(text)
        ))
    })
}

/// `footprint_board`: a minimal board holding one library footprint at the origin, with a board
/// outline around it.  The footprint keeps everything; only the library header tokens a board
/// does not take are dropped.
pub fn footprint_board(footprint_text: &str, lib_id: &str, margin: f64) -> Result<String, Error> {
    static VERSION: OnceLock<Regex> = OnceLock::new();
    static NAME: OnceLock<Regex> = OnceLock::new();
    static LAYER: OnceLock<Regex> = OnceLock::new();
    static POINTS: OnceLock<Regex> = OnceLock::new();
    static TOKENS: [OnceLock<Regex>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let mut text = footprint_text.trim().to_owned();
    let board_version = regex(&VERSION, r"\(version\s+(\d+)\)")
        .captures(&text)
        .map_or_else(|| "20250114".to_owned(), |c| c[1].to_owned());
    if text.starts_with("(kicad_footprint_lib") {
        // KiCad 11 library files wrap the footprint: keep just the (footprint ...) expression
        let start = text[1..]
            .find("(footprint")
            .map(|i| i + 1)
            .ok_or_else(|| Error::Value("substring not found".into()))?;
        text = balanced(&text, start)?.to_owned();
    }
    // The replacement is literal (Python would read backslash escapes in a lib_id)
    text = regex(&NAME, r#"^\(footprint\s+("[^"]*"|\S+)"#)
        .replacen(&text, 1, NoExpand(&format!("(footprint \"{lib_id}\"")))
        .into_owned();
    for (cell, token) in TOKENS
        .iter()
        .zip(["version", "generator_version", "generator"])
    {
        let pattern = format!(r#"\n?\s*\({token}\s+("[^"]*"|[^\s()]+)\)"#);
        text = regex(cell, &pattern).replacen(&text, 1, "").into_owned();
    }
    text = regex(&LAYER, r#"\(layer\s+"F\.Cu"\)"#)
        .replacen(&text, 1, NoExpand("(layer \"F.Cu\")\n  (at 0 0)"))
        .into_owned();
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for c in regex(&POINTS, r"\((?:start|end|at|xy)\s+(-?[\d.]+)\s+(-?[\d.]+)").captures_iter(&text)
    {
        xs.push(py_float(&c[1])?);
        ys.push(py_float(&c[2])?);
    }
    let fold = |values: &[f64], default: f64, smaller: bool| -> f64 {
        values
            .iter()
            .skip(1)
            .fold(values.first().copied().unwrap_or(default), |acc, v| {
                if (smaller && *v < acc) || (!smaller && *v > acc) {
                    *v
                } else {
                    acc
                }
            })
    };
    let x1 = fold(&xs, -5.0, true) - margin;
    let x2 = fold(&xs, 5.0, false) + margin;
    let y1 = fold(&ys, -5.0, true) - margin;
    let y2 = fold(&ys, 5.0, false) + margin;
    Ok(format!(
        "(kicad_pcb\n  (version {board_version})\n  (generator \"kicad-live-mcp\")\n  (generator_version \"10.99\")\n\
         \x20 (general (thickness 1.6) (legacy_teardrops no))\n  (paper \"A4\")\n\
         {text}\n\
         \x20 (gr_rect (start {x1:.3} {y1:.3}) (end {x2:.3} {y2:.3}) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\"))\n)\n"
    ))
}

/// Python's `str(subprocess.TimeoutExpired)` for one of these runs.
fn timeout_text(command: &[String]) -> String {
    let items: Vec<String> = command.iter().map(|c| py_str_repr(c)).collect();
    let list = format!("[{}]", items.join(", "));
    format!("Command '{list}' timed out after {CHECK_TIMEOUT_S} seconds")
}

/// `subprocess.run(command, env=env, capture_output=True, text=True, timeout=300)`.
async fn run_text(
    command: &[String],
    env: &BTreeMap<String, String>,
) -> Result<(i64, String, String), Error> {
    let Finished {
        returncode,
        stdout,
        stderr,
    } = run_process(command, env, None, Duration::from_secs(CHECK_TIMEOUT_S))
        .await
        .map_err(|error| Error::Runtime(error.unwrap_or_else(|| timeout_text(command))))?;
    let text = |data: &[u8]| universal_newlines(&String::from_utf8_lossy(data));
    Ok((returncode, text(&stdout), text(&stderr)))
}

/// The last 800 characters (`text[-800:]`).
fn last_800(text: &str) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(800)).collect()
}

fn scratch() -> Result<tempfile::TempDir, Error> {
    tempfile::Builder::new()
        .prefix("kicad-3d-check-")
        .tempdir()
        .map_err(|error| Error::Runtime(os_error_text(&error, None)))
}

/// `render_footprint_3d`: one footprint's 3D view through kicad-cli; returns PNG bytes.
pub async fn render_footprint_3d(
    cli_path: &str,
    footprint_text: &str,
    lib_id: &str,
    rotate: &str,
    zoom: &Json,
    env: &BTreeMap<String, String>,
) -> Result<Vec<u8>, Error> {
    let folder = scratch()?;
    let board = folder.path().join("check.kicad_pcb");
    let image = folder.path().join("check.png");
    write_text(&board, &footprint_board(footprint_text, lib_id, 2.0)?)?;
    let mut command: Vec<String> = [
        cli_path,
        "pcb",
        "render",
        "-w",
        "900",
        "-h",
        "700",
        "--quality",
        "high",
        "--perspective",
        "--zoom",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    command.push(py_str(zoom));
    command.extend(["--background", "opaque", "-o"].map(str::to_owned));
    command.push(image.to_string_lossy().into_owned());
    command.push(board.to_string_lossy().into_owned());
    if !rotate.is_empty() {
        command.splice(3..3, ["--rotate".to_owned(), rotate.to_owned()]);
    }
    let (returncode, stdout, stderr) = run_text(&command, env).await?;
    if returncode != 0 || !image.exists() {
        let tail = last_800(&stderr);
        let detail = if tail.is_empty() {
            last_800(&stdout)
        } else {
            tail
        };
        return Err(Error::Runtime(format!(
            "kicad-cli render failed ({returncode}): {detail}"
        )));
    }
    std::fs::read(&image)
        .map_err(|error| Error::Runtime(file_error(&error, &image.to_string_lossy())))
}

/// `export_board_step`: the footprint's board as STEP text from kicad-cli (a separate KiCad
/// process), so the output proves KiCad itself resolves the footprint's 3D model.
pub async fn export_board_step(
    cli_path: &str,
    footprint_text: &str,
    lib_id: &str,
    env: &BTreeMap<String, String>,
) -> Result<String, Error> {
    let folder = scratch()?;
    let board = folder.path().join("check.kicad_pcb");
    let output = folder.path().join("check.step");
    write_text(&board, &footprint_board(footprint_text, lib_id, 2.0)?)?;
    let command: Vec<String> = vec![
        cli_path.to_owned(),
        "pcb".into(),
        "export".into(),
        "step".into(),
        "--force".into(),
        "-o".into(),
        output.to_string_lossy().into_owned(),
        board.to_string_lossy().into_owned(),
    ];
    let (returncode, stdout, stderr) = run_text(&command, env).await?;
    if returncode != 0 || !output.exists() {
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(Error::Runtime(format!(
            "kicad-cli STEP export failed ({returncode}): {}",
            last_800(&detail)
        )));
    }
    read_text(&output.to_string_lossy())
}

/// `default_model_dir`: a folder both SolidCraft (Windows) and KiCad can reach, as KiCad sees it.
pub async fn default_model_dir() -> Result<String, Error> {
    if let Some(folder) = std::env::var("KICAD_LIVE_MODEL_DIR")
        .ok()
        .filter(|v| !v.is_empty())
    {
        return Ok(folder);
    }
    static PROFILE: OnceLock<Regex> = OnceLock::new();
    let profile_pattern = regex(&PROFILE, r"^[A-Za-z]:\\");
    if cfg!(windows) {
        let profile = std::env::var("USERPROFILE").unwrap_or_default();
        if profile_pattern.is_match(&profile) {
            return Ok(format!("{}\\kicad-models", profile.trim_end_matches('\\')));
        }
        return Err(Error::Runtime(
            "Set KICAD_LIVE_MODEL_DIR to a folder for SolidCraft's STEP exports".into(),
        ));
    }
    // Inside WSL: ask Windows for the profile folder (any failure counts as no answer)
    let command: Vec<String> = ["cmd.exe", "/c", "echo %USERPROFILE%"]
        .map(str::to_owned)
        .to_vec();
    let env = crate::kicad_cli::child_environment();
    let profile = match run_process(&command, &env, Some("/mnt/c"), Duration::from_secs(20)).await {
        Ok(finished) => universal_newlines(&String::from_utf8_lossy(&finished.stdout))
            .trim()
            .to_owned(),
        Err(_) => String::new(),
    };
    if profile_pattern.is_match(&profile) {
        return Ok(to_wsl(&profile) + "/kicad-models");
    }
    Err(Error::Runtime(
        "Set KICAD_LIVE_MODEL_DIR to a folder on a Windows drive (e.g. /mnt/c/Users/me/kicad-models)"
            .into(),
    ))
}

/// `embedded_file_bytes`: one embedded file from a saved .kicad_mod, base64 of a zstd stream.
pub fn embedded_file_bytes(footprint_text: &str, name: &str) -> Result<Vec<u8>, Error> {
    static PART: OnceLock<Regex> = OnceLock::new();
    let pattern = format!(
        r#"(?s)\(file\s+\(name\s+"{}"\).*?\(data\s+((?:\|[^|]*\|\s*)+)\)"#,
        regex::escape(name)
    );
    let matched = Regex::new(&pattern)
        .ok()
        .and_then(|re| re.captures(footprint_text).map(|c| c[1].to_owned()))
        .ok_or_else(|| Error::Value(format!("no embedded file named {name}")))?;
    let encoded: String = regex(&PART, r"\|([^|]*)\|")
        .captures_iter(&matched)
        .map(|c| c[1].trim().to_owned())
        .collect();
    // b64decode without validation discards characters outside the alphabet
    let cleaned: String = encoded
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
        .collect();
    // (binascii's own error texts for malformed data are not reproduced)
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .map_err(|error| Error::Runtime(format!("Invalid base64-encoded string: {error}")))?;
    // zstandard's decompress(max_output_size=1 << 30): a crafted file (4 GiB of zeros compresses
    // to about 131 KB) must not exhaust memory
    zstd_decompress(&compressed, 1 << 30)
}

/// Every frame of a zstd stream, refused once the output passes `limit` bytes.
fn zstd_decompress(compressed: &[u8], limit: u64) -> Result<Vec<u8>, Error> {
    let zstd_error = |error: &dyn std::fmt::Display| {
        Error::Runtime(format!("zstd decompression error: {error}"))
    };
    let mut source: &[u8] = compressed;
    let mut data = Vec::new();
    loop {
        let decoder =
            ruzstd::decoding::StreamingDecoder::new(&mut source).map_err(|e| zstd_error(&e))?;
        let room = limit - data.len() as u64;
        std::io::Read::read_to_end(&mut std::io::Read::take(decoder, room + 1), &mut data)
            .map_err(|e| zstd_error(&e))?;
        if data.len() as u64 > limit {
            return Err(zstd_error(&format!(
                "decompressed data exceeds {limit} bytes"
            )));
        }
        if source.is_empty() {
            break;
        }
    }
    Ok(data)
}

// ---- The SolidCraft plan --------------------------------------------------------------------

fn n(value: f64) -> Json {
    Num::Float(value).to_json()
}

fn box_steps(name: &str, x1: f64, y1: f64, x2: f64, y2: f64, z0: f64, height: f64) -> Vec<Json> {
    vec![
        json!({"command": "sketch.create", "params": {"plane": "XY", "offset": n(r(z0))}}),
        json!({"command": "sketch.rectangle.two_point",
               "params": {"p0": [n(r(x1)), n(r(y1))], "p1": [n(r(x2)), n(r(y2))]}}),
        json!({"command": "sketch.finish", "params": {}}),
        json!({"command": "solid.extrude",
               "params": {"distance": n(r(height)), "operation": "new", "body_name": name}}),
    ]
}

/// `_gullwing_lead`: profile in the XZ plane at model y, extruded symmetrically by its width.
#[allow(clippy::too_many_arguments)]
fn gullwing_lead(
    name: &str,
    side: i64,
    y: Num,
    body_edge: f64,
    tip: f64,
    foot: Num,
    thickness: f64,
    width: Num,
    exit_z: f64,
) -> Vec<Json> {
    let s = Num::Int(if side > 0 { 1 } else { -1 });
    let (xa, xb) = (body_edge - 0.1, body_edge + 0.25);
    let (xc, xd) = (Num::Float(tip).sub(foot).f(), tip);
    let t = thickness;
    // Centre-line A (inside the body) -> B (shoulder) -> C (knee on the board) -> D (toe)
    let profile = [
        (xa, exit_z + t / 2.0),
        (xb + t / 2.0, exit_z + t / 2.0),
        (xc + t / 2.0, t),
        (xd, t),
        (xd, 0.0),
        (xc - t / 2.0, 0.0),
        (xb - t / 2.0, exit_z - t / 2.0),
        (xa, exit_z - t / 2.0),
    ];
    let points: Vec<Json> = profile
        .iter()
        .map(|(x, z)| json!([s.mul(*x).r().to_json(), n(r(*z))]))
        .collect();
    vec![
        json!({"command": "sketch.create",
               "params": {"plane": {"origin": [0, y.r().to_json(), 0], "x_dir": [1, 0, 0], "y_dir": [0, 0, 1]}}}),
        json!({"command": "sketch.line", "params": {"points": points, "closed": true}}),
        json!({"command": "sketch.finish", "params": {}}),
        json!({"command": "solid.extrude", "params": {"distance": width.r().to_json(), "direction": "symmetric",
                                                      "operation": "new", "body_name": name}}),
    ]
}

/// The model plan's dimensions from the datasheet (live_footprint_model_plan's arguments).
pub struct PlanArgs<'a> {
    pub package: &'a str,
    pub params: &'a Map<String, Json>,
    pub body_height: f64,
    pub standoff: f64,
    pub lead_thickness: f64,
    pub lead_width: Option<f64>,
    pub lead_span: Option<f64>,
    pub foot_length: Option<f64>,
    pub export_path: Option<String>,
}

/// `params.get(key)` as a number when it is one (None when absent or null).
fn param_num(params: &Map<String, Json>, key: &str) -> Result<Option<Num>, Error> {
    match params.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => footprint_gen::num(value).map(Some),
    }
}

/// `model_plan`: SolidCraft batch steps for the package footprint_gen builds from the same params.
pub fn model_plan(a: &PlanArgs<'_>) -> Result<Json, Error> {
    let shape = footprint_gen::generate(a.package, a.params)?;
    let [bx1, by1, bx2, by2] = shape.body;
    let (standoff, body_height) = (a.standoff, a.body_height);
    // Footprint body is in footprint coordinates; flip Y into model coordinates
    let mut steps = box_steps(
        "Body",
        bx1,
        -by2,
        bx2,
        -by1,
        standoff,
        body_height - standoff,
    );
    let mut lead_names: Vec<String> = Vec::new();
    let key = a.package.to_lowercase();
    let pads: Vec<&footprint_gen::Pad> = shape.pads.iter().filter(|p| p.kind == "smd").collect();

    if ["soic", "tssop", "msop", "ssop", "gullwing", "sot"].contains(&key.as_str()) {
        // lead_span or params.get("lead_span"): a falsy lead_span falls through to params
        let span = match a.lead_span.filter(|v| *v != 0.0) {
            Some(span) => Some(Num::Float(span)),
            None => param_num(a.params, "lead_span")?,
        };
        let span = match span {
            Some(span) => span,
            None => {
                // The land pattern's outer pad edges sit TOE beyond the lead tips
                let outer = pads
                    .iter()
                    .map(|p| p.position.0.abs().f() + p.size.0.half())
                    .reduce(|acc, v| if v > acc { v } else { acc })
                    .ok_or_else(|| Error::Value("max() iterable argument is empty".into()))?;
                Num::Float(2.0 * (outer - 0.35))
            }
        };
        let foot = match a.foot_length.filter(|v| *v != 0.0) {
            Some(foot) => Num::Float(foot),
            None => match a.params.get("lead_length") {
                None => Num::Float(0.6),
                Some(value) => footprint_gen::num(value)?,
            },
        };
        let exit_z = standoff + 0.45 * (body_height - standoff);
        for pad in &pads {
            let (x, y) = pad.position;
            if x.abs().f() < 1e-6 {
                continue; // centred exposed pad: modelled below
            }
            let name = format!("Lead{}", pad.number);
            let pad_w = pad.size.1;
            let mut width = match a.lead_width.filter(|v| *v != 0.0) {
                Some(width) => Num::Float(width),
                None => match a.params.get("lead_width").filter(|v| truthy(v)) {
                    Some(value) => footprint_gen::num(value)?,
                    None => pad_w.mul(0.7),
                },
            };
            if pad_w.gt(Num::Float(2.5).mul(width)) {
                // a tab (SOT-223 pin 4) is as wide as most of its pad
                width = pad_w.mul(0.85);
            }
            let right = x.gt(0);
            let edge = if right { bx2 } else { -bx1 };
            steps.extend(gullwing_lead(
                &name,
                if right { 1 } else { -1 },
                y.neg(),
                edge.abs(),
                span.half(),
                foot,
                a.lead_thickness,
                width,
                exit_z,
            ));
            lead_names.push(name);
        }
    } else if key == "qfn" || key == "dfn" {
        // No standoff: the body sits on the board with its terminals flush underneath
        steps = box_steps("Body", bx1, -by2, bx2, -by1, 0.0, body_height);
        let pick = |a: f64, b: f64, larger: bool| {
            if (larger && b > a) || (!larger && b < a) {
                b
            } else {
                a
            }
        };
        for pad in &pads {
            let ((x, y), (w, h)) = (pad.position, pad.size);
            let (tx1, tx2) = (
                pick(x.f() - w.half(), bx1, true),
                pick(x.f() + w.half(), bx2, false),
            );
            let (ty1, ty2) = (
                pick(y.f() - h.half(), by1, true),
                pick(y.f() + h.half(), by2, false),
            );
            let name = format!("Pad{}", pad.number);
            steps.extend(box_steps(&name, tx1, -ty2, tx2, -ty1, 0.0, 0.05));
            lead_names.push(name);
        }
    } else if key == "chip" {
        let length = bx2 - bx1;
        let quarter = 0.25 * length;
        let cap = if quarter < 0.5 { quarter } else { 0.5 };
        steps = box_steps("Body", bx1 + cap, -by2, bx2 - cap, -by1, 0.0, body_height);
        for (name, x1, x2) in [("Cap1", bx1, bx1 + cap), ("Cap2", bx2 - cap, bx2)] {
            steps.extend(box_steps(name, x1, -by2, x2, -by1, 0.0, body_height));
            lead_names.push(name.to_owned());
        }
    }

    if shape.pin1_marker.as_ref().is_some_and(|m| !m.is_empty()) && key != "chip" {
        // Pin-1 dimple near the pin-1 corner of the top face (model +Y is footprint -Y)
        let (cx, cy) = (bx1 + 0.6, -(by1 + 0.6));
        steps.extend([
            json!({"command": "sketch.create", "params": {"plane": "XY", "offset": n(r(body_height))}}),
            json!({"command": "sketch.circle.center", "params": {"center": [n(r(cx)), n(r(cy))], "radius": 0.25}}),
            json!({"command": "sketch.finish", "params": {}}),
            json!({"command": "solid.extrude", "params": {"distance": 0.05, "direction": "negative",
                                                          "operation": "cut", "targets": ["Body"]}}),
        ]);
    }

    steps.push(json!({"command": "appearance.assign", "params": {"bodies": ["Body"], "color": BODY_COLOR}}));
    if !lead_names.is_empty() {
        steps.push(json!({"command": "appearance.assign", "params": {"bodies": lead_names, "color": LEAD_COLOR}}));
    }
    let mut bodies = vec![json!("Body")];
    bodies.extend(lead_names.iter().map(|name| json!(name)));
    let mut plan = json!({
        "footprint": shape.name,
        "conventions": "mm; origin = footprint origin on the board top surface; +Z up; model +Y = \
                        footprint -Y (pin 1 top-left in KiCad is at model x<0, y>0)",
        "solidcraft": {"tool": "batch", "arguments": {"commands": steps}},
        "bodies": bodies,
    });
    if let Some(export_path) = a.export_path.as_deref().filter(|p| !p.is_empty()) {
        plan["export"] = json!({"tool": "export", "arguments": {"path": to_windows(export_path), "format": "step"}});
        plan["then"] =
            json!({"tool": "live_footprint_attach_step", "step_path": kicad_path(export_path)});
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected plans and boards, generated once with the Python module (see generated_by).
    static EXPECTED: &str = include_str!("../testdata/model_handoff.json");

    #[test]
    fn zstd_output_is_bounded_and_every_frame_decoded() {
        use ruzstd::encoding::{compress_to_vec, CompressionLevel};
        let frame = |bytes: &[u8]| compress_to_vec(bytes, CompressionLevel::Fastest);
        let mut two = frame(b"first ");
        two.extend(frame(b"second"));
        assert_eq!(zstd_decompress(&two, 100).unwrap(), b"first second");
        let zeros = frame(&[0u8; 5000]);
        assert_eq!(zstd_decompress(&zeros, 5000).unwrap().len(), 5000);
        assert_eq!(
            zstd_decompress(&zeros, 4999).unwrap_err().to_string(),
            "zstd decompression error: decompressed data exceeds 4999 bytes"
        );
        assert!(zstd_decompress(b"not zstd", 100).is_err());
    }

    #[test]
    fn plans_and_boards_match_python() {
        let data: Json = serde_json::from_str(EXPECTED).unwrap();
        for case in data["plans"].as_array().unwrap() {
            let get = |key: &str| case[key].as_f64();
            let args = PlanArgs {
                package: case["package"].as_str().unwrap(),
                params: case["params"].as_object().unwrap(),
                body_height: get("body_height").unwrap(),
                standoff: get("standoff").unwrap_or(0.1),
                lead_thickness: get("lead_thickness").unwrap_or(0.2),
                lead_width: get("lead_width"),
                lead_span: get("lead_span"),
                foot_length: get("foot_length"),
                export_path: case["export_path"].as_str().map(str::to_owned),
            };
            let ours = model_plan(&args).unwrap();
            let mut expected = case["plan"].clone();
            // export/then are platform-dependent paths, checked separately below
            if let (Some(theirs), Some(path)) =
                (expected.as_object_mut(), args.export_path.as_deref())
            {
                theirs.remove("then");
                assert_eq!(ours["then"]["step_path"], json!(kicad_path(path)));
            }
            let mut ours = ours;
            if let Some(map) = ours.as_object_mut() {
                map.remove("then");
            }
            assert_eq!(ours, expected, "{}", case["package"]);
        }
        for case in data["boards"].as_array().unwrap() {
            let board = footprint_board(
                case["text"].as_str().unwrap(),
                case["lib_id"].as_str().unwrap(),
                2.0,
            );
            match &case["board"] {
                Json::String(text) => assert_eq!(&board.unwrap(), text),
                _ => assert_eq!(
                    board.unwrap_err().to_string(),
                    case["error"].as_str().unwrap()
                ),
            }
        }
        for case in data["embedded"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let name = case["name"].as_str().unwrap();
            let bytes = embedded_file_bytes(text, name).unwrap();
            assert_eq!(
                base64::engine::general_purpose::STANDARD.encode(bytes),
                case["data_b64"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn paths() {
        assert_eq!(to_wsl(r"C:\Users\x\m.step"), "/mnt/c/Users/x/m.step");
        assert_eq!(to_wsl("D:/a/b"), "/mnt/d/a/b");
        assert_eq!(to_wsl("/tmp/x"), "/tmp/x");
        assert_eq!(to_windows("/mnt/c/Users/x/m.step"), r"C:\Users\x\m.step");
        assert_eq!(to_windows("/mnt/C/x"), "/mnt/C/x");
        assert_eq!(py_suffix("/a/b.STEP"), ".STEP");
        assert_eq!(py_suffix("/a/.step"), "");
        assert_eq!(py_suffix("/a/b."), "");
        assert_eq!(py_suffix("/a/b.tar.wrz"), ".wrz");
        assert_eq!(py_path(""), ".");
        if cfg!(windows) {
            assert_eq!(py_path("C:/a//b/./c/"), r"C:\a\b\c");
            assert_eq!(
                py_join(r"C:\models", "SOIC-8.step"),
                r"C:\models\SOIC-8.step"
            );
        } else {
            assert_eq!(py_path("/a//b/./c/"), "/a/b/c");
            assert_eq!(py_path("//a"), "//a");
            assert_eq!(py_join("/models", "SOIC-8.step"), "/models/SOIC-8.step");
        }
    }
}
