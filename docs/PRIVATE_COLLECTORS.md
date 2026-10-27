# Private evidence tools

Private collectors are ordinary direct bindings in an operator-owned
`tools.toml`. EAL/4 owns acquisition dependencies, derived values, acceptance
criteria, reasoning methods and scoped review obligations. Tools supply
observations; they do not certify the argument merely by returning successfully.

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

Keep endpoint selection, credentials and executable paths in operator
configuration. Environment values are supplied at execution; secret values must
not be embedded in source, reports or generated code. The authoring model sees
operation names, versions and any pinned interface schemas.

```sh
engarg analyse --input request.json --tools tools.toml \
  --model configured-model --format json --output analysis.json \
  --eal-host /opt/eal/bin/eal-host

engarg build --input analysis.json --tools tools.toml --output-dir investigation

eal-host run investigation/case.eal --tools investigation/tools.toml \
  --workspace investigation/run
```

The build copies admitted local resources and verifies declared pins. Relative
paths remain relative to the manifest directory after moving the package.
Declare required source credentials explicitly through the runtime's environment
options. An unavailable source, missing permission, historical gap or stale
observation remains an execution gap.

An acquisition without a binding can be implemented when its authored
`operation_spec` supplies a parameterised function definition and input/output
contracts. Use the explicit [tool implementation stage](TOOL_IMPLEMENTATION.md)
with an operator-selected toolchain and call budget. It freezes synthetic
fixtures before coding, verifies the implementation, exports its portable tool
package, and merges its binding into a new TOML. A successful synthetic fixture
run is not evidence that a private production claim is true.

There is one executable package workflow. The former `eal prepare`, JSON export
bundles and automatically manufactured custom stubs have been removed. Reauthor
or explicitly revise old reports into the current canonical report with complete
support elaboration, then build the package. Optional capability discovery can
help select tools; execution requires only the EAL source, concrete TOML and
referenced implementations.

See [executable packages](executable-packages.md),
[reasoning methods](REASONING_METHODS.md) and [tickets](TICKETS.md).
