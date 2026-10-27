# Build an executable evidence package

An EngArg build emits `case.eal`, `tools.toml` and the tool implementations or
retained resources referenced by that manifest. EAL executes those artefacts
without an EngArg runtime, capability catalogue, registration directory or
semantic execution-plan sidecar.

```sh
engarg build --input analysis.json --tools tools.toml --output-dir package
eal-host run package/case.eal --tools package/tools.toml --workspace run
```

The analysis must contain explicit support requirements, acceptance criteria and
acquisition dependencies. Bind each acquisition identifier in `tools.toml`, or
use its compiler-generated EAL operation name. The compiler rejects an absent
binding, conflicting compiler context and a formal query represented as an
acquired observation. Formal queries are authored program data; acquired values
are observations or scoped reviews.

```toml
[tools.measurement]
kind = "command"
version = "1"
argv = ["python3", "tools/measure.py"]
inherit_env = ["PATH"]
timeout_seconds = 15
max_output_bytes = 1048576
```

A command receives the EAL acquisition request on stdin. It returns a JSON
observation envelope with `value` and, when it reports an existing measurement,
its original `observed_at` timestamp. Direct MCP stdio/HTTP, fixed-authority HTTP
GET and retained-file bindings are also supported by EAL; their concrete
transport and response mappings belong in the manifest.

Commands inherit no ambient environment by default. `inherit_env` names the
operator-selected variables; credentials remain external values. Relative paths
and command working directories are anchored to the manifest's directory, so a
package can move independently of its run store. Custom implementations belong
under `tools/`; explicit schema, retained-file and dependency-pin resources are
copied as well. A non-executable data argument outside `tools/` should be a
literal existing relative file or an explicit pinned dependency. Dynamic file
names and external services remain explicit tool inputs, not hidden bundled
resources.

`build.json` is an inspection receipt containing source and executable hashes,
file hashes, emitted claim names and pending reviews. It is not an executable
input. Copying `case.eal`, `tools.toml` and the referenced implementations to a
fresh directory is sufficient to run. A successful build does not supply a
missing review or imply that the claim is supported; the EAL assessment states
which observations and arguments are usable.

The builder validates before publication and stages files privately. It reads
regular resources within the manifest root without following links, checks
supplied SHA-256 pins, bounds the complete written package to 256 MiB, and
preserves executable permission bits. Linux and Apple platforms publish the
completed directory atomically without replacing an existing output. A failed
copy or pin check removes the stage and leaves no final package directory.

The regression `tests/executable_package.rs` builds a bounded synthetic example,
moves only its executable artefacts, destroys the original build inputs and
executes it through a real EAL MCP host. Run the installed integration check with:

```sh
ENGARG_EAL_HOST=/path/to/eal-host cargo test --test executable_package -- --ignored
```

The integration checks also acquire one explicitly requested finite batch once
for several requirements. Omitted, duplicated, substituted, truncated and
unavailable results cannot satisfy its declared checks. This establishes
coverage of the enumerated request, not completeness of an external population.

The other package tests cover conflicting context, reserved output names, path
escape, symbolic links, resource bounds, late pin failure and executable bits.
These tests establish packaging and execution behaviour, not the empirical
truth of an arbitrary claim.

`tests/executable_modes.rs` additionally compiles and moves seven bounded mode
cases, then executes their EAL through the same installed host. The native
cases assert the expected entailment, Wilson bound, Bayesian posterior, mean
contrast, affine intervention result and declared feature match. The practical
case consumes a scoped reviewer record; it does not infer a universal decision
procedure. Negative applicability, stale observations, source failures and
wrong or incomplete review records cannot support the selected conclusion.

```sh
ENGARG_EAL_HOST=/path/to/eal-host cargo test --test executable_modes -- --ignored
```
