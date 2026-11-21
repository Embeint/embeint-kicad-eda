// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline, portable Markdown skills: a port of tools/live_schematic_mcp/skills.py.
//!
//! The skill files are embedded at build time (build.rs), so the bytes and hashes are those of the
//! checked-in files and the server needs no skills directory at run time.  Every check, result
//! key and error message is skills.py's; the front matter is read with yaml-rust2 where Python
//! uses PyYAML.

use std::sync::OnceLock;

use embeint_ipc::client::Error;
use regex::Regex;
use serde_json::{json, Map, Value as Json};
use sha2::{Digest, Sha256};
use yaml_rust2::{Yaml, YamlLoader};

use crate::capabilities::canonical_json;

pub const DEFAULT_SKILL: &str = "kicad-house-style";
pub const INDEX_URI: &str = "kicad-skill://index";
pub const MANIFEST_URI: &str = "kicad-skill://manifest";

/// An entry of the skills directory that Python's `is_dir()` accepts.
pub enum Folder {
    /// A real directory: its `*.md` entries, each with its bytes, or None when it is not a regular
    /// file Python's `_file` accepts (a symlink or a directory).
    Dir(&'static [(&'static str, Option<&'static [u8]>)]),
    /// A symlink or junction to a directory, which `_folder` refuses (none in a normal checkout).
    #[allow(dead_code)]
    Unsafe,
}

// pub static SKILLS: &[(&str, Folder)], in build.rs's order
include!(concat!(env!("OUT_DIR"), "/skills.rs"));

type Entries = &'static [(&'static str, Option<&'static [u8]>)];

/// Python's `re.fullmatch` patterns (Python's `\d` is any Unicode decimal digit, as here).
fn pattern(which: usize) -> &'static Regex {
    static PATTERNS: OnceLock<[Regex; 3]> = OnceLock::new();
    &PATTERNS.get_or_init(|| {
        [
            r"^[a-z0-9]+(?:-[a-z0-9]+)*$",
            r"^[A-Za-z0-9][A-Za-z0-9_-]*\.md$",
            r"^\d+\.\d+\.\d+$",
        ]
        .map(|source| Regex::new(source).expect("fixed pattern"))
    })[which]
}

/// Whether a path component names this entry: Windows file systems ignore case, so Python's
/// lookups there do too (`SKILLS_DIR / "pcb.MD"` opens pcb.md).
fn same_name(entry: &str, requested: &str) -> bool {
    if cfg!(windows) {
        entry.eq_ignore_ascii_case(requested)
    } else {
        entry == requested
    }
}

fn value(message: impl Into<String>) -> Error {
    Error::Value(message.into())
}

/// `_folder(skill)`.
fn folder(skill: &str) -> Result<Entries, Error> {
    if !pattern(0).is_match(skill) {
        return Err(value("invalid skill name"));
    }
    match SKILLS.iter().find(|(name, _)| same_name(name, skill)) {
        Some((_, Folder::Dir(entries))) => Ok(entries),
        _ => Err(value(format!("unknown or unsafe skill: {skill}"))),
    }
}

/// `_file(folder, name)`: the file's bytes.
fn file(entries: Entries, name: &str) -> Result<&'static [u8], Error> {
    if !pattern(1).is_match(name) {
        return Err(value("invalid skill file name"));
    }
    entries
        .iter()
        .find(|(entry, _)| same_name(entry, name))
        .and_then(|(_, bytes)| *bytes)
        .ok_or_else(|| value(format!("missing or unsafe skill file: {name}")))
}

/// `sorted(path.name for path in folder.glob("*.md"))` (build.rs keeps only what the glob matches).
fn markdown_names(entries: Entries) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = entries.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    names
}

