#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Identity checks for the follow-up merge of the corrected #42 head.

Run from the repository with HEAD at the follow-up runtime. Confirms that the
previous runtime/record is an ancestor, the #42 delta since the previously
integrated head holds exact owner blobs, no other path changed (including the
7207de24ed record and every earlier proof), the MD5 record equals its owner, all
unit records keep #39's bytes or owner bytes, the guard's historical 199 checks
are an exact prefix, and golden policy holds against every base.
"""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile

repo = Path(sys.argv[1]).resolve()
PREV, NEW42, OLD42 = sys.argv[2:5]
BASES = json.loads(sys.argv[5])


def git(*args, check=True, binary=False):
    return subprocess.run(["git", "-c", "core.autocrlf=false", *args], cwd=repo, check=check,
                          capture_output=True, text=not binary,
                          encoding=None if binary else "utf-8")


def tree(rev):
    out = {}
    for row in git("ls-tree", "-r", "-z", "--full-tree", rev).stdout.split("\0"):
        if row:
            meta, path = row.split("\t", 1)
            out[path] = meta.split()[2]
    return out


H, TP, T42 = tree("HEAD"), tree(PREV), tree(NEW42)
d42 = set(git("diff", "--name-only", OLD42, NEW42).stdout.split())
report = {"head": git("rev-parse", "HEAD").stdout.strip(), "previous": PREV,
          "previous_is_ancestor": git("merge-base", "--is-ancestor", PREV, "HEAD", check=False).returncode == 0,
          "delta42_paths": len(d42),
          "delta42_not_owner_blob": sorted(p for p in d42 if H.get(p) != T42.get(p)),
          "changed_outside_delta42": sorted(p for p in (set(H) | set(TP)) - d42 if H.get(p) != TP.get(p))}
evidence = [p for p in TP if p.startswith("port/findings/phase0-")]
report["integration_evidence_paths"] = len(evidence)
report["integration_evidence_changed"] = sorted(p for p in evidence if H.get(p) != TP.get(p))
md5 = "port/units/libs.kimath.src.md5_hash.application.pure.p001.toml"
report["md5_record_equals_owner"] = H.get(md5) == T42.get(md5)
report["md5_record_unchanged_from_previous"] = H.get(md5) == TP.get(md5)
unit_paths = sorted(p for p in H if p.startswith("port/units/") and p.endswith(".toml"))
report["unit_records"] = len(unit_paths)
report["unit_records_changed_from_previous"] = sorted(p for p in unit_paths if H[p] != TP.get(p))


def load_guard(revision):
    text = git("show", f"{revision}:port/oracle/test_md5_inputs.py", binary=True).stdout
    with tempfile.TemporaryDirectory() as d:
        path = Path(d) / f"md5_inputs_{revision[:8]}.py"
        path.write_bytes(text)
        spec = importlib.util.spec_from_file_location(path.stem, path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module.regression_cases()


old, new = load_guard(OLD42), load_guard("HEAD")
encode = lambda checks: (''.join(t + '\n' for t, _ in checks).encode(),
                         ''.join(json.dumps(v, separators=(',', ':')) + '\n' for _, v in checks).encode())
old_raw, old_expected = encode(old)
new_raw, new_expected = encode(new)
validation = "rust/kicad-md5-shim/validation/2026-11-22-da17dc54"
committed = {name: git("show", f"HEAD:{validation}/{name}", binary=True).stdout
             for name in ("requests.jsonl", "expected.jsonl", "oracle.stdout.jsonl", "rust.stdout.jsonl")}
report["guard"] = {
    "old_checks": len(old), "new_checks": len(new),
    "new_refusals": sum("error" in v for _, v in new), "new_successes": sum("result" in v for _, v in new),
    "old_199_checks_are_prefix": new[:len(old)] == old,
    "raw_and_expected_byte_prefix": new_raw.startswith(old_raw) and new_expected.startswith(old_expected),
    "requests_equal_committed_transcript": new_raw == committed["requests.jsonl"],
    "expected_equal_committed_transcript": new_expected == committed["expected.jsonl"],
}
# Fresh adapter stdout is compared byte for byte with these transcripts in WSL.
policy = {}
for name, base in BASES.items():
    r = subprocess.run([sys.executable, "-B", "port/tools/check_goldens_policy.py", "--base", base],
                       cwd=repo, capture_output=True, text=True)
    policy[name] = {"base": base, "exit": r.returncode, "output": (r.stdout + r.stderr).strip()}
report["golden_policy"] = policy
report["phase0_required"] = [l for l in git("grep", "-n", "PHASE0_REQUIRED:", "HEAD", "--", ".github").stdout.splitlines()]
print(json.dumps(report, indent=2))
