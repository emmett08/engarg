# Execute an argument package

The catalogue helper builds a portable package and executes its EAL file. The
package contains `case.eal`, `tools.toml` and any local tool implementation files.
The EAL source declares acquisitions, derived values, method inputs and claim
requirements. TOML binds the source's operation names to concrete commands or MCP
servers. Running the package acquires evidence and retains observations before
assessing the selected claim.

## Run the synthetic example

Install the matching EAL runtime with `bash scripts/install-eal-runtime.sh`, or
supply an installed host with `--eal-host PATH`. Then run:

```bash
bash scripts/run-catalog-sync-authoring.sh --simple-example \
  --output-dir engarg-runs/finite-inventory
```

The example starts a local MCP server that returns a synthetic inventory. A
separate command audits the returned identifiers; EAL independently derives the
count and distinct count and checks the final conjunction. Its claim concerns
only that finite synthetic snapshot. It makes no claim about a production
catalogue, Kubernetes cluster or Grafana installation.

The script performs no model calls. It needs Bash 3.2 or later, Python 3 and the
matching `eal-host`. Its default invocation, with no arguments, runs this same
example in a new timestamped directory.

## Build and run an authored case

Authoring is a separate, explicit step. Supply a request and the tool bindings
that the author may refer to:

```bash
engarg analyse --input request.json --evidence-planning direct \
  --tools tools.toml --output analysis.json
```

The model receives operation names, versions and pinned interface schemas. Executable paths, environment
values and connection credentials stay out of the authoring prompt. The report
retains the argument, support elaboration, transcript and explicit unbound
operations. An unavailable operation remains a gap.

Review the report and tool implementations, then build and execute it:

```bash
bash scripts/run-catalog-sync-authoring.sh \
  --input analysis.json --tools tools.toml \
  --output-dir engarg-runs/catalogue-package
```

This is equivalent to:

```bash
engarg build --input analysis.json --tools tools.toml \
  --output-dir engarg-runs/catalogue-package

eal-host run engarg-runs/catalogue-package/case.eal \
  --tools engarg-runs/catalogue-package/tools.toml \
  --workspace engarg-runs/catalogue-package/run
```

The helper selects the emitted current claim from the build receipt. Use
`--claim ID` to select another emitted claim, `--workspace DIR` to choose the run
location, and `--eal-host PATH` or `EAL_HOST` to choose the host. `ENGARG_BIN`
selects the EngArg executable. The output directory must be new.

## Bind tools directly

A command consumes the EAL request envelope on standard input and returns its
bounded JSON response on standard output. An MCP binding names the concrete
remote tool and its response projection:

```toml
[tools.snapshot_source]
kind = "mcp_stdio"
version = "1"
argv = ["python3", "tools/snapshot.py"]
inherit_env = ["PATH"]
timeout_seconds = 10
max_output_bytes = 65536

[tools.snapshot_source.direct]
tool = "snapshot"
response_format = "structured"
value_pointer = "/value"
observed_at_pointer = "/observed_at"
```

Relative files and working directories resolve from the manifest directory.
Declare inherited environment names explicitly. A capability catalogue or
registration directory is unnecessary for execution. An operator may use
optional discovery to help choose a binding, then retain the resulting concrete
TOML and implementation.

## Interpret the result

A completed run is not necessarily a supported claim. Missing sources, stale
observations, invalid method inputs, failed criteria and absent scoped reviews
remain visible in the assessment. The helper preserves the host's exit status and artifacts. A successfully executed assessment may contain an
unsupported claim; inspect `assessment.claims[ID].status` in the response and the
retained collection. Structural validation is not evidence of truth.

Changing a tool binding requires rebuilding the package. Changing an argument
requires an explicit revision; replaying or replanning a prior report does not
silently replace its warrant or acceptance criteria. `engarg eal revise` validates
a supplied candidate while retaining the previous argument and dissent history.
Rebuild after the revision, then acquire evidence against the resulting exact
requests. Historical observations retain their original source and timestamps.

The script has no qualification, registration or hidden repair stage. Paid tool
implementation or argument authoring must be invoked separately. See
[executable packages](executable-packages.md) and
[reasoning methods](REASONING_METHODS.md) for the compiler and method contracts.
