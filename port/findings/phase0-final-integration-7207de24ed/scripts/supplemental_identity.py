#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Supplemental identity checks for the continued phase zero integration.

Run from the repository with HEAD at the integration runtime. Confirms:
- the published PR49 head is an ancestor and every path of it outside the 39/44
  deltas keeps its blob (prior evidence, the six unions, the guard fixture);
- the 39 and 44 deltas since the previously integrated heads are exact owner blobs;
- the four pilot/co-port records equal their owners' published bytes;
- every unit record equals exact PR39 except those four, which keep #39's
  protected fields;
- golden change policy against every base.
"""
import json
from pathlib import Path
import subprocess
import sys
import tomllib

repo = Path(sys.argv[1]).resolve()
P49, A39, OLD39, B44, OLD44 = sys.argv[2:7]
R42, R43, RUST = sys.argv[7:10]


def git(*args, check=True):
    return subprocess.run(["git", "-c", "core.autocrlf=false", *args], cwd=repo, check=check,
                          capture_output=True, text=True, encoding="utf-8")


def tree(rev):
    out = {}
    for row in git("ls-tree", "-r", "-z", "--full-tree", rev).stdout.split("\0"):
        if row:
            meta, path = row.split("\t", 1)
            out[path] = meta.split()[2]
    return out


head = git("rev-parse", "HEAD").stdout.strip()
H, T49, T39, T44 = tree("HEAD"), tree(P49), tree(A39), tree(B44)
d39 = set(git("diff", "--name-only", OLD39, A39).stdout.split())
d44 = set(git("diff", "--name-only", OLD44, B44).stdout.split())
report = {"head": head, "published_49": P49,
          "published_49_is_ancestor": git("merge-base", "--is-ancestor", P49, "HEAD", check=False).returncode == 0,
          "delta39_paths": len(d39), "delta44_paths": len(d44), "delta_overlap": sorted(d39 & d44)}
report["delta39_not_owner_blob"] = sorted(p for p in d39 if H.get(p) != T39.get(p))
report["delta44_not_owner_blob"] = sorted(p for p in d44 if H.get(p) != T44.get(p))
others = (set(T49) | set(H)) - d39 - d44
report["published_49_paths_changed_outside_deltas"] = sorted(p for p in others if H.get(p) != T49.get(p))
evidence = [p for p in T49 if p.startswith("port/findings/phase0-")]
report["prior_integration_evidence_paths"] = len(evidence)
report["prior_integration_evidence_changed"] = sorted(p for p in evidence if H.get(p) != T49.get(p))
shared = ["port/oracle/CMakeLists.txt", "port/oracle/audit.py", "port/oracle/pin.py",
          "port/oracle/registry.cpp", "rust/Cargo.toml", "rust/Cargo.lock",
          "port/oracle/test_iec60664_guards.py"]
report["shared_unions_equal_published_49"] = {p: H.get(p) == T49.get(p) for p in shared}
owners = {"port/units/libs.kimath.src.md5_hash.application.pure.p001.toml": R42,
          "port/units/pcb_calculator.calculator_panels.application.pure.p001.toml": R43,
          "port/units/pcb_calculator.application.pure.p001.toml": B44,
          "port/units/pcb_calculator.application.thin.replacement.p001.toml": B44}
report["pilot_records_equal_owner"] = {p: H.get(p) == tree(r).get(p) for p, r in owners.items()}
report["resistor_status"] = tomllib.loads(git("show", "HEAD:port/units/pcb_calculator.application.pure.p001.toml").stdout).get("status")
units = sorted(p for p in H if p.startswith("port/units/") and p.endswith(".toml"))
differ = [p for p in units if H[p] != T39.get(p)]
report["unit_records"] = len(units)
report["unit_records_differing_from_39"] = differ
report["unit_ids_equal_39"] = units == sorted(p for p in T39 if p.startswith("port/units/") and p.endswith(".toml"))
policy = {}
for name, base in {"rust": RUST, "39": A39, "42": R42, "43": R43, "44": B44, "published49": P49}.items():
    r = subprocess.run([sys.executable, "-B", "port/tools/check_goldens_policy.py", "--base", base],
                       cwd=repo, capture_output=True, text=True)
    policy[name] = {"base": base, "exit": r.returncode, "output": (r.stdout + r.stderr).strip()}
report["golden_policy"] = policy
flags = git("grep", "-n", "PHASE0_REQUIRED", "HEAD", "--", ".github").stdout.strip().splitlines()
report["phase0_required_lines"] = flags
print(json.dumps(report, indent=2))
