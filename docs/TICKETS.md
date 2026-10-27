# Durable argument tickets

`engarg ticket` retains an authoring run, validates its direct tool bindings,
compiles a standalone package, and executes its EAL through the official MCP
host. The coordinator records each stage and its artifacts. The package contains
`case.eal`, `tools.toml`, tool implementations and an inspection-only `build.json`.
The EAL source and TOML are the executable inputs.

```sh
engarg ticket --spec ticket.json --run runs/example \
  --eal-coordinate /opt/eal/bin/eal-coordinate
```

A ticket configuration uses `engarg.ticket/2`:

```json
{
  "schema_version": "engarg.ticket/2",
  "ticket_id": "finite_inventory",
  "request": {
    "claim": "The bounded inventory satisfies the declared acceptance criteria.",
    "context": "The specified snapshot and interpretation only.",
    "evidence": []
  },
  "debate": {
    "participants": 3,
    "rounds": 1,
    "topology": { "kind": "ring" }
  },
  "tools": "tools.toml",
  "authoring_args": ["--model", "configured-model"],
  "eal_host": "/opt/eal/bin/eal-host",
  "environment": ["PATH", "HOME", "ENGARG_API_KEY"],
  "collector_environment": ["PATH"],
  "timeout_seconds": 3600,
  "stage_timeout_seconds": 900,
  "input_files": []
}
```

Paths are relative to the configuration file. The run directory must be new.
The EAL host requires its installed MCP server, method worker and process
supervisor companions. EngArg requires its `engarg-process-supervisor` companion.
The coordinator defaults to the host's sibling `eal-coordinate` unless supplied
explicitly. `export_policy` can name an explicit review freshness policy.

`tools.toml` declares the operator's concrete operations. Tool keys match authored
acquisition IDs; the compiler assigns their EAL names. A command example is:

```toml
[tools.inventory]
kind = "command"
version = "1"
argv = ["python3", "tools/inventory.py"]
inherit_env = ["PATH"]
```

Every inherited tool environment name and direct transport credential reference
must appear in `collector_environment`, and every collector name must also appear
in `environment`. Values are inherited at execution and are not stored in ticket
configuration. Authoring arguments accept only provider and bounded budget options.
The model receives operation names and versions, not executable or credential
selection authority.

To author missing implementations, add this optional object. Omitting it leaves
unbound operations as reported gaps:

```json
"implementation": {
  "language": "python",
  "toolchain": "toolchains/python.json",
  "store": ".engarg-tools",
  "provider_args": ["--model", "configured-coding-model"],
  "max_calls": 16,
  "timeout_seconds": 900
}
```

The implementation deadline must fit `stage_timeout_seconds`. Its toolchain is
pinned before authoring. Provider arguments cannot override paths, tools or the
selected host. Toolchain environment references and selected Docker environment
names must be explicitly included in `collector_environment` and `environment`.
The merged manifest is retained as the `integrated_tools` artifact and used by
export. Failed generation retains its original analysis and bounded diagnostics.

The ticket pins its configuration, tool manifest, implementation resources,
explicit additional input files, host and coordinator. Each stage rechecks those
identities. Package resources must remain within the manifest directory. Retained input files
have a 16 MiB individual bound and a 48 MiB aggregate bound, with at most 127
files; the configuration receipt reserves the remaining coordinator capacity.
Executable inputs are hashed with a separate 256 MiB bound.

The four stages are:

1. **Author:** `analyse --evidence-planning direct --tools ...` runs the selected
   debate topology and retains the argument and its support elaboration.
2. **Integrate:** match every acquisition to the manifest. When explicitly
   configured, implement and verify missing operation contracts through the same
   `eal implement` stage, then retain the merged TOML. Remaining gaps block the
   ticket with a retained explanation.
3. **Export:** `engarg build --input ... --tools ... --output-dir ...` writes the
   executable package. Review obligations without a supplied evidential basis
   remain explicit pending obligations.
4. **Execute:** the official host calls `eal_execute` with the package source,
   manifest context and selected claim. EAL resolves dependencies, acquires
   observations, evaluates derived evidence and runs the authored reasoning
   methods. Collection, assessment and the execution envelope are retained.

A complete, measured negative result can finish a ticket with an unsupported
claim. Missing, stale or failed required evidence blocks completion. An accepted
ticket is therefore not a declaration that its claim is true. Inspection-only
receipts do not provide observations or replace runtime assessment.

The previous ticket configuration schema is intentionally rejected. Migrate the
operator bindings into `tools.toml`; there is no registration, qualification or
mandatory application-register stage in the current workflow.

Native integration tests use a real EAL installation and a local bounded fixture:

```sh
ENGARG_EAL_HOST=/opt/eal/bin/eal-host \
ENGARG_EAL_COORDINATE=/opt/eal/bin/eal-coordinate \
ENGARG_TICKET_TOPOLOGIES=ring \
cargo test --test ticket_native -- --ignored
```

Omit `ENGARG_TICKET_TOPOLOGIES` to exercise all nine topology presets. The tests
also check measured negatives, unavailable sources and missing operation bindings.
