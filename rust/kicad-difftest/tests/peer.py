# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic process faults for the native runner's integration tests only.

The mode is a comma-separated list of faults. Whole-run faults: failure, timeout,
deferred, error, changed, missing, duplicate, reorder, noisy. Per-case faults name
the case after a hyphen: defer-<case> (well-formed unported reply), error-<case>,
changed-<case>, and malformed deferrals nounit-<case>, extra-<case>, badowner-<case>
and emptyupstream-<case>.
"""
import json
import sys
import time

modes = set(sys.argv[1].split(","))
if "--provenance" in sys.argv:
    print(json.dumps({"upstream_commit": "pin", "compiler": "test peer", "flags": "test only"}))
    sys.exit(0)
raw = sys.stdin.buffer.read()
if "failure" in modes:
    print("oracle crashed", file=sys.stderr)
    sys.exit(7)
if "timeout" in modes:
    time.sleep(5)
requests = [json.loads(line) for line in raw.splitlines()]
deferral = {"kind": "unported", "upstream": "f", "unit": "dependency"}
malformed = {
    "nounit": {"kind": "unported", "upstream": "f"},
    "extra": dict(deferral, message="extra field"),
    "badowner": dict(deferral, unit="../dependency"),
    "emptyupstream": dict(deferral, upstream=""),
}
replies = []
for request in requests:
    case = request["case"]
    reply = {"unit": request["unit"], "case": case}
    if "deferred" in modes or f"defer-{case}" in modes:
        reply["error"] = deferral
    elif "error" in modes or f"error-{case}" in modes:
        reply["error"] = {"kind": "execution_error", "message": "failed despite exit zero"}
    elif any(f"{name}-{case}" in modes for name in malformed):
        name = next(name for name in malformed if f"{name}-{case}" in modes)
        reply["error"] = malformed[name]
    else:
        changed = "changed" in modes or f"changed-{case}" in modes
        reply["result"] = {"bytes": raw.hex(), "n": 9007199254740993 + changed}
    replies.append(reply)
if "missing" in modes:
    replies.pop()
if "duplicate" in modes:
    replies.append(replies[0])
if "reorder" in modes:
    replies.reverse()
if "noisy" in modes:
    print("unexpected diagnostic on stdout")
for reply in replies:
    print(json.dumps(reply))
