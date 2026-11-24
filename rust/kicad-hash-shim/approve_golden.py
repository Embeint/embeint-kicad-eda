# SPDX-License-Identifier: GPL-3.0-or-later
"""Approve only the exact hash baseline independently audited from live run 45345332013.

This supplies no oracle answers. The unchanged runner must first obtain full live
agreement, then re-evaluate the same binaries and provenance when blessing.
"""
import argparse
import hashlib
import json
from pathlib import Path

UNIT = "libs.kimath.include.application.pure.p001"
CASES = "5464c930c65efecf25c50e37f26a20804c50cbb354e4be2f2c947c419674466d"
OLD = "e1b95812a96a314af616bb1f7a21a56961df7b0d67a692bffea0f2bec891f4b9"
NEW = "1ef5a58c552ee2898575b7563807e4a8cf5325eae78d226244e7ca58623e08d9"
EMPTY = hashlib.sha256(b"").hexdigest()


def digest(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def replacement(proposal, old_text, case_text, *, base_empty=False):
    old_sha = EMPTY if base_empty else OLD
    if (proposal["unit"] != UNIT or digest(old_text) != old_sha or digest(case_text) != CASES
            or proposal["old_sha256"] != old_sha or proposal["cases_sha256"] != CASES):
        raise ValueError("unit, original golden or corpus differs from the audited baseline")
    old_lines = old_text.splitlines()
    old = {json.loads(line)["case"]: (i, line) for i, line in enumerate(old_lines)}
    cases = [json.loads(line)["case"] for line in case_text.splitlines()]
    if len(old) != len(old_lines) or len(set(cases)) != len(cases):
        raise ValueError("duplicate case identity")
    changes = {}
    for change in proposal["changes"]:
        if set(change) != {"case", "old_index", "new_index", "old", "new"}:
            raise ValueError("unexpected change fields")
        case = change["case"]
        if case in changes:
            raise ValueError("duplicate change")
        changes[case] = change
        before = old.get(case)
        if before:
            if type(change["old_index"]) is not int or (change["old_index"], change["old"]) != before:
                raise ValueError("change does not name the actual original line")
        elif change["old_index"] is not None or change["old"] is not None:
            raise ValueError("addition has invented original data")
        index, line = change["new_index"], change["new"]
        if index is None:
            if line is not None or before is None or case in cases:
                raise ValueError("invalid removal")
        else:
            if (type(index) is not int or not 0 <= index < len(cases) or cases[index] != case
                    or not isinstance(line, str) or "\n" in line or "\r" in line):
                raise ValueError("invalid replacement position or line")
            value = json.loads(line)
            if value.get("case") != case or value.get("unit") != UNIT:
                raise ValueError("replacement changes the case or unit")
    lines = []
    for index, case in enumerate(cases):
        if case in changes:
            change = changes[case]
            if change["new_index"] != index:
                raise ValueError("replacement order differs")
            lines.append(change["new"])
        else:
            if case not in old or old[case][0] != index:
                raise ValueError("missing addition or movement")
            lines.append(old[case][1])
    if set(old) - set(cases) != {c for c, x in changes.items() if x["new_index"] is None}:
        raise ValueError("missing removal record")
    text = "\n".join(lines) + "\n"
    if digest(text) != NEW or proposal["new_sha256"] != NEW:
        raise ValueError("replacement differs from the exact audited live output")
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--approval", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--base-empty", action="store_true",
                        help="consolidated evidence against the empty official-rust baseline")
    args = parser.parse_args()
    proposal = json.loads(args.approval.read_bytes())
    golden_path = args.root / "port/goldens" / f"{UNIT}.jsonl"
    if args.base_empty:
        if golden_path.exists():
            raise ValueError("official baseline must have no unit golden")
        old_text = ""
    else:
        old_text = golden_path.read_bytes().decode()
    replacement(proposal, old_text,
                (args.root / "port/cases" / f"{UNIT}.jsonl").read_bytes().decode(),
                base_empty=args.base_empty)
    runtime = json.loads(args.runtime.read_bytes())
    if runtime["oracle_sha256"] != proposal["oracle"]["oracle_sha256"]:
        raise ValueError("runtime evidence belongs to another oracle")
    proposal["reason"] = (
        ("Consolidated live evidence against official rust's empty unit baseline. "
         if args.base_empty else "Incremental live evidence against the original 1315-case golden. ") +
        "Consolidate the corrected defined-domain hash/Bézier baseline after fresh live agreement. "
        "The original 1315 requests retain 979 unchanged values; 321 snapshots replace invented "
        "MMH3 zero bytes with unwritten-byte markers; fifteen undefined reads remain permanent "
        "refusal regressions. Add 1341 generated valid cases. The 2641-case live debug/release "
        "run 45345332013, 441 guards per profile, 1983 sensitive requests per profile and wasm "
        "passed. The exact measured GCC13.3 adapter object and libstdc++14.2 runtime bind "
        "per-site NaN selection and grouping. This approval pins every replacement byte to "
        f"SHA256 {NEW}; the runner rechecks live equality and provenance before blessing. "
        "No tolerance, fabricated oracle result, defined-input exclusion or reference-image "
        "adoption. Historical evidence is preserved."
    )
    args.approval.write_text(json.dumps(proposal, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
