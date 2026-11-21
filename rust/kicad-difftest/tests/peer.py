# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic process faults for the native runner's integration tests only."""
import json
import sys
import time

mode = sys.argv[1]
if "--provenance" in sys.argv:
    print(json.dumps({"upstream_commit": "pin", "compiler": "test peer", "flags": "test only"}))
    sys.exit(0)
raw = sys.stdin.buffer.read()
if mode == "failure":
    print("oracle crashed", file=sys.stderr)
    sys.exit(7)
if mode == "timeout":
    time.sleep(5)
requests = [json.loads(line) for line in raw.splitlines()]
replies = []
for request in requests:
    reply = {"unit": request["unit"], "case": request["case"]}
    if mode == "deferred":
        reply["error"] = {"kind": "unported", "upstream": "f", "unit": "dependency"}
    elif mode == "error":
        reply["error"] = {"kind": "execution_error", "message": "failed despite exit zero"}
    else:
        reply["result"] = {"bytes": raw.hex(), "n": 9007199254740993 + (mode == "changed")}
    replies.append(reply)
if mode == "missing":
    replies.pop()
if mode == "duplicate":
    replies.append(replies[0])
if mode == "reorder":
    replies.reverse()
if mode == "noisy":
    print("unexpected diagnostic on stdout")
for reply in replies:
    print(json.dumps(reply))
