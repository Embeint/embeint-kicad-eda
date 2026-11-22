#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract a conservative call graph with explicit source/configuration coverage.

Requires Python >=3.11, clang Python bindings and matching libclang (Apache-2.0
with LLVM exception). Run in WSL; see port/extractor/README.md.
"""
import argparse
from collections import Counter
from concurrent.futures import ProcessPoolExecutor, as_completed
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.clang_graph import extract_translation_unit
from extractor.model import PIN, classify, digest, expand_virtual, materialize_definitions, read_json, write_cache, write_json
from extractor.generated_sources import generator_dependencies, verify as verify_generated

SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp", ".hxx", ".m", ".mm"}


def category(path, content):
    if path.startswith("thirdparty/"):
        return "thirdparty"
    if path.startswith("qa/"):
        return "qa"
    if "_base." in path or any(x in content[:8000].lower() for x in
            (b"automatically generated", b"auto-generated", b"do not edit")):
        return "generated"
    return "application"


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--compile-commands", type=Path, required=True)
    p.add_argument("--source-root", type=Path, required=True)
    p.add_argument("--original-source-root", required=True)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--generated-manifest", type=Path, help="frozen generated-sources.json beside its sources/ storage")
    p.add_argument("--output", type=Path, default=Path("port/extractor/generated"))
    p.add_argument("--cache", type=Path, required=True)
    p.add_argument("--libclang", default="/usr/lib/llvm-21/lib/libclang-21.so.1")
    p.add_argument("--jobs", type=int, default=8)
    p.add_argument("--limit", type=int)
    p.add_argument("--match", default="")
    a = p.parse_args()
    if not 1 <= a.jobs <= 8:
        p.error("--jobs must be between 1 and 8")
    manifest = json.loads(a.manifest.read_text(encoding="utf-8"))
    root = a.source_root.resolve()
    raw = a.compile_commands.read_bytes()
    commands = json.loads(raw)
    inventory = []
    for path in sorted(manifest["files"]):
        content = (root / path).read_bytes().replace(b"\r\n", b"\n")
        inventory.append({"path": path, "lines": len(content.splitlines()),
                          "sha256": digest(content), "category": category(path, content)})
    tracked_snapshot = digest(json.dumps(inventory, sort_keys=True).encode())
    build = None
    if a.generated_manifest:
        generated_manifest = read_json(a.generated_manifest)
        build = Path(generated_manifest['build_root']).resolve()
        inventory.extend(verify_generated(generated_manifest, build, a.generated_manifest.parent))
        if a.cache.resolve().is_relative_to(build) or a.output.resolve().is_relative_to(build):
            p.error('Keep extraction cache/output outside the frozen build tree')
    frozen_paths = {entry['path'] for entry in inventory}
    jobs = []
    libclang_sha256 = digest(Path(a.libclang).read_bytes())
    for command in commands:
        job = dict(command, root=str(root), old_root=a.original_source_root, libclang=a.libclang)
        if build:
            job['build_root'] = str(build)
            job['generated_sources_sha256'] = digest(a.generated_manifest.read_bytes())
        job['libclang_sha256'] = libclang_sha256
        if a.match and a.match not in job["file"]:
            continue
        # Keep build-generated include paths unchanged; only replace source paths.
        job["extra_args"] = ["-isystem", "/usr/include/c++/15", "-isystem",
                             "/usr/include/x86_64-linux-gnu/c++/15"]
        jobs.append(job)
    jobs.sort(key=lambda j: ("/libs/kimath/" not in j["file"], j["file"], j.get("command", "")))
    if a.limit:
        jobs = jobs[:a.limit]
    a.cache.mkdir(parents=True, exist_ok=True)
    # Unity sources contain absolute includes of the original checkout. Remap
    # those too, so generated unity TUs cannot mix two copies of a header.
    overlay = a.cache / "source-overlay.json"
    write_json(overlay, {"version": 0, "roots": [{"type": "directory-remap",
               "name": a.original_source_root, "external-contents": str(root)}]})
    for job in jobs:
        job["extra_args"] += ["-ivfsoverlay", str(overlay)]
        job["extractor_sha256"] = digest(b''.join(path.read_bytes().replace(b'\r\n', b'\n') for path in
             [Path(__file__), *(Path(__file__).resolve().parents[1] / 'extractor' / name
               for name in ('clang_graph.py', 'model.py', 'generated_sources.py'))]))
    # Cache includes the entire source inventory: changing any header invalidates all TUs.
    snapshot = digest(json.dumps(inventory, sort_keys=True).encode())
    functions, calls, overrides, unresolved, results = {}, set(), set(), set(), []
    variants, primaries, observed, template_aliases = {}, {}, set(), {}
    locations, location_primaries = {}, {}

    def consume(result):
        missing_generated = {path for path in result.get('includes', []) + [result['file']]
                             if path.startswith('@build/') and path not in frozen_paths}
        if missing_generated:
            raise ValueError('Generated includes lack frozen source mappings; refreeze with these inputs: '
                             + ', '.join(sorted(missing_generated)))
        identity = (result["file"], result.get("command_sha256", ""))
        for usr, record in result["functions"].items():
            location = (record["file"], record["start"], record["end"])
            if (usr, location) not in location_primaries or identity < location_primaries[usr, location]:
                locations.setdefault(usr, {})[location] = dict(record)
                location_primaries[usr, location] = identity
            if usr not in functions:
                functions[usr], primaries[usr] = record, identity
            else:
                names = sorted(set(functions[usr]["wx"]) | set(record["wx"]))
                if functions[usr]["sha256"] != record["sha256"]:
                    variants.setdefault(usr, set()).update([functions[usr]["sha256"], record["sha256"]])
                if identity < primaries[usr]:
                    functions[usr], primaries[usr] = record, identity
                functions[usr]["wx"] = names
        calls.update(tuple(c) for c in result["calls"])
        overrides.update(tuple(c) for c in result["overrides"])
        for instance, primary in result.get("template_aliases", {}).items():
            template_aliases.setdefault(instance, set()).add(primary)
        unresolved.update((c["caller"], result["file"], c["line"], c["expression"]) for c in result["unresolved"])
        if result["status"] == "ok":
            observed.update(f["file"] for f in result["functions"].values())
            observed.update(result.get("includes", []))
            observed.add(result["file"])
        summary = {k: v for k, v in result.items() if k not in
                   ("functions", "calls", "overrides", "unresolved", "template_aliases")}
        summary["function_count"] = len(result["functions"])
        results.append(summary)

    pending = {}
    with ProcessPoolExecutor(max_workers=a.jobs, max_tasks_per_child=1) as pool:
        def consume_safely(result):
            try:
                consume(result)
            except BaseException:
                # Do not silently drain thousands of queued parses while the
                # executor context waits to expose an aggregation failure.
                pool.shutdown(wait=False, cancel_futures=True)
                raise

        for job in jobs:
            key = digest(json.dumps([snapshot, job], sort_keys=True).encode())
            cache = a.cache / (key + ".json")
            if cache.exists() or cache.with_suffix(".json.gz").exists():
                result = read_json(cache)
                artifact = cache if cache.exists() else cache.with_suffix('.json.gz')
                result['cache_reused_from'] = {'path': str(artifact), 'sha256': digest(artifact.read_bytes())}
                consume_safely(result)
            else:
                pending[pool.submit(extract_translation_unit, job)] = (cache, job)
        for future in as_completed(pending):
            cache, job = pending.pop(future)
            try:
                result = future.result()
            except Exception as exc:
                result = {"file": job["file"].replace(a.original_source_root, str(root)).removeprefix(str(root) + "/"),
                          "status": "worker-error", "functions": {}, "calls": [], "overrides": [],
                          "unresolved": [], "diagnostics": [{"severity": 4, "message": repr(exc)}]}
            result["command_sha256"] = digest(json.dumps(job, sort_keys=True).encode())
            source = Path(job["file"].replace(a.original_source_root, str(root)))
            result["source_sha256"] = digest(source.read_bytes()) if source.is_file() else None
            write_cache(cache, result)
            consume_safely(result)
            print(f"{len(results)}/{len(jobs)} {result['status']} {result['file']}", flush=True)
    calls, overrides, definition_copies = materialize_definitions(functions, calls, overrides, locations)
    if a.generated_manifest:
        verify_generated(generated_manifest, build, a.generated_manifest.parent)
    edges = expand_virtual(calls, overrides)
    classify(functions, edges)
    command_files = {r["file"] for r in results}
    for entry in inventory:
        entry["coverage"] = ("parsed" if entry["path"] in observed else
                             "command-only" if entry["path"] in command_files else "uncompiled")
    failures = [{"file": r["file"], "status": r["status"], "diagnostics": r["diagnostics"]}
                for r in results if r["status"] != "ok"]
    missing = [e["path"] for e in inventory if e["category"] in ("application", "generated")
               and Path(e["path"]).suffix in (".c", ".cc", ".cpp", ".cxx", ".m", ".mm")
               and e["path"] not in observed]
    provenance = {"upstream_pin": PIN, "source_commit": manifest["source_commit"],
                  "source_inventory_sha256": snapshot, "compile_commands_sha256": digest(raw),
                  "tracked_source_inventory_sha256": tracked_snapshot,
                  "generated_sources_manifest_sha256": digest(a.generated_manifest.read_bytes()) if a.generated_manifest else None,
                  "generated_input_dependencies": generator_dependencies(generated_manifest) if build else {},
                  "source_manifest_sha256": digest(a.manifest.read_bytes().replace(b"\r\n", b"\n")),
                  "source_manifest_hash_policy": "SHA-256 after CRLF-to-LF normalization",
                  "source_hash_policy": "SHA-256 after CRLF-to-LF normalization; function offsets refer to snapshot bytes",
                  "extractor_sha256": jobs[0]["extractor_sha256"] if jobs else None,
                  "extractor_hash_policy": "extract_graph.py, clang_graph.py, model.py, generated_sources.py concatenated after CRLF-to-LF normalization",
                  "compile_commands_path": str(a.compile_commands.resolve()),
                  "compile_commands_count": len(commands), "commands_attempted": len(results),
                  "source_root": str(root), "original_source_root": a.original_source_root,
                  "build_root": str(build) if build else None,
                  "libclang": a.libclang, "clang_python": "21.1.7", "jobs": a.jobs,
                  "libclang_sha256": libclang_sha256,
                  "configuration": "Linux WSL compile database; active preprocessor branches only",
                  "extra_args": jobs[0]["extra_args"] if jobs else [],
                  "inventory_complete": True, "parse_complete": not failures,
                  "compile_database_complete": len(results) == len(commands),
                  "all_configurations_complete": False,
                  "gate_complete": not failures and len(results) == len(commands),
                  "gate_definition": "Extractor diagnostic only: all WSL commands parsed and all tracked sources inventoried; not the PORT_PLAN phase-0 gate",
                  "limitations": ["No Windows/macOS or inactive preprocessor branch extraction",
                                  "Opaque template layouts conservatively union fixed storage/base types from visible primary and partial declarations; T is not assumed wx",
                                  "Indirect function pointers and unresolved dependent template calls are recorded, not guessed",
                                  "Delete and explicit destructors dispatch to observed virtual overrides; local/by-value objects and destructor bases/fields add visible cleanup dependencies",
                                  "Implicit destructor bodies/overrides absent from libclang are unresolved, not invented; temporary, exception-unwind, union/variant and global lifetime cleanup is not exhaustively modeled"],
                  "parse_failures": failures, "uncompiled_application_sources": missing,
                  "unresolved_call_count": len(unresolved),
                  "definition_variant_count": len(variants),
                  "additional_definition_body_count": len(definition_copies),
                  "template_alias_variant_count": sum(len(v) > 1 for v in template_aliases.values()),
                  "cache_reused_commands": sum(bool(r.get("cache_reused_from")) for r in results),
                  "counts": dict(Counter(e["category"] for e in inventory))}
    translation_units = [{"file": r["file"], "status": r["status"],
                          "category": "generated-build" if r["file"].startswith('@build/') or Path(r["file"]).is_absolute() else "tracked-source",
                          "command_sha256": r.get("command_sha256"), "source_sha256": r.get("source_sha256"),
                          "cache_reused_from": r.get("cache_reused_from"),
                          "function_count": r["function_count"], "includes": r.get("includes", []),
                          "diagnostics": r["diagnostics"]} for r in sorted(results, key=lambda r: (r["file"], r.get("command_sha256", "")))]
    graph = {"schema_version": 1, "provenance": provenance, "functions": functions,
             "edges": edges, "overrides": sorted(overrides),
             "unresolved_calls": [{"caller": c, "file": f, "line": line, "expression": e}
                                  for c, f, line, e in sorted(unresolved)],
             "translation_units": translation_units,
             "template_aliases": {k: sorted(v) for k, v in sorted(template_aliases.items())},
             "definition_copies": dict(sorted(definition_copies.items())),
             "definition_variants": {k: sorted(v) for k, v in sorted(variants.items())}}
    from extractor.model import write_graph
    write_graph(a.output / "graph.json", graph)
    write_json(a.output / "inventory.json", {"schema_version": 1, "provenance": provenance, "files": inventory})
    print(json.dumps({"functions": len(functions), "edges": len(edges), "failures": len(failures),
                      "uncompiled_application_sources": len(missing)}))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
