#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare the MD5 unit record and protected artifacts with the corrected dependency."""
import json
import subprocess
import sys
import tomllib

UNIT = "libs.kimath.src.md5_hash.application.pure.p001"
RECORD = f"port/units/{UNIT}.toml"
DEPENDENCY = "19035296e129cf8640e89331bc9e8ad84f0fe38c"
HISTORICAL = "a7ec77beffd3b2e5d60409423fa28b6c116c6e00"
PRIOR_HEAD = "cfed960cc9d7c4638c0cf6d8042d336e43a405c0"
PROTECTED = ["id", "upstream", "depends_on", "soft_depends_on", "est_lines", "layer",
             "coverage_gaps", "functions", "schema_version", "source_category",
             "source_ranges", "wx_class"]
PORT_METADATA = ["rust_module", "tests", "status", "claimed_by", "claimed_at", "difftest"]
UNCHANGED = [f"port/cases/{UNIT}.jsonl", f"port/goldens/{UNIT}.jsonl", "port/goldens/evidence",
             "port/oracle/shims/md5.cpp", "rust/kicad-md5", "rust/kicad-difftest/src/exact.rs"]


def git(*args):
    return subprocess.check_output(["git", *args], text=True)


def record(revision):
    return tomllib.loads(git("show", f"{revision}:{RECORD}"))


def main():
    revision = sys.argv[1] if len(sys.argv) > 1 else "HEAD"
    current, dependency, historical, prior = (record(r) for r in
                                              (revision, DEPENDENCY, HISTORICAL, PRIOR_HEAD))
    protected = {key: current[key] for key in PROTECTED}
    old_evidence = [path for path in git("ls-tree", "--name-only", f"{PRIOR_HEAD}:rust/kicad-md5-shim/validation").split()]
    changed_units = git("diff", "--name-only", DEPENDENCY, revision, "--", "port/units").split()
    changed_protected = git("diff", "--name-only", PRIOR_HEAD, revision, "--", *UNCHANGED,
                            *(f"rust/kicad-md5-shim/validation/{p}" for p in old_evidence)).split()
    proof = {
        "revision": git("rev-parse", revision).strip(),
        "corrected_dependency": DEPENDENCY,
        "historical_record": HISTORICAL,
        "canonical_record_sha256": "71c89c90c9df44c2ba2abb81a367c650c951fd058b1f3e7dd4e788b04c3120ee",
        "protected_fields": protected,
        "all_protected_fields_identical": all(
            protected[key] == dependency[key] == historical[key] for key in PROTECTED),
        "existing_port_metadata_preserved_except_maintained_notes": all(
            current.get(key) == prior.get(key) for key in PORT_METADATA),
        "only_owned_unit_differs_from_dependency": changed_units == [RECORD],
        "prior_evidence_directories": old_evidence,
        "corpus_goldens_historical_evidence_portable_bodies_comparator_unchanged_since_prior_head":
            changed_protected == [],
        "local_green_provisional": True,
        "parent_final_independent_artifact_acceptance": "pending",
    }
    print(json.dumps(proof, indent=2))
    checks = [key for key, value in proof.items() if value is False]
    if checks:
        raise SystemExit(f"protected scope checks failed: {checks}")


if __name__ == "__main__":
    main()