fn text(bytes: &'static [u8]) -> Result<&'static str, Error> {
    // build.rs refuses skill files that are not UTF-8
    std::str::from_utf8(bytes).map_err(|error| value(error.to_string()))
}

/// Python's `str.splitlines()`: every line boundary it knows, no empty line after a final one.
fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        let boundary = matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if !boundary {
            continue;
        }
        lines.push(&text[start..index]);
        start = index + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
            chars.next();
            start += 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Python's `str.strip()` whitespace: Unicode White_Space plus the information separators.
fn is_python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// The Python type name of a loaded YAML value, for the AttributeError `.get` raises.
fn python_type(node: &Yaml) -> &'static str {
    match node {
        Yaml::String(_) => "str",
        Yaml::Integer(_) => "int",
        Yaml::Real(_) => "float",
        Yaml::Boolean(_) => "bool",
        Yaml::Array(_) => "list",
        Yaml::Hash(_) => "dict",
        Yaml::Null | Yaml::Alias(_) | Yaml::BadValue => "NoneType",
    }
}

fn key(name: &str) -> Yaml {
    Yaml::String(name.into())
}

/// The body of `_metadata`'s try block: (description, version) or the error's text.
fn front_matter(text: &str, folder_name: &str) -> Result<(String, String), String> {
    let lines = splitlines(text);
    if lines.first() != Some(&"---") {
        return Err("missing frontmatter".into());
    }
    let end = lines
        .iter()
        .skip(1)
        .position(|line| *line == "---")
        .map(|index| index + 1)
        .ok_or("'---' is not in list")?;
    // yaml-rust2 reads YAML 1.2 where PyYAML reads 1.1: plain yes/on, dates, 12:30 and 1_000 are
    // strings here but bool/date/int there, 0o17 is the reverse, duplicate keys are refused rather
    // than the last kept, and parse errors are worded differently.  The bundled skills use none
    // of these (bundled_front_matter_reads_alike checks); a new skill shows up in the parity
    // fixtures, which read every file Python's index lists.
    let mut documents =
        YamlLoader::load_from_str(&lines[1..end].join("\n")).map_err(|error| error.to_string())?;
    // yaml.safe_load: an empty stream is None, and it wants at most one document
    let meta = match documents.len() {
        0 => Yaml::Null,
        1 => documents.remove(0),
        _ => return Err("expected a single document in the stream".into()),
    };
    let Yaml::Hash(meta) = meta else {
        return Err("skill name must match its folder".into());
    };
    if meta.get(&key("name")) != Some(&Yaml::String(folder_name.into())) {
        return Err("skill name must match its folder".into());
    }
    let description = match meta.get(&key("description")) {
        Some(Yaml::String(description))
            if !description.trim_matches(is_python_space).is_empty() =>
        {
            description.clone()
        }
        _ => return Err("skill description is required".into()),
    };
    // meta.get("metadata", {}).get("version")
    let version = match meta.get(&key("metadata")) {
        None => None,
        Some(Yaml::Hash(metadata)) => metadata.get(&key("version")),
        Some(other) => {
            return Err(format!(
                "'{}' object has no attribute 'get'",
                python_type(other)
            ))
        }
    };
    match version {
        Some(Yaml::String(version)) if pattern(2).is_match(version) => {
            Ok((description, version.clone()))
        }
        _ => Err("metadata.version must be a quoted major.minor.patch string".into()),
    }
}

/// `_metadata(folder)`: (name, description, version).  Reading SKILL.md happens before the try
/// block, so its errors are not wrapped.
fn metadata(entries: Entries, folder_name: &str) -> Result<(String, String), Error> {
    let raw = text(file(entries, "SKILL.md")?)?;
    // read_text() reads with universal newlines
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    front_matter(&text, folder_name).map_err(|error| {
        value(format!(
            "invalid skill frontmatter in {folder_name}: {error}"
        ))
    })
}

