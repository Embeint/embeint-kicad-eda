#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Losslessly compress completed per-TU cache bytes, preserving their cache keys."""
import argparse
import gzip
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import digest, write_json


def compact(cache, minimum_age=60):
    records = []
    cutoff = time.time() - minimum_age
    for path in sorted(cache.glob("*.json")):
        if path.name == "source-overlay.json" or path.stat().st_mtime > cutoff:
            continue
        raw = path.read_bytes()
        try:
            value = json.loads(raw)
        except ValueError:
            continue
        if not isinstance(value, dict) or "status" not in value or "functions" not in value:
            continue
        compressed = path.with_suffix(".json.gz")
        if compressed.exists():
            if gzip.decompress(compressed.read_bytes()) != raw:
                raise ValueError(f"cache representations disagree: {path}")
        else:
            data = bytearray(gzip.compress(raw, compresslevel=1, mtime=0))
            data[9] = 255
            temporary = compressed.with_suffix(".gz.tmp")
            temporary.write_bytes(data)
            temporary.replace(compressed)
        if gzip.decompress(compressed.read_bytes()) != raw or path.read_bytes() != raw:
            raise ValueError(f"cache changed during compression: {path}")
        # The original bytes, including diagnostics and whitespace, are now
        # checksum-verified in the gzip companion before removing duplication.
        path.unlink()
        records.append({"key": path.stem, "original_sha256": digest(raw),
                        "original_bytes": len(raw), "compressed_bytes": compressed.stat().st_size})
    return {"entries": records, "original_bytes": sum(r["original_bytes"] for r in records),
            "compressed_bytes": sum(r["compressed_bytes"] for r in records)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--cache", type=Path, required=True)
    p.add_argument("--minimum-age-seconds", type=int, default=60)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    if not a.cache.is_dir() or a.minimum_age_seconds < 0:
        p.error("an existing cache directory and nonnegative minimum age are required")
    result = compact(a.cache.resolve(), a.minimum_age_seconds)
    write_json(a.output, result)
    print(json.dumps({"entries": len(result["entries"]), "original_bytes": result["original_bytes"],
                      "compressed_bytes": result["compressed_bytes"]}))


if __name__ == "__main__":
    main()
