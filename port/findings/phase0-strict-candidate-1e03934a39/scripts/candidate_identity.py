#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Source identity of the private strict-gate candidate against tested runtimes.

Lists every path that differs from the published head and from runtime
412b54a546, checks that the oracle overlay, tools, cases, goldens, pins and all
Rust sources other than the resistor README are identical to the tested
runtimes, compares all 1801 records' protected fields with #39, and runs golden
change policy against every base on the committed candidate.
"""
import json
from pathlib import Path
import subprocess
import sys
import tomllib

repo = Path(sys.argv[1]).resolve()
published, r412, r7207, a39 = sys.argv[2:6]
bases = json.loads(sys.argv[6])


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


H, P, R412, R7207, T39 = tree("HEAD"), tree(published), tree(r412), tree(r7207), tree(a39)
diff = lambda A, B: sorted(p for p in set(A) | set(B) if A.get(p) != B.get(p))
runtime_prefixes = ("port/oracle/", "port/tools/", "port/cases/", "port/goldens/", "port/pins.toml",
                    "port/extractor/", "rust/", ".port", "west.yml")
report = {"candidate": git("rev-parse", "HEAD").stdout.strip(), "published_head": published,
          "published_is_ancestor": git("merge-base", "--is-ancestor", published, "HEAD", check=False).returncode == 0,
          "changed_vs_published": diff(H, P),
          "changed_vs_412b54a546_excluding_records": [p for p in diff(H, R412)
                                                      if not p.startswith("port/findings/phase0-final-integration-412b54a546")],
          "runtime_paths_changed_vs_412b54a546": [p for p in diff(H, R412) if p.startswith(runtime_prefixes)],
          "runtime_paths_changed_vs_7207de24ed": sorted({p for p in diff(H, R7207) if p.startswith(runtime_prefixes)})}
for name, rev in (("rust", "rust"), ("port/oracle", "port/oracle"), ("port/cases", "port/cases"),
                  ("port/goldens", "port/goldens"), ("port/tools", "port/tools")):
    report[f"tree_{name}"] = {"candidate": git("rev-parse", f"HEAD:{rev}").stdout.strip(),
                              "412b54a546": git("rev-parse", f"{r412}:{rev}").stdout.strip(),
                              "7207de24ed": git("rev-parse", f"{r7207}:{rev}").stdout.strip()}
editable = {"rust_module", "tests", "status", "claimed_by", "claimed_at", "notes", "difftest"}
units = sorted(p for p in H if p.startswith("port/units/") and p.endswith(".toml"))
protected, statuses, changed39 = [], {}, []
for path in units:
    if H[path] != T39.get(path):
        changed39.append(path)
        new = tomllib.loads(git("show", f"HEAD:{path}").stdout)
        old = tomllib.loads(git("show", f"{a39}:{path}").stdout)
        protected += [{"path": path, "field": k} for k in set(new) | set(old) if k not in editable and new.get(k) != old.get(k)]
        statuses[path.split("/")[-1]] = new.get("status")
report.update({"unit_records": len(units), "records_differing_from_39": changed39, "statuses": statuses,
               "protected_field_differences": protected})
report["phase0_required"] = [l for l in git("grep", "-n", "PHASE0_REQUIRED:", "HEAD", "--", ".github").stdout.splitlines()]
policy = {}
for name, base in bases.items():
    r = subprocess.run([sys.executable, "-B", "port/tools/check_goldens_policy.py", "--base", base],
                       cwd=repo, capture_output=True, text=True)
    policy[name] = {"base": base, "exit": r.returncode, "output": (r.stdout + r.stderr).strip()}
report["golden_policy"] = policy
print(json.dumps(report, indent=2))
