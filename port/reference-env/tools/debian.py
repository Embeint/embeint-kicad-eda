#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Debian control parsing, version ordering and a closed dependency resolver.

The resolver does not imitate apt's heuristics. It computes one explicit package
set from a fixed archive snapshot; the image then installs exactly those local
.deb files with no package index, so apt cannot pick anything else. Any
ambiguity (a virtual package with several providers and no recorded choice) is
an error rather than a silent preference.
"""

import re

_FIELD = re.compile(r"^([A-Za-z0-9][A-Za-z0-9-]*):[ \t]?(.*)$")
_RELATION = re.compile(
    r"^\s*([a-z0-9][a-z0-9+.-]+)(?::([a-z0-9-]+))?\s*(?:\(\s*(<<|<=|=|>=|>>)\s*([^)\s]+)\s*\))?\s*$")


def parse_paragraphs(text):
    """Yield one dict per RFC822-style paragraph; continuation lines are kept."""
    for block in re.split(r"\n\s*\n", text):
        fields = {}
        last = None
        for line in block.splitlines():
            if not line.strip():
                continue
            if line[0] in " \t":
                if last is None:
                    raise ValueError("Continuation line without a field: " + line)
                fields[last] += "\n" + line
                continue
            match = _FIELD.match(line)
            if not match:
                raise ValueError("Malformed control line: " + line)
            last = match.group(1)
            fields[last] = match.group(2)
        if fields:
            yield fields


def _order(char):
    if char == "~":
        return -1
    if char.isalpha():
        return ord(char)
    return ord(char) + 256


def _compare_part(left, right):
    """dpkg's verrevcmp for one upstream version or revision string."""
    i = j = 0
    while i < len(left) or j < len(right):
        first_diff = 0
        while (i < len(left) and not left[i].isdigit()) or (j < len(right) and not right[j].isdigit()):
            lc = _order(left[i]) if i < len(left) and not left[i].isdigit() else 0
            rc = _order(right[j]) if j < len(right) and not right[j].isdigit() else 0
            if lc != rc:
                return lc - rc
            i += 1
            j += 1
        while i < len(left) and left[i] == "0":
            i += 1
        while j < len(right) and right[j] == "0":
            j += 1
        while i < len(left) and left[i].isdigit() and j < len(right) and right[j].isdigit():
            if not first_diff:
                first_diff = ord(left[i]) - ord(right[j])
            i += 1
            j += 1
        if i < len(left) and left[i].isdigit():
            return 1
        if j < len(right) and right[j].isdigit():
            return -1
        if first_diff:
            return first_diff
    return 0


def split_version(version):
    epoch = 0
    rest = version
    if ":" in version:
        head, rest = version.split(":", 1)
        epoch = int(head)
    upstream, revision = (rest.rsplit("-", 1) if "-" in rest else (rest, "0"))
    return epoch, upstream, revision


def compare_versions(left, right):
    """Return negative, zero or positive like dpkg --compare-versions."""
    le, lu, lr = split_version(left)
    re_, ru, rr = split_version(right)
    if le != re_:
        return le - re_
    return _compare_part(lu, ru) or _compare_part(lr, rr)


def satisfies(version, op, wanted):
    if op is None:
        return True
    result = compare_versions(version, wanted)
    return {"<<": result < 0, "<=": result <= 0, "=": result == 0,
            ">=": result >= 0, ">>": result > 0}[op]


def parse_relations(value):
    """Parse Depends-like text into [[(name, arch, op, version), ...alternatives], ...]."""
    groups = []
    if not value or not value.strip():
        return groups
    for group in value.replace("\n", " ").split(","):
        alternatives = []
        for alternative in group.split("|"):
            if "[" in alternative or "<" in alternative.split("(")[0]:
                raise ValueError("Architecture/profile restriction in binary relation: " + alternative)
            match = _RELATION.match(alternative)
            if not match:
                raise ValueError("Malformed relation: " + alternative)
            alternatives.append(match.groups())
        groups.append(alternatives)
    return groups


class Archive:
    """Highest-version candidates and providers across the snapshot suites."""

    def __init__(self, stanzas, architecture="amd64"):
        self.candidates = {}
        self.providers = {}
        for stanza in stanzas:
            if stanza.get("Architecture") not in (architecture, "all"):
                continue
            name = stanza["Package"]
            current = self.candidates.get(name)
            if current is None or compare_versions(stanza["Version"], current["Version"]) > 0:
                self.candidates[name] = stanza
        for name, stanza in self.candidates.items():
            for group in parse_relations(stanza.get("Provides", "")):
                for provided, _arch, op, version in group:
                    if op not in (None, "="):
                        raise ValueError("Unsupported Provides relation in " + name)
                    self.providers.setdefault(provided, []).append((name, version))


