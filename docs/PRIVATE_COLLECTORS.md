# Connect evidence from a private system

[Documentation](index.md) · [Support requirements](support-planning.md)

A CI investigation often needs records that only the engineering team can read.
The operator binds that source in `tools.toml`, choosing the executable or MCP
endpoint and the credentials it may use. The authored EAL programme specifies
which observations are needed and how they affect the claim. This separates the
question being investigated from the authority to access a particular system.

For example, bind an existing MCP tool to an authored acquisition ID:

```toml
[tools.inventory_snapshot]
kind = "mcp_stdio"
version = "1"
argv = ["python3", "tools/inventory_server.py"]
inherit_env = ["PATH", "INVENTORY_TOKEN"]
timeout_seconds = 30
max_output_bytes = 1048576

[tools.inventory_snapshot.direct]
tool = "snapshot"
response_format = "structured"
value_pointer = "/value"
observed_at_pointer = "/observed_at"
```

The response projection extracts the observation value and its original time.
A successful MCP exchange establishes that a response was obtained. The EAL
requirements separately check whether that value meets the claim's acceptance
criteria, scope and freshness requirements.

Keep endpoint selection and executable paths in operator configuration. Supply
secret values through the runtime environment rather than source, reports or
generated code. Authoring receives operation names, versions and any pinned
interface schemas. It can reason about the declared operation without receiving
credential values or choosing a new executable.

## Build with the bindings that will actually run

After configuring the host as described in [USAGE.md](USAGE.md), author and build:

```sh
engarg analyse --input request.json --tools tools.toml \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY --format json --output analysis.json \
  --eal-host "$EAL_HOST"

engarg build --input analysis.json --tools tools.toml --output-dir investigation

eal-host run investigation/case.eal --tools investigation/tools.toml \
  --workspace investigation/run
```

The build copies admitted local resources and verifies declared pins. Relative
paths resolve from the manifest directory, including after the package is moved.
The direct `eal-host` command uses the calling environment, subject to the
manifest's `inherit_env` list. EngArg's separate collection wrappers add an
explicit `--collector-env` forwarding boundary, as shown in [USAGE.md](USAGE.md).
A missing permission, unavailable source or stale observation remains an
acquisition gap in the retained execution result.

## Implement a missing operation from its contract

An unbound acquisition can be implemented when its authored `operation_spec`
defines a parameterised operation and its input/output contracts. The explicit
[tool implementation stage](TOOL_IMPLEMENTATION.md) freezes synthetic fixtures,
generates or repairs source under a selected toolchain and call budget, then
verifies and exports a portable package. A passing fixture demonstrates the
recorded implementation cases. Production evidence still requires access to the
actual source under the intended scope.

Review obligations follow the same identity discipline. A scoped judgement
needs the exact criterion, target and context, with the reviewer's original
verdict, reasons and time. Reading a retained review file preserves its original
age. See [the compiler mapping](EAL_MAPPING.md#scoped-reviews) for that contract
and [USAGE.md](USAGE.md) for importing reviews before execution.

Once these bindings are present, the [package workflow](executable-packages.md)
uses the EAL source, TOML and referenced implementations directly. Optional
[tickets](TICKETS.md) coordinate the same stages when a durable run record is
useful.
