# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline, portable Markdown skills; identical paths in source and PyInstaller bundles."""

from hashlib import sha256
import json
from pathlib import Path
import re

import yaml


SKILLS_DIR = Path(__file__).resolve().parent / "skills"
DEFAULT_SKILL = "kicad-house-style"
INDEX_URI = "kicad-skill://index"
MANIFEST_URI = "kicad-skill://manifest"


def _folder(skill):
    if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", skill):
        raise ValueError("invalid skill name")
    folder = SKILLS_DIR / skill
    if folder.is_symlink() or not folder.is_dir() or folder.resolve().parent != SKILLS_DIR.resolve():
        raise ValueError(f"unknown or unsafe skill: {skill}")
    return folder


def _file(folder, name):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*\.md", name):
        raise ValueError("invalid skill file name")
    path = folder / name
    if path.is_symlink() or not path.is_file() or path.resolve().parent != folder.resolve():
        raise ValueError(f"missing or unsafe skill file: {name}")
    return path


def _metadata(folder):
    text = _file(folder, "SKILL.md").read_text(encoding="utf-8")
    lines = text.splitlines()
    try:
        if not lines or lines[0] != "---":
            raise ValueError("missing frontmatter")
        end = lines.index("---", 1)
        meta = yaml.safe_load("\n".join(lines[1:end]))
        if not isinstance(meta, dict) or meta.get("name") != folder.name:
            raise ValueError("skill name must match its folder")
        if not isinstance(meta.get("description"), str) or not meta["description"].strip():
            raise ValueError("skill description is required")
        version = meta.get("metadata", {}).get("version")
        if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise ValueError("metadata.version must be a quoted major.minor.patch string")
    except (ValueError, AttributeError, yaml.YAMLError) as error:
        raise ValueError(f"invalid skill frontmatter in {folder.name}: {error}") from error
    return meta, version


def skill_index():
    """List flat Markdown references only; missing data never falls back to local paths."""
    if not SKILLS_DIR.is_dir():
        return []
    entries = []
    for candidate in sorted(SKILLS_DIR.iterdir()):
        if not candidate.is_dir():
            continue
        folder = _folder(candidate.name)
        meta, version = _metadata(folder)
        files = sorted(path.name for path in folder.glob("*.md"))
        for name in files:
            _file(folder, name)
        files.remove("SKILL.md")
        entries.append({"skill": folder.name, "name": meta["name"],
                        "description": meta["description"], "version": version,
                        "files": ["SKILL.md", *files],
                        "uri": f"kicad-skill://{folder.name}/SKILL.md"})
    return entries


def read_skill(skill="", file="SKILL.md"):
    """List skills or read one allowlisted file, without editor discovery or IPC."""
    if not skill:
        return {"default": DEFAULT_SKILL, "skills": skill_index(),
                "usage": 'live_skill_reference {"skill":"<skill>","file":"<file from files>"}'}
    folder = _folder(skill)
    _metadata(folder)
    path = _file(folder, file)
    raw = path.read_bytes()
    files = ["SKILL.md", *sorted(p.name for p in folder.glob("*.md") if p.name != "SKILL.md")]
    return {"skill": skill, "file": file, "files": files, "text": raw.decode("utf-8"),
            "sha256": sha256(raw).hexdigest(), "uri": f"kicad-skill://{skill}/{file}"}


def skill_manifest():
    """Fingerprint data independently of tool/protobuf schemas and source revision.

    Hash raw UTF-8 file bytes, then canonical sorted JSON of the relative POSIX
    path-to-hash map. Version labels are included in the hashed SKILL.md bytes.
    """
    entries = skill_index()
    if DEFAULT_SKILL not in {entry["skill"] for entry in entries}:
        raise ValueError(f"bundled default skill is missing: {DEFAULT_SKILL}")
    files = {f"{entry['skill']}/{name}": read_skill(entry["skill"], name)["sha256"]
             for entry in entries for name in entry["files"]}
    digest = sha256(json.dumps(files, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"manifest_version": 1, "default": DEFAULT_SKILL,
            "skill_versions": {entry["skill"]: entry["version"] for entry in entries},
            "skill_file_sha256": files, "skill_content_sha256": digest}
