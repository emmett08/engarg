#!/usr/bin/env python3
"""Exact finite list audit. Equality is case-sensitive Unicode string equality."""
import json
import sys

request = json.load(sys.stdin)
items = request["input"]["items"]
if not isinstance(items, list) or not all(isinstance(item, str) for item in items):
    raise ValueError("items must be a list of strings")
print(json.dumps({"value": {"count": len(items), "distinct": len(items) == len(set(items))}}))