def _installed_provides(stanza):
    for group in parse_relations(stanza.get("Provides", "")):
        for provided, _arch, _op, version in group:
            yield provided, version


def relation_satisfied(relation, installed):
    """installed maps package name to its stanza (base or selected)."""
    name, _arch, op, version = relation
    stanza = installed.get(name)
    if stanza is not None and satisfies(stanza["Version"], op, version):
        return True
    for other in installed.values():
        for provided, provided_version in _installed_provides(other):
            if provided != name:
                continue
            if op is None or (provided_version is not None and satisfies(provided_version, op, version)):
                return True
    return False


def resolve(archive, base, requested, provider_choices=None):
    """Return (selected stanzas by name, upgraded base names).

    base: name -> stanza from the image's dpkg status. requested: package names.
    provider_choices: virtual name -> chosen real package.
    """
    provider_choices = provider_choices or {}
    installed = dict(base)
    selected = {}
    upgraded = set()
    queue = []

    def select(name):
        stanza = archive.candidates.get(name)
        if stanza is None:
            raise ValueError("Package not in snapshot: " + name)
        if name in installed and installed[name]["Version"] == stanza["Version"]:
            return
        if name in base:
            upgraded.add(name)
        installed[name] = stanza
        selected[name] = stanza
        queue.append(stanza)

    for name in requested:
        select(name)

    while True:
        while queue:
            stanza = queue.pop(0)
            for field in ("Pre-Depends", "Depends"):
                for group in parse_relations(stanza.get(field, "")):
                    if any(relation_satisfied(rel, installed) for rel in group):
                        continue
                    chosen = None
                    for name, _arch, op, version in group:
                        candidate = archive.candidates.get(name)
                        if candidate is not None and satisfies(candidate["Version"], op, version):
                            chosen = name
                            break
                    if chosen is None:
                        for name, _arch, op, version in group:
                            options = sorted(
                                provider for provider, provided_version in archive.providers.get(name, [])
                                if op is None or (provided_version is not None
                                                  and satisfies(provided_version, op, version)))
                            if not options:
                                continue
                            if name in provider_choices:
                                if provider_choices[name] not in options:
                                    raise ValueError(f"Recorded provider for {name} is not available: "
                                                     f"{provider_choices[name]} not in {options}")
                                chosen = provider_choices[name]
                            elif len(options) == 1:
                                chosen = options[0]
                            else:
                                raise ValueError(f"Ambiguous virtual package {name} for "
                                                 f"{stanza['Package']}: record one of {options}")
                            break
                    if chosen is None:
                        raise ValueError(f"Unsatisfiable dependency of {stanza['Package']}: "
                                         f"{stanza.get(field)}")
                    select(chosen)
        # An upgrade can break a base package's exact dependency; upgrade it too.
        broken = sorted(name for name, stanza in installed.items() if name not in selected and any(
            not any(relation_satisfied(rel, installed) for rel in group)
            for field in ("Pre-Depends", "Depends")
            for group in parse_relations(stanza.get(field, ""))))
        if not broken:
            break
        for name in broken:
            if name in archive.candidates and installed[name]["Version"] == archive.candidates[name]["Version"]:
                raise ValueError("Unsatisfiable dependency of current snapshot package: " + name)
            select(name)
    for name, stanza in sorted(installed.items()):
        for field in ("Pre-Depends", "Depends"):
            if any(not any(relation_satisfied(rel, installed) for rel in group)
                   for group in parse_relations(stanza.get(field, ""))):
                raise ValueError("Unsatisfiable final dependencies of " + name)
    check_conflicts(installed)
    return selected, upgraded


def check_conflicts(installed):
    for name, stanza in sorted(installed.items()):
        for field in ("Conflicts", "Breaks"):
            for group in parse_relations(stanza.get(field, "")):
                for other, _arch, op, version in group:
                    others = {n: s for n, s in installed.items() if n != name}
                    if relation_satisfied((other, _arch, op, version), others):
                        raise ValueError(f"{name} {field} {other} (including virtual providers)")
