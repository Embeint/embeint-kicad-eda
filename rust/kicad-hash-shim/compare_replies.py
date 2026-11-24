#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare two programs' replies to the same request file, case by case.

Each reply line is parsed JSON; its `result` or `error` must be equal (key order ignored), and
every request must have exactly one reply in each file. Nothing is normalised, tolerated or
skipped. Used for the supplementary sensitive requests, which have no golden.

Usage: compare_replies.py REQUESTS ORACLE_REPLIES RUST_REPLIES [REPORT.json]
Exit status 0 when every case agrees, 1 otherwise.
"""

import json
import sys


def load(path):
    replies = {}
    for line in open(path, encoding="utf-8"):
        if line.strip():
            reply = json.loads(line)
            if reply.get("case") in replies:
                raise SystemExit(f"duplicate reply for {reply.get('case')} in {path}")
            replies[reply.get("case")] = reply
    return replies


def outcome(reply):
    return {key: reply[key] for key in ("result", "error") if key in reply}


def main():
    if len(sys.argv) not in (4, 5):
        raise SystemExit(__doc__)
    cases = [json.loads(line)["case"] for line in open(sys.argv[1], encoding="utf-8") if line.strip()]
    oracle, rust = load(sys.argv[2]), load(sys.argv[3])
    differ = [case for case in cases if case not in oracle or case not in rust
              or outcome(oracle[case]) != outcome(rust[case])]
    extra = sorted((set(oracle) | set(rust)) - set(cases), key=str)
    summary = {"requests": len(cases), "agree": len(cases) - len(differ), "differ": len(differ),
               "extra_replies": len(extra), "differing_cases": differ}
    if len(sys.argv) == 5:
        with open(sys.argv[4], "w", encoding="utf-8") as report:
            json.dump(summary, report, indent=1)
    print(f"{summary['requests']} requests: {summary['agree']} agree, {summary['differ']} differ, "
          f"{summary['extra_replies']} extra replies" + (f"; first: {differ[:5]}" if differ else ""))
    return 0 if not differ and not extra else 1


if __name__ == "__main__":
    sys.exit(main())
