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
    build = job.get("build_root")

    def logical(file):
        path = os.path.normpath(str(file))
        if path.startswith(root + "/"):
            return path.removeprefix(root + "/")
        if build and path.startswith(build + "/"):
            return "@build/" + path.removeprefix(build + "/")
        return None

    out = {"file": logical(source) or source, "functions": {},
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
        type_nodes, type_cache = {}, {}
        destructor_cache = {}
        aliases = {}
        template_partials = {}

        def template_root(cursor):
            seen = set()
            while cursor and cursor.get_usr() not in seen:
                seen.add(cursor.get_usr())
                primary = cursor.specialized_template
                if not primary:
                    break
                cursor = primary
            return cursor

        declaration_containers = {ci.CursorKind.TRANSLATION_UNIT, ci.CursorKind.NAMESPACE,
            ci.CursorKind.LINKAGE_SPEC, ci.CursorKind.UNEXPOSED_DECL,
            ci.CursorKind.CLASS_DECL, ci.CursorKind.STRUCT_DECL, ci.CursorKind.UNION_DECL,
            ci.CursorKind.CLASS_TEMPLATE, ci.CursorKind.CLASS_TEMPLATE_PARTIAL_SPECIALIZATION}

        def collect_partials(cursor):
            # Partial specializations can be in external headers too. Function
            # bodies cannot declare class templates, so prune them here.
            if cursor.kind not in declaration_containers:
                return
            if cursor.kind == ci.CursorKind.CLASS_TEMPLATE_PARTIAL_SPECIALIZATION:
                primary = template_root(cursor)
                if primary:
                    template_partials.setdefault(primary.get_usr(), []).append(cursor)
            for child in cursor.get_children():
                collect_partials(child)

        collect_partials(tu.cursor)

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
            return file and logical(file) is not None

        def source_range(cursor, file):
            """Use a conservative clang parent range for null template extents.

            libclang 21 can return a null extent for constrained `auto` method
            templates. The enclosing declaration is real clang source evidence;
            widening it avoids guessing tokens or splitting an unknown signature.
            """
            candidate = cursor
            while candidate:
                start, end = candidate.extent.start, candidate.extent.end
                if (start.file and end.file and start.line > 0 and end.line >= start.line
                        and os.path.normpath(str(start.file)) == file
                        and os.path.normpath(str(end.file)) == file and end.offset > start.offset):
                    kind = "function" if candidate == cursor else "enclosing-declaration"
                    return start, end, kind
                candidate = candidate.semantic_parent
            raise ValueError(f"No valid clang source extent for {cursor.get_usr()} in {file}")

        def type_wx(type):
            """Follow canonical types, template arguments and record storage.

            Cache only complete reachable closures. A recursive field graph must
            not cache an incomplete result while another node is being visited.
            """
            def key(t):
                decl = t.get_declaration()
                return t.kind.value, t.spelling, t.get_canonical().spelling, decl.get_usr() if decl else ""

            initial = key(type)
            if initial in type_cache:
                return type_cache[initial]
            names, visited, pending = set(), set(), [type]
            while pending:
                t = pending.pop()
                k = key(t)
                if k in visited or t.kind == ci.TypeKind.INVALID:
                    continue
                visited.add(k)
                if k in type_cache:
                    names.update(type_cache[k])
                    continue
                if k not in type_nodes:
                    children = []
                    canonical = t.get_canonical()
                    if key(canonical) != k:
                        children.append(canonical)
                    if t.kind in (ci.TypeKind.POINTER, ci.TypeKind.LVALUEREFERENCE,
                                  ci.TypeKind.RVALUEREFERENCE, ci.TypeKind.MEMBERPOINTER):
                        children.append(t.get_pointee())
                    if t.kind in (ci.TypeKind.CONSTANTARRAY, ci.TypeKind.INCOMPLETEARRAY,
                                  ci.TypeKind.VARIABLEARRAY, ci.TypeKind.DEPENDENTSIZEDARRAY):
                        children.append(t.get_array_element_type())
                    if t.kind in (ci.TypeKind.FUNCTIONPROTO, ci.TypeKind.FUNCTIONNOPROTO):
                        children.append(t.get_result())
                        if t.kind == ci.TypeKind.FUNCTIONPROTO:
                            children.extend(t.argument_types())
                    children.extend(t.get_template_argument_type(i)
                                    for i in range(max(0, t.get_num_template_arguments())))
                    declaration = t.get_declaration()
                    if declaration and declaration.kind in (ci.CursorKind.TYPEDEF_DECL, ci.CursorKind.TYPE_ALIAS_DECL):
                        children.append(declaration.underlying_typedef_type)
                    # wx replacement types are atomic classification inputs;
                    # their private implementation is not part of the port.
                    if t.kind == ci.TypeKind.RECORD and not (declaration and re.match(r"wx[A-Z]", declaration.spelling)):
                        children.extend(field.type for field in t.get_fields())
                        if declaration:
                            primary = template_root(declaration)
                            sources = [declaration.get_definition() or declaration]
                            if primary and primary != declaration:
                                sources.append(primary.get_definition() or primary)
                                sources.extend(template_partials.get(primary.get_usr(), []))
                            # libclang can expose an empty specialization cursor
                            # even after layout instantiation. Union its visible
                            # template source storage/bases; do not guess that T
                            # itself is wx or inject instantiations into the TU.
                            for owner in sources:
                                children.extend(c.type for c in owner.get_children()
                                    if c.kind in (ci.CursorKind.CXX_BASE_SPECIFIER, ci.CursorKind.FIELD_DECL))
                    type_nodes[k] = (set(re.findall(r"\bwx[A-Z]\w*", t.spelling)), children)
                direct, children = type_nodes[k]
                names.update(direct)
                pending.extend(children)
            type_cache[initial] = names
            return names

        def wx(cursor):
            texts = [cursor.type.spelling, cursor.spelling]
            names = set(type_wx(cursor.type))
            ref = cursor.referenced
            if ref:
                texts.extend([ref.type.spelling, ref.spelling])
                names.update(type_wx(ref.type))
                parent = ref.semantic_parent
                if parent:
                    texts.append(parent.spelling)
            return names | set(re.findall(r"\bwx[A-Z]\w*", " ".join(texts)))

        def class_wx(cursor):
            if not cursor or cursor.kind not in (ci.CursorKind.CLASS_DECL, ci.CursorKind.STRUCT_DECL,
                                                  ci.CursorKind.CLASS_TEMPLATE, ci.CursorKind.UNION_DECL,
                                                  ci.CursorKind.CLASS_TEMPLATE_PARTIAL_SPECIALIZATION):
                return set()
            names = set(type_wx(cursor.type))
            for child in cursor.get_children():
                if child.kind in (ci.CursorKind.CXX_BASE_SPECIFIER, ci.CursorKind.FIELD_DECL):
                    names.update(type_wx(child.type))
            return names

        def virtual(cursor):
            return cursor.kind in (ci.CursorKind.CXX_METHOD, ci.CursorKind.DESTRUCTOR,
                                   ci.CursorKind.CONVERSION_FUNCTION) and cursor.is_virtual_method()

        def destruction(type, owner, cursor, dispatch=False):
            type = type.get_canonical()
            while type.kind in (ci.TypeKind.CONSTANTARRAY, ci.TypeKind.INCOMPLETEARRAY,
                                ci.TypeKind.VARIABLEARRAY, ci.TypeKind.DEPENDENTSIZEDARRAY):
                type = type.get_array_element_type().get_canonical()
            if type.kind != ci.TypeKind.RECORD:
                return
            decl = type.get_declaration()
            if not decl:
                out["unresolved"].append({"caller": owner, "line": cursor.location.line,
                                          "expression": "destructor type unavailable: " + type.spelling})
                return
            decl = decl.get_definition() or decl
            key = decl.get_usr(), type.spelling
            if key not in destructor_cache:
                destructor_cache[key] = next((c for c in decl.get_children()
                                              if c.kind == ci.CursorKind.DESTRUCTOR), None)
            target = destructor_cache[key]
            if target and target.get_usr():
                out["calls"].append([owner, identity(target), dispatch and virtual(target)])
                return virtual(target)
            if not type.is_pod():
                out["unresolved"].append({"caller": owner, "line": cursor.location.line,
                    "expression": "implicit destructor not exposed by libclang: " + type.spelling})

        def qualified(cursor):
            parts = [cursor.displayname]
            parent = cursor.semantic_parent
            while parent and parent.kind != ci.CursorKind.TRANSLATION_UNIT:
                if parent.spelling:
                    parts.append(parent.spelling)
                parent = parent.semantic_parent
            return "::".join(reversed(parts))

        def walk(cursor, owner=None, parent_kind=None):
            if owner is None and cursor.kind != ci.CursorKind.TRANSLATION_UNIT and not local(cursor):
                return
            current = owner
            if cursor.kind in functions:
                usr = identity(cursor)
                if cursor.kind in (ci.CursorKind.CXX_METHOD, ci.CursorKind.DESTRUCTOR, ci.CursorKind.CONVERSION_FUNCTION):
                    ptr, count = ctypes.POINTER(ci.Cursor)(), ctypes.c_uint()
                    get_overrides(cursor, ctypes.byref(ptr), ctypes.byref(count))
                    for i in range(count.value):
                        base = ptr[i]
                        base._tu = cursor._tu
                        out["overrides"].append([identity(base), usr])
                    dispose(ptr)
                if cursor.is_definition() and usr and local(cursor):
                    current = usr
                    file = os.path.normpath(str(cursor.location.file))
                    if file not in text_cache:
                        text_cache[file] = Path(file).read_bytes()
                    start, end, range_kind = source_range(cursor, file)
                    body = text_cache[file][start.offset:end.offset]
                    out["functions"][usr] = {
                        "name": qualified(cursor), "file": logical(file),
                        "start": start.line, "end": end.line,
                        "start_offset": start.offset, "end_offset": end.offset,
                        "range_kind": range_kind, "definition_line": cursor.location.line,
                        "lines": end.line - start.line + 1, "sha256": digest(body.replace(b"\r\n", b"\n")),
                        "wx": sorted(wx(cursor) | class_wx(cursor.semantic_parent)), "virtual": virtual(cursor)}
                    if cursor.kind == ci.CursorKind.DESTRUCTOR:
                        for member in cursor.semantic_parent.get_children():
                            if member.kind in (ci.CursorKind.CXX_BASE_SPECIFIER, ci.CursorKind.FIELD_DECL):
                                destruction(member.type, current, cursor)
                else:
                    current = None
            if current:
                record = out["functions"][current]
                if cursor.kind in wx_kinds:
                    record["wx"] = sorted(set(record["wx"]) | wx(cursor))
                if cursor.kind == ci.CursorKind.CALL_EXPR:
                    ref = cursor.referenced
                    if ref and ref.kind in functions and ref.get_usr():
                        out["calls"].append([current, identity(ref), virtual(ref)])
                        if ref.kind == ci.CursorKind.DESTRUCTOR and virtual(ref):
                            out['unresolved'].append({'caller': current, 'line': cursor.location.line,
                                'expression': 'virtual destructor dispatch: compiler-implicit derived overrides may be absent'})
                        # External wx calls still contribute direct classification.
                        record["wx"] = sorted(set(record["wx"]) | wx(ref))
                    else:
                        out["unresolved"].append({"caller": current, "line": cursor.location.line,
                                                  "expression": cursor.spelling})
                    result_type = cursor.type.get_canonical()
                    if (result_type.kind == ci.TypeKind.RECORD and not result_type.is_pod()
                            and parent_kind not in (ci.CursorKind.VAR_DECL, ci.CursorKind.CXX_NEW_EXPR)):
                        out['unresolved'].append({'caller': current, 'line': cursor.location.line,
                            'expression': 'temporary lifetime cleanup/elision not modeled: ' + result_type.spelling})
                if cursor.kind == ci.CursorKind.CXX_DELETE_EXPR:
                    operands = list(cursor.get_children())
                    if operands:
                        if destruction(operands[-1].type.get_canonical().get_pointee(), current, cursor, dispatch=True):
                            out['unresolved'].append({'caller': current, 'line': cursor.location.line,
                                'expression': 'virtual destructor dispatch: compiler-implicit derived overrides may be absent'})
                    else:
                        out["unresolved"].append({"caller": current, "line": cursor.location.line,
                                                  "expression": "delete operand type unavailable"})
                    out['unresolved'].append({'caller': current, 'line': cursor.location.line,
                        'expression': 'delete deallocation function selection is not exposed by libclang'})
                elif cursor.kind in (ci.CursorKind.VAR_DECL, ci.CursorKind.PARM_DECL):
                    destruction(cursor.type, current, cursor)
            for child in cursor.get_children():
                walk(child, current, cursor.kind)

        walk(tu.cursor)
        out["includes"] = sorted({logical(i.include) for i in tu.get_includes() if logical(i.include) is not None})
        out["template_aliases"] = aliases
        out["status"] = "parse-error" if any(d["severity"] >= 3 for d in out["diagnostics"]) else "ok"
    except Exception as exc:
        out["status"] = "extract-error"
        out["diagnostics"].append({"severity": 4, "message": repr(exc)})
    return out