/// `skill_index()`: every skill with its version and files, in the directory's sorted order.
pub fn skill_index() -> Result<Vec<Json>, Error> {
    let mut candidates: Vec<&str> = SKILLS.iter().map(|(name, _)| *name).collect();
    // sorted(Path) compares names case-insensitively on Windows
    if cfg!(windows) {
        candidates.sort_by_key(|name| name.to_lowercase());
    } else {
        candidates.sort_unstable();
    }
    let mut index = Vec::new();
    for name in candidates {
        let entries = folder(name)?;
        let (description, version) = metadata(entries, name)?;
        let mut files = markdown_names(entries);
        for file_name in &files {
            file(entries, file_name)?;
        }
        let Some(position) = files.iter().position(|file_name| *file_name == "SKILL.md") else {
            return Err(value("list.remove(x): x not in list"));
        };
        files.remove(position);
        files.insert(0, "SKILL.md");
        index.push(json!({
            "skill": name,
            "name": name,
            "description": description,
            "version": version,
            "files": files,
            "uri": format!("kicad-skill://{name}/SKILL.md"),
        }));
    }
    Ok(index)
}

/// `read_skill(skill, file)`: the listing for an empty skill, else one allowlisted file.
pub fn read_skill(skill: &str, file_name: &str) -> Result<Json, Error> {
    if skill.is_empty() {
        return Ok(json!({
            "default": DEFAULT_SKILL,
            "skills": skill_index()?,
            "usage": "live_skill_reference {\"skill\":\"<skill>\",\"file\":\"<file from files>\"}",
        }));
    }
    let entries = folder(skill)?;
    metadata(entries, skill)?;
    let raw = file(entries, file_name)?;
    let mut files = vec!["SKILL.md"];
    files.extend(
        markdown_names(entries)
            .into_iter()
            .filter(|name| *name != "SKILL.md"),
    );
    Ok(json!({
        "skill": skill,
        "file": file_name,
        "files": files,
        "text": text(raw)?,
        "sha256": hex(&Sha256::digest(raw)),
        "uri": format!("kicad-skill://{skill}/{file_name}"),
    }))
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `skill_manifest()`: versions and raw-byte hashes, and the hash of the canonical path-to-hash
/// JSON, independent of the tool and protobuf schemas.
pub fn skill_manifest() -> Result<Json, Error> {
    let entries = skill_index()?;
    if !entries.iter().any(|entry| entry["skill"] == DEFAULT_SKILL) {
        return Err(value(format!(
            "bundled default skill is missing: {DEFAULT_SKILL}"
        )));
    }
    let mut files = Map::new();
    let mut versions = Map::new();
    for entry in &entries {
        let skill = entry["skill"].as_str().unwrap_or_default();
        versions.insert(skill.into(), entry["version"].clone());
        for name in entry["files"].as_array().into_iter().flatten() {
            let name = name.as_str().unwrap_or_default();
            let read = read_skill(skill, name)?;
            files.insert(format!("{skill}/{name}"), read["sha256"].clone());
        }
    }
    let files = Json::Object(files);
    let digest = hex(&Sha256::digest(canonical_json(&files, false).as_bytes()));
    Ok(json!({
        "manifest_version": 1,
        "default": DEFAULT_SKILL,
        "skill_versions": versions,
        "skill_file_sha256": files,
        "skill_content_sha256": digest,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_as_python() {
        assert_eq!(
            splitlines("a\nb\r\nc\rd\u{2028}e\n"),
            ["a", "b", "c", "d", "e"]
        );
        assert_eq!(splitlines("a\n\nb"), ["a", "", "b"]);
        assert!(splitlines("").is_empty());
        assert_eq!(splitlines("\n"), [""]);
    }

    #[test]
    fn front_matter_checks() {
        let ok = "---\nname: x\ndescription: D\nmetadata:\n  version: \"1.2.3\"\n---\nbody";
        assert_eq!(
            front_matter(ok, "x"),
            Ok(("D".to_owned(), "1.2.3".to_owned()))
        );
        assert_eq!(
            front_matter(ok, "y").unwrap_err(),
            "skill name must match its folder"
        );
        assert_eq!(
            front_matter("--- \n", "x").unwrap_err(),
            "missing frontmatter"
        );
        assert_eq!(
            front_matter("---\nname: x\n", "x").unwrap_err(),
            "'---' is not in list"
        );
        assert_eq!(
            front_matter("---\nname: x\ndescription: \" \"\n---", "x").unwrap_err(),
            "skill description is required"
        );
        assert_eq!(
            front_matter("---\nname: x\ndescription: d\nmetadata: v\n---", "x").unwrap_err(),
            "'str' object has no attribute 'get'"
        );
        assert_eq!(
            front_matter(
                "---\nname: x\ndescription: d\nmetadata:\n  version: 1.2\n---",
                "x"
            )
            .unwrap_err(),
            "metadata.version must be a quoted major.minor.patch string"
        );
        // Python's \d accepts any decimal digit
        assert!(front_matter(
            "---\nname: x\ndescription: d\nmetadata:\n  version: \"\u{ff11}.0.0\"\n---",
            "x"
        )
        .is_ok());
    }

    /// The embedded table is exactly the Markdown files of the skills directory on disk.
    #[test]
    fn embedded_skills_equal_the_files() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/live_schematic_mcp/skills");
        let mut on_disk = Vec::new();
        for skill in std::fs::read_dir(&root).unwrap() {
            let skill = skill.unwrap().path();
            if !skill.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(&skill).unwrap() {
                let file = file.unwrap().path();
                if file.extension().and_then(|e| e.to_str()) == Some("md") {
                    let key = format!(
                        "{}/{}",
                        skill.file_name().unwrap().to_str().unwrap(),
                        file.file_name().unwrap().to_str().unwrap()
                    );
                    on_disk.push((key, std::fs::read(&file).unwrap()));
                }
            }
        }
        on_disk.sort();
        let manifest = skill_manifest().unwrap();
        let hashes = manifest["skill_file_sha256"].as_object().unwrap();
        let mut listed: Vec<&String> = hashes.keys().collect();
        listed.sort();
        assert_eq!(
            listed,
            on_disk.iter().map(|(key, _)| key).collect::<Vec<_>>()
        );
        for (key, bytes) in &on_disk {
            let digest: String = Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(hashes[key], digest.as_str(), "{key}");
        }
    }

    /// The bundled front matter has no plain scalar that YAML 1.1 (PyYAML) reads differently.
    #[test]
    fn bundled_front_matter_reads_alike() {
        for (name, _) in SKILLS {
            let read = read_skill(name, "SKILL.md").unwrap();
            let text = read["text"].as_str().unwrap();
            let front = text.split("---").nth(1).unwrap();
            for line in front.lines() {
                let Some((_, value)) = line.split_once(':') else {
                    continue;
                };
                let value = value.trim().to_ascii_lowercase();
                let words = [
                    "yes", "no", "on", "off", "y", "n", "true", "false", "~", "null",
                ];
                let ambiguous = words.contains(&value.as_str())
                    || (value.contains('_')
                        && value.chars().all(|c| c.is_ascii_digit() || c == '_'))
                    || value.starts_with("0o")
                    || (value.len() == 10 && value.as_bytes().get(4) == Some(&b'-'));
                assert!(!ambiguous, "{name}: {line} reads differently in YAML 1.1");
            }
        }
    }

    #[test]
    fn bundled_skills_are_served() {
        let manifest = skill_manifest().unwrap();
        assert_eq!(manifest["default"], DEFAULT_SKILL);
        let read = read_skill(DEFAULT_SKILL, "SKILL.md").unwrap();
        assert_eq!(read["files"][0], "SKILL.md");
        assert_eq!(
            manifest["skill_file_sha256"][format!("{DEFAULT_SKILL}/SKILL.md")],
            read["sha256"]
        );
        assert_eq!(
            read_skill(DEFAULT_SKILL, "../SKILL.md").unwrap_err(),
            Error::Value("invalid skill file name".into())
        );
        assert_eq!(
            read_skill("nope", "SKILL.md").unwrap_err(),
            Error::Value("unknown or unsafe skill: nope".into())
        );
    }
}
