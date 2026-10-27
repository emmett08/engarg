# Optional tool implementation

Tool authoring is a separate, explicit stage. An existing implementation can be
bound directly in `tools.toml`; no generation or qualification catalogue is
required. When an operation is missing, retain its input/output contract and
acceptance tests before generating or writing an implementation.

EngArg's optional implementation stage supports operator-selected language
profiles and bounded model calls. The profile fixes the compiler or interpreter,
container image, literal build/run arguments, environment grants, network policy
and resource limits. Generated source cannot replace those execution settings.
[Python](../examples/toolchains/python.json),
[JavaScript](../examples/toolchains/javascript.json) and
[Rust](../examples/toolchains/rust.json) profiles are included. Their dependencies
and compilers must already be available in the selected image.

`engarg eal implement --help` describes the explicit implementation command.
Optional discovery authoring can use `--evidence-planning graph --implement-tools`
with the required profile, store, output and budget options. Direct authoring
uses `--tools` and does not silently spend coding calls. The catalogue example
helper also performs no generation.

## Verify and retain

The measurement contract and fixtures are frozen before coding. Verification
checks positive, measured-negative and acquisition-error cases, malformed
requests, request identity, output schema and bounded execution. Original source
timestamps remain necessary for observations; a calculator's execution time
cannot refresh its inputs. Passing fixtures demonstrate the recorded cases,
not general scientific adequacy or source completeness.

Repairs retain the frozen contract, previous candidate, bounded failure
diagnostics and remaining budgets. Source/build/test defects can receive another
explicitly budgeted coding call. Provider errors, cancellation, infrastructure
failure and exhausted budgets remain failures. Retained packages and receipts
preserve the original execution identity. Changed or tampered source cannot
reuse an old passing receipt.

An infrastructure failure can be retried without model calls:

```sh
engarg eal tool-replay --attempt PATH/FAILED-ATTEMPT \
  --tool-store PATH/STORE --output replay-result.json
```

Replay keeps the original failed attempt and repeats the frozen verification
under the declared runner identity. Use a new output path and inspect the result.

## Export a direct executable binding

Export a verified package using its recorded package manifest:

```sh
engarg eal tool-export --package PATH/package.json \
  --tool-id measurement --output-dir tools/measurement
```

The new directory contains the source, pinned input/output schemas,
`verification.json`, `tool.toml`, a direct `tools.toml` binding and execution
instructions. It invokes EAL's `eal-tool-run`, which enforces the package's pinned
image, schemas, build/run arguments and resource policy. EAL executes this tool
without the EngArg authoring executable, a capability registry or a model.
Install the matching `eal-tool-run` and `eal-process-supervisor` companions,
Docker and the already verified immutable image. The runner does not pull images.

Keep the exported directory intact when deploying it. Incorporate its concrete
binding and referenced files into the investigation's operator manifest, then:

```sh
engarg analyse --input request.json --tools tools.toml --output analysis.json
engarg build --input analysis.json --tools tools.toml --output-dir investigation
eal-host run investigation/case.eal --tools investigation/tools.toml --workspace run
```

Independent scoped reviews remain separate evidence obligations. Neither tool
verification nor a successful acquisition establishes the claim by itself. See
[executable packages](executable-packages.md) for package movement and runtime
identity, and [support planning](support-planning.md) for authored requirements.
