#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Audit every original request of the superseded 8c54103 baseline against the live oracle.

The 1315 original requests and their blessed golden lines are read from Git at
8c54103a7d9a888419e663abb54082c3370fee0a. Each request is sent, byte for byte, to the corrected
oracle adapter (no zero-filled MMH3 blocks). Every case is classified:

- "unchanged": the new reply line is byte-identical to the old golden line;
- "blocks-only": an MMH3 case whose new reply differs from the old golden only in event
  "blocks" strings, and only where the old byte was "00" and the new one "??" (bytes the
  reference never wrote, which the old adapter had invented);
- "refused": the corrected adapter refuses the request because the reference would read an
  unwritten byte (the 15 cases of kimath-header-hash-mmh3-unwritten-reads.md).

Anything else is an audit failure. Writes audit.json beside this script (or to OUT).

    python3 port/findings/kimath-header-hash-mmh3-audit/audit.py ORACLE [CASES GOLDEN [OUT]]

Without CASES/GOLDEN the baseline files are read with `git show`; when given, they must be
those files exported from the baseline commit (their SHA-256 hashes are recorded).
"""
import hashlib
import json
import pathlib
import subprocess
import sys

UNIT = "libs.kimath.include.application.pure.p001"
BASELINE = "8c54103a7d9a888419e663abb54082c3370fee0a"
HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def git_lines(path):
    text = subprocess.run(["git", "show", f"{BASELINE}:{path}"], cwd=ROOT, check=True,
                          capture_output=True).stdout.decode("utf-8")
    return text.splitlines()


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def blocks_only(old, new):
    """True when the replies differ only in blocks strings, each change '00' -> '??'."""
    old_events = old.get("result", {}).get("events")
    new_events = new.get("result", {}).get("events")
    if old_events is None or new_events is None or len(old_events) != len(new_events):
        return False, 0
    unwritten = 0
    for a, b in zip(old_events, new_events):
        if {k: v for k, v in a.items() if k != "blocks"} != {k: v for k, v in b.items() if k != "blocks"}:
            return False, 0
        if len(a["blocks"]) != 32 or len(b["blocks"]) != 32:
            return False, 0
        for i in range(0, 32, 2):
            x, y = a["blocks"][i:i + 2], b["blocks"][i:i + 2]
            if y == "??":
                if x != "00":
                    return False, 0
                unwritten += 1
            elif x != y:
                return False, 0
    rest_old = {k: v for k, v in old.items() if k != "result"}
    rest_new = {k: v for k, v in new.items() if k != "result"}
    return rest_old == rest_new and old != new, unwritten


def main():
    oracle = sys.argv[1]
    if len(sys.argv) >= 4:
        requests = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8").splitlines()
        golden = pathlib.Path(sys.argv[3]).read_text(encoding="utf-8").splitlines()
    else:
        requests = git_lines(f"port/cases/{UNIT}.jsonl")
        golden = git_lines(f"port/goldens/{UNIT}.jsonl")
    out = pathlib.Path(sys.argv[4]) if len(sys.argv) >= 5 else HERE / "audit.json"
    if len(requests) != 1315 or len(golden) != 1315:
        raise SystemExit("expected the 1315-case baseline")
    run = subprocess.run([oracle], input="\n".join(requests) + "\n", capture_output=True, text=True, check=True)
    replies = run.stdout.splitlines()
    if len(replies) != len(requests):
        raise SystemExit("oracle reply count mismatch")
    entries, counts, failures = [], {}, []
    for request, old_line, new_line in zip(requests, golden, replies):
        case = json.loads(request)["case"]
        old, new = json.loads(old_line), json.loads(new_line)
        if old.get("case") != case or new.get("case") != case:
            raise SystemExit(f"identity mismatch at {case}")
        entry = {"case": case, "old_sha256": sha(old_line), "new_sha256": sha(new_line)}
        if old_line == new_line:
            entry["class"] = "unchanged"
        elif "error" in new and new["error"].get("message", "").find("reads unwritten block bytes") > 0:
            entry["class"] = "refused"
            entry["refusal"] = new["error"]["message"]
            entry["old_reply"] = old_line
        else:
            ok, unwritten = blocks_only(old, new)
            if ok:
                entry["class"] = "blocks-only"
                entry["unwritten_bytes_reported"] = unwritten
            else:
                entry["class"] = "failure"
                failures.append(case)
        counts[entry["class"]] = counts.get(entry["class"], 0) + 1
        entries.append(entry)
    mmh3 = sum(1 for e in entries if e["case"].startswith("mmh3-"))
    summary = {"baseline": BASELINE, "unit": UNIT, "requests": len(requests), "mmh3_cases": mmh3,
               "classes": counts, "failures": failures,
               "requests_sha256": sha("\n".join(requests) + "\n"),
               "golden_sha256": sha("\n".join(golden) + "\n"),
               "oracle_output_sha256": sha(run.stdout)}
    out.write_text(json.dumps({"summary": summary, "cases": entries}, indent=1) + "\n",
                                     encoding="utf-8", newline="\n")
    print(json.dumps(summary, indent=1))
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
