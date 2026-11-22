#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regenerate the IEC60664 corpus with its unchanged generator and compare it with
the committed blob (the Windows working file may carry CRLF conversion)."""
import hashlib
import importlib.util
from pathlib import Path
import subprocess
import sys

repo = Path(sys.argv[1]).resolve()
spec = importlib.util.spec_from_file_location("iec_cases", repo / "rust/kicad-iec60664/cases.py")
cases = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cases)
source = subprocess.check_output(["git", "show", f"{cases.PIN}:{cases.SOURCE}"], cwd=repo)
data = cases.serialize(cases.generate(source.decode("utf-8")))
path = f"port/cases/{cases.UNIT}.jsonl"
committed = subprocess.check_output(["git", "show", f"HEAD:{path}"], cwd=repo)
working = (repo / path).read_bytes()
print("regenerated", len(data.splitlines()), "cases sha256", hashlib.sha256(data).hexdigest())
print("committed blob sha256", hashlib.sha256(committed).hexdigest())
print("working file sha256", hashlib.sha256(working).hexdigest(),
      "(CRLF-only difference)" if working.replace(b"\r\n", b"\n") == committed else "")
assert data == committed, "regenerated corpus differs from the committed blob"
print("IEC60664 corpus regenerates byte for byte from the pinned source")
