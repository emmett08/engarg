#!/usr/bin/env python3
"""Small synthetic MCP protocol fixture, not a production MCP server.

The EAL client uses the official MCP SDK. This fixture implements only the
initialisation, tools/list and tools/call messages exercised by the example.
"""
import datetime
import json
import sys


def reply(message, result):
    if "id" in message:
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}), flush=True)


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        reply(message, {"protocolVersion": message["params"]["protocolVersion"],
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "synthetic-snapshot", "version": "1"}})
    elif method == "tools/list":
        reply(message, {"tools": [{"name": "snapshot", "description": "Return a named finite synthetic snapshot.",
                                   "inputSchema": {"type": "object", "properties": {"scenario": {"type": "string"}},
                                                   "required": ["scenario"], "additionalProperties": False}}]})
    elif method == "tools/call":
        if message["params"]["name"] != "snapshot":
            reply(message, {"content": [{"type": "text", "text": "Unknown operation"}], "isError": True})
            continue
        scenario = message["params"]["arguments"]["scenario"]
        if scenario == "error":
            reply(message, {"content": [{"type": "text", "text": "Synthetic source unavailable"}], "isError": True})
            continue
        items = ["Alpha", "Beta", "Gamma"]
        if scenario == "duplicate":
            items = ["Alpha", "Beta", "Alpha"]
        elif scenario == "case-sensitive":
            items = ["Alpha", "alpha", "Beta"]
        observed_at = "2000-01-01T00:00:00Z" if scenario == "stale" else datetime.datetime.now(datetime.timezone.utc).isoformat()
        value = {"snapshot": "fixture-v1", "items": items, "complete": scenario != "incomplete",
                 "manifest_count": 4 if scenario == "manifest-mismatch" else 3}
        reply(message, {"content": [], "structuredContent": {"value": value, "observed_at": observed_at}, "isError": False})
    elif method == "ping":
        reply(message, {})
