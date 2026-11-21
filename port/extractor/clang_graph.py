# SPDX-License-Identifier: GPL-3.0-or-later
"""libclang cursor extraction. The compile database and its build tree stay read-only."""
import ctypes
import os
import re
import shlex
from pathlib import Path

from .model import digest


def extract_translation_unit(job):
    root, old = job["root"], job["old_root"]
    source = job["file"].replace(old, root)
    out = {"file": source.removeprefix(root + "/"), "functions": {},
           "calls": [], "overrides": [], "unresolved": [], "diagnostics": []}
    if not Path(source).is_file():
        out["status"] = "missing-source"
        out["diagnostics"].append({"severity": 4, "message": "Compilation source is absent; not parsed"})
        return out
    from clang import cindex as ci
    if not ci.Config.loaded:
        ci.Config.set_library_file(job["libclang"])
    args = job.get("arguments") or shlex.split(job["command"])
    cleaned, skip = [], False
    for arg in args[1:]:
        if skip:
            skip = False
            continue
        if arg in ("-o", "-MF", "-MT", "-MQ"):
            skip = True
        elif arg not in ("-c", "-MD", "-MMD") and arg != job["file"]:
            cleaned.append(arg.replace(old, root))
    # GCC knows its C++ include search implicitly; libclang does not.
    cleaned += job.get("extra_args", [])
    os.chdir(job["directory"])
    try:
        tu = ci.Index.create().parse(source, args=cleaned,
                                    options=ci.TranslationUnit.PARSE_DETAILED_PROCESSING_RECORD)
        out["diagnostics"] = [{"severity": d.severity, "message": str(d)} for d in tu.diagnostics]
        functions = {ci.CursorKind.FUNCTION_DECL, ci.CursorKind.CXX_METHOD,
                     ci.CursorKind.CONSTRUCTOR, ci.CursorKind.DESTRUCTOR,
                     ci.CursorKind.CONVERSION_FUNCTION, ci.CursorKind.FUNCTION_TEMPLATE}
        get_overrides = ci.conf.lib.clang_getOverriddenCursors
        get_overrides.argtypes = [ci.Cursor, ctypes.POINTER(ctypes.POINTER(ci.Cursor)),
                                  ctypes.POINTER(ctypes.c_uint)]
        dispose = ci.conf.lib.clang_disposeOverriddenCursors
        dispose.argtypes = [ctypes.POINTER(ci.Cursor)]
        text_cache = {}
        class_cache = {}
        aliases = {}

        def identity(cursor):
            """Connect instantiated calls to the primary body that owns their source.

            An explicit specialization has its own source location and body; it
            must retain its own USR. Only implicit same-location instances alias.
            """
            original = cursor.get_usr()
            seen = set()
            while cursor.get_usr() not in seen:
                seen.add(cursor.get_usr())
                primary = cursor.specialized_template
                if primary is None:
                    break
                if primary.location != cursor.location:
                    break
                cursor = primary
            target = cursor.get_usr()
            if original != target:
                aliases[original] = target
            return target
        wx_kinds = {ci.CursorKind.TYPE_REF, ci.CursorKind.TEMPLATE_REF, ci.CursorKind.DECL_REF_EXPR,
                    ci.CursorKind.MEMBER_REF_EXPR, ci.CursorKind.VAR_DECL, ci.CursorKind.PARM_DECL,
                    ci.CursorKind.CALL_EXPR, ci.CursorKind.CXX_BASE_SPECIFIER}

        def local(cursor):
            file = cursor.location.file
            return file and str(file).startswith(root + "/")

        def wx(cursor):
            texts = [cursor.type.spelling, cursor.spelling]
            ref = cursor.referenced
            if ref:
                texts.extend([ref.type.spelling, ref.spelling])
                parent = ref.semantic_parent
                if parent:
                    texts.append(parent.spelling)
            return set(re.findall(r"\bwx[A-Z]\w*", " ".join(texts)))

        def class_wx(cursor, visiting=None):
            if not cursor or cursor.kind not in (ci.CursorKind.CLASS_DECL, ci.CursorKind.STRUCT_DECL,
                                                  ci.CursorKind.CLASS_TEMPLATE):
                return set()
            key = cursor.get_usr()
            if key in class_cache:
                return class_cache[key]
            visiting = set() if visiting is None else visiting
            if key in visiting:
                return set()
            visiting.add(key)
            names = wx(cursor)
            for child in cursor.get_children():
                if child.kind == ci.CursorKind.CXX_BASE_SPECIFIER:
                    names.update(wx(child))
                    names.update(class_wx(child.referenced, visiting))
                elif child.kind == ci.CursorKind.FIELD_DECL:
                    names.update(re.findall(r"\bwx[A-Z]\w*", child.type.spelling))
            class_cache[key] = names
            return names

        def qualified(cursor):
            parts = [cursor.displayname]
            parent = cursor.semantic_parent
            while parent and parent.kind != ci.CursorKind.TRANSLATION_UNIT:
                if parent.spelling:
                    parts.append(parent.spelling)
                parent = parent.semantic_parent
            return "::".join(reversed(parts))

        def walk(cursor, owner=None):
            if owner is None and cursor.kind != ci.CursorKind.TRANSLATION_UNIT and not local(cursor):
                return
            current = owner
            if cursor.kind in functions:
                usr = identity(cursor)
                if cursor.kind == ci.CursorKind.CXX_METHOD:
                    ptr, count = ctypes.POINTER(ci.Cursor)(), ctypes.c_uint()
                    get_overrides(cursor, ctypes.byref(ptr), ctypes.byref(count))
                    for i in range(count.value):
                        base = ptr[i]
                        base._tu = cursor._tu
                        out["overrides"].append([identity(base), usr])
                    dispose(ptr)
                if cursor.is_definition() and usr:
                    current = usr
                    file = str(cursor.location.file)
                    if file not in text_cache:
                        text_cache[file] = Path(file).read_bytes()
                    start, end = cursor.extent.start, cursor.extent.end
                    body = text_cache[file][start.offset:end.offset]
                    out["functions"][usr] = {
                        "name": qualified(cursor), "file": file.removeprefix(root + "/"),
                        "start": start.line, "end": end.line,
                        "start_offset": start.offset, "end_offset": end.offset,
                        "lines": end.line - start.line + 1, "sha256": digest(body.replace(b"\r\n", b"\n")),
                        "wx": sorted(wx(cursor) | class_wx(cursor.semantic_parent)), "virtual": cursor.kind == ci.CursorKind.CXX_METHOD
                        and cursor.is_virtual_method()}
                else:
                    current = None
            if current:
                record = out["functions"][current]
                if cursor.kind in wx_kinds:
                    record["wx"] = sorted(set(record["wx"]) | wx(cursor))
                if cursor.kind == ci.CursorKind.CALL_EXPR:
                    ref = cursor.referenced
                    if ref and ref.kind in functions and ref.get_usr():
                        virtual = ref.kind == ci.CursorKind.CXX_METHOD and ref.is_virtual_method()
                        out["calls"].append([current, identity(ref), virtual])
                        # External wx calls still contribute direct classification.
                        record["wx"] = sorted(set(record["wx"]) | wx(ref))
                    else:
                        out["unresolved"].append({"caller": current, "line": cursor.location.line,
                                                  "expression": cursor.spelling})
            for child in cursor.get_children():
                walk(child, current)

        walk(tu.cursor)
        out["includes"] = sorted({str(i.include).removeprefix(root + "/")
                                  for i in tu.get_includes() if str(i.include).startswith(root + "/")})
        out["template_aliases"] = aliases
        out["status"] = "parse-error" if any(d["severity"] >= 3 for d in out["diagnostics"]) else "ok"
    except Exception as exc:
        out["status"] = "extract-error"
        out["diagnostics"].append({"severity": 4, "message": repr(exc)})
    return out
