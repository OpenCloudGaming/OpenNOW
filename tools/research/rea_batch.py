#!/usr/bin/env python3
"""Replay read-only REA MCP queries against an exact, caller-supplied binary."""

import argparse
import hashlib
import json
import os
import pathlib
import selectors
import subprocess
import time


READ_ONLY = {
    "binary_overview", "search_procedures", "search_strings", "list_procedures",
    "list_strings", "procedure_pseudo_code", "analyze_function", "batch_decompile",
    "xrefs", "procedure_references", "address_name", "address_to_file_offset",
    "get_evidence_bundle", "procedure_callers", "read_bytes",
}


class Connection:
    def __init__(self, command, timeout):
        self.process = subprocess.Popen([command, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.timeout = timeout
        self.buffer = b""
        self.identifier = 0

    def send(self, message):
        self.process.stdin.write((json.dumps(message) + "\n").encode())
        self.process.stdin.flush()

    def request(self, method, params):
        self.identifier += 1
        identifier = self.identifier
        self.send(dict(jsonrpc="2.0", id=identifier, method=method, params=params))
        deadline = time.monotonic() + self.timeout
        while True:
            if b"\n" in self.buffer:
                line, self.buffer = self.buffer.split(b"\n", 1)
                message = json.loads(line)
                if message.get("id") != identifier:
                    continue
                if "error" in message:
                    raise RuntimeError(json.dumps(message["error"]))
                return message["result"]
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not self.selector.select(remaining):
                raise TimeoutError(f"REA MCP request timed out: {method}")
            chunk = os.read(self.process.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError("REA MCP closed before replying")
            self.buffer += chunk
            if len(self.buffer) > 128 * 1024 * 1024:
                raise RuntimeError("REA MCP response exceeds 128 MiB")

    def close(self):
        self.selector.close()
        self.process.stdin.close()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()


def run(args):
    target = args.target.resolve()
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    if digest != args.sha256.lower():
        raise ValueError("Target SHA-256 does not match the required artifact")
    queries = json.loads(args.plan.read_text())
    if not isinstance(queries, list) or not queries:
        raise ValueError("Plan must be a nonempty query list")
    for query in queries:
        if query["name"] not in READ_ONLY or not isinstance(query.get("arguments", {}), dict):
            raise ValueError("Plan contains an unsupported or non-read-only query")
    args.output.mkdir(parents=True, exist_ok=False)
    connection = Connection(args.rea_command, args.timeout)
    try:
        initialized = connection.request("initialize", dict(protocolVersion="2025-06-18", capabilities={},
                                                           clientInfo=dict(name="opennow-gfn-research", version="1")))
        (args.output / "initialize.json").write_text(json.dumps(initialized, indent=2) + "\n")
        connection.send(dict(jsonrpc="2.0", method="notifications/initialized"))
        opening = connection.request("tools/call", dict(name="open_binary", arguments=dict(path=str(target), provider_id="ghidra")))
        (args.output / "open.json").write_text(json.dumps(opening, indent=2) + "\n")
        if opening.get("isError"):
            raise RuntimeError("REA failed to open the target; see open.json")
        for index, query in enumerate(queries):
            result = connection.request("tools/call", dict(name=query["name"], arguments=query.get("arguments", {})))
            destination = args.output / f"{index:03d}-{query['name']}.json"
            destination.write_text(json.dumps(result, indent=2) + "\n")
            print(destination)
            if result.get("isError"):
                raise RuntimeError(f"REA query failed; inspect {destination}")
        connection.request("tools/call", dict(name="close_binary", arguments={}))
    finally:
        connection.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, type=pathlib.Path)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--plan", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--rea-command", default="rea")
    parser.add_argument("--timeout", type=float, default=1500)
    run(parser.parse_args())
