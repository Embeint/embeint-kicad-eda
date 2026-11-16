# SPDX-License-Identifier: GPL-3.0-or-later
"""A minimal MCP stdio client for `solidcraft-cli mcp`, standing in for the agent in tests.

An agent normally holds both MCP servers and carries the plan between them; this lets a test do
the same.  Set SOLIDCRAFT_CLI to the solidcraft-cli executable (a Windows .exe works from WSL).
"""

import base64
import json
import os
import subprocess


class SolidCraftMcp:
    def __init__(self, allow_paths=(), executable=None):
        executable = executable or os.environ.get("SOLIDCRAFT_CLI")
        if not executable:
            raise RuntimeError("Set SOLIDCRAFT_CLI to the solidcraft-cli executable")
        command = [executable, "mcp", "--headless"]
        for path in allow_paths:
            command += ["--allow-path", path]
        self.proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, text=True, encoding="utf-8")
        self.next_id = 0
        self._request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                     "clientInfo": {"name": "kicad-live-mcp", "version": "1"}})
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _send(self, message):
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def _request(self, method, params):
        self.next_id += 1
        self._send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError("SolidCraft exited: " + self.proc.stderr.read()[-2000:])
            reply = json.loads(line)
            if reply.get("id") == self.next_id:
                if "error" in reply:
                    raise RuntimeError(f"{method}: {reply['error']}")
                return reply["result"]

    def call(self, tool, arguments):
        """Run a tool; returns (parsed JSON text or raw text, list of image bytes)."""
        result = self._request("tools/call", {"name": tool, "arguments": arguments})
        texts = [c["text"] for c in result.get("content", []) if c.get("type") == "text"]
        images = [base64.b64decode(c["data"]) for c in result.get("content", []) if c.get("type") == "image"]
        text = "\n".join(texts)
        if result.get("isError"):
            raise RuntimeError(f"SolidCraft {tool} failed: {text[:1500]}")
        try:
            return json.loads(text), images
        except ValueError:
            return text, images

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=30)
        except (OSError, subprocess.TimeoutExpired):
            self.proc.kill()
