# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline skill delivery, confinement and content provenance contracts."""

import asyncio
from hashlib import sha256
import json
import re
import shutil

import pytest
import yaml

import server
import skills


def test_skill_delivery_links_frontmatter_and_no_editor(monkeypatch):
    def reject_ipc(*_args, **_kwargs):
        raise AssertionError("skill delivery must not contact the editor")

    monkeypatch.setattr(server.client, "send", reject_ipc)
    listing = server.live_skill_reference()
    assert listing["default"] == skills.DEFAULT_SKILL
    assert json.loads(server.skill_index_resource()) == listing["skills"]
    assert json.loads(server.skill_manifest_resource()) == skills.skill_manifest()
    for entry in listing["skills"]:
        main = skills.read_skill(entry["skill"])["text"]
        metadata = yaml.safe_load(main.split("---", 2)[1])
        assert metadata["name"] == entry["skill"]
        assert metadata["metadata"]["version"] == entry["version"]
        assert metadata["description"] == entry["description"]
        for name in entry["files"]:
            result = server.live_skill_reference(entry["skill"], name)
            assert result["text"] == server.skill_file_resource(entry["skill"], name)
            assert result["sha256"] == sha256(result["text"].encode()).hexdigest()
            for target in re.findall(r"\]\(([^)#]+\.md)\)", result["text"]):
                assert target in entry["files"], f"{name} links to missing {target}"
            if name != "SKILL.md":
                assert f"({name})" in main


@pytest.fixture
def local_skills(tmp_path, monkeypatch):
    copied = tmp_path / "skills"
    shutil.copytree(skills.SKILLS_DIR, copied)
    monkeypatch.setattr(skills, "SKILLS_DIR", copied)
    return copied


def test_content_hash_is_independent_of_capability_schema(local_skills):
    before = skills.skill_manifest()
    schema = asyncio.run(server.live_mcp_capabilities())["schema_sha256"]
    path = local_skills / skills.DEFAULT_SKILL / "pcb.md"
    path.write_bytes(path.read_bytes() + b"\nA test-only content change.\n")
    after = skills.skill_manifest()
    assert before["skill_content_sha256"] != after["skill_content_sha256"]
    changed = {name for name in before["skill_file_sha256"]
               if before["skill_file_sha256"][name] != after["skill_file_sha256"][name]}
    assert changed == {f"{skills.DEFAULT_SKILL}/pcb.md"}
    assert before["skill_versions"] == after["skill_versions"]
    assert schema == asyncio.run(server.live_mcp_capabilities())["schema_sha256"]


@pytest.mark.parametrize("skill,file", [
    ("missing", "SKILL.md"), ("..", "SKILL.md"), ("../kicad-house-style", "SKILL.md"),
    ("C:\\secret", "SKILL.md"), ("/tmp/secret", "SKILL.md"),
    (skills.DEFAULT_SKILL, "missing.md"), (skills.DEFAULT_SKILL, "../server.py"),
    (skills.DEFAULT_SKILL, "..\\secret.md"), (skills.DEFAULT_SKILL, "/tmp/secret.md"),
    (skills.DEFAULT_SKILL, "C:\\secret.md"), (skills.DEFAULT_SKILL, "%2e%2e%2fsecret.md"),
    (skills.DEFAULT_SKILL, "SKILL.md:secret"), (skills.DEFAULT_SKILL, "nested/ref.md"),
])
def test_unknown_and_traversal_paths_rejected(skill, file):
    with pytest.raises(ValueError):
        skills.read_skill(skill, file)


def test_missing_data_never_falls_back_to_workstation(tmp_path, monkeypatch):
    monkeypatch.setattr(skills, "SKILLS_DIR", tmp_path / "absent")
    assert skills.skill_index() == []
    with pytest.raises(ValueError, match="missing"):
        skills.skill_manifest()
    with pytest.raises(ValueError):
        skills.read_skill(skills.DEFAULT_SKILL)


def test_missing_entrypoint_and_malformed_frontmatter(local_skills):
    main = local_skills / skills.DEFAULT_SKILL / "SKILL.md"
    main.unlink()
    with pytest.raises(ValueError, match="missing"):
        skills.skill_index()
    for text in ("no frontmatter", "---\nname: wrong\n---\n", "---\nname: [\n---\n"):
        main.write_text(text)
        with pytest.raises(ValueError, match="frontmatter"):
            skills.skill_index()


@pytest.mark.parametrize("kind", ["file", "folder"])
def test_symlink_escape_is_rejected(local_skills, tmp_path, kind):
    folder = local_skills / skills.DEFAULT_SKILL
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "secret.md").write_text("private data")
    link = folder / "escape.md" if kind == "file" else local_skills / "escape"
    target = outside / "secret.md" if kind == "file" else outside
    try:
        link.symlink_to(target, target_is_directory=kind == "folder")
    except OSError as error:
        pytest.skip(f"symlink creation unavailable: {error}")
    with pytest.raises(ValueError, match="unsafe"):
        skills.read_skill(skills.DEFAULT_SKILL, "escape.md") if kind == "file" else skills.read_skill("escape")
    with pytest.raises(ValueError, match="unsafe"):
        skills.skill_manifest()
