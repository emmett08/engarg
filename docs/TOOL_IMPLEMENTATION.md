# Optional collector implementation

EngArg can generate and test missing collector implementations after argument planning.
The stage is opt-in. Existing collectors and verified cached packages remain reusable.

## Prepare a language toolchain

The language switch names the implementation language.
An operator-owned profile selects the compiler or interpreter, image, literal build and run arguments, and resource limits.
The profile language must match the switch.
The model supplies source files and cannot select these execution settings.

The examples use Docker images which must already be installed:

```sh
docker pull python:3.13-slim
docker pull node:24-slim
docker build --tag engarg-rust-tools:1.99.1 examples/toolchains/rust
```

[Python](../examples/toolchains/python.json) and
[JavaScript](../examples/toolchains/javascript.json) and
[Rust](../examples/toolchains/rust.json) profiles are included.
The Rust image supplies pinned Serde, arbitrary-precision JSON and blocking
Reqwest libraries. Its fixed compiler wrapper builds `/src/main.rs` offline
against those libraries and writes `/build/collector`.
Other languages use the same profile contract with an appropriate preloaded image and compiler commands.
Compiled outputs and compiler caches belong under `/build`.
Dependencies needed by an offline build must already be available in the image.

The [catalog sync authoring script](CATALOG_SYNC_AUTHORING.md) enables this Rust
profile with `--implement-tools` and prepares its image automatically. The switch
also selects a persistent verified tool store for reuse across runs.

The stage resolves and records the immutable image ID before generation.
It also records the Docker executable, EngArg runner and supervisor digests.
Changes invalidate the corresponding verified reuse identity.

## Generate an argument and its missing tools

```sh
engarg analyse --input request.json --model "$ENGARG_MODEL" \
  --api-key-env OPENAI_API_KEY \
  --implement-tools --tool-language python \
  --toolchain examples/toolchains/python.json \
  --tool-store .engarg-local/tool-store \
  --tool-output-dir .engarg-local/tool-runs/attempt-001 \
  --tool-max-calls 16 --tool-timeout-secs 3600 \
  --format json --output .engarg-local/analysis-with-tools.json
```

Omitting `--implement-tools` preserves the argument-only flow.
Implementation options require that switch during `analyse`.
The toolchain, persistent store and a new output directory are explicit.
The complete argument is retained before the first coding call.
A coding or verification failure preserves the argument, stage audit and any candidate or test results already produced.

The tooling budget is separate from the argument's `--max-calls`.
A new collector normally uses one contract/fixture call followed by one coding call.
A retry can reuse frozen fixtures.
A qualifying verified package consumes no coding calls.
The stage checks the required budget before starting its coding calls.

## Work from a saved analysis

Argument generation can be skipped when an analysis is already available:

```sh
engarg eal implement --input .engarg-local/analysis.json \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --tool-language javascript \
  --toolchain examples/toolchains/javascript.json \
  --tool-store .engarg-local/tool-store \
  --tool-output-dir .engarg-local/tool-runs/attempt-002
```

The dedicated command enables the implementation stage.
It writes `analysis.with-tools.json` in the new output directory.
The original canonical argument and argument transcript are preserved.
Tooling has its own metadata and audit.

A later invocation can use the same store with a new output directory.
Reuse checks the exact contract, supplied request, language, source, fixtures and runner identity.
Changed or tampered packages cannot qualify through an old test receipt.
Known collector plans bypass code generation.
The stage writes the combined verified catalogue to `collectors.json` in its output directory.
Select that file through `--collector-catalogue` in later argument runs.

## Contract and verification gates

The contract and fixtures are frozen before the coding call.
They retain the evidence need's measurement, comparator and acceptance criterion.
The coding model receives the selected language, operator toolchain interface and the frozen contract.
Expected fixture values remain outside its coding prompt.

The stage executes the implementation and compares observed results with the recorded expectations.
It requires positive, measured-negative and acquisition-error cases.
Independent protocol probes exercise malformed input, missing identity and invalid context.
Successful observations must preserve the complete request and context, including large integers and escaped strings.
A measured negative remains a successful acquisition with its negative value.
An acquisition error produces no successful observation.

Test receipts bind the observed exits and output digests to the contract, source, fixture set and runner.
Registration is constructed only after every required check passes.
The registration path rechecks retained files and receipts.
An edited implementation requires fresh verification.

Each retained package exposes its `contract.json`, including the frozen fixtures,
the implementation files under `source/`, and the observed `test-results.json`.
Failed attempts retain the available review artefacts without adding a catalogue entry.

Passing these tests establishes conformance to the frozen fixtures.
Applicability, measurement truth and inferential sufficiency retain their separate review obligations.

## Isolated execution

Fixture containers use a read-only root filesystem and read-only source and fixture mounts.
They run as an unprivileged user with dropped capabilities, no new privileges, bounded processes, memory and CPU, and bounded temporary filesystems.
Fixture networking is disabled and production credentials are not forwarded.
The supervisor bounds elapsed time and captured output.
Explicit container cleanup also covers daemon-owned execution after timeout or cancellation.

Live collection is a later explicit action.
Its network setting and forwarded environment names come from the operator profile.
The default network setting is `none`.
Collector code is rebuilt and run from the verified source snapshot using the recorded image.
No live Kubernetes, Grafana or other service read is implied by a fixture test.

## Export matching EAL and registry files

Export the analysis carrying verified tooling metadata:

```sh
engarg eal export \
  --input .engarg-local/tool-runs/attempt-002/analysis.with-tools.json \
  --output .engarg-local/argument.eal \
  --tools-output .engarg-local/argument.tools.toml \
  --manifest .engarg-local/argument.bundle.json
```

The exporter validates the tooling receipt and builds matching source, context and registry bindings together.
Preparation applies the same verified registration path.
Generated registry commands invoke the trusted EngArg launcher with the verified package.
They grant the execution settings recorded in the operator profile.
Independent review tooling remains separately configured.

The implementation updates this pipeline.
It does not repair previously generated TOML or HTML files.
