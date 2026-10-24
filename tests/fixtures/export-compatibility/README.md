# Historical export fixtures

These persisted bundles were emitted by separately compiled reviewed producers.
They test the promised persisted format rather than serving as an oracle for new semantics.

- v1.json was emitted by EngArg 0.8.0 at 3fe69d2 using its examples/demo.json.
- v2.json was emitted by EngArg 0.9.0 at c4606e52d9dd07cf0e4e364d0dab30687118682c using its examples/demo.json.

v2-decimals.json uses the same v2 producer with a selected laboratory catalogue and an independently specified arbitrary-precision decimal schema and input.

The command was `engarg eal export --input examples/demo.json --manifest BUNDLE --output SOURCE`.
The compatibility tests require exact reconstructed source and complete bundle identity.
The new writer emits v3 and creates a new revision.
