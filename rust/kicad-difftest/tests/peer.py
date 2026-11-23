# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic process faults for the native runner's integration tests only.

The mode is a comma-separated list of faults. Whole-run faults: failure, timeout,
deferred, error, changed, missing, duplicate, reorder, noisy. Per-case faults name
the case after a hyphen: defer-<case> (well-formed unported reply), error-<case>,
changed-<case>, and malformed deferrals nounit-<case>, extra-<case>, badowner-<case>
and emptyupstream-<case>. Output faults after flushing the replies: exitafter (exit 7),
sleepafter (outlive the timeout), truncate (cut the last reply mid-line, no newline),
garbage (append a non-reply line) and skip-<case> (omit that reply). Byte faults:
badbyte-<case> (an 0xff byte), badcont-<case> (a broken continuation byte) and
truncmb-<case> (a multibyte character cut at the end of the line) make that reply
invalid UTF-8; badextra appends an invalid UTF-8 line; crlf ends lines with CRLF;
stderrbig writes STDERR_BYTES (default 4 MiB) of diagnostics before replying.
"""
import os
import json
import sys
import time

modes = set(sys.argv[1].split(","))
if "--provenance" in sys.argv:
    print(json.dumps({"upstream_commit": "pin", "compiler": "test peer", "flags": "test only"}))
    sys.exit(0)
raw = sys.stdin.buffer.read()
if "stderrbig" in modes:
    chunk = b"e" * 65536
    remaining = int(os.environ.get("STDERR_BYTES", str(4 * 1024 * 1024)))
    while remaining > 0:
        sys.stderr.buffer.write(chunk[:remaining])
        remaining -= len(chunk)
    sys.stderr.buffer.flush()
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
    sys.stdout.buffer.write(b"unexpected diagnostic on stdout\n")
replies = [reply for reply in replies if f"skip-{reply['case']}" not in modes]
lines = []
for reply in replies:
    line = json.dumps(reply).encode()
    case = reply["case"]
    # Each byte fault leaves the rest of the line intact so only UTF-8 is wrong.
    if f"badbyte-{case}" in modes:
        line = line[:1] + b'"\xff": 0, ' + line[1:]
    if f"badcont-{case}" in modes:
        line = line[:1] + b'"\xc3\x28": 0, ' + line[1:]
    if f"truncmb-{case}" in modes:
        line = line + b"\xe2\x82"
    lines.append(line)
if "truncate" in modes:
    lines[-1] = lines[-1][: len(lines[-1]) // 2]
if "garbage" in modes:
    lines.append(b"EXTRA_REFERENCE_EVIDENCE")
if "badextra" in modes:
    lines.append(b"EXTRA\xffEVIDENCE")
ending = b"\r\n" if "crlf" in modes else b"\n"
for index, line in enumerate(lines):
    last = index == len(lines) - 1
    sys.stdout.buffer.write(line + (b"" if last and "truncate" in modes else ending))
    sys.stdout.buffer.flush()
if "exitafter" in modes:
    print("exiting after replies", file=sys.stderr)
    sys.exit(7)
if "sleepafter" in modes:
    time.sleep(5)
