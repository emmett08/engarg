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

A missing acquisition with an explicit parameterised `operation_spec` can be
implemented from the canonical report. Start with operator-owned bindings, even
if the initial manifest contains only `[tools]`:

```sh
engarg analyse --input request.json --tools tools.toml \
  --model configured-model --format json --output analysis.json \
  --eal-host /opt/eal/bin/eal-host

engarg eal implement --input analysis.json --tools tools.toml \
  --model configured-coding-model --eal-host /opt/eal/bin/eal-host \
  --tool-language python --toolchain examples/toolchains/python.json \
  --tool-store .engarg-tools --tool-output-dir implemented \
  --tool-max-calls 16 --tool-timeout-secs 3600

engarg build --input implemented/analysis.with-tools.json \
  --tools implemented/bindings/tools.toml --output-dir investigation

eal-host run investigation/case.eal --tools investigation/tools.toml --workspace run
```

The selected host's EAL/4 capabilities are checked before implementation model
calls. `implemented/analysis.with-tools.json` uses the same typed report contract
as authoring, revision and build. Its `tooling.tools` field identifies the merged
manifest; `tooling.operations` retains the exact tested package per acquisition.
Original bindings and their environment grants are preserved. Missing operation
semantics remain explicit gaps, and missing source access or credentials cannot
be supplied by code generation.

`analyse --implement-tools` runs this same stage after authoring when `--tools`,
`--format json`, `--output` and the toolchain/store/budget options above are
supplied. `engarg ticket` can opt into the same stage through an explicit
`implementation` configuration. Neither command uses per-need custom stubs or a
second registration workflow.

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
engarg analyse --input request.json --tools tools.toml \
  --model configured-model --format json --output analysis.json
engarg build --input analysis.json --tools tools.toml --output-dir investigation
eal-host run investigation/case.eal --tools investigation/tools.toml --workspace run
```

Independent scoped reviews remain separate evidence obligations. Neither tool
verification nor a successful acquisition establishes the claim by itself. See
[executable packages](executable-packages.md) for package movement and runtime
identity, and [support planning](support-planning.md) for authored requirements.
