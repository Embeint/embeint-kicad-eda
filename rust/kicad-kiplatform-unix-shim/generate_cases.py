#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for
libs.kiplatform.os.unix.application.ui.p001.

Writes port/cases/<unit>.jsonl (requests only: an operation, its arguments and the host
script the scripted host and the C++ oracle's fakes answer from) and
port/oracle/fixtures/kiplatform-unix-ui-p001-controls.jsonl (requests both adapters must
refuse, with the expected refusal: malformed requests and inputs whose reference evaluation
is undefined). It never computes a reference reply; the expected-replies counts only say
which cases reach a deferred continuation. Doubles are given as their bit patterns, built
with struct from decimal literals (correctly rounded on every host); no arithmetic is done on
them. `--check` verifies the committed files.
"""
import argparse
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.kiplatform.os.unix.application.ui.p001"
CASES = ROOT / "port/cases" / (UNIT + ".jsonl")
CONTROLS = ROOT / "port/oracle/fixtures/kiplatform-unix-ui-p001-controls.jsonl"
EXPECTED = ROOT / "rust/kicad-kiplatform-unix-shim/expected-replies.json"
I32_MIN, I32_MAX = -(1 << 31), (1 << 31) - 1
I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1
S_IFREG, S_IFLNK, S_IFDIR = 0o100000, 0o120000, 0o040000


def U(text):
    """The wchar_t units of a Python string (code points)."""
    return [ord(c) for c in text]


def H(data):
    """Lower-case hex of bytes (or of a str's UTF-8)."""
    if isinstance(data, str):
        data = data.encode("utf-8")
    return data.hex()


def B(value):
    """A double's bits from a decimal literal or a special name."""
    special = {"inf": "7ff0000000000000", "-inf": "fff0000000000000",
               "nan": "7ff8000000000000", "-0": "8000000000000000",
               "snan": "7ff4000000000000", "tiny": "0000000000000001",
               "max": "7fefffffffffffff"}
    if value in special:
        return special[value]
    return struct.pack(">d", float(value)).hex()


def stat(result=0, mode=S_IFREG | 0o644, mtime=1700000000, size=10):
    return {"result": result, "mode": mode, "mtime": mtime, "size": size}


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.deferred = 0
        self.native_only = 0
        self.ids = set()

    def add(self, name, op, args, host=None, deferred=False):
        case = f"{name}-{len(self.cases):04d}"
        assert case not in self.ids
        self.ids.add(case)
        self.cases.append({"unit": UNIT, "case": case,
                           "input": {"op": op, "args": args, "host": host or {}}})
        self.deferred += bool(deferred)
        self.native_only += op == "app_terminate_with_code"

    def refuse(self, name, input_, message):
        case = f"refuse-{name}-{len(self.controls):05d}"
        self.controls.append({"request": {"unit": UNIT, "case": case, "input": input_},
                              "error": {"kind": "invalid_input", "message": message}})


MODES = ("ascii", "latin1", "utf8", "fail")

# Strings whose conversions exercise each converter: ASCII, Latin-1, BMP, astral, a lone
# surrogate, a code above U+10FFFF, a negative unit and embedded NULs.
STRINGS = (
    U(""), U("a"), U("/tmp/a.txt"), U("café"), U("€/中"), U("\U0001f600"),
    [0x61, 0xD800], [0x110000], [-1], U("a\0b"), U("\0"),
)


def trivial(c):
    for function in ("is_operating_system_unsupported", "unregister_application_restart",
                     "supports_shutdown_block_reason",
                     "force_timer_messages_to_be_created_if_necessary", "get_app_user_model_id",
                     "get_command_line_str"):
        c.add("trivial", "trivial", {"function": function})
    for flag in (False, True):
        c.add("trivial", "trivial", {"function": "enable_dark_mode", "flag": flag})
        c.add("trivial", "trivial", {"function": "attach_console", "flag": flag})
    for function in ("register_application_restart", "add_dynamic_library_search_path",
                     "is_network_path", "verify_file_signature", "add_to_recent_docs",
                     "capture_target_attributes", "long_path_adjustment"):
        for text in (U(""), U("/home/u/x.kicad_pro"), [0x110000, 0]):
            c.add("trivial", "trivial", {"function": function, "string": text})
    for window in (False, True):
        c.add("trivial", "trivial", {"function": "remove_shutdown_block_reason", "window": window})
        c.add("trivial", "trivial", {"function": "set_shutdown_block_reason", "window": window,
                                     "string": U("Saving")})
        c.add("trivial", "trivial", {"function": "set_app_details_for_window", "window": window,
                                     "string": U("kicad"), "name": U("KiCad")})
    for value, captured in ((0, False), (7, True), (4294967295, False)):
        c.add("trivial", "trivial", {"function": "apply_target_attributes", "string": U("p"),
                                     "value": value, "captured": captured})


def app(c):
    c.add("build", "build_config", {})
    for value in (None, [], U("1"), U("0"), U("11"), U(" 1"), U("1\0"), U("١"), [0x31, 0x110000]):
        for mode in ("ascii", "fail"):
            env = {"KICAD_SHOW_GTK_MESSAGES": value}
            c.add("app-init", "app_init", {}, {"env": env, "libc": mode})
    c.add("app-init", "app_init", {}, {"env": {"OTHER": U("1")}})
    for raise_ in (False, True):
        for tid, result in ((1234, 0), (0, -1), (-1, 0), (1 << 31, 0), ((1 << 32) + 5, 1),
                            (I64_MAX, 0), (I64_MIN, -1)):
            c.add("priority", "app_set_current_thread_priority", {"raise": raise_},
                  {"replies": {"syscall": [tid], "setpriority": [result]}})
    for code in (0, 1, 42, 255, 256, 300, -1, I32_MIN, I32_MAX):
        c.add("terminate", "app_terminate_with_code", {"code": code})


def environment(c):
    desktops = (None, U("Unity"), U("unity"), U("UNITY"), U("GNOME"), U("Unity:GNOME"),
                U("Unit"), U("Unityx"), U("Un\0ity"), U("Unity\0"), U("Unİty"),
                U("Ünity"), [0x55, 0x110000], U(""))
    for desktop in desktops:
        for wayland in (None, U("wayland-0")):
            c.add("env-init", "env_init", {},
                  {"env": {"XDG_CURRENT_DESKTOP": desktop, "WAYLAND_DISPLAY": wayland}})
    c.add("env-init", "env_init", {}, {"env": {"XDG_CURRENT_DESKTOP": U("Unity")}, "setenv": False})

    for path in (U("/tmp/a.kicad_sch"), U("/tmp/café"), U("/tmp/a\0b"), [0x2f, 0x110000],
                 U("")):
        for mode in ("utf8", "ascii", "fail"):
            c.add("trash", "env_move_to_trash", {"path": path, "error": U("old")},
                  {"filename": mode, "replies": {"g_file_trash": [{"result": True, "error": None}]}})
    for result, error in ((False, None), (False, H("Operation not supported")),
                          (True, H("ignored")), (False, H("café")), (False, H(b"\xff\xfe"))):
        for libc in ("ascii", "latin1", "utf8", "fail"):
            c.add("trash", "env_move_to_trash", {"path": U("/x"), "error": U("old")},
                  {"libc": libc, "replies": {"g_file_trash": [{"result": result, "error": error}]}})

    for resolved in (None, H("/media/usb/a")):
        for mount in (False, True):
            for drive in (False, True):
                for removable in (False, True):
                    if resolved is None and (mount or drive or removable):
                        continue
                    if not mount and (drive or removable):
                        continue
                    if not drive and removable:
                        continue
                    c.add("removable", "env_is_removable_path", {"path": U("/media/usb/a")},
                          {"replies": {"realpath": [resolved],
                                       "g_file_find_enclosing_mount": [mount],
                                       "g_mount_get_drive": [drive],
                                       "g_drive_is_removable": [removable]}})
    c.add("removable", "env_is_removable_path", {"path": [0x110000]},
          {"replies": {"realpath": [None]}})

    for op, fn in (("env_get_user_config_path", "g_get_user_config_dir"),
                   ("env_get_user_data_path", "g_get_user_data_dir"),
                   ("env_get_user_local_data_path", "g_get_user_data_dir"),
                   ("env_get_user_cache_path", "g_get_user_cache_dir")):
        for value in (None, H(""), H("/home/u/.config"), H("/home/u/café"), H(b"/x/\xff")):
            for libc in ("ascii", "latin1", "utf8", "fail"):
                c.add("user-path", op, {}, {"libc": libc, "replies": {fn: [value]}})
    for value in (H("/home/u/.local/share"), H("/data/café")):
        for libc in ("utf8", "latin1"):
            c.add("documents", "env_get_documents_path", {},
                  {"libc": libc, "replies": {"g_get_user_data_dir": [value]}})
    # An empty data directory reaches the wxFileName fallback (MakeAbsolute), deferred.
    for data in (None, H("")):
        c.add("documents", "env_get_documents_path", {},
              {"replies": {"g_get_user_data_dir": [data], "g_get_home_dir": [H("/home/u")]}},
              deferred=True)
    # (A home directory the converter fails on would make the reference's MakeAbsolute read
    # the process working directory: not a deterministic reference reply, so not a case.)

    proxies = (
        "http://proxy:8080", "http://proxy", "https://proxy.example.org:3128/",
        "socks5://127.0.0.1:1080", "http://user:secret@proxy:3128", "http://user@proxy:3128",
        "http://:pw@proxy", "http://user:@proxy", "http://u:p:q@h:1", "http://[::1]:8080",
        "http://[2001:db8::1]:443", "http://[1:2:3:4:5:6:7:8]:1", "http://[1:2:3:4:5:6:1.2.3.4]:9",
        "http://[::ffff:1.2.3.4]:9", "http://[v1.fe80::a]:5", "http://[bad]:5", "http://[::1]",
        "http://1.2.3.4:80", "http://255.255.255.255:1", "http://256.1.1.1:2", "http://1.2.3:4",
        "http://01.2.3.4:5", "http://h:", "http://h:12ab", "http://h:99999999999", "http://%41b:1",
        "http://a b:1", "http://a%zz:1", "http://é.example:1", "HTTP://H:1", "h+t.p-1://h:2",
        "1http://h:2", "proxy:8080", "//proxy:8080", "file://proxy:1", "file:///tmp",
        "http://h/p/../q?x#y", "http:", "http://", "", "http://@h", "http://u@", "http://[]:1",
        "http://[::]:2", "http://[::1:2:3:4:5:6:7]:3", "http://[1::2::3]:4",
        "http://[12345::1]:5", "http://[v.x]:6", "http://[vz.x]:7", "http://[v1.]:8",
        "http://u%40x@h:1", "http://h#frag", "http://h?q:1", "http://h/:1",
    )
    for proxy in proxies:
        c.add("proxy", "env_get_system_proxy_config",
              {"url": U("https://kicad.org/"), "host": U("stale"), "username": U("stale"),
               "password": U("stale")},
              {"replies": {"g_proxy_resolver_get_default": [True],
                           "g_proxy_resolver_lookup": [{"list": [H(proxy)], "error": None}]}})
    lists = ([], ["direct://"], ["direct://", "http://p:1"], ["http://a:1", "http://b:2"],
             ["direct://", "direct://"], ["DIRECT://", "http://p:1"])
    for entries in lists:
        c.add("proxy", "env_get_system_proxy_config",
              {"url": U("http://x/"), "host": U("h"), "username": U("u"), "password": U("p")},
              {"replies": {"g_proxy_resolver_get_default": [True],
                           "g_proxy_resolver_lookup": [{"list": [H(e) for e in entries],
                                                        "error": None}]}})
    for raw in (b"\xff\xfe", b"http://h\xe0\x80\x80:1", b"http://h:1\xe0\x80\x80x",
                b"http://\xf4\x90\x80\x80:1", b"http://\xed\xa0\x80:1"):
        c.add("proxy", "env_get_system_proxy_config",
              {"url": U("http://x/"), "host": U(""), "username": U(""), "password": U("")},
              {"replies": {"g_proxy_resolver_get_default": [True],
                           "g_proxy_resolver_lookup": [{"list": [raw.hex()], "error": None}]}})
    for libc in ("ascii", "fail"):
        for proxy in ("http://[::1]:8080", "file://proxy:1", "http://1.2.3.4:5"):
            c.add("proxy", "env_get_system_proxy_config",
                  {"url": U("http://x/"), "host": U(""), "username": U(""), "password": U("")},
                  {"libc": libc, "replies": {"g_proxy_resolver_get_default": [True],
                                             "g_proxy_resolver_lookup": [{"list": [H(proxy)],
                                                                          "error": None}]}})
    c.add("proxy", "env_get_system_proxy_config",
          {"url": U("http://x/"), "host": U("h"), "username": U("u"), "password": U("p")},
          {"replies": {"g_proxy_resolver_get_default": [False]}})
    for lookup in ({"list": None, "error": None}, {"list": None, "error": H("no resolver")},
                   {"list": [H("http://p:1")], "error": H("both")}):
        c.add("proxy", "env_get_system_proxy_config",
              {"url": U("http://x/"), "host": U("h"), "username": U("u"), "password": U("p")},
              {"replies": {"g_proxy_resolver_get_default": [True],
                           "g_proxy_resolver_lookup": [lookup]}})
    for url in ([0x110000], U("http://café/"), U("a\0b")):
        c.add("proxy", "env_get_system_proxy_config",
              {"url": url, "host": U(""), "username": U(""), "password": U("")},
              {"replies": {"g_proxy_resolver_get_default": [True],
                           "g_proxy_resolver_lookup": [{"list": None, "error": None}]}})


def io(c):
    for fp, fd, advice, closed in ((False, 0, 0, 0), (True, 3, 0, 0), (True, 3, 22, 0),
                                   (True, -1, -1, -1)):
        for mode in ("utf8", "ascii"):
            c.add("seq-f-open", "io_seq_f_open", {"path": U("/tmp/café"), "mode": U("rb")},
                  {"filename": mode, "replies": {"fopen": [fp], "fileno": [fd],
                                                 "posix_fadvise": [advice], "fclose": [closed]}})
    for src_stat, chmod in ((stat(), 0), (stat(mode=S_IFREG | 0o7777), 0), (stat(result=-1), 0),
                            (stat(mode=0o100751), -1), (stat(mode=0xFFFFFFFF), 0)):
        c.add("duplicate", "io_duplicate_permissions", {"src": U("/a"), "dest": U("/b\0c")},
              {"replies": {"stat": [src_stat], "chmod": [chmod]}})
        c.add("writeable", "io_make_writeable", {"path": U("/a")},
              {"replies": {"stat": [src_stat], "chmod": [chmod]}})
    c.add("writeable", "io_make_writeable", {"path": [0x110000]},
          {"filename": "utf8", "replies": {"stat": [stat(result=-1)]}})
    names = ("", ".", "..", ".x", "x", "a.b", ".a.b", "a/.x", "a/x", "/.x", "/", "a/", ".a/",
             "a/.b/c", "a/b.c/.d", "a.b/c", ".a.b/c.d", "./x", "../.y", "dir/.", "\0.x", ".\0x",
             "x/é.y", "/home/u/.config/kicad")
    for name in names:
        c.add("hidden", "io_is_file_hidden", {"path": U(name)})
    c.add("hidden", "io_is_file_hidden", {"path": [0x2E, 0x110000]})

    def timestamp(name, entries, dir_ok=True, spec=U("*.kicad_sch"), dir_=U("/proj"), host=None):
        replies = {"opendir": [dir_ok], "readdir": [], "fnmatch": [], "lstat": [],
                   "readlink": [], "closedir": [0]}
        for entry in entries:
            replies["readdir"].append(H(entry["name"]))
            replies["fnmatch"].append(entry.get("match", 0))
            if entry.get("match", 0) != 0:
                continue
            replies["lstat"].append(entry.get("stat", stat()))
            if "link" in entry:
                replies["readlink"].append(entry["link"])
                if entry["link"]["result"] > 0:
                    replies["lstat"].append(entry.get("linked", stat()))
        replies["readdir"].append(None)
        script = {"replies": replies}
        script.update(host or {})
        c.add(name, "io_timestamp_dir", {"dir": dir_, "spec": spec}, script)

    timestamp("timestamp", [], dir_ok=False)
    timestamp("timestamp", [])
    timestamp("timestamp", [{"name": "a.kicad_sch"}, {"name": "b.kicad_sch", "stat": stat(mtime=5, size=7)}])
    timestamp("timestamp", [{"name": "x.txt", "match": 1}, {"name": "y", "match": -1},
                            {"name": "z.kicad_sch", "stat": stat(mtime=-3, size=-4)}])
    timestamp("timestamp", [{"name": "d", "stat": stat(mode=S_IFDIR | 0o755)}])
    timestamp("timestamp", [{"name": name, "stat": stat(result=-1)} for name in
                            ("gone", "", "a" * 8, "a" * 9, "café", "x" * 255)])
    timestamp("timestamp", [{"name": "l", "stat": stat(mode=S_IFLNK | 0o777),
                             "link": {"result": 6, "bytes": H("target")},
                             "linked": stat(mtime=11, size=12)}])
    timestamp("timestamp", [{"name": "l", "stat": stat(mode=S_IFLNK | 0o777),
                             "link": {"result": 6, "bytes": H("target")},
                             "linked": stat(result=-1)}])
    timestamp("timestamp", [{"name": "l", "stat": stat(mode=S_IFLNK | 0o777),
                             "link": {"result": 0, "bytes": ""}}])
    timestamp("timestamp", [{"name": "l", "stat": stat(mode=S_IFLNK | 0o777),
                             "link": {"result": -1, "bytes": ""}}])
    timestamp("timestamp", [{"name": "l", "stat": stat(mode=S_IFLNK | 0o777),
                             "link": {"result": 5, "bytes": H(b"ab\0cd")},
                             "linked": stat(mode=S_IFDIR | 0o755)}])
    timestamp("timestamp", [{"name": "l", "stat": stat(mode=S_IFLNK | 0o777),
                             "link": {"result": 4096, "bytes": H("p" * 4096)},
                             "linked": stat(mtime=1, size=1)}])
    timestamp("timestamp", [{"name": "big", "stat": stat(mtime=9223372036854775, size=807)}])
    timestamp("timestamp", [{"name": "neg", "stat": stat(mtime=-9223372036854775, size=-808)}])
    timestamp("timestamp", [{"name": "a"}], spec=U("*.k\0x"), dir_=U("/p\0q"))
    timestamp("timestamp", [{"name": "a"}], spec=U("*"), dir_=[0x2f, 0x110000])
    timestamp("timestamp", [{"name": "a"}], host={"filename": "fail"})

    for null, flush, fd, sync in ((True, 0, 0, 0), (False, -1, 0, 0), (False, 0, -1, 0),
                                  (False, 0, 3, 0), (False, 0, 3, -1), (False, 0, 0, 0)):
        c.add("flush", "io_flush_to_disk", {"null": null},
              {"replies": {"fflush": [flush], "fileno": [fd], "fsync": [sync]}})

    def mapped(name, replies, file_name=U("/tmp/a.pcb"), host=None, deferred=False):
        script = {"replies": replies}
        script.update(host or {})
        c.add(name, "io_mapped_file", {"name": file_name}, script, deferred=deferred)

    for libc in ("ascii", "utf8", "fail"):
        mapped("mapped", {"open": [-1]}, U("/tmp/café"), {"libc": libc})
        mapped("mapped", {"open": [3], "fstat": [stat(result=-1)], "close": [0]},
               U("/tmp/a\0b"), {"libc": libc})
    mapped("mapped", {"open": [3], "fstat": [stat(size=0)], "close": [0]})
    mapped("mapped", {"open": [0], "fstat": [stat(size=4)], "close": [0, 0],
                      "mmap": [H("abcd")], "madvise": [0], "munmap": [0]})
    mapped("mapped", {"open": [7], "fstat": [stat(size=1)], "close": [-1],
                      "mmap": ["00"], "madvise": [-1], "munmap": [-1]})
    mapped("mapped", {"open": [7], "fstat": [stat(size=3)], "close": [0],
                      "mmap": [H(b"\x00\xff\x00")], "madvise": [0], "munmap": [0]},
           U("/x"), {"filename": "fail"})
    # mmap fails: readIntoBuffer (os/common/io.cpp), deferred to its owner.
    for ftell, data in ((2, H("hi")), (-1, ""), (3, H("ab"))):
        mapped("mapped-fallback", {"open": [3], "fstat": [stat(size=2)], "close": [0],
                                   "mmap": [None], "fopen": [True], "fseek": [0, 0],
                                   "ftell": [ftell], "fread": [data], "fclose": [0]},
               deferred=True)
    mapped("mapped-fallback", {"open": [3], "fstat": [stat(size=2)], "close": [0],
                               "mmap": [None], "fopen": [False]}, deferred=True)


def secrets(c):
    names = ((U("kicad"), U("token"), U("s3crét")), (U(""), U(""), U("")),
             (U("svc:x"), U("k\0y"), U("\U0001f600")), ([0x110000], U("k"), [0xD800]))
    for service, key, secret in names:
        for libc in ("ascii", "fail"):
            for error in (None, H("locked")):
                c.add("store", "secrets_store", {"service": service, "key": key, "secret": secret},
                      {"libc": libc, "replies": {"secret_password_store_sync": [
                          {"result": error is None, "error": error}]}})
    c.add("store", "secrets_store", {"service": U("a"), "key": U("b"), "secret": U("c")},
          {"replies": {"secret_password_store_sync": [{"result": False, "error": None}]}})
    for done in (False, True):
        for complete in (False, True):
            for stored in (False, True):
                if not complete and stored:
                    continue
                c.add("store-async", "secrets_store_async",
                      {"service": U("kicad"), "key": U("token"), "secret": U("x"), "done": done,
                       "complete": complete},
                      {"replies": {"secret_password_store_finish": [stored]}})
    c.add("store-async", "secrets_store_async",
          {"service": [0x110000], "key": U("k"), "secret": U("s"), "done": True, "complete": True},
          {"replies": {"secret_password_store_finish": [True]}})
    for value, error in ((None, None), (None, H("dbus")), (H("v"), None), (H("café"), None),
                         (H(b"\xff"), None), (H(b"a\xe0\x80\x80b"), None), (H("v"), H("both"))):
        c.add("get", "secrets_get", {"service": U("s"), "key": U("k"), "secret": U("old")},
              {"replies": {"secret_password_lookup_sync": [{"secret": value, "error": error}]}})
    c.add("get", "secrets_get", {"service": [0x110000], "key": [0x110001], "secret": U("old")},
          {"replies": {"secret_password_lookup_sync": [{"secret": None, "error": None}]}})
    for done in (False, True):
        for complete in (False, True):
            for value in (None, H("found")):
                if not complete and value is not None:
                    continue
                c.add("get-async", "secrets_get_async",
                      {"service": U("s"), "key": U("k"), "done": done, "complete": complete},
                      {"replies": {"secret_password_lookup_finish": [value]}})
    item = lambda locked, text: {"locked": locked, "secret": None if text is False else {"text": text}}
    searches = (
        {"items": [], "error": None}, {"items": [], "error": H("denied")},
        {"items": [item(False, H("one"))], "error": None},
        {"items": [item(True, H("locked")), item(False, H("two"))], "error": None},
        {"items": [item(False, False), item(False, None), item(False, H("three")),
                   item(False, H("never"))], "error": None},
        {"items": [item(True, H("a")), item(True, H("b"))], "error": None},
        {"items": [item(False, H(b"\xff"))], "error": None},
        {"items": [item(False, H("x"))], "error": H("both")},
    )
    for search in searches:
        c.add("no-prompt", "secrets_get_no_prompt",
              {"service": U("s"), "key": U("k"), "secret": U("old")},
              {"replies": {"secret_service_search_sync": [search]}})
    for result, error in ((True, None), (False, None), (False, H("gone")), (True, H("both"))):
        c.add("delete", "secrets_delete", {"service": U("s"), "key": U("ké")},
              {"replies": {"secret_password_clear_sync": [{"result": result, "error": error}]}})


def run_reply(emit=(), result=1, settings=None, error=None, cairo=True, width="595.0",
              height="842.0"):
    return {"context": {"cairo": cairo, "width": B(width), "height": B(height)},
            "emit": list(emit), "settings": settings, "result": result, "error": error}


def page(width, height):
    return {"width": B(width), "height": B(height)}


def printing(c):
    def print_case(name, calls, replies, host=None):
        script = {"replies": replies}
        script.update(host or {})
        c.add(name, "printing_print_pdf", {"calls": calls}, script)

    def call(file="/tmp/board.pdf", page=0, fit=None):
        return {"file": H(file), "current_page": page, "fit": fit}

    def full(pages=1, run=None, extra=None, active=(False, False)):
        replies = {"access": [0], "g_filename_to_uri": [H("file:///tmp/board.pdf")],
                   "poppler_document_new_from_file": [{"doc": True, "error": None}],
                   "poppler_document_get_n_pages": [pages, pages], "gtk_print_operation_new": [True],
                   "wxGetActiveWindow": [active[0]], "wxGetTopLevelParent": [active[1]],
                   "gtk_print_operation_run": [run or run_reply()]}
        replies.update(extra or {})
        return replies

    print_case("print", [call()], {"access": [-1]})
    print_case("print", [call()], {"access": [0], "g_filename_to_uri": [None]})
    for trace in (False, True):
        for libc in ("ascii", "utf8"):
            print_case("print", [call("/tmp/café.pdf")],
                       {"access": [0], "g_filename_to_uri": [H("file:///x")],
                        "poppler_document_new_from_file": [{"doc": False, "error": H("PDF érror")}]},
                       {"trace": trace, "libc": libc})
    print_case("print", [call()], {"access": [0], "g_filename_to_uri": [H("u")],
                                   "poppler_document_new_from_file": [{"doc": True, "error": H("both")}]})
    print_case("print", [call()], {"access": [0], "g_filename_to_uri": [H("u")],
                                   "poppler_document_new_from_file": [{"doc": False, "error": None}]})
    for pages in (0, -1):
        print_case("print", [call()], {"access": [0], "g_filename_to_uri": [H("u")],
                                       "poppler_document_new_from_file": [{"doc": True, "error": None}],
                                       "poppler_document_get_n_pages": [pages]})
    print_case("print", [call()], {"access": [0], "g_filename_to_uri": [H("u")],
                                   "poppler_document_new_from_file": [{"doc": True, "error": None}],
                                   "poppler_document_get_n_pages": [1],
                                   "gtk_print_operation_new": [False]})
    for result in (0, 1, 2, 3, -7):
        print_case("print-result", [call()], full(run=run_reply(result=result)))
    for trace in (False, True):
        print_case("print-error", [call()], full(run=run_reply(result=1, error=H("no printer"))),
                   {"trace": trace})
    for current in (0, 1, 2, 3, -1, I32_MAX, I32_MIN, None):
        print_case("print-page", [call(page=current)], full(pages=2))
    for fit in (True, False):
        print_case("print-fit", [call(page=1, fit=fit)], full())
    for file in ("/tmp/board.pdf", "board", "/a.b/c.d.pdf", "/x/.hidden", "", "/x/",
                 "café.pdf"):
        print_case("print-name", [call(file)], full())
    for raw in (b"/x/\xff.pdf", b"/x/a\0b.pdf", b"/x/\xe0\x80\x80.pdf", b"/x/\xf4\x90\x80\x80.pdf"):
        print_case("print-name", [{"file": raw.hex(), "current_page": 0, "fit": None}], full())
    for active in ((False, True), (True, False), (True, True)):
        for handle, toplevel, is_toplevel in ((False, False, False), (True, True, True),
                                              (True, True, False), (True, False, False)):
            extra = {"GetHandle": [handle, handle], "gtk_widget_get_toplevel": [toplevel],
                     "gtk_widget_is_toplevel": [is_toplevel]}
            print_case("print-parent", [call()], full(active=active, extra=extra))
    # Signals the dialog emits while printing.
    emissions = (
        ([{"signal": "begin-print"}], {"poppler_document_get_n_pages": [2, 2]}),
        ([{"signal": "begin-print"}], {"poppler_document_get_n_pages": [2, 0]}),
        ([{"signal": "begin-print"}], {"poppler_document_get_n_pages": [2, -3]}),
        ([{"signal": "request-page-setup", "page": 0}, {"signal": "request-page-setup", "page": 1},
          {"signal": "request-page-setup", "page": 2}],
         {"poppler_document_get_page": [page("842", "595"), page("595", "842"), None]}),
        ([{"signal": "request-page-setup", "page": 0}],
         {"poppler_document_get_page": [page("100", "100")]}),
        ([{"signal": "request-page-setup", "page": 0}],
         {"poppler_document_get_page": [page("nan", "1")]}),
        ([{"signal": "request-page-setup", "page": 0}],
         {"poppler_document_get_page": [page("0", "0")]}),
        ([{"signal": "draw-page", "page": 0}], {"poppler_document_get_page": [None]}),
        ([{"signal": "draw-page", "page": 0}], {"poppler_document_get_page": [page("595", "842")]}),
        ([{"signal": "draw-page", "page": 1}], {"poppler_document_get_page": [page("842", "595")]}),
    )
    for emit, extra in emissions:
        print_case("print-signal", [call()], full(pages=2, run=run_reply(emit=emit), extra=extra))
    contexts = (("595.0", "842.0"), ("612", "792"), ("1e-300", "1e300"), ("inf", "1"),
                ("nan", "5"), ("-0", "3"), ("tiny", "max"), ("0.1", "0.3"))
    pages_ = (("595.0", "842.0"), ("842", "595"), ("1", "3"), ("3", "1"), ("-2", "4"),
              ("inf", "inf"), ("nan", "1"), ("tiny", "1"), ("max", "max"), ("1e308", "1e-308"))
    for index, (cw, ch) in enumerate(contexts):
        for pw, ph in pages_[index % 3::3]:
            for fit in (True, False):
                emit = [{"signal": "draw-page", "page": 0}]
                print_case("print-draw", [call(page=0, fit=fit)],
                           full(run=run_reply(emit=emit, width=cw, height=ch),
                                extra={"poppler_document_get_page": [page(pw, ph)]}))
    print_case("print-draw", [call(page=0, fit=False)],
               full(run=run_reply(emit=[{"signal": "draw-page", "page": 0}], width="10", height="10"),
                    extra={"poppler_document_get_page": [page("0", "5")]}))
    print_case("print-draw", [call()],
               full(run=run_reply(emit=[{"signal": "draw-page", "page": 0}], cairo=False),
                    extra={"poppler_document_get_page": [page("1", "1")]}))
    # Accepted dialogs: the settings kept for the next print.
    uris = (
        None, H("file:///home/u/out/board.pdf"), H("file:///home/u/out/gtkprintAB12CD"),
        H("file:///tmp/gtkprint_ZZ9999.pdf"), H("file:///"), H("file:///home/u/gtkprintab12cd"),
    )
    for uri in uris:
        for parent, basename, path in ((True, H("board.pdf"), H("/home/u/out")),
                                       (False, H("board.pdf"), None),
                                       (True, None, H("/home/u/out")),
                                       (True, H("gtkprintAB12CD"), H("/tmp")),
                                       (True, H("gtkprint_AB12CD.pdf"), H("/tmp")),
                                       (True, H("gtkprintAB12CD\n"), H("/tmp")),
                                       (True, H(b"\xff"), H("/tmp")),
                                       (True, H(b"gtkprint\xed\xa0\x80"), H("/tmp"))):
            settings = [] if uri is None else [[H("output-uri"), uri]]
            settings += [[H("page-ranges"), H("1-2")], [H("print-pages"), H("ranges")]]
            extra = {"g_file_get_parent": [parent], "g_file_get_basename": [basename],
                     "g_file_get_path": [path]}
            if uri is None and (not parent or basename is None):
                continue
            print_case("print-remember", [call()], full(run=run_reply(settings=settings), extra=extra))
    # s_lastSettings carries from one print to the next.
    second = full(run=run_reply(settings=[[H("printer"), H("Office")],
                                          [H("output-uri"), H("file:///home/u/a.pdf")]]),
                  extra={"g_file_get_parent": [True], "g_file_get_basename": [H("a.pdf")],
                         "g_file_get_path": [H("/home/u")]})
    for key in ("access", "g_filename_to_uri", "poppler_document_new_from_file",
                "poppler_document_get_n_pages", "gtk_print_operation_new", "wxGetActiveWindow",
                "wxGetTopLevelParent", "gtk_print_operation_run"):
        second[key] = second[key] * 3
    second["gtk_print_operation_run"] = [
        run_reply(settings=[[H("printer"), H("Office")],
                            [H("output-uri"), H("file:///home/u/a.pdf")]]),
        run_reply(result=2),
        run_reply(settings=[[H("printer"), H("Lab")]]),
    ]
    print_case("print-sequence", [call("/tmp/a.pdf"), call("/tmp/b.pdf", None),
                                  call("/tmp/c.pdf", 1, False)], second)


def reset(c):
    names = ("", "board.pdf", "gtkprintAB12CD", "/tmp/gtkprint_AB12CD", "/tmp/gtkprintAB12C",
             "/tmp/gtkprintAB12CDE", "/tmp/gtkprintab12cd", "/tmp/gtkprint-AB12CD",
             "/tmp/gtkprintAB12CD\n", "/tmp/gtkprintAB12CD\r\n", "/tmp/gtkprintAB12CD.pdf",
             "gtkprint__AB12CD", "xgtkprintAB12CD")
    for name in names:
        c.add("reset", "printing_reset", {"filename": U(name), "native": "other", "settings": []})
    c.add("reset", "printing_reset",
          {"filename": [0x67, 0xD800], "native": "other", "settings": []})
    c.add("reset", "printing_reset",
          {"filename": U("gtkprint") + [0x110000], "native": "other", "settings": []})
    uris = ("file:///tmp/gtkprintAB12CD", "file:/tmp/gtkprint_AB12CD", "/tmp/gtkprintAB12CD",
            "file:///tmp/board.pdf", "file:///tmp/gtkprint%41B12CD", "file:///tmp/%67tkprintAB12CD",
            "file:///tmp/gtkprintAB12CD%0A", "file:///tmp/gtkprintAB12CD%00x",
            "file:///tmp/gtkprint%ED%A0%80AB12C", "file:///tmp/a%zz", "file:///tmp/%4",
            "file:%41", "", "file://", "file:///tmp/gtkprintAB12CD.pdf",
            "file:///tmp/caf%C3%A9/gtkprintAB12CD", "file:///tmp/é/gtkprintAB12CD")
    for uri in uris:
        for filename in (U("board.pdf"), U("gtkprintAB12CD")):
            c.add("reset-gtk", "printing_reset",
                  {"filename": filename, "native": "gtk",
                   "settings": [[H("output-uri"), H(uri)], [H("printer"), H("P")]]})
    c.add("reset-gtk", "printing_reset",
          {"filename": U("x"), "native": "gtk", "settings": [[H("printer"), H("P")]]})
    c.add("reset-gtk", "printing_reset",
          {"filename": U("x"), "native": "gtk", "settings": [[H("output-uri"), "ff"]]})


def controls(c):
    good = {"op": "app_init", "args": {}, "host": {}}
    c.refuse("input", {}, "unexpected or missing field: input")
    c.refuse("input", {"op": "app_init", "args": {}}, "unexpected or missing field: input")
    c.refuse("input", {**good, "extra": 1}, "unexpected or missing field: input")
    c.refuse("op", {**good, "op": 1}, "expected string: op")
    c.refuse("op", {**good, "op": "nope"}, "unknown operation")
    c.refuse("host", {**good, "host": []}, "expected host object")
    c.refuse("host", {**good, "host": {"bogus": 1}}, "unknown host key: bogus")
    c.refuse("host", {**good, "host": {"zz": 1, "libc": "nope"}}, "unknown converter mode: libc")
    c.refuse("host", {**good, "host": {"filename": 3}}, "expected string: filename")
    c.refuse("host", {**good, "host": {"env": []}}, "expected env object")
    c.refuse("host", {**good, "host": {"env": {"A": "x"}}}, "expected unit array: env")
    c.refuse("host", {**good, "host": {"env": {"A": [1.5]}}}, "expected integer: env")
    c.refuse("host", {**good, "host": {"env": {"A": [1 << 40]}}}, "integer out of range: env")
    c.refuse("host", {**good, "host": {"setenv": 1}}, "expected boolean: setenv")
    c.refuse("host", {**good, "host": {"trace": None}}, "expected boolean: trace")
    c.refuse("host", {**good, "host": {"replies": []}}, "expected replies object")
    c.refuse("host", {**good, "host": {"replies": {"nope": []}}}, "unknown reply function: nope")
    c.refuse("host", {**good, "host": {"replies": {"open": 1}}}, "expected reply array: open")
    c.refuse("host", {**good, "host": {"replies": {"open": [0] * 257}}}, "expected reply array: open")
    c.refuse("args", {**good, "args": {"x": 1}}, "unexpected or missing field: args")
    c.refuse("trivial", {"op": "trivial", "args": {}, "host": {}}, "missing field: function")
    c.refuse("trivial", {"op": "trivial", "args": {"function": "nope"}, "host": {}},
             "unknown trivial function")
    c.refuse("trivial", {"op": "trivial", "args": {"function": "attach_console"}, "host": {}},
             "unexpected or missing field: args")
    c.refuse("trivial", {"op": "trivial", "args": {"function": "attach_console", "flag": 1},
                         "host": {}}, "expected boolean: flag")
    c.refuse("trivial", {"op": "trivial", "args": {"function": "is_network_path", "string": "x"},
                         "host": {}}, "expected unit array: string")
    c.refuse("trivial", {"op": "trivial", "args": {"function": "is_network_path",
                                                   "string": [0] * 1025}, "host": {}},
             "expected unit array: string")
    c.refuse("trivial", {"op": "trivial", "args": {"function": "apply_target_attributes",
                                                   "string": [], "value": -1, "captured": False},
                         "host": {}}, "integer out of range: value")
    c.refuse("priority", {"op": "app_set_current_thread_priority", "args": {"raise": True},
                          "host": {}}, "missing reply: syscall")
    c.refuse("priority", {"op": "app_set_current_thread_priority", "args": {"raise": True},
                          "host": {"replies": {"syscall": ["1"]}}}, "expected integer: reply syscall")
    c.refuse("priority", {"op": "app_set_current_thread_priority", "args": {"raise": True},
                          "host": {"replies": {"syscall": [1], "setpriority": [1 << 40]}}},
             "integer out of range: reply setpriority")
    c.refuse("terminate", {"op": "app_terminate_with_code", "args": {"code": 1 << 40}, "host": {}},
             "integer out of range: code")
    c.refuse("trash", {"op": "env_move_to_trash", "args": {"path": [], "error": []},
                       "host": {"replies": {"g_file_trash": [{"result": True}]}}},
             "unexpected or missing field: reply g_file_trash")
    c.refuse("trash", {"op": "env_move_to_trash", "args": {"path": [], "error": []},
                       "host": {"replies": {"g_file_trash": [{"result": True, "error": "00"}]}}},
             "NUL in C string: reply g_file_trash")
    c.refuse("trash", {"op": "env_move_to_trash", "args": {"path": [], "error": []},
                       "host": {"replies": {"g_file_trash": [{"result": True, "error": "0g"}]}}},
             "bad hex: reply g_file_trash")
    c.refuse("user-path", {"op": "env_get_user_cache_path", "args": {},
                           "host": {"replies": {"g_get_user_cache_dir": [1]}}},
             "expected string: reply g_get_user_cache_dir")
    c.refuse("proxy", {"op": "env_get_system_proxy_config",
                       "args": {"url": [], "host": [], "username": [], "password": []},
                       "host": {"replies": {"g_proxy_resolver_get_default": [True],
                                            "g_proxy_resolver_lookup": [{"list": {}, "error": None}]}}},
             "bad reply g_proxy_resolver_lookup")
    c.refuse("stat", {"op": "io_make_writeable", "args": {"path": []},
                      "host": {"replies": {"stat": [{"result": 0, "mode": -1, "mtime": 0, "size": 0}]}}},
             "integer out of range: reply stat")
    c.refuse("readlink", {"op": "io_timestamp_dir", "args": {"dir": [], "spec": []},
                          "host": {"replies": {"opendir": [True], "readdir": [H("l")], "fnmatch": [0],
                                               "lstat": [stat(mode=S_IFLNK)],
                                               "readlink": [{"result": 3, "bytes": H("ab")}]}}},
             "bad reply readlink")
    c.refuse("readdir", {"op": "io_timestamp_dir", "args": {"dir": [], "spec": []},
                         "host": {"replies": {"opendir": [True], "readdir": [H("x" * 256)]}}},
             "bad reply readdir")
    # Signed overflow in TimestampDir is undefined in the reference.
    overflow = (
        [stat(mtime=9223372036854776, size=0)],
        [stat(mtime=9223372036854775, size=808)],
        [stat(mtime=-9223372036854776, size=-1)],
        [stat(mtime=9223372036854775, size=0), stat(mtime=1, size=0)],
        [stat(size=I64_MAX)],
    )
    for stats in overflow:
        replies = {"opendir": [True], "readdir": [H(f"f{i}") for i in range(len(stats))] + [None],
                   "fnmatch": [0] * len(stats), "lstat": stats, "readlink": [], "closedir": [0]}
        c.refuse("overflow", {"op": "io_timestamp_dir", "args": {"dir": U("/d"), "spec": U("*")},
                              "host": {"replies": replies}},
                 "undefined: signed overflow in TimestampDir")
    c.refuse("mmap", {"op": "io_mapped_file", "args": {"name": U("/x")},
                      "host": {"replies": {"open": [3], "fstat": [stat(size=2)], "close": [0],
                                           "mmap": [H("abc")]}}}, "bad reply mmap")
    c.refuse("secrets", {"op": "secrets_get_no_prompt",
                         "args": {"service": [], "key": [], "secret": []},
                         "host": {"replies": {"secret_service_search_sync": [
                             {"items": [{"locked": False}], "error": None}]}}},
             "unexpected or missing field: reply secret_service_search_sync item")
    c.refuse("print", {"op": "printing_print_pdf", "args": {"calls": []}, "host": {}},
             "expected 1 to 8 print calls")
    c.refuse("print", {"op": "printing_print_pdf",
                       "args": {"calls": [{"file": "", "current_page": None, "fit": True}]},
                       "host": {}}, "the fit overload has no default page")
    c.refuse("print", {"op": "printing_print_pdf",
                       "args": {"calls": [{"file": "", "current_page": 0}]}, "host": {}},
             "unexpected or missing field: print call")
    # Fitting a zero-sized page divides by zero, undefined in the reference.
    base = {"access": [0], "g_filename_to_uri": [H("u")],
            "poppler_document_new_from_file": [{"doc": True, "error": None}],
            "poppler_document_get_n_pages": [1], "gtk_print_operation_new": [True],
            "wxGetActiveWindow": [False], "wxGetTopLevelParent": [False]}
    for pw, ph in (("0", "1"), ("1", "0"), ("-0", "5"), ("0", "0")):
        replies = dict(base)
        replies["gtk_print_operation_run"] = [run_reply(emit=[{"signal": "draw-page", "page": 0}])]
        replies["poppler_document_get_page"] = [page(pw, ph)]
        c.refuse("divide", {"op": "printing_print_pdf",
                            "args": {"calls": [{"file": H("/a.pdf"), "current_page": 0, "fit": None}]},
                            "host": {"replies": replies}},
                 "undefined: floating division by zero in draw_page")
    for uri in ("file://%", "file:%", "%"):
        c.refuse("unescape", {"op": "printing_reset",
                              "args": {"filename": [], "native": "gtk",
                                       "settings": [[H("output-uri"), H(uri)]]}, "host": {}},
                 "undefined: wxURI::Unescape forms end - 2 before the buffer")
    c.refuse("reset", {"op": "printing_reset",
                       "args": {"filename": [], "native": "mac", "settings": []}, "host": {}},
             "unknown native data")
    c.refuse("reset", {"op": "printing_reset",
                       "args": {"filename": [], "native": "gtk", "settings": [["00"]]}, "host": {}},
             "bad settings: settings")


def build():
    c = Corpus()
    trivial(c)
    app(c)
    environment(c)
    io(c)
    secrets(c)
    printing(c)
    reset(c)
    controls(c)
    return c


def lines(records):
    return "".join(json.dumps(r, separators=(",", ":"), ensure_ascii=False) + "\n" for r in records)


def outputs():
    corpus = build()
    expected = {"cases": len(corpus.cases), "compared": len(corpus.cases) - corpus.deferred,
                "deferred": corpus.deferred, "native_only": corpus.native_only,
                "controls": len(corpus.controls)}
    return {CASES: lines(corpus.cases), CONTROLS: lines(corpus.controls),
            EXPECTED: json.dumps(expected, indent=2) + "\n"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale = []
    for path, content in outputs().items():
        if args.check:
            if not path.exists() or path.read_bytes() != content.encode("utf-8"):
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content.encode("utf-8"))
    if stale:
        raise SystemExit("stale generated files: " + ", ".join(stale))


if __name__ == "__main__":
    main()
